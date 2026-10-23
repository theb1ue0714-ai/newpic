//! Global shortcut bookkeeping for the four capture actions.
//!
//! The registration logic is independent of Tauri so that the approved
//! mapping, per-action failure isolation, and replacement rollback can be
//! tested with a fake registrar.

use std::str::FromStr;

use newpic_core::settings::Hotkeys;
use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::Shortcut;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureAction {
    Region,
    RepeatRegion,
    FixedRegion,
    ActiveWindow,
}

impl CaptureAction {
    pub const ALL: [Self; 4] = [
        Self::Region,
        Self::RepeatRegion,
        Self::FixedRegion,
        Self::ActiveWindow,
    ];

    /// The user-facing label used by the tray menu and diagnostics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Region => "영역 캡처",
            Self::RepeatRegion => "마지막 영역 다시 캡처",
            Self::FixedRegion => "저장 영역 이동 캡처",
            Self::ActiveWindow => "활성 창 캡처",
        }
    }

    /// A stable identifier for menu items and IPC payloads.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::RepeatRegion => "repeat_region",
            Self::FixedRegion => "fixed_region",
            Self::ActiveWindow => "active_window",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }

    const fn index(self) -> usize {
        match self {
            Self::Region => 0,
            Self::RepeatRegion => 1,
            Self::FixedRegion => 2,
            Self::ActiveWindow => 3,
        }
    }
}

/// The accelerator configured for each action, in [`CaptureAction::ALL`] order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBindings([String; 4]);

impl HotkeyBindings {
    pub fn from_settings(hotkeys: &Hotkeys) -> Self {
        Self([
            hotkeys.region.clone(),
            hotkeys.repeat_region.clone(),
            hotkeys.fixed_region.clone(),
            hotkeys.active_window.clone(),
        ])
    }

    pub fn get(&self, action: CaptureAction) -> &str {
        &self.0[action.index()]
    }

    pub fn entries(&self) -> [(CaptureAction, &str); 4] {
        CaptureAction::ALL.map(|action| (action, self.get(action)))
    }

    pub fn into_settings(self) -> Hotkeys {
        let [region, repeat_region, fixed_region, active_window] = self.0;
        Hotkeys {
            region,
            repeat_region,
            fixed_region,
            active_window,
        }
    }
}

/// Port to the operating system's global shortcut service.
pub trait ShortcutRegistrar {
    fn register(&mut self, shortcut: &Shortcut) -> Result<(), String>;
    fn unregister(&mut self, shortcut: &Shortcut) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RegistrationStatus {
    Registered { accelerator: String },
    Failed { accelerator: String, reason: String },
}

impl RegistrationStatus {
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    pub fn accelerator(&self) -> &str {
        match self {
            Self::Registered { accelerator } | Self::Failed { accelerator, .. } => accelerator,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistrationEntry {
    pub action: CaptureAction,
    pub label: &'static str,
    pub status: RegistrationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HotkeyError {
    #[error("단축키 \"{accelerator}\"를 해석할 수 없습니다: {reason}")]
    Invalid { accelerator: String, reason: String },
    #[error("단축키 \"{accelerator}\"는 이미 {}에 사용 중입니다", .other.label())]
    Conflict {
        accelerator: String,
        other: CaptureAction,
    },
    #[error("단축키 \"{accelerator}\"를 등록할 수 없습니다: {reason}")]
    Registration { accelerator: String, reason: String },
}

/// Result of applying a full set of requested bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// Bindings actually in effect; rejected actions keep their previous value.
    pub effective: Hotkeys,
    pub rejected: Vec<(CaptureAction, HotkeyError)>,
}

#[derive(Debug, Clone)]
struct Slot {
    status: RegistrationStatus,
    /// Present only while the shortcut is registered with the OS.
    active: Option<Shortcut>,
}

#[derive(Debug, Clone)]
pub struct HotkeyManager {
    slots: [Slot; 4],
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotkeyManager {
    pub fn new() -> Self {
        let empty = || Slot {
            status: RegistrationStatus::Failed {
                accelerator: String::new(),
                reason: "not registered yet".to_owned(),
            },
            active: None,
        };
        Self {
            slots: [empty(), empty(), empty(), empty()],
        }
    }

    /// Registers every action independently. A failing action is recorded
    /// as failed and never prevents the remaining actions from registering.
    pub fn register_all(&mut self, registrar: &mut dyn ShortcutRegistrar, hotkeys: &Hotkeys) {
        self.unregister_all(registrar);
        let bindings = HotkeyBindings::from_settings(hotkeys);
        for (action, accelerator) in bindings.entries() {
            let status = match self.try_register(registrar, action, accelerator) {
                Ok(shortcut) => {
                    self.slots[action.index()].active = Some(shortcut);
                    RegistrationStatus::Registered {
                        accelerator: accelerator.to_owned(),
                    }
                }
                Err(error) => RegistrationStatus::Failed {
                    accelerator: accelerator.to_owned(),
                    reason: error.to_string(),
                },
            };
            self.slots[action.index()].status = status;
        }
    }

    /// Replaces one action's shortcut. The new shortcut is registered before
    /// the old one is released, so any failure leaves the previous shortcut
    /// active and the other actions untouched.
    pub fn replace(
        &mut self,
        registrar: &mut dyn ShortcutRegistrar,
        action: CaptureAction,
        accelerator: &str,
    ) -> Result<(), HotkeyError> {
        let shortcut = parse(accelerator)?;
        let previous = self.slots[action.index()].active;
        if previous == Some(shortcut) {
            self.slots[action.index()].status = RegistrationStatus::Registered {
                accelerator: accelerator.to_owned(),
            };
            return Ok(());
        }
        let shortcut = self.try_register(registrar, action, accelerator)?;
        if let Some(previous) = previous {
            // Releasing can only fail if the OS already dropped it; the new
            // shortcut is active either way.
            let _ = registrar.unregister(&previous);
        }
        self.slots[action.index()] = Slot {
            status: RegistrationStatus::Registered {
                accelerator: accelerator.to_owned(),
            },
            active: Some(shortcut),
        };
        Ok(())
    }

    /// Applies requested bindings action by action. Unchanged actions are
    /// left alone; rejected changes keep the previous binding.
    pub fn apply(
        &mut self,
        registrar: &mut dyn ShortcutRegistrar,
        hotkeys: &Hotkeys,
    ) -> ApplyOutcome {
        let requested = HotkeyBindings::from_settings(hotkeys);
        let mut effective = self.bindings();
        let mut rejected = Vec::new();
        for (action, accelerator) in requested.entries() {
            let slot = &self.slots[action.index()];
            if slot.active.is_some() && slot.status.accelerator() == accelerator {
                continue;
            }
            match self.replace(registrar, action, accelerator) {
                Ok(()) => effective.0[action.index()] = accelerator.to_owned(),
                Err(error) => {
                    if self.slots[action.index()].active.is_none() {
                        // Nothing was active before, so the requested value
                        // becomes the recorded (failed) binding.
                        effective.0[action.index()] = accelerator.to_owned();
                        self.slots[action.index()].status = RegistrationStatus::Failed {
                            accelerator: accelerator.to_owned(),
                            reason: error.to_string(),
                        };
                    }
                    rejected.push((action, error));
                }
            }
        }
        ApplyOutcome {
            effective: effective.into_settings(),
            rejected,
        }
    }

    pub fn unregister_all(&mut self, registrar: &mut dyn ShortcutRegistrar) {
        for slot in &mut self.slots {
            if let Some(shortcut) = slot.active.take() {
                let _ = registrar.unregister(&shortcut);
            }
        }
    }

    pub fn status(&self, action: CaptureAction) -> &RegistrationStatus {
        &self.slots[action.index()].status
    }

    pub fn action_for(&self, shortcut: &Shortcut) -> Option<CaptureAction> {
        CaptureAction::ALL
            .into_iter()
            .find(|action| self.slots[action.index()].active.as_ref() == Some(shortcut))
    }

    pub fn report(&self) -> Vec<RegistrationEntry> {
        CaptureAction::ALL
            .into_iter()
            .map(|action| RegistrationEntry {
                action,
                label: action.label(),
                status: self.status(action).clone(),
            })
            .collect()
    }

    fn bindings(&self) -> HotkeyBindings {
        HotkeyBindings(
            CaptureAction::ALL.map(|action| self.status(action).accelerator().to_owned()),
        )
    }

    fn try_register(
        &self,
        registrar: &mut dyn ShortcutRegistrar,
        action: CaptureAction,
        accelerator: &str,
    ) -> Result<Shortcut, HotkeyError> {
        let shortcut = parse(accelerator)?;
        if let Some(other) = CaptureAction::ALL
            .into_iter()
            .find(|other| *other != action && self.slots[other.index()].active == Some(shortcut))
        {
            return Err(HotkeyError::Conflict {
                accelerator: accelerator.to_owned(),
                other,
            });
        }
        registrar
            .register(&shortcut)
            .map_err(|reason| HotkeyError::Registration {
                accelerator: accelerator.to_owned(),
                reason,
            })?;
        Ok(shortcut)
    }
}

fn parse(accelerator: &str) -> Result<Shortcut, HotkeyError> {
    Shortcut::from_str(accelerator.trim()).map_err(|error| HotkeyError::Invalid {
        accelerator: accelerator.to_owned(),
        reason: error.to_string(),
    })
}

/// [`ShortcutRegistrar`] backed by the official global-shortcut plugin.
struct PluginRegistrar<'a, R: tauri::Runtime>(&'a tauri_plugin_global_shortcut::GlobalShortcut<R>);

impl<R: tauri::Runtime> ShortcutRegistrar for PluginRegistrar<'_, R> {
    fn register(&mut self, shortcut: &Shortcut) -> Result<(), String> {
        self.0
            .register(*shortcut)
            .map_err(|error| error.to_string())
    }

    fn unregister(&mut self, shortcut: &Shortcut) -> Result<(), String> {
        self.0
            .unregister(*shortcut)
            .map_err(|error| error.to_string())
    }
}

/// Runs `f` with the OS registrar, or returns `None` when the global
/// shortcut service failed to start on this session.
pub fn with_registrar<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    f: impl FnOnce(&mut dyn ShortcutRegistrar) -> T,
) -> Option<T> {
    use tauri::Manager;
    let shortcuts = app.try_state::<tauri_plugin_global_shortcut::GlobalShortcut<R>>()?;
    Some(f(&mut PluginRegistrar(&shortcuts)))
}

/// Marks every action as failed because no shortcut service is available.
impl HotkeyManager {
    pub fn mark_unavailable(&mut self, hotkeys: &Hotkeys, reason: &str) {
        for (action, accelerator) in HotkeyBindings::from_settings(hotkeys).entries() {
            self.slots[action.index()] = Slot {
                status: RegistrationStatus::Failed {
                    accelerator: accelerator.to_owned(),
                    reason: reason.to_owned(),
                },
                active: None,
            };
        }
    }
}
