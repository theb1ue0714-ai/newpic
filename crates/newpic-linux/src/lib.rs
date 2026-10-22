//! Linux integration for NewPic.

use newpic_platform::{
    capabilities::{CapabilityStatus, CaptureBackend, CaptureCapabilities},
    capture::{CaptureError, CaptureExecution, CaptureProvider, CaptureRequest, CapturedImage},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxSession {
    X11,
    Wayland,
}

impl LinuxSession {
    pub fn from_environment(session_type: Option<&str>, wayland_display: Option<&str>) -> Self {
        match session_type
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("wayland") => Self::Wayland,
            Some("x11") => Self::X11,
            _ if wayland_display.is_some_and(|value| !value.trim().is_empty()) => Self::Wayland,
            _ => Self::X11,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinuxCaptureProvider {
    session: LinuxSession,
}

impl LinuxCaptureProvider {
    pub fn detect() -> Self {
        Self::for_session(LinuxSession::from_environment(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        ))
    }

    pub const fn for_session(session: LinuxSession) -> Self {
        Self { session }
    }

    pub const fn session(&self) -> LinuxSession {
        self.session
    }
}

impl Default for LinuxCaptureProvider {
    fn default() -> Self {
        Self::detect()
    }
}

impl CaptureProvider for LinuxCaptureProvider {
    fn capabilities(&self) -> CaptureCapabilities {
        match self.session {
            LinuxSession::X11 => CaptureCapabilities::new(CaptureBackend::X11)
                .with_region_capture(CapabilityStatus::supported())
                .with_active_window_capture(CapabilityStatus::limited(
                    "content-only bounds are unavailable; NewPic captures the whole window",
                    None::<String>,
                ))
                .with_display_capture(CapabilityStatus::supported())
                .with_unattended_capture(CapabilityStatus::supported())
                .with_pointer_capture(CapabilityStatus::unsupported(
                    "xcap 0.9.8 does not expose pointer inclusion for still captures",
                    Some("capture without the pointer"),
                )),
            LinuxSession::Wayland => CaptureCapabilities::new(CaptureBackend::XdgDesktopPortal)
                .with_region_capture(CapabilityStatus::limited(
                    "the compositor owns area selection; an exact preselected rectangle or unattended repeat cannot be guaranteed",
                    Some("select the requested area in the portal prompt"),
                ))
                .with_active_window_capture(CapabilityStatus::unsupported(
                    "Wayland does not expose a reliable focused-window capture through this backend",
                    Some("use region capture instead"),
                ))
                .with_display_capture(CapabilityStatus::unsupported(
                    "the Screenshot portal does not identify a requested physical display in its response",
                    Some("use compositor-owned area selection instead"),
                ))
                .with_unattended_capture(CapabilityStatus::limited(
                    "Wayland capture may require user consent",
                    None::<String>,
                ))
                .with_pointer_capture(CapabilityStatus::limited(
                    "pointer inclusion is controlled by the compositor and cannot be guaranteed",
                    None::<String>,
                )),
        }
    }

    fn displays(&self) -> Result<Vec<newpic_platform::geometry::Display>, CaptureError> {
        match self.session {
            LinuxSession::X11 => platform::displays_x11(),
            LinuxSession::Wayland => Err(CaptureError::PortalOwnedSelection {
                operation: "display enumeration",
                reason: "the Screenshot portal owns source selection and does not return reusable display identities"
                    .into(),
            }),
        }
    }

    fn capture(&self, request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        if self.session == LinuxSession::X11 && request.options().include_pointer() {
            return Err(CaptureError::Unsupported {
                operation: "pointer capture",
                reason: self
                    .capabilities()
                    .pointer_capture()
                    .reason()
                    .unwrap_or("pointer capture is unavailable")
                    .to_owned(),
            });
        }
        if self.session == LinuxSession::Wayland
            && matches!(
                request.source(),
                newpic_platform::capture::CaptureSource::ActiveWindow
            )
        {
            return Err(CaptureError::Unsupported {
                operation: "active-window capture",
                reason:
                    "Wayland does not expose a reliable focused-window capture through this backend"
                        .into(),
            });
        }
        if self.session == LinuxSession::Wayland
            && matches!(
                request.source(),
                newpic_platform::capture::CaptureSource::Display(_)
            )
        {
            return Err(CaptureError::Unsupported {
                operation: "specific-display capture",
                reason: "the XDG Screenshot portal does not identify a requested display".into(),
            });
        }
        let backend = self.capabilities().backend();
        let portal_fallback = (self.session == LinuxSession::Wayland).then_some(
            "the compositor selected the actual area; the requested rectangle was not applied",
        );
        match self.session {
            LinuxSession::X11 => platform::capture_x11(request),
            LinuxSession::Wayland => platform::capture_wayland(request),
        }
        .map(|image| image.with_execution(CaptureExecution::new(backend, portal_fallback)))
    }
}

pub fn classify_xcap_error(message: &str) -> CaptureError {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("cancel")
        || normalized.contains("canceled")
        || normalized.contains("cancelled")
    {
        CaptureError::UserCancelled
    } else if normalized.contains("permission") || normalized.contains("denied") {
        CaptureError::PermissionDenied {
            settings_hint: Some(
                "allow screenshots in the desktop portal or compositor settings".into(),
            ),
        }
    } else {
        CaptureError::Backend(message.to_owned())
    }
}

/// Maps an XDG Desktop Portal response without falling through to a less
/// restrictive capture mechanism after cancellation or denial.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowFacts {
    pid: u32,
    focused: bool,
    minimized: bool,
}

impl WindowFacts {
    const fn new(pid: u32, focused: bool, minimized: bool) -> Self {
        Self {
            pid,
            focused,
            minimized,
        }
    }
}

fn select_active_window(windows: &[WindowFacts], current_pid: u32) -> Option<usize> {
    windows
        .iter()
        .position(|window| window.focused && !window.minimized && window.pid != current_pid)
}

fn orientation_from_rotation(
    rotation: f32,
    width: u32,
    height: u32,
) -> newpic_platform::geometry::DisplayOrientation {
    use newpic_platform::geometry::DisplayOrientation;
    let normalized = rotation.round().rem_euclid(360.0) as u16;
    match normalized {
        90 => DisplayOrientation::Portrait,
        180 => DisplayOrientation::LandscapeFlipped,
        270 => DisplayOrientation::PortraitFlipped,
        _ if height > width => DisplayOrientation::Portrait,
        _ => DisplayOrientation::Landscape,
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::collections::HashSet;

    use ashpd::{
        desktop::{
            screenshot::{AvailableTargets, Screenshot},
            ResponseError,
        },
        PortalError,
    };
    use newpic_platform::{
        capture::{
            capture_scale_metadata_for_region, normalize_native_rgba8, CaptureError,
            CaptureFallback, CaptureProvenance, CaptureRequest, CaptureScaleMetadata,
            CaptureSource, CapturedImage, NativePixelFormat,
        },
        geometry::{Display, DisplayId, PhysicalRect, ScaleFactor},
    };
    use xcap::{Monitor, Window};

    use super::{
        classify_xcap_error, orientation_from_rotation, select_active_window, WindowFacts,
    };

    fn backend_error(error: impl std::fmt::Display) -> CaptureError {
        classify_xcap_error(&error.to_string())
    }

    pub(super) fn portal_error(error: ashpd::Error) -> CaptureError {
        match error {
            ashpd::Error::Response(ResponseError::Cancelled)
            | ashpd::Error::Portal(PortalError::Cancelled(_)) => CaptureError::UserCancelled,
            ashpd::Error::Portal(PortalError::NotAllowed(_)) => CaptureError::PermissionDenied {
                settings_hint: Some(
                    "allow screenshots in the desktop portal or compositor settings".into(),
                ),
            },
            opaque => CaptureError::Backend(opaque.to_string()),
        }
    }

    fn scaled_i32(value: i32, scale: f64) -> Result<i32, CaptureError> {
        let scaled = (f64::from(value) * scale).round();
        if scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
            return Err(CaptureError::Backend(
                "display coordinate is out of range".into(),
            ));
        }
        Ok(scaled as i32)
    }

    fn scaled_u32(value: u32, scale: f64) -> Result<u32, CaptureError> {
        let scaled = (f64::from(value) * scale).round();
        if !(1.0..=f64::from(u32::MAX)).contains(&scaled) {
            return Err(CaptureError::Backend("display size is out of range".into()));
        }
        Ok(scaled as u32)
    }

    fn display_from_monitor(monitor: &Monitor) -> Result<Display, CaptureError> {
        let id = monitor.id().map_err(backend_error)?;
        let name = monitor.name().map_err(backend_error)?;
        let scale = f64::from(monitor.scale_factor().map_err(backend_error)?);
        let scale_factor = ScaleFactor::new(scale)?;
        let width = scaled_u32(monitor.width().map_err(backend_error)?, scale)?;
        let height = scaled_u32(monitor.height().map_err(backend_error)?, scale)?;
        let bounds = PhysicalRect::new(
            scaled_i32(monitor.x().map_err(backend_error)?, scale)?,
            scaled_i32(monitor.y().map_err(backend_error)?, scale)?,
            width,
            height,
        )?;
        Ok(Display::new(
            DisplayId::new(format!("linux:{id}:{name}"))?,
            bounds,
            scale_factor,
            monitor.is_primary().map_err(backend_error)?,
            orientation_from_rotation(monitor.rotation().map_err(backend_error)?, width, height),
        ))
    }

    fn native_displays() -> Result<Vec<(Monitor, Display)>, CaptureError> {
        let monitors = Monitor::all().map_err(backend_error)?;
        let mut ids = HashSet::new();
        monitors
            .into_iter()
            .map(|monitor| {
                let display = display_from_monitor(&monitor)?;
                if !ids.insert(display.id().clone()) {
                    return Err(CaptureError::Backend(format!(
                        "capture backend returned duplicate display ID {:?}",
                        display.id()
                    )));
                }
                Ok((monitor, display))
            })
            .collect()
    }

    pub(super) fn displays_x11() -> Result<Vec<Display>, CaptureError> {
        native_displays().map(|items| items.into_iter().map(|(_, display)| display).collect())
    }

    fn normalized_image(
        image: xcap::image::RgbaImage,
    ) -> Result<(u32, u32, Vec<u8>), CaptureError> {
        let width = image.width();
        let height = image.height();
        let stride = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        let rgba = normalize_native_rgba8(
            width,
            height,
            stride,
            image.into_raw(),
            NativePixelFormat::Rgba8,
        )?;
        Ok((width, height, rgba))
    }

    fn display_capture(id: &DisplayId) -> Result<CapturedImage, CaptureError> {
        let items = native_displays()?;
        let (monitor, display) = items
            .iter()
            .find(|(_, display)| display.id() == id)
            .ok_or_else(|| CaptureError::DisplayNotFound(id.clone()))?;
        let (width, height, rgba) =
            normalized_image(monitor.capture_image().map_err(backend_error)?)?;
        CapturedImage::new(
            width,
            height,
            usize::try_from(width).unwrap_or(0) * 4,
            rgba,
            CaptureScaleMetadata::uniform(display.scale_factor()),
            CaptureProvenance::Display(id.clone()),
        )
    }

    fn region_capture(region: PhysicalRect) -> Result<CapturedImage, CaptureError> {
        let items = native_displays()?;
        let displays: Vec<_> = items.iter().map(|(_, display)| display.clone()).collect();
        let metadata = capture_scale_metadata_for_region(region, &displays)?;
        let width = region.size().width();
        let height = region.size().height();
        let row_stride = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        let output_len = row_stride
            .checked_mul(usize::try_from(height).map_err(|_| CaptureError::ImageLayoutOverflow)?)
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        let mut rgba = vec![0_u8; output_len];
        let mut written = vec![
            false;
            usize::try_from(width)
                .ok()
                .and_then(|width| width.checked_mul(usize::try_from(height).ok()?))
                .ok_or(CaptureError::ImageLayoutOverflow)?
        ];

        for (monitor, display) in &items {
            let Some(piece) = region.intersection(display.bounds()) else {
                continue;
            };
            let source_x = usize::try_from(piece.left() - display.bounds().left())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let source_y = usize::try_from(piece.top() - display.bounds().top())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let (monitor_width, monitor_height, monitor_rgba) =
                normalized_image(monitor.capture_image().map_err(backend_error)?)?;
            if (monitor_width, monitor_height)
                != (
                    display.bounds().size().width(),
                    display.bounds().size().height(),
                )
            {
                return Err(CaptureError::Backend(format!(
                    "backend returned {monitor_width}x{monitor_height} for a display reported as {}x{}",
                    display.bounds().size().width(),
                    display.bounds().size().height()
                )));
            }
            let destination_x = usize::try_from(piece.left() - region.left())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let destination_y = usize::try_from(piece.top() - region.top())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let monitor_width =
                usize::try_from(monitor_width).map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let piece_width = usize::try_from(piece.size().width())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let piece_height = usize::try_from(piece.size().height())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            for row in 0..piece_height {
                for column in 0..piece_width {
                    let destination_pixel = (destination_y + row)
                        * usize::try_from(width).unwrap_or(0)
                        + destination_x
                        + column;
                    if written[destination_pixel] {
                        continue;
                    }
                    let source_pixel = (source_y + row) * monitor_width + source_x + column;
                    rgba[destination_pixel * 4..destination_pixel * 4 + 4]
                        .copy_from_slice(&monitor_rgba[source_pixel * 4..source_pixel * 4 + 4]);
                    written[destination_pixel] = true;
                }
            }
        }
        CapturedImage::new(
            width,
            height,
            row_stride,
            rgba,
            metadata,
            CaptureProvenance::Region,
        )
    }

    fn active_window_capture() -> Result<CapturedImage, CaptureError> {
        let windows = Window::all().map_err(backend_error)?;
        let facts: Result<Vec<_>, CaptureError> = windows
            .iter()
            .map(|window| {
                Ok(WindowFacts::new(
                    window.pid().map_err(backend_error)?,
                    window.is_focused().map_err(backend_error)?,
                    window.is_minimized().map_err(backend_error)?,
                ))
            })
            .collect();
        let index = select_active_window(&facts?, std::process::id()).ok_or_else(|| {
            CaptureError::BackendUnavailable("no eligible focused non-minimized window".into())
        })?;
        let window = &windows[index];
        let monitor = window.current_monitor().map_err(backend_error)?;
        let scale = ScaleFactor::new(f64::from(monitor.scale_factor().map_err(backend_error)?))?;
        let (width, height, rgba) =
            normalized_image(window.capture_image().map_err(backend_error)?)?;
        CapturedImage::new(
            width,
            height,
            usize::try_from(width).unwrap_or(0) * 4,
            rgba,
            CaptureScaleMetadata::uniform(scale),
            CaptureProvenance::ActiveWindow {
                fallback: Some(CaptureFallback::WholeWindow),
            },
        )
    }

    pub(super) fn capture_x11(request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        match request.source() {
            CaptureSource::Region(region) => region_capture(*region),
            CaptureSource::ActiveWindow => active_window_capture(),
            CaptureSource::Display(id) => display_capture(id),
        }
    }

    pub(super) fn capture_wayland(request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        let requested_region = match request.source() {
            CaptureSource::Region(region) => *region,
            CaptureSource::Display(_) => {
                return Err(CaptureError::Unsupported {
                    operation: "specific-display capture",
                    reason: "the XDG Screenshot portal does not identify a requested display"
                        .into(),
                });
            }
            CaptureSource::ActiveWindow => {
                return Err(CaptureError::Unsupported {
                    operation: "active-window capture",
                    reason:
                        "the XDG Desktop Portal screenshot API does not expose the focused window"
                            .into(),
                });
            }
        };
        let response = futures_lite::future::block_on(async {
            Screenshot::request()
                .modal(true)
                .interactive(true)
                .target(AvailableTargets::Area)
                .send()
                .await
                .map_err(portal_error)?
                .response()
                .map_err(portal_error)
        })?;
        let uri = url::Url::parse(response.uri().as_str())
            .map_err(|error| CaptureError::Backend(format!("invalid portal URI: {error}")))?;
        let path = uri.to_file_path().map_err(|()| {
            CaptureError::Backend("portal returned a non-file screenshot URI".into())
        })?;
        struct PortalScreenshot(std::path::PathBuf);
        impl Drop for PortalScreenshot {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let screenshot = PortalScreenshot(path);
        let image = image::open(&screenshot.0)
            .map_err(|error| CaptureError::Backend(format!("cannot read portal image: {error}")))?
            .to_rgba8();
        let width = image.width();
        let height = image.height();
        let row_stride = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        CapturedImage::new(
            width,
            height,
            row_stride,
            image.into_raw(),
            CaptureScaleMetadata::unknown(),
            CaptureProvenance::PortalOwnedSelection {
                requested_region,
                reason: "the compositor selected the actual area; exact returned desktop coordinates are unknown"
                    .into(),
            },
        )
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use newpic_platform::{
        capture::{CaptureError, CaptureRequest, CapturedImage},
        geometry::Display,
    };

    pub(super) fn displays_x11() -> Result<Vec<Display>, CaptureError> {
        Err(CaptureError::BackendUnavailable(
            "the Linux capture provider requires a Linux target".into(),
        ))
    }

    pub(super) fn capture_x11(_request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        Err(CaptureError::BackendUnavailable(
            "the Linux capture provider requires a Linux target".into(),
        ))
    }

    pub(super) fn capture_wayland(_request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        Err(CaptureError::BackendUnavailable(
            "the Linux capture provider requires a Linux target".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newpic_platform::geometry::DisplayOrientation;

    #[test]
    fn session_detection_prefers_explicit_wayland_session() {
        assert_eq!(
            LinuxSession::from_environment(Some("wayland"), Some("wayland-0")),
            LinuxSession::Wayland
        );
        assert_eq!(
            LinuxSession::from_environment(Some("x11"), Some("wayland-0")),
            LinuxSession::X11
        );
    }

    #[test]
    fn rotation_is_preserved_in_display_orientation() {
        assert_eq!(
            orientation_from_rotation(0.0, 1920, 1080),
            DisplayOrientation::Landscape
        );
        assert_eq!(
            orientation_from_rotation(90.0, 1080, 1920),
            DisplayOrientation::Portrait
        );
        assert_eq!(
            orientation_from_rotation(180.0, 1920, 1080),
            DisplayOrientation::LandscapeFlipped
        );
        assert_eq!(
            orientation_from_rotation(270.0, 1080, 1920),
            DisplayOrientation::PortraitFlipped
        );
    }

    #[test]
    fn active_window_requires_focus_and_excludes_newpic() {
        let windows = [
            WindowFacts::new(1, true, false),
            WindowFacts::new(2, false, false),
            WindowFacts::new(3, true, false),
        ];
        assert_eq!(select_active_window(&windows, 1), Some(2));
        assert_eq!(
            select_active_window(&[WindowFacts::new(9, true, true)], 99),
            None
        );
    }

    #[test]
    fn typed_portal_cancellation_and_denial_are_preserved() {
        assert_eq!(
            platform::portal_error(ashpd::Error::Response(
                ashpd::desktop::ResponseError::Cancelled,
            )),
            CaptureError::UserCancelled
        );
        assert_eq!(
            platform::portal_error(ashpd::Error::Portal(ashpd::PortalError::Cancelled(
                "cancelled".into(),
            ))),
            CaptureError::UserCancelled
        );
        assert!(matches!(
            platform::portal_error(ashpd::Error::Portal(ashpd::PortalError::NotAllowed(
                "denied".into(),
            ))),
            CaptureError::PermissionDenied { .. }
        ));
    }
}
