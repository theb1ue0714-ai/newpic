use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use newpic_platform::geometry::{
    Display, GeometryError, PhysicalPoint, PhysicalRect, PhysicalSize,
};

use crate::app_state::AppState;
pub use crate::app_state::{CaptureAction, SelectionMode, SelectionRequest};
use crate::diagnostics::EventLevel;

pub const OVERLAY_WINDOW_PREFIX: &str = "overlay-";

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum OverlayError {
    #[error("capture or selection is already active")]
    AlreadyActive,
    #[error("selection cancelled by user")]
    Cancelled,
    #[error("geometry error: {0}")]
    Geometry(#[from] GeometryError),
    #[error("no active selection")]
    NoActiveSelection,
    #[error("window error: {0}")]
    Window(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalRectDto {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl TryFrom<PhysicalRectDto> for PhysicalRect {
    type Error = GeometryError;

    fn try_from(dto: PhysicalRectDto) -> Result<Self, Self::Error> {
        PhysicalRect::new(dto.x, dto.y, dto.width, dto.height)
    }
}

impl From<PhysicalRect> for PhysicalRectDto {
    fn from(rect: PhysicalRect) -> Self {
        Self {
            x: rect.origin().x,
            y: rect.origin().y,
            width: rect.size().width(),
            height: rect.size().height(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayInfo {
    pub request: SelectionRequest,
    pub display_bounds: PhysicalRectDto,
    pub scale_factor: f64,
}

#[derive(Debug, Default)]
pub struct OverlayCoordinator {
    active_request: Option<SelectionRequest>,
    last_region: Option<PhysicalRect>,
}

impl OverlayCoordinator {
    pub const fn new() -> Self {
        Self {
            active_request: None,
            last_region: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active_request.is_some()
    }

    pub fn active_request(&self) -> Option<&SelectionRequest> {
        self.active_request.as_ref()
    }

    pub fn start_selection(&mut self, request: SelectionRequest) -> Result<(), OverlayError> {
        if self.is_active() {
            return Err(OverlayError::AlreadyActive);
        }
        self.active_request = Some(request);
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.active_request = None;
        // Preserves last_region for F8 repeat
    }

    pub fn complete(&mut self, rect: PhysicalRect) -> Result<(), OverlayError> {
        self.active_request = None;
        self.last_region = Some(rect);
        Ok(())
    }

    pub const fn last_region(&self) -> Option<PhysicalRect> {
        self.last_region
    }

    pub fn clamp_fixed_region(
        pointer: PhysicalPoint,
        requested: PhysicalSize,
        display: &Display,
    ) -> Result<PhysicalRect, GeometryError> {
        newpic_platform::geometry::fixed_region_at(pointer, requested, display)
    }

    pub fn validate_repeat_region(&self, displays: &[Display]) -> Option<PhysicalRect> {
        let region = self.last_region?;
        let intersections =
            newpic_platform::geometry::intersections_with_displays(region, displays);
        if intersections.is_empty() {
            None
        } else {
            Some(region)
        }
    }
}

/// Spawns transparent borderless always-on-top overlay windows across attached displays.
pub fn open_overlays<R: Runtime>(app: &AppHandle<R>, _request: &SelectionRequest) {
    let state = app.state::<AppState>();
    let displays = match state.displays() {
        Ok(list) if !list.is_empty() => list,
        _ => {
            state.record(
                EventLevel::Error,
                "cannot open overlay: no displays detected",
            );
            return;
        }
    };

    for (index, display) in displays.iter().enumerate() {
        let label = format!("{OVERLAY_WINDOW_PREFIX}{index}");
        // Close any pre-existing window with this label
        if let Some(existing) = app.get_webview_window(&label) {
            let _ = existing.close();
        }

        let bounds = display.bounds();
        let scale = display.scale_factor().get();

        // Convert physical bounds to logical for Tauri window builder
        let logical_x = bounds.left() as f64 / scale;
        let logical_y = bounds.top() as f64 / scale;
        let logical_w = bounds.size().width() as f64 / scale;
        let logical_h = bounds.size().height() as f64 / scale;

        let builder =
            WebviewWindowBuilder::new(app, &label, WebviewUrl::App("overlay.html".into()))
                .title("NewPic Overlay")
                .transparent(true)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .position(logical_x, logical_y)
                .inner_size(logical_w, logical_h);

        match builder.build() {
            Ok(window) => {
                let _ = window.set_focus();
            }
            Err(error) => {
                state.record(
                    EventLevel::Error,
                    format!("failed to create overlay window {label}: {error}"),
                );
            }
        }
    }
}

/// Closes all open overlay windows and yields to allow the compositor to clear them before taking screenshot.
pub fn close_overlays<R: Runtime>(app: &AppHandle<R>) {
    let windows = app.webview_windows();
    for (label, window) in windows {
        if label.starts_with(OVERLAY_WINDOW_PREFIX) {
            let _ = window.close();
        }
    }
    // Brief sleep to ensure overlay windows are fully unmapped by compositor before screen capture
    std::thread::sleep(Duration::from_millis(50));
}
