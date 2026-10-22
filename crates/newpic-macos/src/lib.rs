//! macOS ScreenCaptureKit screenshot adapter.

use newpic_platform::{
    capabilities::{CapabilityStatus, CaptureBackend, CaptureCapabilities},
    capture::{CaptureError, CaptureExecution, CaptureProvider, CaptureRequest, CapturedImage},
};
use std::sync::Arc;

const SCREEN_RECORDING_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

#[derive(Debug, Clone, Default)]
pub struct MacOsCaptureProvider {
    excluded_window_ids: Arc<[u32]>,
}
impl MacOsCaptureProvider {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_excluded_window_ids(mut self, ids: impl IntoIterator<Item = u32>) -> Self {
        self.excluded_window_ids = ids.into_iter().collect::<Vec<_>>().into();
        self
    }
}

impl CaptureProvider for MacOsCaptureProvider {
    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities::new(CaptureBackend::MacOsScreenCaptureKit)
            .with_region_capture(CapabilityStatus::supported())
            .with_active_window_capture(CapabilityStatus::limited(
                "ScreenCaptureKit returns the complete independent window, not content-only bounds",
                None::<String>,
            ))
            .with_display_capture(CapabilityStatus::supported())
            .with_unattended_capture(CapabilityStatus::limited(
                "ScreenCaptureKit requires Screen Recording permission",
                Some("grant access in Privacy & Security > Screen Recording"),
            ))
            .with_pointer_capture(CapabilityStatus::supported())
    }
    fn displays(&self) -> Result<Vec<newpic_platform::geometry::Display>, CaptureError> {
        platform::displays()
    }
    fn capture(&self, request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        platform::capture(request, &self.excluded_window_ids).map(|image| {
            image.with_execution(CaptureExecution::new(
                CaptureBackend::MacOsScreenCaptureKit,
                None::<String>,
            ))
        })
    }
}

pub fn classify_screen_capture_error(message: &str) -> CaptureError {
    let value = message.to_ascii_lowercase();
    let access = value.contains("permission")
        || value.contains("not granted")
        || value.contains("not authorized")
        || value.contains("denied")
        || value.contains("declined");
    if access
        && (value.contains("screen")
            || value.contains("capture")
            || value.contains("shareable content"))
    {
        CaptureError::PermissionDenied {
            settings_hint: Some(SCREEN_RECORDING_SETTINGS.into()),
        }
    } else {
        CaptureError::Backend(message.into())
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowFacts {
    window_id: u32,
    pid: u32,
    active: bool,
    on_screen: bool,
    layer: i32,
}

#[cfg(any(target_os = "macos", test))]
fn select_active_window(windows: &[WindowFacts], own_pid: u32, excluded: &[u32]) -> Option<usize> {
    windows.iter().position(|w| {
        w.active
            && w.on_screen
            && w.layer == 0
            && w.pid != own_pid
            && !excluded.contains(&w.window_id)
    })
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{classify_screen_capture_error, select_active_window, WindowFacts};
    use newpic_platform::{
        capture::{
            capture_scale_metadata_for_region, normalize_native_rgba8, CaptureError,
            CaptureFallback, CaptureProvenance, CaptureRequest, CaptureScaleMetadata,
            CaptureSource, CapturedImage, NativePixelFormat,
        },
        geometry::{Display, DisplayId, DisplayOrientation, PhysicalRect, ScaleFactor},
    };
    use screencapturekit::{
        cg::CGRect,
        cm::{CMSampleBufferExt, CMSampleBufferSCExt},
        cv::CVPixelBufferLockFlags,
        prelude::*,
    };
    use std::{collections::HashSet, sync::mpsc, time::Duration};

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayRotation(display: u32) -> f64;
    }

    struct Frame {
        width: u32,
        height: u32,
        stride: usize,
        bytes: Vec<u8>,
    }
    fn error(e: impl std::fmt::Display) -> CaptureError {
        classify_screen_capture_error(&e.to_string())
    }
    fn stride(width: u32) -> Result<usize, CaptureError> {
        (width as usize)
            .checked_mul(4)
            .ok_or(CaptureError::ImageLayoutOverflow)
    }
    fn i32_coord(v: f64) -> Result<i32, CaptureError> {
        let v = v.round();
        if !v.is_finite() || v < i32::MIN as f64 || v > i32::MAX as f64 {
            Err(CaptureError::Backend(
                "display coordinate is out of range".into(),
            ))
        } else {
            Ok(v as i32)
        }
    }
    fn orientation(id: u32, width: u32, height: u32) -> DisplayOrientation {
        match (unsafe { CGDisplayRotation(id) }).round().rem_euclid(360.0) as u16 {
            90 => DisplayOrientation::Portrait,
            180 => DisplayOrientation::LandscapeFlipped,
            270 => DisplayOrientation::PortraitFlipped,
            _ if height > width => DisplayOrientation::Portrait,
            _ => DisplayOrientation::Landscape,
        }
    }
    fn display(native: &SCDisplay) -> Result<Display, CaptureError> {
        let frame = native.frame();
        if frame.size.width <= 0.0 || frame.size.height <= 0.0 {
            return Err(CaptureError::Backend(
                "ScreenCaptureKit returned an empty display".into(),
            ));
        }
        let (width, height) = (native.width(), native.height());
        let sx = width as f64 / frame.size.width;
        let sy = height as f64 / frame.size.height;
        if (sx - sy).abs() > 0.01 {
            return Err(CaptureError::Backend("inconsistent display scale".into()));
        }
        let scale = ScaleFactor::new((sx + sy) / 2.0)?;
        let id = native.display_id();
        Ok(Display::new(
            DisplayId::new(format!("macos:{id}"))?,
            PhysicalRect::new(
                i32_coord(frame.origin.x * scale.get())?,
                i32_coord(frame.origin.y * scale.get())?,
                width,
                height,
            )?,
            scale,
            id == unsafe { CGMainDisplayID() },
            orientation(id, width, height),
        ))
    }
    fn content() -> Result<SCShareableContent, CaptureError> {
        SCShareableContent::create()
            .with_exclude_desktop_windows(true)
            .with_on_screen_windows_only(false)
            .get()
            .map_err(error)
    }
    fn native_displays() -> Result<Vec<(SCDisplay, Display)>, CaptureError> {
        let mut ids = HashSet::new();
        content()?
            .displays()
            .into_iter()
            .map(|n| {
                let d = display(&n)?;
                if !ids.insert(d.id().clone()) {
                    return Err(CaptureError::Backend("duplicate display ID".into()));
                }
                Ok((n, d))
            })
            .collect()
    }
    pub(super) fn displays() -> Result<Vec<Display>, CaptureError> {
        native_displays().map(|v| v.into_iter().map(|(_, d)| d).collect())
    }
    fn filter(display: &SCDisplay) -> Result<SCContentFilter, CaptureError> {
        SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build()
            .map_err(error)
    }
    fn frame(
        filter: &SCContentFilter,
        width: u32,
        height: u32,
        rect: Option<CGRect>,
        cursor: bool,
    ) -> Result<Frame, CaptureError> {
        let mut config = SCStreamConfiguration::new()
            .with_width(width as usize)
            .with_height(height as usize)
            .with_pixel_format(PixelFormat::BGRA)
            .with_shows_cursor(cursor);
        if let Some(rect) = rect {
            config = config.with_source_rect(rect);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let mut stream = SCStream::new(filter, &config).map_err(error)?;
        stream
            .add_output_handler(
                move |sample, kind| {
                    if kind != SCStreamOutputType::Screen
                        || !sample.frame_status().is_some_and(|s| s.has_content())
                    {
                        return;
                    }
                    let result = (|| {
                        let pixels = sample.pixel_buffer().ok_or("frame has no pixel buffer")?;
                        let guard = pixels
                            .lock(CVPixelBufferLockFlags::READ_ONLY)
                            .map_err(|_| "pixel buffer lock failed")?;
                        let bytes =
                            unsafe { guard.as_slice() }.ok_or("pixel buffer is not contiguous")?;
                        Ok(Frame {
                            width: guard.width() as u32,
                            height: guard.height() as u32,
                            stride: guard.bytes_per_row(),
                            bytes: bytes.to_vec(),
                        })
                    })()
                    .map_err(str::to_owned);
                    let _ = tx.try_send(result);
                },
                SCStreamOutputType::Screen,
            )
            .map_err(error)?;
        stream.start_capture().map_err(error)?;
        let received = rx.recv_timeout(Duration::from_secs(5));
        let stopped = stream.stop_capture().map_err(error);
        let frame = received
            .map_err(|_| CaptureError::Backend("ScreenCaptureKit frame timed out".into()))?
            .map_err(CaptureError::Backend)?;
        stopped?;
        Ok(frame)
    }
    fn rgba(f: Frame) -> Result<(u32, u32, Vec<u8>), CaptureError> {
        let bytes = normalize_native_rgba8(
            f.width,
            f.height,
            f.stride,
            f.bytes,
            NativePixelFormat::Bgra8,
        )?;
        Ok((f.width, f.height, bytes))
    }
    fn display_capture(id: &DisplayId, cursor: bool) -> Result<CapturedImage, CaptureError> {
        let items = native_displays()?;
        let (native, d) = items
            .iter()
            .find(|(_, d)| d.id() == id)
            .ok_or_else(|| CaptureError::DisplayNotFound(id.clone()))?;
        let (w, h, pixels) = rgba(frame(
            &filter(native)?,
            d.bounds().size().width(),
            d.bounds().size().height(),
            None,
            cursor,
        )?)?;
        CapturedImage::new(
            w,
            h,
            stride(w)?,
            pixels,
            CaptureScaleMetadata::uniform(d.scale_factor()),
            CaptureProvenance::Display(id.clone()),
        )
    }
    fn region_capture(region: PhysicalRect, cursor: bool) -> Result<CapturedImage, CaptureError> {
        let items = native_displays()?;
        let displays: Vec<_> = items.iter().map(|(_, d)| d.clone()).collect();
        let metadata = capture_scale_metadata_for_region(region, &displays)?;
        let (width, height) = (region.size().width(), region.size().height());
        let row_stride = stride(width)?;
        let mut output = vec![
            0;
            row_stride
                .checked_mul(height as usize)
                .ok_or(CaptureError::ImageLayoutOverflow)?
        ];
        for (native, d) in &items {
            let Some(piece) = region.intersection(d.bounds()) else {
                continue;
            };
            let scale = d.scale_factor().get();
            let source = CGRect::new(
                (piece.left() - d.bounds().left()) as f64 / scale,
                (piece.top() - d.bounds().top()) as f64 / scale,
                piece.size().width() as f64 / scale,
                piece.size().height() as f64 / scale,
            );
            let (w, h, pixels) = rgba(frame(
                &filter(native)?,
                piece.size().width(),
                piece.size().height(),
                Some(source),
                cursor,
            )?)?;
            if (w, h) != (piece.size().width(), piece.size().height()) {
                return Err(CaptureError::Backend("region dimensions differ".into()));
            }
            let dx = (piece.left() - region.left()) as usize;
            let dy = (piece.top() - region.top()) as usize;
            let src_stride = stride(w)?;
            for y in 0..h as usize {
                let src = y * src_stride;
                let dst = (dy + y) * row_stride + dx * 4;
                output[dst..dst + src_stride].copy_from_slice(&pixels[src..src + src_stride]);
            }
        }
        CapturedImage::new(
            width,
            height,
            row_stride,
            output,
            metadata,
            CaptureProvenance::Region,
        )
    }
    fn active_capture(excluded: &[u32], cursor: bool) -> Result<CapturedImage, CaptureError> {
        let content = content()?;
        let snap = content
            .snapshot()
            .ok_or_else(|| CaptureError::Backend("shareable content snapshot failed".into()))?;
        if snap.truncation.windows || snap.truncation.applications {
            return Err(CaptureError::Backend("window snapshot truncated".into()));
        }
        let facts: Vec<_> = snap
            .windows
            .iter()
            .map(|w| WindowFacts {
                window_id: w.window_id,
                pid: w
                    .owning_app_index
                    .and_then(|i| snap.applications.get(i))
                    .map_or(0, |a| a.process_id.max(0) as u32),
                active: w.is_active,
                on_screen: w.is_on_screen,
                layer: w.window_layer,
            })
            .collect();
        let index =
            select_active_window(&facts, std::process::id(), excluded).ok_or_else(|| {
                CaptureError::BackendUnavailable("no eligible active on-screen window".into())
            })?;
        let selected = &snap.windows[index];
        let windows = content.windows();
        let window = windows
            .iter()
            .find(|w| w.window_id() == selected.window_id)
            .ok_or_else(|| CaptureError::Backend("active window disappeared".into()))?;
        let display = content
            .displays()
            .into_iter()
            .max_by(|a, b| {
                intersection_area(window.frame(), a.frame())
                    .total_cmp(&intersection_area(window.frame(), b.frame()))
            })
            .ok_or_else(|| CaptureError::Backend("no display for active window".into()))?;
        let scale = ScaleFactor::new(display.width() as f64 / display.frame().size.width)?;
        let width = (window.frame().size.width * scale.get()).round() as u32;
        let height = (window.frame().size.height * scale.get()).round() as u32;
        let filter = SCContentFilter::create()
            .with_window(window)
            .build()
            .map_err(error)?;
        let (w, h, pixels) = rgba(frame(&filter, width, height, None, cursor)?)?;
        CapturedImage::new(
            w,
            h,
            stride(w)?,
            pixels,
            CaptureScaleMetadata::uniform(scale),
            CaptureProvenance::ActiveWindow {
                fallback: Some(CaptureFallback::WholeWindow),
            },
        )
    }
    fn intersection_area(a: CGRect, b: CGRect) -> f64 {
        let w =
            (a.origin.x + a.size.width).min(b.origin.x + b.size.width) - a.origin.x.max(b.origin.x);
        let h = (a.origin.y + a.size.height).min(b.origin.y + b.size.height)
            - a.origin.y.max(b.origin.y);
        w.max(0.0) * h.max(0.0)
    }
    pub(super) fn capture(
        request: CaptureRequest,
        excluded: &[u32],
    ) -> Result<CapturedImage, CaptureError> {
        let cursor = request.options().include_pointer();
        match request.source() {
            CaptureSource::Region(r) => region_capture(*r, cursor),
            CaptureSource::ActiveWindow => active_capture(excluded, cursor),
            CaptureSource::Display(id) => display_capture(id, cursor),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use newpic_platform::{
        capture::{CaptureError, CaptureRequest, CapturedImage},
        geometry::Display,
    };
    pub(super) fn displays() -> Result<Vec<Display>, CaptureError> {
        Err(CaptureError::BackendUnavailable(
            "the macOS provider requires macOS".into(),
        ))
    }
    pub(super) fn capture(_: CaptureRequest, _: &[u32]) -> Result<CapturedImage, CaptureError> {
        Err(CaptureError::BackendUnavailable(
            "the macOS provider requires macOS".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn w(id: u32, pid: u32, active: bool, on_screen: bool) -> WindowFacts {
        WindowFacts {
            window_id: id,
            pid,
            active,
            on_screen,
            layer: 0,
        }
    }
    #[test]
    fn selection_excludes_pid_and_exact_window_id() {
        assert_eq!(
            select_active_window(
                &[
                    w(1, 7, true, true),
                    w(2, 8, true, true),
                    w(3, 9, true, true)
                ],
                7,
                &[2]
            ),
            Some(2)
        );
    }
    #[test]
    fn selection_requires_active_visible_normal_window() {
        let mut top = w(3, 9, true, true);
        top.layer = 1;
        assert_eq!(
            select_active_window(&[w(1, 7, false, true), w(2, 8, true, false), top], 99, &[]),
            None
        );
    }
    #[test]
    fn ordinary_error_is_not_permission_error() {
        assert!(matches!(
            classify_screen_capture_error("transport failed"),
            CaptureError::Backend(_)
        ));
    }
}
