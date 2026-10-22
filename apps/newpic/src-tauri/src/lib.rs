pub mod app_state;
pub mod autostart;
pub mod commands;
pub mod diagnostics;
pub mod hotkeys;
pub mod tray;

use newpic_core::settings::{
    load_or_recover, JsonFileSettingsRepository, RecoveryNotice, Settings,
};
use newpic_platform::capture::CaptureProvider;
use newpic_platform::clipboard::ArboardClipboard;
use tauri::{App, Manager, Runtime, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::app_state::{lock, AppPaths, AppState, Notice};
use crate::diagnostics::EventLevel;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            setup(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == tray::MAIN_WINDOW
                    && window.state::<AppState>().settings().run_in_tray
                {
                    // The settings window only hides; NewPic keeps running
                    // in the tray.
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::apply_default_preset,
            commands::choose_save_folder,
            commands::trigger_capture,
            commands::get_diagnostics,
            commands::open_permission_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NewPic");
}

fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let paths = resolve_paths(app)?;
    let repository = JsonFileSettingsRepository::new(paths.settings_file.clone());
    let (settings, settings_notice) = match load_or_recover(&repository) {
        Ok(outcome) => (
            outcome.settings,
            outcome.notice.map(|notice| match notice {
                RecoveryNotice::CorruptedSettings { backup_path } => format!(
                    "설정 파일이 손상되어 기본값으로 복구했습니다. 백업: {}",
                    backup_path.display()
                ),
            }),
        ),
        // Run with the preset in memory rather than refusing to start; the
        // file is left untouched for inspection.
        Err(error) => (
            Settings::default(),
            Some(format!(
                "설정을 불러오지 못해 기본값으로 실행합니다: {error}"
            )),
        ),
    };

    app.manage(AppState::new(
        paths,
        repository,
        settings.clone(),
        settings_notice.clone(),
        platform_provider(),
        Box::new(ArboardClipboard::new()),
    ));
    let handle = app.handle().clone();
    let state = app.state::<AppState>();

    if let Some(message) = settings_notice {
        state.record(EventLevel::Warning, message.clone());
        commands::notify(
            &handle,
            &Notice {
                title: "NewPic 설정 복구".to_owned(),
                body: message,
                level: EventLevel::Warning,
            },
        );
    }

    // Plugins whose setup can fail on some sessions are added at runtime so
    // a failure degrades that feature instead of stopping the app.
    let shortcut_plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                let action = lock(&app.state::<AppState>().hotkeys).action_for(shortcut);
                if let Some(action) = action {
                    commands::run_action(app, action);
                }
            }
        })
        .build();
    match handle.plugin(shortcut_plugin) {
        Ok(()) => {
            let mut manager = lock(&state.hotkeys);
            hotkeys::with_registrar(&handle, |registrar| {
                manager.register_all(registrar, &settings.hotkeys)
            });
        }
        Err(error) => lock(&state.hotkeys).mark_unavailable(
            &settings.hotkeys,
            &format!("global shortcut service is unavailable: {error}"),
        ),
    }
    let failed: Vec<String> = lock(&state.hotkeys)
        .report()
        .into_iter()
        .filter(|entry| entry.status.is_failed())
        .map(|entry| format!("{} ({})", entry.label, entry.status.accelerator()))
        .collect();
    if !failed.is_empty() {
        let message = format!("다음 단축키를 등록하지 못했습니다: {}", failed.join(", "));
        state.record(EventLevel::Warning, message.clone());
        commands::notify(
            &handle,
            &Notice {
                title: "단축키 등록 실패".to_owned(),
                body: message,
                level: EventLevel::Warning,
            },
        );
    }

    if let Err(error) = handle.plugin(tauri_plugin_autostart::Builder::new().build()) {
        state.record(
            EventLevel::Error,
            format!("login startup service is unavailable: {error}"),
        );
    }
    commands::reconcile_autostart(&handle, &state, settings.autostart);

    tray::build(app)?;
    state.record(EventLevel::Info, "NewPic started");
    Ok(())
}

fn resolve_paths<R: Runtime>(app: &App<R>) -> tauri::Result<AppPaths> {
    let path = app.path();
    let downloads_dir = path
        .download_dir()
        .or_else(|_| path.home_dir().map(|home| home.join("Downloads")))?;
    Ok(AppPaths {
        settings_file: path.app_config_dir()?.join("settings.json"),
        downloads_dir,
        recovery_dir: path.app_local_data_dir()?.join("recovery"),
    })
}

fn platform_provider() -> Box<dyn CaptureProvider> {
    #[cfg(target_os = "linux")]
    {
        Box::new(newpic_linux::LinuxCaptureProvider::detect())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(newpic_windows::WindowsCaptureProvider::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(newpic_macos::MacOsCaptureProvider::new())
    }
}
