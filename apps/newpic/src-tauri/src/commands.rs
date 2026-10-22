//! Typed IPC commands for the settings window and the shared capture runner.

use std::path::PathBuf;
use std::thread;

use newpic_core::settings::{SaveDirectory, Settings, SettingsRepository, Size};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

use crate::app_state::{
    delivery_notice, lock, plan_capture, AppState, CapturePlan, Notice, SelectionMode,
    SelectionRequest,
};
use crate::autostart::{self, AutostartStatus};
use crate::diagnostics::{Diagnostics, EventLevel};
use crate::hotkeys::{self, CaptureAction, RegistrationEntry};
use crate::overlay::{self, OverlayError, OverlayInfo, PhysicalRectDto};
use newpic_platform::capture::CaptureSource;
use newpic_platform::geometry::{Display, PhysicalRect, PhysicalSize, SavedFixedRegion};

pub const SELECTION_REQUESTED_EVENT: &str = "newpic://selection-requested";

#[cfg(target_os = "macos")]
const SCREEN_RECORDING_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub message: String,
}

impl CommandError {
    fn new(message: impl std::fmt::Display) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RejectedShortcut {
    pub action: CaptureAction,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SaveSettingsResponse {
    /// The settings now in effect. Rejected shortcuts keep their previous value.
    pub settings: Settings,
    pub hotkeys: Vec<RegistrationEntry>,
    pub rejected_shortcuts: Vec<RejectedShortcut>,
    pub autostart: AutostartStatus,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<SaveSettingsResponse, CommandError> {
    persist_settings(&app, &state, settings)
}

/// Restores the approved preset. The settings window confirms first; captured
/// images are never touched.
#[tauri::command]
pub fn apply_default_preset<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<SaveSettingsResponse, CommandError> {
    let response = persist_settings(&app, &state, Settings::default())?;
    state.record(EventLevel::Info, "default preset reapplied");
    Ok(response)
}

/// Sets a custom save folder chosen by the user. The path is validated by the
/// settings rules before anything is written.
#[tauri::command]
pub fn choose_save_folder<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: String,
) -> Result<SaveSettingsResponse, CommandError> {
    let mut settings = state.settings();
    settings.save.directory = SaveDirectory::Custom(PathBuf::from(path));
    persist_settings(&app, &state, settings)
}

#[tauri::command]
pub fn trigger_capture<R: Runtime>(app: AppHandle<R>, action: CaptureAction) {
    run_action(&app, action);
}

#[tauri::command]
pub fn get_overlay_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> Result<OverlayInfo, CommandError> {
    let coordinator = lock(&state.overlay);
    let request = coordinator
        .active_request()
        .cloned()
        .ok_or_else(|| CommandError::new("no active selection request"))?;
    let displays = state
        .displays()
        .map_err(|e| CommandError::new(format!("could not query displays: {e}")))?;
    overlay::overlay_info_for_label(window.label(), &request, &displays).map_err(CommandError::new)
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommittedSelection {
    pub request: SelectionRequest,
    pub settings: Settings,
}

/// Atomically persists F9 before making the coordinator observable as
/// complete. For F7 no settings write is needed; its completion updates only
/// the in-memory F8 slot.
pub fn commit_selection<R: SettingsRepository>(
    coordinator: &mut overlay::OverlayCoordinator,
    current: &Settings,
    repository: &R,
    displays: &[Display],
    rect: PhysicalRect,
) -> Result<CommittedSelection, CommandError> {
    let request = coordinator
        .active_request()
        .cloned()
        .ok_or_else(|| CommandError::new(OverlayError::NoActiveSelection))?;
    let mut settings = current.clone();
    if request.action == CaptureAction::FixedRegion {
        let saved = SavedFixedRegion::capture(rect, displays).map_err(CommandError::new)?;
        settings.fixed_region = Size::new(rect.size().width(), rect.size().height());
        settings.saved_fixed_region = Some(saved);
        repository.save(&settings).map_err(CommandError::new)?;
    }
    let completed = coordinator.complete(rect).map_err(CommandError::new)?;
    Ok(CommittedSelection {
        request: completed,
        settings,
    })
}

pub fn complete_selection_with_effects<R: SettingsRepository>(
    coordinator: &mut overlay::OverlayCoordinator,
    current: &Settings,
    repository: &R,
    displays: &[Display],
    rect: PhysicalRect,
    effects: (
        impl FnOnce(&CommittedSelection),
        impl FnOnce(),
        impl FnOnce(),
    ),
) -> Result<CommittedSelection, CommandError> {
    let committed = commit_selection(coordinator, current, repository, displays, rect)?;
    let (apply_settings, close_windows, dispatch_capture) = effects;
    apply_settings(&committed);
    close_windows();
    dispatch_capture();
    Ok(committed)
}

#[tauri::command]
pub fn complete_selection<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    rect: PhysicalRectDto,
) -> Result<(), CommandError> {
    let physical_rect = PhysicalRect::try_from(rect).map_err(CommandError::new)?;
    let displays = state
        .displays()
        .map_err(|error| CommandError::new(format!("could not query displays: {error}")))?;
    complete_selection_with_effects(
        &mut lock(&state.overlay),
        &state.settings(),
        &state.repository,
        &displays,
        physical_rect,
        (
            |committed| {
                if committed.request.action == CaptureAction::FixedRegion {
                    state.replace_settings(committed.settings.clone());
                }
            },
            || overlay::close_overlays(&app),
            || dispatch_region_capture(app.clone(), physical_rect),
        ),
    )?;
    Ok(())
}

fn dispatch_region_capture<R: Runtime>(app: AppHandle<R>, physical_rect: PhysicalRect) {
    thread::spawn(move || {
        let state = app.state::<AppState>();
        let Some(_guard) = state.try_begin_capture() else {
            state.record(
                EventLevel::Warning,
                "capture ignored: another capture is running",
            );
            return;
        };
        let notice = match state.capture(CaptureSource::Region(physical_rect)) {
            Ok(image) => {
                let result = state.deliver(&image);
                delivery_notice(&result, state.settings().success_notifications)
            }
            Err(reason) => Some(Notice {
                title: "영역 캡처 실패".to_owned(),
                body: reason,
                level: EventLevel::Error,
            }),
        };
        if let Some(notice) = notice {
            state.record(notice.level, notice.title.clone());
            notify(&app, &notice);
        }
    });
}

#[tauri::command]
pub fn cancel_selection<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    overlay::close_overlays(&app);
    lock(&state.overlay).cancel();
    state.record(EventLevel::Info, "selection cancelled");
    Ok(())
}

#[tauri::command]
pub fn get_diagnostics(state: State<'_, AppState>) -> Diagnostics {
    state.diagnostics()
}

/// Opens the OS capture-permission page. Returns `false` where the platform
/// has no such page (Windows, Linux).
#[tauri::command]
pub fn open_permission_settings() -> Result<bool, CommandError> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(SCREEN_RECORDING_SETTINGS)
            .spawn()
            .map_err(CommandError::new)?;
        Ok(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}

fn persist_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    mut requested: Settings,
) -> Result<SaveSettingsResponse, CommandError> {
    requested.validate().map_err(CommandError::new)?;
    let previous = state.settings();
    let displays = if requested.fixed_region != previous.fixed_region
        && requested.saved_fixed_region.is_some()
    {
        state
            .displays()
            .map_err(|error| CommandError::new(format!("could not query displays: {error}")))?
    } else {
        Vec::new()
    };

    let rejected_shortcuts = {
        let mut manager = lock(&state.hotkeys);
        let outcome = hotkeys::with_registrar(app, |registrar| {
            manager.apply(registrar, &requested.hotkeys)
        });
        match outcome {
            Some(outcome) => {
                requested.hotkeys = outcome.effective;
                outcome
                    .rejected
                    .into_iter()
                    .map(|(action, error)| RejectedShortcut {
                        action,
                        message: error.to_string(),
                    })
                    .collect()
            }
            // The shortcut service is unavailable; keep the requested values
            // so they apply once it is.
            None => Vec::new(),
        }
    };

    let requested =
        match persist_settings_to_repository(&state.repository, &previous, requested, &displays) {
            Ok(requested) => requested,
            Err(error) => {
                // Roll shortcuts back so the running app matches the file on disk.
                let mut manager = lock(&state.hotkeys);
                hotkeys::with_registrar(app, |registrar| {
                    manager.apply(registrar, &previous.hotkeys)
                });
                return Err(error);
            }
        };

    if requested.autostart != previous.autostart {
        reconcile_autostart(app, state, requested.autostart);
    }
    state.replace_settings(requested.clone());
    for rejected in &rejected_shortcuts {
        state.record(EventLevel::Warning, rejected.message.clone());
    }
    state.record(EventLevel::Info, "settings saved");

    Ok(SaveSettingsResponse {
        settings: requested,
        hotkeys: lock(&state.hotkeys).report(),
        rejected_shortcuts,
        autostart: lock(&state.autostart).clone(),
    })
}

pub fn prepare_settings_for_save(
    previous: &Settings,
    mut requested: Settings,
    displays: &[Display],
) -> Result<Settings, CommandError> {
    requested.validate().map_err(CommandError::new)?;
    if requested.fixed_region != previous.fixed_region {
        if let Some(saved) = requested.saved_fixed_region.clone() {
            let size =
                PhysicalSize::new(requested.fixed_region.width, requested.fixed_region.height)
                    .map_err(CommandError::new)?;
            let saved = saved.with_size(size, displays).map_err(CommandError::new)?;
            requested.fixed_region =
                Size::new(saved.rect().size().width(), saved.rect().size().height());
            requested.saved_fixed_region = Some(saved);
        }
    }
    Ok(requested)
}

pub fn persist_settings_to_repository<R: SettingsRepository>(
    repository: &R,
    previous: &Settings,
    requested: Settings,
    displays: &[Display],
) -> Result<Settings, CommandError> {
    let prepared = prepare_settings_for_save(previous, requested, displays)?;
    repository.save(&prepared).map_err(CommandError::new)?;
    Ok(prepared)
}

/// Reconciles login startup and reports failures without stopping the app.
pub fn reconcile_autostart<R: Runtime>(app: &AppHandle<R>, state: &AppState, desired: bool) {
    let status = match app.try_state::<tauri_plugin_autostart::AutoLaunchManager>() {
        Some(manager) => autostart::reconcile(desired, &*manager),
        None => AutostartStatus::Failed {
            desired,
            reason: "login startup service is unavailable".to_owned(),
        },
    };
    if let AutostartStatus::Failed { reason, .. } = &status {
        let message = format!("로그인 시 자동 실행을 설정하지 못했습니다: {reason}");
        state.record(EventLevel::Error, message.clone());
        notify(
            app,
            &Notice {
                title: "자동 실행 설정 실패".to_owned(),
                body: message,
                level: EventLevel::Error,
            },
        );
    }
    *lock(&state.autostart) = status;
}

/// Starts a capture action from a shortcut, the tray, or the settings window.
pub fn run_action<R: Runtime>(app: &AppHandle<R>, action: CaptureAction) {
    let state = app.state::<AppState>();
    let plan = match action {
        CaptureAction::RepeatRegion => {
            let displays = state.displays().unwrap_or_default();
            if let Some(region) = lock(&state.overlay).validate_repeat_region(&displays) {
                CapturePlan::Direct(CaptureSource::Region(region))
            } else {
                CapturePlan::Select(SelectionRequest {
                    action: CaptureAction::Region,
                    mode: SelectionMode::FreeRegion,
                    fixed_size: None,
                    fallback_from: Some(CaptureAction::RepeatRegion),
                    initial_rect: None,
                })
            }
        }
        other => {
            let displays = if other == CaptureAction::ActiveWindow {
                Vec::new()
            } else {
                match state.displays() {
                    Ok(displays) => displays,
                    Err(error) => {
                        state.record(
                            EventLevel::Error,
                            format!("could not query displays: {error}"),
                        );
                        return;
                    }
                }
            };
            match plan_capture(other, &state.settings(), &displays) {
                Ok(plan) => plan,
                Err(error) => {
                    state.record(
                        EventLevel::Error,
                        format!("could not plan capture: {error}"),
                    );
                    return;
                }
            }
        }
    };

    match plan {
        CapturePlan::Select(request) => {
            let mut overlay = lock(&state.overlay);
            if let Err(OverlayError::AlreadyActive) = overlay.start_selection(request.clone()) {
                state.record(
                    EventLevel::Warning,
                    "capture ignored: another capture or selection is running",
                );
                notify(
                    app,
                    &Notice {
                        title: "캡처 진행 중".to_owned(),
                        body: "이미 캡처 또는 영역 선택이 진행 중입니다.".to_owned(),
                        level: EventLevel::Warning,
                    },
                );
                return;
            }
            drop(overlay);
            overlay::open_overlays(app, &request);
        }
        CapturePlan::Direct(source) => {
            let app = app.clone();
            thread::spawn(move || {
                let state = app.state::<AppState>();
                let Some(_guard) = state.try_begin_capture() else {
                    state.record(
                        EventLevel::Warning,
                        "capture ignored: another capture is running",
                    );
                    return;
                };
                let notice = match state.capture(source) {
                    Ok(image) => {
                        let result = state.deliver(&image);
                        delivery_notice(&result, state.settings().success_notifications)
                    }
                    Err(reason) => Some(Notice {
                        title: format!("{} 실패", action.label()),
                        body: reason,
                        level: EventLevel::Error,
                    }),
                };
                if let Some(notice) = notice {
                    state.record(notice.level, notice.title.clone());
                    notify(&app, &notice);
                }
            });
        }
    }
}

pub fn notify<R: Runtime>(app: &AppHandle<R>, notice: &Notice) {
    if let Err(error) = app
        .notification()
        .builder()
        .title(&notice.title)
        .body(&notice.body)
        .show()
    {
        app.state::<AppState>()
            .record(EventLevel::Warning, format!("notification failed: {error}"));
    }
}
