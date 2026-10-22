//! Windows screenshot adapter for NewPic.

use newpic_platform::{
    capabilities::{CapabilityStatus, CaptureBackend, CaptureCapabilities},
    capture::{CaptureError, CaptureExecution, CaptureProvider, CaptureRequest, CapturedImage},
};

/// Why a Windows capture used the secondary GDI backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsBackendFallback {
    WgcFailed { reason: String },
}

/// A Windows capture plus the native backend that actually produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowsCaptureResult {
    image: CapturedImage,
    backend: CaptureBackend,
    fallback: Option<WindowsBackendFallback>,
}

impl WindowsCaptureResult {
    pub const fn image(&self) -> &CapturedImage {
        &self.image
    }

    pub const fn backend(&self) -> CaptureBackend {
        self.backend
    }

    pub const fn fallback(&self) -> Option<&WindowsBackendFallback> {
        self.fallback.as_ref()
    }

    pub fn into_image(self) -> CapturedImage {
        self.image
    }
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendCapture<T> {
    value: T,
    backend: CaptureBackend,
    fallback: Option<WindowsBackendFallback>,
}

#[cfg(any(target_os = "windows", test))]
fn capture_with_runtime_fallback<T>(
    wgc: impl FnOnce() -> Result<T, CaptureError>,
    gdi: impl FnOnce() -> Result<T, CaptureError>,
) -> Result<BackendCapture<T>, CaptureError> {
    match wgc() {
        Ok(value) => Ok(BackendCapture {
            value,
            backend: CaptureBackend::WindowsGraphicsCapture,
            fallback: None,
        }),
        Err(wgc_error) => {
            let reason = wgc_error.to_string();
            match gdi() {
                Ok(value) => Ok(BackendCapture {
                    value,
                    backend: CaptureBackend::WindowsGdi,
                    fallback: Some(WindowsBackendFallback::WgcFailed { reason }),
                }),
                Err(gdi_error) => Err(CaptureError::Backend(format!(
                    "Windows Graphics Capture failed ({wgc_error}); GDI fallback failed ({gdi_error})"
                ))),
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WindowsCaptureProvider;

impl WindowsCaptureProvider {
    /// Creates the normal provider. Windows builds prefer `xcap/wgc` and
    /// retry failed captures through the Win32 GDI adapter.
    pub const fn new() -> Self {
        Self
    }

    /// Captures while retaining the backend and runtime-fallback provenance.
    pub fn capture_detailed(
        &self,
        request: CaptureRequest,
    ) -> Result<WindowsCaptureResult, CaptureError> {
        reject_pointer_request(&request, &self.capabilities())?;
        platform::capture_detailed(request).map(|mut result| {
            let reason = result.fallback.as_ref().map(|fallback| match fallback {
                WindowsBackendFallback::WgcFailed { reason } => reason.clone(),
            });
            result.image = result
                .image
                .with_execution(CaptureExecution::new(result.backend, reason));
            result
        })
    }
}

impl CaptureProvider for WindowsCaptureProvider {
    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities::new(CaptureBackend::WindowsGraphicsCapture)
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
            ))
    }

    fn displays(&self) -> Result<Vec<newpic_platform::geometry::Display>, CaptureError> {
        platform::displays()
    }

    fn capture(&self, request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        self.capture_detailed(request)
            .map(WindowsCaptureResult::into_image)
    }
}

fn reject_pointer_request(
    request: &CaptureRequest,
    capabilities: &CaptureCapabilities,
) -> Result<(), CaptureError> {
    if request.options().include_pointer() {
        return Err(CaptureError::Unsupported {
            operation: "pointer capture",
            reason: capabilities
                .pointer_capture()
                .reason()
                .unwrap_or("pointer capture is unavailable")
                .to_owned(),
        });
    }
    Ok(())
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowFacts {
    id: u64,
    pid: u32,
    focused: bool,
    minimized: bool,
}

#[cfg(any(target_os = "windows", test))]
impl WindowFacts {
    const fn new(id: u64, pid: u32, focused: bool, minimized: bool) -> Self {
        Self {
            id,
            pid,
            focused,
            minimized,
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn select_active_window(
    windows: &[WindowFacts],
    current_pid: u32,
    excluded_window_ids: &[u64],
) -> Option<usize> {
    windows.iter().position(|window| {
        window.focused
            && !window.minimized
            && window.pid != current_pid
            && !excluded_window_ids.contains(&window.id)
    })
}

#[cfg(any(target_os = "windows", test))]
fn orientation_from_rotation(
    rotation: f32,
    width: u32,
    height: u32,
) -> newpic_platform::geometry::DisplayOrientation {
    use newpic_platform::geometry::DisplayOrientation;
    match rotation.round().rem_euclid(360.0) as u16 {
        90 => DisplayOrientation::Portrait,
        180 => DisplayOrientation::LandscapeFlipped,
        270 => DisplayOrientation::PortraitFlipped,
        _ if height > width => DisplayOrientation::Portrait,
        _ => DisplayOrientation::Landscape,
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::{collections::HashSet, ffi::c_void, mem};

    use newpic_platform::{
        capture::{
            capture_scale_metadata_for_region, normalize_native_rgba8, CaptureError,
            CaptureFallback, CaptureProvenance, CaptureRequest, CaptureScaleMetadata,
            CaptureSource, CapturedImage, NativePixelFormat,
        },
        geometry::{Display, DisplayId, PhysicalRect, ScaleFactor},
    };
    use windows::Win32::{
        Foundation::{HWND, RECT},
        Graphics::Gdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
            GetWindowDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
            HBITMAP, HDC, HGDIOBJ, SRCCOPY,
        },
        Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS},
        UI::WindowsAndMessaging::{GetDesktopWindow, GetForegroundWindow, GetWindowRect},
    };
    use xcap::{Monitor, Window};

    use super::{
        capture_with_runtime_fallback, orientation_from_rotation, select_active_window,
        WindowFacts, WindowsCaptureResult,
    };

    fn backend_error(error: impl std::fmt::Display) -> CaptureError {
        CaptureError::Backend(error.to_string())
    }

    fn checked_stride(width: u32) -> Result<usize, CaptureError> {
        usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(CaptureError::ImageLayoutOverflow)
    }

    struct WindowDc {
        window: HWND,
        dc: HDC,
    }

    impl WindowDc {
        fn new(window: HWND) -> Result<Self, CaptureError> {
            let dc = unsafe { GetWindowDC(Some(window)) };
            if dc.0.is_null() {
                return Err(CaptureError::Backend("GDI GetWindowDC failed".into()));
            }
            Ok(Self { window, dc })
        }
    }

    impl Drop for WindowDc {
        fn drop(&mut self) {
            unsafe {
                ReleaseDC(Some(self.window), self.dc);
            }
        }
    }

    struct MemoryDc(HDC);

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            unsafe {
                DeleteDC(self.0);
            }
        }
    }

    struct OwnedBitmap(HBITMAP);

    impl Drop for OwnedBitmap {
        fn drop(&mut self) {
            unsafe {
                DeleteObject(self.0.into());
            }
        }
    }

    struct SelectedObject {
        dc: HDC,
        previous: HGDIOBJ,
    }

    impl Drop for SelectedObject {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.previous);
            }
        }
    }

    fn gdi_surface(
        source_window: HWND,
        source_x: i32,
        source_y: i32,
        width: u32,
        height: u32,
        print_window: bool,
    ) -> Result<(u32, u32, Vec<u8>), CaptureError> {
        let width_i32 = i32::try_from(width).map_err(|_| CaptureError::ImageLayoutOverflow)?;
        let height_i32 = i32::try_from(height).map_err(|_| CaptureError::ImageLayoutOverflow)?;
        let source = WindowDc::new(source_window)?;
        let memory = MemoryDc(unsafe { CreateCompatibleDC(Some(source.dc)) });
        if memory.0 .0.is_null() {
            return Err(CaptureError::Backend(
                "GDI CreateCompatibleDC failed".into(),
            ));
        }
        let bitmap =
            OwnedBitmap(unsafe { CreateCompatibleBitmap(source.dc, width_i32, height_i32) });
        if bitmap.0 .0.is_null() {
            return Err(CaptureError::Backend(
                "GDI CreateCompatibleBitmap failed".into(),
            ));
        }
        let _selection = SelectedObject {
            dc: memory.0,
            previous: unsafe { SelectObject(memory.0, bitmap.0.into()) },
        };

        let copied = if print_window {
            unsafe { PrintWindow(source_window, memory.0, PRINT_WINDOW_FLAGS(2)).as_bool() }
            || unsafe {
                BitBlt(
                    memory.0,
                    0,
                    0,
                    width_i32,
                    height_i32,
                    Some(source.dc),
                    source_x,
                    source_y,
                    SRCCOPY,
                )
                .is_ok()
            }
        } else {
            unsafe {
                BitBlt(
                    memory.0,
                    0,
                    0,
                    width_i32,
                    height_i32,
                    Some(source.dc),
                    source_x,
                    source_y,
                    SRCCOPY,
                )
                .is_ok()
            }
        };
        if !copied {
            return Err(CaptureError::Backend("GDI pixel copy failed".into()));
        }

        let row_stride = checked_stride(width)?;
        let byte_len = row_stride
            .checked_mul(usize::try_from(height).map_err(|_| CaptureError::ImageLayoutOverflow)?)
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        let mut bgra = vec![0_u8; byte_len];
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width_i32,
                biHeight: -height_i32,
                biPlanes: 1,
                biBitCount: 32,
                biSizeImage: u32::try_from(byte_len)
                    .map_err(|_| CaptureError::ImageLayoutOverflow)?,
                ..Default::default()
            },
            ..Default::default()
        };
        let rows = unsafe {
            GetDIBits(
                memory.0,
                bitmap.0,
                0,
                height,
                Some(bgra.as_mut_ptr().cast::<c_void>()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        if rows != height_i32 {
            return Err(CaptureError::Backend(format!(
                "GDI GetDIBits returned {rows} rows for {height_i32} requested rows"
            )));
        }
        let rgba =
            normalize_native_rgba8(width, height, row_stride, bgra, NativePixelFormat::Bgra8)?;
        Ok((width, height, rgba))
    }

    fn display_from_monitor(monitor: &Monitor) -> Result<Display, CaptureError> {
        let id = monitor.id().map_err(backend_error)?;
        let name = monitor.name().map_err(backend_error)?;
        let width = monitor.width().map_err(backend_error)?;
        let height = monitor.height().map_err(backend_error)?;
        Ok(Display::new(
            DisplayId::new(format!("windows:{id}:{name}"))?,
            PhysicalRect::new(
                monitor.x().map_err(backend_error)?,
                monitor.y().map_err(backend_error)?,
                width,
                height,
            )?,
            ScaleFactor::new(f64::from(monitor.scale_factor().map_err(backend_error)?))?,
            monitor.is_primary().map_err(backend_error)?,
            orientation_from_rotation(monitor.rotation().map_err(backend_error)?, width, height),
        ))
    }

    fn native_displays() -> Result<Vec<(Monitor, Display)>, CaptureError> {
        let mut ids = HashSet::new();
        Monitor::all()
            .map_err(backend_error)?
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

    pub(super) fn displays() -> Result<Vec<Display>, CaptureError> {
        native_displays().map(|items| items.into_iter().map(|(_, display)| display).collect())
    }

    fn normalized_image(
        image: xcap::image::RgbaImage,
    ) -> Result<(u32, u32, Vec<u8>), CaptureError> {
        let width = image.width();
        let height = image.height();
        let rgba = normalize_native_rgba8(
            width,
            height,
            checked_stride(width)?,
            image.into_raw(),
            NativePixelFormat::Rgba8,
        )?;
        Ok((width, height, rgba))
    }

    fn wgc_display_capture(id: &DisplayId) -> Result<CapturedImage, CaptureError> {
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
            checked_stride(width)?,
            rgba,
            CaptureScaleMetadata::uniform(display.scale_factor()),
            CaptureProvenance::Display(id.clone()),
        )
    }

    fn wgc_region_capture(region: PhysicalRect) -> Result<CapturedImage, CaptureError> {
        let items = native_displays()?;
        let displays: Vec<_> = items.iter().map(|(_, display)| display.clone()).collect();
        let metadata = capture_scale_metadata_for_region(region, &displays)?;
        let width = region.size().width();
        let height = region.size().height();
        let row_stride = checked_stride(width)?;
        let output_len = row_stride
            .checked_mul(usize::try_from(height).map_err(|_| CaptureError::ImageLayoutOverflow)?)
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        let mut rgba = vec![0; output_len];

        for (monitor, display) in &items {
            let Some(piece) = region.intersection(display.bounds()) else {
                continue;
            };
            let local_x = u32::try_from(piece.left() - display.bounds().left())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let local_y = u32::try_from(piece.top() - display.bounds().top())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let image = monitor
                .capture_region(
                    local_x,
                    local_y,
                    piece.size().width(),
                    piece.size().height(),
                )
                .map_err(backend_error)?;
            let (piece_width, piece_height, pixels) = normalized_image(image)?;
            if (piece_width, piece_height) != (piece.size().width(), piece.size().height()) {
                return Err(CaptureError::Backend(format!(
                    "backend returned {piece_width}x{piece_height} for requested {}x{} region",
                    piece.size().width(),
                    piece.size().height()
                )));
            }
            let destination_x = usize::try_from(piece.left() - region.left())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let destination_y = usize::try_from(piece.top() - region.top())
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let piece_stride = checked_stride(piece_width)?;
            for source_y in 0..usize::try_from(piece_height).unwrap_or(0) {
                let source_start = source_y * piece_stride;
                let destination_start = (destination_y + source_y) * row_stride + destination_x * 4;
                rgba[destination_start..destination_start + piece_stride]
                    .copy_from_slice(&pixels[source_start..source_start + piece_stride]);
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

    fn active_window() -> Result<(Vec<Window>, usize), CaptureError> {
        let windows = Window::all().map_err(backend_error)?;
        let facts: Result<Vec<_>, CaptureError> = windows
            .iter()
            .map(|window| {
                Ok(WindowFacts::new(
                    u64::from(window.id().map_err(backend_error)?),
                    window.pid().map_err(backend_error)?,
                    window.is_focused().map_err(backend_error)?,
                    window.is_minimized().map_err(backend_error)?,
                ))
            })
            .collect();
        let facts = facts?;
        let own_window_ids: Vec<_> = facts
            .iter()
            .filter(|window| window.pid == std::process::id())
            .map(|window| window.id)
            .collect();
        let index =
            select_active_window(&facts, std::process::id(), &own_window_ids).ok_or_else(|| {
                CaptureError::BackendUnavailable("no eligible focused non-minimized window".into())
            })?;
        Ok((windows, index))
    }

    fn wgc_active_window_capture() -> Result<CapturedImage, CaptureError> {
        let (windows, index) = active_window()?;
        let window = &windows[index];
        let scale = ScaleFactor::new(f64::from(
            window
                .current_monitor()
                .map_err(backend_error)?
                .scale_factor()
                .map_err(backend_error)?,
        ))?;
        let (width, height, rgba) =
            normalized_image(window.capture_image().map_err(backend_error)?)?;
        CapturedImage::new(
            width,
            height,
            checked_stride(width)?,
            rgba,
            CaptureScaleMetadata::uniform(scale),
            CaptureProvenance::ActiveWindow {
                fallback: Some(CaptureFallback::WholeWindow),
            },
        )
    }

    fn wgc_capture(request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        match request.source() {
            CaptureSource::Region(region) => wgc_region_capture(*region),
            CaptureSource::ActiveWindow => wgc_active_window_capture(),
            CaptureSource::Display(id) => wgc_display_capture(id),
        }
    }

    fn gdi_display_capture(id: &DisplayId) -> Result<CapturedImage, CaptureError> {
        let items = native_displays()?;
        let (_, display) = items
            .iter()
            .find(|(_, display)| display.id() == id)
            .ok_or_else(|| CaptureError::DisplayNotFound(id.clone()))?;
        let bounds = display.bounds();
        let desktop = unsafe { GetDesktopWindow() };
        let (width, height, rgba) = gdi_surface(
            desktop,
            bounds.origin().x,
            bounds.origin().y,
            bounds.size().width(),
            bounds.size().height(),
            false,
        )?;
        CapturedImage::new(
            width,
            height,
            checked_stride(width)?,
            rgba,
            CaptureScaleMetadata::uniform(display.scale_factor()),
            CaptureProvenance::Display(id.clone()),
        )
    }

    fn gdi_region_capture(region: PhysicalRect) -> Result<CapturedImage, CaptureError> {
        let displays = displays()?;
        let metadata = capture_scale_metadata_for_region(region, &displays)?;
        let desktop = unsafe { GetDesktopWindow() };
        let (width, height, rgba) = gdi_surface(
            desktop,
            region.origin().x,
            region.origin().y,
            region.size().width(),
            region.size().height(),
            false,
        )?;
        CapturedImage::new(
            width,
            height,
            checked_stride(width)?,
            rgba,
            metadata,
            CaptureProvenance::Region,
        )
    }

    fn gdi_active_window_capture() -> Result<CapturedImage, CaptureError> {
        let (windows, index) = active_window()?;
        let window = &windows[index];
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return Err(CaptureError::BackendUnavailable(
                "no foreground window is available for GDI capture".into(),
            ));
        }
        let expected_id = window.id().map_err(backend_error)?;
        if hwnd.0 as usize as u32 != expected_id {
            return Err(CaptureError::BackendUnavailable(
                "the foreground window changed while selecting the GDI fallback target".into(),
            ));
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect).map_err(backend_error)? };
        let width =
            u32::try_from(rect.right - rect.left).map_err(|_| CaptureError::ImageLayoutOverflow)?;
        let height =
            u32::try_from(rect.bottom - rect.top).map_err(|_| CaptureError::ImageLayoutOverflow)?;
        let scale = ScaleFactor::new(f64::from(
            window
                .current_monitor()
                .map_err(backend_error)?
                .scale_factor()
                .map_err(backend_error)?,
        ))?;
        let (width, height, rgba) = gdi_surface(hwnd, 0, 0, width, height, true)?;
        CapturedImage::new(
            width,
            height,
            checked_stride(width)?,
            rgba,
            CaptureScaleMetadata::uniform(scale),
            CaptureProvenance::ActiveWindow {
                fallback: Some(CaptureFallback::WholeWindow),
            },
        )
    }

    fn gdi_capture(request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        match request.source() {
            CaptureSource::Region(region) => gdi_region_capture(*region),
            CaptureSource::ActiveWindow => gdi_active_window_capture(),
            CaptureSource::Display(id) => gdi_display_capture(id),
        }
    }

    pub(super) fn capture_detailed(
        request: CaptureRequest,
    ) -> Result<WindowsCaptureResult, CaptureError> {
        let wgc_request = request.clone();
        let result =
            capture_with_runtime_fallback(|| wgc_capture(wgc_request), || gdi_capture(request))?;
        Ok(WindowsCaptureResult {
            image: result.value,
            backend: result.backend,
            fallback: result.fallback,
        })
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::WindowsCaptureResult;
    use newpic_platform::{
        capture::{CaptureError, CaptureRequest},
        geometry::Display,
    };

    pub(super) fn displays() -> Result<Vec<Display>, CaptureError> {
        Err(CaptureError::BackendUnavailable(
            "the Windows capture provider requires a Windows target".into(),
        ))
    }

    pub(super) fn capture_detailed(
        _request: CaptureRequest,
    ) -> Result<WindowsCaptureResult, CaptureError> {
        Err(CaptureError::BackendUnavailable(
            "the Windows capture provider requires a Windows target".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newpic_platform::{
        capture::{CaptureOptions, CaptureSource},
        geometry::{DisplayId, DisplayOrientation},
    };

    #[test]
    fn rotation_and_portrait_fallback_are_preserved() {
        assert_eq!(
            orientation_from_rotation(180.0, 1920, 1080),
            DisplayOrientation::LandscapeFlipped
        );
        assert_eq!(
            orientation_from_rotation(0.0, 1080, 1920),
            DisplayOrientation::Portrait
        );
    }

    #[test]
    fn active_window_is_focused_visible_and_not_ours() {
        let windows = [
            WindowFacts::new(70, 7, true, false),
            WindowFacts::new(80, 8, true, true),
            WindowFacts::new(90, 9, true, false),
        ];
        assert_eq!(select_active_window(&windows, 7, &[70]), Some(2));
    }

    #[test]
    fn active_window_exclusion_uses_pid_and_window_identity() {
        let windows = [
            WindowFacts::new(70, 9, true, false),
            WindowFacts::new(90, 9, true, false),
        ];
        assert_eq!(select_active_window(&windows, 7, &[70]), Some(1));
    }

    #[test]
    fn successful_wgc_capture_does_not_invoke_gdi() {
        let mut gdi_called = false;
        let result = capture_with_runtime_fallback(
            || Ok::<_, CaptureError>("wgc pixels"),
            || {
                gdi_called = true;
                Ok("gdi pixels")
            },
        )
        .unwrap();

        assert_eq!(result.value, "wgc pixels");
        assert_eq!(result.backend, CaptureBackend::WindowsGraphicsCapture);
        assert_eq!(result.fallback, None);
        assert!(!gdi_called);
    }

    #[test]
    fn failed_wgc_capture_uses_gdi_and_records_the_reason() {
        let result = capture_with_runtime_fallback(
            || Err::<&str, _>(CaptureError::Backend("WGC unavailable".into())),
            || Ok("gdi pixels"),
        )
        .unwrap();

        assert_eq!(result.value, "gdi pixels");
        assert_eq!(result.backend, CaptureBackend::WindowsGdi);
        assert_eq!(
            result.fallback,
            Some(WindowsBackendFallback::WgcFailed {
                reason: "capture backend failed: WGC unavailable".into(),
            })
        );
    }

    #[test]
    fn both_backend_errors_are_preserved() {
        let error = capture_with_runtime_fallback(
            || Err::<(), _>(CaptureError::Backend("WGC failed".into())),
            || Err(CaptureError::Backend("GDI failed".into())),
        )
        .unwrap_err();

        assert!(error.to_string().contains("WGC failed"));
        assert!(error.to_string().contains("GDI failed"));
    }

    #[test]
    fn pointer_request_is_rejected_before_native_access() {
        let provider = WindowsCaptureProvider::new();
        let request = CaptureRequest::new(
            CaptureSource::Display(DisplayId::new("test").unwrap()),
            CaptureOptions::default().with_include_pointer(true),
        );
        assert!(matches!(
            provider.capture(request),
            Err(CaptureError::Unsupported {
                operation: "pointer capture",
                ..
            })
        ));
    }
}
