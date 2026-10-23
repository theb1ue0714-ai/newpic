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
    #[error("overlay windows did not close: {0:?}")]
    CloseTimeout(Vec<String>),
    #[error("{primary}; cleanup failures: {cleanup:?}")]
    CleanupFailures {
        primary: String,
        cleanup: Vec<String>,
    },
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
    active_displays: Vec<Display>,
    last_region: Option<PhysicalRect>,
    next_session_id: u64,
    active_session_id: Option<u64>,
}

impl OverlayCoordinator {
    pub const fn new() -> Self {
        Self {
            active_request: None,
            active_displays: Vec::new(),
            last_region: None,
            next_session_id: 0,
            active_session_id: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active_request.is_some()
    }

    pub fn active_request(&self) -> Option<&SelectionRequest> {
        self.active_request.as_ref()
    }

    pub fn start_selection(&mut self, request: SelectionRequest) -> Result<u64, OverlayError> {
        self.start_selection_with_displays(request, Vec::new())
    }

    pub fn start_selection_with_displays(
        &mut self,
        request: SelectionRequest,
        displays: Vec<Display>,
    ) -> Result<u64, OverlayError> {
        if self.is_active() {
            return Err(OverlayError::AlreadyActive);
        }
        self.active_request = Some(request);
        self.active_displays = displays;
        self.next_session_id = self.next_session_id.wrapping_add(1).max(1);
        self.active_session_id = Some(self.next_session_id);
        Ok(self.next_session_id)
    }

    pub fn active_displays(&self) -> &[Display] {
        &self.active_displays
    }

    pub fn active_session_id(&self) -> Option<u64> {
        self.active_session_id
    }

    pub fn overlay_labels(&self) -> Vec<String> {
        self.active_session_id.map_or_else(Vec::new, |session| {
            (0..self.active_displays.len())
                .map(|index| overlay_label(session, index))
                .collect()
        })
    }

    pub fn active_overlay_label(&self) -> Option<String> {
        let session = self.active_session_id?;
        let request = self.active_request.as_ref()?;
        let index = if request.mode == SelectionMode::FixedSize {
            active_fixed_display_index(request, &self.active_displays).ok()?
        } else {
            0
        };
        Some(overlay_label(session, index))
    }

    pub fn cancel(&mut self) {
        self.active_request = None;
        self.active_displays.clear();
        self.active_session_id = None;
        // Preserves last_region for F8 repeat
    }

    pub fn complete(&mut self, rect: PhysicalRect) -> Result<SelectionRequest, OverlayError> {
        let request = self
            .active_request
            .take()
            .ok_or(OverlayError::NoActiveSelection)?;
        self.active_displays.clear();
        self.active_session_id = None;
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

pub fn start_overlay_session(
    coordinator: &mut OverlayCoordinator,
    request: SelectionRequest,
    displays: Vec<Display>,
    open: impl FnOnce(u64, &SelectionRequest, &[Display]) -> Result<(), OverlayError>,
) -> Result<(), OverlayError> {
    let session_id =
        coordinator.start_selection_with_displays(request.clone(), displays.clone())?;
    if let Err(error) = open(session_id, &request, &displays) {
        coordinator.cancel();
        return Err(error);
    }
    Ok(())
}

pub fn overlay_label(session_id: u64, index: usize) -> String {
    format!("{OVERLAY_WINDOW_PREFIX}{session_id}-{index}")
}

fn display_contains(display: &Display, rect: PhysicalRect) -> bool {
    display.bounds().intersection(rect) == Some(rect)
}

fn display_index_from_label(label: &str, session_id: u64) -> Result<usize, OverlayError> {
    let suffix = label
        .strip_prefix(&format!("{OVERLAY_WINDOW_PREFIX}{session_id}-"))
        .ok_or_else(|| OverlayError::InvalidWindowLabel(label.to_owned()))?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(OverlayError::InvalidWindowLabel(label.to_owned()));
    }
    let index: usize = suffix
        .parse()
        .map_err(|_| OverlayError::InvalidWindowLabel(label.to_owned()))?;
    if label != overlay_label(session_id, index) {
        return Err(OverlayError::InvalidWindowLabel(label.to_owned()));
    }
    Ok(index)
}

fn active_fixed_display_index(
    request: &SelectionRequest,
    displays: &[Display],
) -> Result<usize, OverlayError> {
    if let Some(selected_id) = &request.selected_display_id {
        return displays
            .iter()
            .position(|display| display.id().as_str() == selected_id)
            .ok_or(OverlayError::Geometry(GeometryError::DisplayNotFound(
                newpic_platform::geometry::DisplayId::new(selected_id.clone())?,
            )));
    }
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
    session_id: u64,
    request: &SelectionRequest,
    displays: &[Display],
) -> Result<OverlayInfo, OverlayError> {
    let index = display_index_from_label(label, session_id)?;
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
    fn cleanup(&mut self, label: &str) -> Result<(), OverlayError>;
}

fn cleanup_after_failure(
    factory: &mut impl OverlayWindowFactory,
    labels: impl IntoIterator<Item = String>,
    primary: OverlayError,
) -> OverlayError {
    let cleanup: Vec<_> = labels
        .into_iter()
        .filter_map(|label| factory.cleanup(&label).err().map(|error| error.to_string()))
        .collect();
    if cleanup.is_empty() {
        primary
    } else {
        OverlayError::CleanupFailures {
            primary: primary.to_string(),
            cleanup,
        }
    }
}

pub fn create_overlay_windows(
    session_id: u64,
    request: &SelectionRequest,
    displays: &[Display],
    factory: &mut impl OverlayWindowFactory,
) -> Result<(), OverlayError> {
    let mut built: Vec<String> = Vec::new();
    for (index, display) in displays.iter().enumerate() {
        let label = overlay_label(session_id, index);
        if let Err(error) = factory.build(&label, display) {
            let labels = std::iter::once(label).chain(built.into_iter().rev());
            return Err(cleanup_after_failure(factory, labels, error));
        }
        built.push(label);
    }

    if request.mode == SelectionMode::FixedSize {
        let active = match active_fixed_display_index(request, displays) {
            Ok(active) => active,
            Err(error) => {
                return Err(cleanup_after_failure(
                    factory,
                    built.into_iter().rev(),
                    error,
                ));
            }
        };
        if let Err(error) = factory.focus(&overlay_label(session_id, active)) {
            return Err(cleanup_after_failure(
                factory,
                built.into_iter().rev(),
                error,
            ));
        }
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

    fn cleanup(&mut self, label: &str) -> Result<(), OverlayError> {
        match self.app.get_webview_window(label) {
            Some(window) => {
                window
                    .hide()
                    .map_err(|error| OverlayError::Window(error.to_string()))?;
                window
                    .close()
                    .map_err(|error| OverlayError::Window(error.to_string()))
            }
            None => Ok(()),
        }
    }
}

/// Spawns transparent borderless always-on-top overlay windows across attached displays.
pub fn open_overlays<R: Runtime>(
    app: &AppHandle<R>,
    session_id: u64,
    request: &SelectionRequest,
    displays: &[Display],
) -> Result<(), OverlayError> {
    let state = app.state::<AppState>();
    if displays.is_empty() {
        return Err(OverlayError::Window("no displays detected".into()));
    }

    let current_prefix = format!("{OVERLAY_WINDOW_PREFIX}{session_id}-");
    for (label, window) in app.webview_windows() {
        if label.starts_with(OVERLAY_WINDOW_PREFIX) && !label.starts_with(&current_prefix) {
            let _ = window.hide();
            let _ = window.close();
        }
    }

    let mut factory = TauriOverlayWindowFactory { app };
    if let Err(error) = create_overlay_windows(session_id, request, displays, &mut factory) {
        state.record(
            EventLevel::Error,
            format!("failed to create overlay windows: {error}"),
        );
        return Err(error);
    }
    Ok(())
}

pub trait OverlayWindowCloser {
    fn hide(&mut self, label: &str) -> Result<(), OverlayError>;
    fn show(&mut self, label: &str) -> Result<(), OverlayError>;
    fn focus(&mut self, label: &str) -> Result<(), OverlayError>;
    fn is_visible(&self, label: &str) -> bool;
    fn close_best_effort(&mut self, label: &str);
    fn wait_for_state_change(&mut self);
}

pub fn close_overlay_windows(
    closer: &mut impl OverlayWindowCloser,
    labels: &[String],
    active_label: Option<&str>,
) -> Result<(), OverlayError> {
    let mut hidden: Vec<String> = Vec::new();
    for label in labels {
        if let Err(error) = closer.hide(label) {
            let mut restore_errors = Vec::new();
            for hidden_label in &hidden {
                if let Err(restore) = closer.show(hidden_label) {
                    restore_errors.push(restore.to_string());
                }
            }
            if let Some(active_label) = active_label {
                if let Err(restore) = closer.focus(active_label) {
                    restore_errors.push(restore.to_string());
                }
            }
            return Err(if restore_errors.is_empty() {
                error
            } else {
                OverlayError::CleanupFailures {
                    primary: error.to_string(),
                    cleanup: restore_errors,
                }
            });
        }
        hidden.push(label.clone());
    }
    for _ in 0..100 {
        let visible: Vec<_> = labels
            .iter()
            .filter(|label| closer.is_visible(label))
            .cloned()
            .collect();
        if visible.is_empty() {
            for label in labels {
                closer.close_best_effort(label);
            }
            return Ok(());
        }
        closer.wait_for_state_change();
    }
    let primary = OverlayError::CloseTimeout(
        labels
            .iter()
            .filter(|label| closer.is_visible(label))
            .cloned()
            .collect(),
    );
    let mut restore_errors = Vec::new();
    for label in labels {
        if let Err(error) = closer.show(label) {
            restore_errors.push(error.to_string());
        }
    }
    if let Some(active_label) = active_label {
        if let Err(error) = closer.focus(active_label) {
            restore_errors.push(error.to_string());
        }
    }
    Err(if restore_errors.is_empty() {
        primary
    } else {
        OverlayError::CleanupFailures {
            primary: primary.to_string(),
            cleanup: restore_errors,
        }
    })
}

struct TauriOverlayWindowCloser<'a, R: Runtime> {
    app: &'a AppHandle<R>,
}

impl<R: Runtime> OverlayWindowCloser for TauriOverlayWindowCloser<'_, R> {
    fn hide(&mut self, label: &str) -> Result<(), OverlayError> {
        match self.app.get_webview_window(label) {
            Some(window) => window
                .hide()
                .map_err(|error| OverlayError::Window(error.to_string())),
            None => Ok(()),
        }
    }

    fn show(&mut self, label: &str) -> Result<(), OverlayError> {
        match self.app.get_webview_window(label) {
            Some(window) => window
                .show()
                .map_err(|error| OverlayError::Window(error.to_string())),
            None => Ok(()),
        }
    }

    fn focus(&mut self, label: &str) -> Result<(), OverlayError> {
        match self.app.get_webview_window(label) {
            Some(window) => window
                .set_focus()
                .map_err(|error| OverlayError::Window(error.to_string())),
            None => Ok(()),
        }
    }

    fn is_visible(&self, label: &str) -> bool {
        self.app
            .get_webview_window(label)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
    }

    fn close_best_effort(&mut self, label: &str) {
        if let Some(window) = self.app.get_webview_window(label) {
            let _ = window.close();
        }
    }

    fn wait_for_state_change(&mut self) {
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Requests closure and polls the native window registry until every overlay
/// is actually gone, so no overlay pixels can leak into the capture.
pub fn close_overlays<R: Runtime>(
    app: &AppHandle<R>,
    labels: &[String],
    active_label: Option<&str>,
) -> Result<(), OverlayError> {
    close_overlay_windows(&mut TauriOverlayWindowCloser { app }, labels, active_label)
}
