use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use newpic_platform::geometry::{
    Display, GeometryError, PhysicalPoint, PhysicalRect, PhysicalSize,
};

use crate::app_state::AppState;
pub use crate::app_state::{CaptureAction, PhysicalRectDto, SelectionMode, SelectionRequest};
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
    #[error("invalid overlay window label: {0}")]
    InvalidWindowLabel(String),
    #[error("overlay display index is out of range: {0}")]
    DisplayIndexOutOfRange(usize),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverlayInfo {
    pub request: SelectionRequest,
    pub display_bounds: PhysicalRectDto,
    pub scale_factor: f64,
    pub active_display: bool,
    pub initial_rect: Option<PhysicalRectDto>,
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

    pub fn complete(&mut self, rect: PhysicalRect) -> Result<SelectionRequest, OverlayError> {
        let request = self
            .active_request
            .take()
            .ok_or(OverlayError::NoActiveSelection)?;
        if request.action == CaptureAction::Region {
            self.last_region = Some(rect);
        }
        Ok(request)
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

fn display_contains(display: &Display, rect: PhysicalRect) -> bool {
    display.bounds().intersection(rect) == Some(rect)
}

fn display_index_from_label(label: &str) -> Result<usize, OverlayError> {
    let suffix = label
        .strip_prefix(OVERLAY_WINDOW_PREFIX)
        .ok_or_else(|| OverlayError::InvalidWindowLabel(label.to_owned()))?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(OverlayError::InvalidWindowLabel(label.to_owned()));
    }
    let index: usize = suffix
        .parse()
        .map_err(|_| OverlayError::InvalidWindowLabel(label.to_owned()))?;
    if label != format!("{OVERLAY_WINDOW_PREFIX}{index}") {
        return Err(OverlayError::InvalidWindowLabel(label.to_owned()));
    }
    Ok(index)
}

fn active_fixed_display_index(
    request: &SelectionRequest,
    displays: &[Display],
) -> Result<usize, OverlayError> {
    let initial = request
        .initial_rect
        .ok_or(OverlayError::NoActiveSelection)
        .and_then(|rect| PhysicalRect::try_from(rect).map_err(OverlayError::from))?;
    displays
        .iter()
        .position(|display| display_contains(display, initial))
        .ok_or(OverlayError::Geometry(
            GeometryError::RegionNotContainedBySingleDisplay,
        ))
}

pub fn overlay_info_for_label(
    label: &str,
    request: &SelectionRequest,
    displays: &[Display],
) -> Result<OverlayInfo, OverlayError> {
    let index = display_index_from_label(label)?;
    let display = displays
        .get(index)
        .ok_or(OverlayError::DisplayIndexOutOfRange(index))?;
    let active_display = match request.mode {
        SelectionMode::FreeRegion => true,
        SelectionMode::FixedSize => active_fixed_display_index(request, displays)? == index,
    };
    Ok(OverlayInfo {
        request: request.clone(),
        display_bounds: display.bounds().into(),
        scale_factor: display.scale_factor().get(),
        active_display,
        initial_rect: active_display.then_some(request.initial_rect).flatten(),
    })
}

pub trait OverlayWindowFactory {
    fn build(&mut self, label: &str, display: &Display) -> Result<(), OverlayError>;
    fn focus(&mut self, label: &str) -> Result<(), OverlayError>;
}

pub fn create_overlay_windows(
    request: &SelectionRequest,
    displays: &[Display],
    factory: &mut impl OverlayWindowFactory,
) -> Result<(), OverlayError> {
    for (index, display) in displays.iter().enumerate() {
        factory.build(&format!("{OVERLAY_WINDOW_PREFIX}{index}"), display)?;
    }

    if request.mode == SelectionMode::FixedSize {
        let active = active_fixed_display_index(request, displays)?;
        factory.focus(&format!("{OVERLAY_WINDOW_PREFIX}{active}"))?;
    }
    Ok(())
}

struct TauriOverlayWindowFactory<'a, R: Runtime> {
    app: &'a AppHandle<R>,
}

impl<R: Runtime> OverlayWindowFactory for TauriOverlayWindowFactory<'_, R> {
    fn build(&mut self, label: &str, display: &Display) -> Result<(), OverlayError> {
        if let Some(existing) = self.app.get_webview_window(label) {
            let _ = existing.close();
        }
        let bounds = display.bounds();
        let scale = display.scale_factor().get();
        WebviewWindowBuilder::new(self.app, label, WebviewUrl::App("overlay.html".into()))
            .title("NewPic Overlay")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .position(bounds.left() as f64 / scale, bounds.top() as f64 / scale)
            .inner_size(
                bounds.size().width() as f64 / scale,
                bounds.size().height() as f64 / scale,
            )
            .build()
            .map(|_| ())
            .map_err(|error| OverlayError::Window(error.to_string()))
    }

    fn focus(&mut self, label: &str) -> Result<(), OverlayError> {
        let window = self
            .app
            .get_webview_window(label)
            .ok_or_else(|| OverlayError::Window(format!("missing built overlay {label}")))?;
        window
            .set_focus()
            .map_err(|error| OverlayError::Window(error.to_string()))
    }
}

/// Spawns transparent borderless always-on-top overlay windows across attached displays.
pub fn open_overlays<R: Runtime>(app: &AppHandle<R>, request: &SelectionRequest) {
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

    let mut factory = TauriOverlayWindowFactory { app };
    if let Err(error) = create_overlay_windows(request, &displays, &mut factory) {
        state.record(
            EventLevel::Error,
            format!("failed to create overlay windows: {error}"),
        );
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
