//! System tray menu.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, Runtime};

use crate::commands::run_action;
use crate::hotkeys::CaptureAction;

pub const SETTINGS_ID: &str = "settings";
pub const QUIT_ID: &str = "quit";
pub const MAIN_WINDOW: &str = "main";

pub fn build<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let mut capture_items = Vec::new();
    for action in CaptureAction::ALL {
        capture_items.push(MenuItem::with_id(
            app,
            action.id(),
            action.label(),
            true,
            None::<&str>,
        )?);
    }
    let settings = MenuItem::with_id(app, SETTINGS_ID, "설정", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "NewPic 종료", true, None::<&str>)?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<R>> = Vec::new();
    for item in &capture_items {
        items.push(item);
    }
    items.push(&first_separator);
    items.push(&settings);
    items.push(&second_separator);
    items.push(&quit);
    let menu = Menu::with_items(app, &items)?;

    let mut tray = TrayIconBuilder::with_id("newpic")
        .tooltip("NewPic")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()));
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn handle_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    if let Some(action) = CaptureAction::from_id(id) {
        run_action(app, action);
        return;
    }
    match id {
        SETTINGS_ID => show_settings(app),
        QUIT_ID => app.exit(0),
        _ => {}
    }
}

pub fn show_settings<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
