//! Typed IPC commands for the settings window and the shared capture runner.

use std::path::PathBuf;
use std::thread;

use newpic_core::settings::{SaveDirectory, Settings, SettingsRepository};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_notification::NotificationExt;

use crate::app_state::{delivery_notice, lock, plan_capture, AppState, CapturePlan, Notice};
use crate::autostart::{self, AutostartStatus};
use crate::diagnostics::{Diagnostics, EventLevel};
use crate::hotkeys::{self, CaptureAction, RegistrationEntry};

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

    if let Err(error) = state.repository.save(&requested) {
        // Roll shortcuts back so the running app matches the file on disk.
        let mut manager = lock(&state.hotkeys);
        hotkeys::with_registrar(app, |registrar| manager.apply(registrar, &previous.hotkeys));
        return Err(CommandError::new(error));
    }

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
    match plan_capture(action, &state.settings()) {
        CapturePlan::Select(request) => {
            if let Err(error) = app.emit(SELECTION_REQUESTED_EVENT, &request) {
                state.record(
                    EventLevel::Error,
                    format!("could not open selection for {}: {error}", action.id()),
                );
            }
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
