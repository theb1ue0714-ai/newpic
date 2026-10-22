//! macOS ScreenCaptureKit screenshot adapter.

#[cfg(any(target_os = "macos", test))]
use newpic_platform::geometry::PhysicalRect;
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
            .with_region_capture(CapabilityStatus::limited(
                "cross-display capture rejects ambiguous, non-edge-aligned mixed-scale layouts",
                None::<String>,
            ))
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
    on_screen: bool,
    layer: i32,
}

#[cfg(any(target_os = "macos", test))]
fn select_active_window(
    windows: &[WindowFacts],
    frontmost_pid: u32,
    own_pid: u32,
    excluded: &[u32],
    active_window_ids: Option<&[u32]>,
    ordered_window_ids: &[u32],
) -> Option<usize> {
    let eligible = |w: &WindowFacts| {
        w.pid == frontmost_pid
            && w.on_screen
            && w.layer == 0
            && w.pid != own_pid
            && !excluded.contains(&w.window_id)
    };
    let exact_eligible = |window_id: &u32| {
        windows
            .iter()
            .position(|window| window.window_id == *window_id && eligible(window))
    };

    active_window_ids
        .and_then(|ids| ids.iter().find_map(exact_eligible))
        .or_else(|| ordered_window_ids.iter().find_map(exact_eligible))
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy)]
struct DisplayLayoutFacts {
    id: u32,
    logical_x: f64,
    logical_y: f64,
    logical_width: f64,
    logical_height: f64,
    pixel_width: u32,
    pixel_height: u32,
    primary: bool,
}

#[cfg(any(target_os = "macos", test))]
fn physical_display_layout(
    facts: &[DisplayLayoutFacts],
) -> Result<Vec<PhysicalRect>, CaptureError> {
    const EPSILON: f64 = 0.01;
    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() <= EPSILON
    }
    if facts.is_empty() || facts.iter().filter(|d| d.primary).count() != 1 {
        return Err(CaptureError::Backend(
            "macOS display topology requires exactly one primary display".into(),
        ));
    }
    for fact in facts {
        if !fact.logical_x.is_finite()
            || !fact.logical_y.is_finite()
            || !fact.logical_width.is_finite()
            || !fact.logical_height.is_finite()
            || fact.logical_width <= 0.0
            || fact.logical_height <= 0.0
            || fact.pixel_width == 0
            || fact.pixel_height == 0
        {
            return Err(CaptureError::Backend(
                "invalid macOS display geometry".into(),
            ));
        }
    }
    let primary = facts.iter().position(|d| d.primary).unwrap();
    let mut origins = vec![None; facts.len()];
    origins[primary] = Some((0_i32, 0_i32));
    while origins.iter().any(Option::is_none) {
        let mut progress = false;
        for index in 0..facts.len() {
            if origins[index].is_some() {
                continue;
            }
            let current = facts[index];
            let mut x = None;
            let mut y = None;
            for (other_index, origin) in origins.iter().enumerate() {
                let Some((other_x, other_y)) = origin else {
                    continue;
                };
                let other = facts[other_index];
                let current_scale = f64::from(current.pixel_width) / current.logical_width;
                let other_scale = f64::from(other.pixel_width) / other.logical_width;
                let same_scale = near(current_scale, other_scale);
                let x_candidate = if same_scale {
                    Some(
                        i64::from(*other_x)
                            + ((current.logical_x - other.logical_x) * current_scale).round()
                                as i64,
                    )
                } else if near(current.logical_x, other.logical_x) {
                    Some(i64::from(*other_x))
                } else if near(
                    current.logical_x + current.logical_width,
                    other.logical_x + other.logical_width,
                ) {
                    Some(
                        i64::from(*other_x) + i64::from(other.pixel_width)
                            - i64::from(current.pixel_width),
                    )
                } else if near(current.logical_x, other.logical_x + other.logical_width) {
                    Some(i64::from(*other_x) + i64::from(other.pixel_width))
                } else if near(current.logical_x + current.logical_width, other.logical_x) {
                    Some(i64::from(*other_x) - i64::from(current.pixel_width))
                } else {
                    None
                };
                let y_candidate = if same_scale {
                    Some(
                        i64::from(*other_y)
                            + ((current.logical_y - other.logical_y) * current_scale).round()
                                as i64,
                    )
                } else if near(current.logical_y, other.logical_y) {
                    Some(i64::from(*other_y))
                } else if near(
                    current.logical_y + current.logical_height,
                    other.logical_y + other.logical_height,
                ) {
                    Some(
                        i64::from(*other_y) + i64::from(other.pixel_height)
                            - i64::from(current.pixel_height),
                    )
                } else if near(current.logical_y, other.logical_y + other.logical_height) {
                    Some(i64::from(*other_y) + i64::from(other.pixel_height))
                } else if near(current.logical_y + current.logical_height, other.logical_y) {
                    Some(i64::from(*other_y) - i64::from(current.pixel_height))
                } else {
                    None
                };
                if let Some(candidate) = x_candidate {
                    if x.is_some_and(|known| known != candidate) {
                        return Err(CaptureError::Backend(
                            "ambiguous macOS display X topology".into(),
                        ));
                    }
                    x = Some(candidate);
                }
                if let Some(candidate) = y_candidate {
                    if y.is_some_and(|known| known != candidate) {
                        return Err(CaptureError::Backend(
                            "ambiguous macOS display Y topology".into(),
                        ));
                    }
                    y = Some(candidate);
                }
            }
            if let (Some(x), Some(y)) = (x, y) {
                origins[index] = Some((
                    i32::try_from(x).map_err(|_| CaptureError::ImageLayoutOverflow)?,
                    i32::try_from(y).map_err(|_| CaptureError::ImageLayoutOverflow)?,
                ));
                progress = true;
            }
        }
        if !progress {
            return Err(CaptureError::Backend(
                "ambiguous cross-display macOS physical topology".into(),
            ));
        }
    }
    facts
        .iter()
        .zip(origins)
        .map(|(fact, origin)| {
            let _ = fact.id;
            let (x, y) = origin.expect("layout assigned above");
            PhysicalRect::new(x, y, fact.pixel_width, fact.pixel_height).map_err(Into::into)
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
#[allow(clippy::too_many_arguments)]
fn composite_first_writer_masked(
    output: &mut [u8],
    written: &mut [bool],
    output_width: u32,
    output_height: u32,
    dx: usize,
    dy: usize,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), CaptureError> {
    let output_pixels = usize::try_from(output_width)
        .ok()
        .and_then(|w| w.checked_mul(output_height as usize))
        .ok_or(CaptureError::ImageLayoutOverflow)?;
    let source_pixels = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(height as usize))
        .ok_or(CaptureError::ImageLayoutOverflow)?;
    if written.len() != output_pixels
        || output.len()
            != output_pixels
                .checked_mul(4)
                .ok_or(CaptureError::ImageLayoutOverflow)?
        || pixels.len()
            != source_pixels
                .checked_mul(4)
                .ok_or(CaptureError::ImageLayoutOverflow)?
        || dx + width as usize > output_width as usize
        || dy + height as usize > output_height as usize
    {
        return Err(CaptureError::Backend(
            "invalid macOS compositor layout".into(),
        ));
    }
    for y in 0..height as usize {
        for x in 0..width as usize {
            let source = (y * width as usize + x) * 4;
            let destination_pixel = (dy + y) * output_width as usize + dx + x;
            if !written[destination_pixel] {
                let destination = destination_pixel * 4;
                output[destination..destination + 4].copy_from_slice(&pixels[source..source + 4]);
                written[destination_pixel] = true;
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{
        classify_screen_capture_error, composite_first_writer_masked, physical_display_layout,
        select_active_window, DisplayLayoutFacts, WindowFacts,
    };
    use newpic_platform::{
        capture::{
            capture_scale_metadata_for_region, normalize_native_rgba8, CaptureError,
            CaptureFallback, CaptureProvenance, CaptureRequest, CaptureScaleMetadata,
            CaptureSource, CapturedImage, NativePixelFormat,
        },
        geometry::{Display, DisplayId, DisplayOrientation, PhysicalRect, ScaleFactor},
    };
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSProcessInfo;
    use screencapturekit::{
        cg::CGRect,
        cm::{CMSampleBufferExt, CMSampleBufferSCExt},
        cv::CVPixelBufferLockFlags,
        prelude::*,
    };
    use std::{collections::HashSet, ffi::c_void, sync::mpsc, time::Duration};

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayRotation(display: u32) -> f64;
        fn CGWindowListCreate(option: u32, relative_to_window: u32) -> *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFNumberGetValue(number: *const c_void, number_type: isize, value: *mut c_void) -> bool;
        fn CFRelease(value: *const c_void);
    }

    fn supports_scwindow_is_active() -> bool {
        let version = NSProcessInfo::processInfo().operatingSystemVersion();
        version.majorVersion > 13 || (version.majorVersion == 13 && version.minorVersion >= 1)
    }

    fn ordered_window_ids() -> Result<Vec<u32>, CaptureError> {
        const ON_SCREEN_ONLY: u32 = 1 << 0;
        const EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
        const SINT64_TYPE: isize = 4;

        let array = unsafe { CGWindowListCreate(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, 0) };
        if array.is_null() {
            return Err(CaptureError::Backend(
                "CoreGraphics window ordering is unavailable".into(),
            ));
        }
        let count = unsafe { CFArrayGetCount(array) };
        let mut ids = Vec::with_capacity(count.max(0) as usize);
        for index in 0..count {
            let number = unsafe { CFArrayGetValueAtIndex(array, index) };
            let mut value = 0_i64;
            if !number.is_null()
                && unsafe { CFNumberGetValue(number, SINT64_TYPE, (&mut value as *mut i64).cast()) }
            {
                if let Ok(id) = u32::try_from(value) {
                    ids.push(id);
                }
            }
        }
        unsafe { CFRelease(array) };
        Ok(ids)
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
    fn orientation(id: u32, width: u32, height: u32) -> DisplayOrientation {
        match (unsafe { CGDisplayRotation(id) }).round().rem_euclid(360.0) as u16 {
            90 => DisplayOrientation::Portrait,
            180 => DisplayOrientation::LandscapeFlipped,
            270 => DisplayOrientation::PortraitFlipped,
            _ if height > width => DisplayOrientation::Portrait,
            _ => DisplayOrientation::Landscape,
        }
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
        let natives = content()?.displays();
        let main = unsafe { CGMainDisplayID() };
        let facts: Vec<_> = natives
            .iter()
            .map(|native| {
                let frame = native.frame();
                if frame.size.width <= 0.0 || frame.size.height <= 0.0 {
                    return Err(CaptureError::Backend(
                        "ScreenCaptureKit returned an empty display".into(),
                    ));
                }
                Ok(DisplayLayoutFacts {
                    id: native.display_id(),
                    logical_x: frame.origin.x,
                    logical_y: frame.origin.y,
                    logical_width: frame.size.width,
                    logical_height: frame.size.height,
                    pixel_width: native.width(),
                    pixel_height: native.height(),
                    primary: native.display_id() == main,
                })
            })
            .collect::<Result<_, CaptureError>>()?;
        let bounds = physical_display_layout(&facts)?;
        natives
            .into_iter()
            .zip(facts)
            .zip(bounds)
            .map(|((native, fact), bounds)| {
                let sx = fact.pixel_width as f64 / fact.logical_width;
                let sy = fact.pixel_height as f64 / fact.logical_height;
                if (sx - sy).abs() > 0.01 {
                    return Err(CaptureError::Backend("inconsistent display scale".into()));
                }
                let id = DisplayId::new(format!("macos:{}", fact.id))?;
                if !ids.insert(id.clone()) {
                    return Err(CaptureError::Backend("duplicate display ID".into()));
                }
                Ok((
                    native,
                    Display::new(
                        id,
                        bounds,
                        ScaleFactor::new((sx + sy) / 2.0)?,
                        fact.primary,
                        orientation(fact.id, fact.pixel_width, fact.pixel_height),
                    ),
                ))
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
        let mut written = vec![false; width as usize * height as usize];
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
            composite_first_writer_masked(
                &mut output,
                &mut written,
                width,
                height,
                dx,
                dy,
                w,
                h,
                &pixels,
            )?;
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
        let frontmost_pid = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
            .filter(|pid| *pid > 0)
            .map(|pid| pid as u32)
            .ok_or_else(|| CaptureError::BackendUnavailable("no frontmost application".into()))?;
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
                on_screen: w.is_on_screen,
                layer: w.window_layer,
            })
            .collect();
        let active_window_ids = supports_scwindow_is_active().then(|| {
            snap.windows
                .iter()
                .filter(|window| window.is_active)
                .map(|window| window.window_id)
                .collect::<Vec<_>>()
        });
        let ordered_window_ids = ordered_window_ids()?;
        let index = select_active_window(
            &facts,
            frontmost_pid,
            std::process::id(),
            excluded,
            active_window_ids.as_deref(),
            &ordered_window_ids,
        )
        .ok_or_else(|| {
            CaptureError::BackendUnavailable("no eligible frontmost on-screen window".into())
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
    fn w(id: u32, pid: u32, on_screen: bool) -> WindowFacts {
        WindowFacts {
            window_id: id,
            pid,
            on_screen,
            layer: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn display(
        id: u32,
        x: f64,
        y: f64,
        logical_width: f64,
        logical_height: f64,
        pixel_width: u32,
        pixel_height: u32,
        primary: bool,
    ) -> DisplayLayoutFacts {
        DisplayLayoutFacts {
            id,
            logical_x: x,
            logical_y: y,
            logical_width,
            logical_height,
            pixel_width,
            pixel_height,
            primary,
        }
    }

    #[test]
    fn selection_uses_frontmost_pid_and_excludes_exact_window_id() {
        assert_eq!(
            select_active_window(
                &[w(1, 8, true), w(2, 7, true), w(3, 7, true)],
                7,
                99,
                &[2],
                None,
                &[1, 2, 3],
            ),
            Some(2)
        );
    }

    #[test]
    fn selection_requires_visible_normal_window_from_frontmost_process() {
        let mut top = w(3, 9, true);
        top.layer = 1;
        assert_eq!(
            select_active_window(
                &[w(1, 9, false), w(2, 8, true), top],
                9,
                99,
                &[],
                None,
                &[1, 2, 3],
            ),
            None
        );
    }

    #[test]
    fn active_window_flag_wins_over_core_graphics_order_when_available() {
        let windows = [w(10, 7, true), w(20, 7, true)];

        assert_eq!(
            select_active_window(&windows, 7, 99, &[], Some(&[20]), &[10, 20]),
            Some(1)
        );
    }

    #[test]
    fn macos_13_0_uses_core_graphics_front_to_back_window_ids() {
        let windows = [w(20, 7, true), w(10, 7, true)];

        assert_eq!(
            select_active_window(&windows, 7, 99, &[], None, &[10, 20]),
            Some(1)
        );
    }

    #[test]
    fn ordered_selection_maps_exact_id_and_skips_ineligible_windows() {
        let windows = [w(10, 8, true), w(20, 7, true), w(30, 7, true)];

        assert_eq!(
            select_active_window(&windows, 7, 99, &[20], None, &[10, 20, 30]),
            Some(2)
        );
    }

    #[test]
    fn ordered_selection_never_falls_back_to_arbitrary_snapshot_order() {
        let windows = [w(20, 7, true), w(10, 7, true)];

        assert_eq!(
            select_active_window(&windows, 7, 99, &[], None, &[999]),
            None
        );
    }

    #[test]
    fn physical_layout_uses_local_sizes_for_mixed_scale_adjacent_displays() {
        let layouts = physical_display_layout(&[
            display(1, 0.0, 0.0, 1440.0, 900.0, 2880, 1800, true),
            display(2, 1440.0, 0.0, 1920.0, 1080.0, 1920, 1080, false),
        ])
        .unwrap();

        assert_eq!(layouts[0], PhysicalRect::new(0, 0, 2880, 1800).unwrap());
        assert_eq!(layouts[1], PhysicalRect::new(2880, 0, 1920, 1080).unwrap());
    }

    #[test]
    fn physical_layout_accepts_same_scale_side_by_side_with_vertical_offset() {
        let layouts = physical_display_layout(&[
            display(1, 0.0, 0.0, 1920.0, 1080.0, 1920, 1080, true),
            display(2, 1920.0, 120.0, 1600.0, 900.0, 1600, 900, false),
        ])
        .unwrap();

        assert_eq!(layouts[1], PhysicalRect::new(1920, 120, 1600, 900).unwrap());
    }

    #[test]
    fn physical_layout_preserves_negative_and_vertical_topology() {
        let layouts = physical_display_layout(&[
            display(1, 0.0, 0.0, 1920.0, 1080.0, 1920, 1080, true),
            display(2, -1280.0, 0.0, 1280.0, 720.0, 2560, 1440, false),
            display(3, 0.0, -900.0, 1600.0, 900.0, 1600, 900, false),
        ])
        .unwrap();

        assert_eq!(layouts[1], PhysicalRect::new(-2560, 0, 2560, 1440).unwrap());
        assert_eq!(layouts[2], PhysicalRect::new(0, -900, 1600, 900).unwrap());
    }

    #[test]
    fn physical_layout_uses_rotated_displays_reported_physical_size() {
        let layouts = physical_display_layout(&[
            display(1, 0.0, 0.0, 1920.0, 1080.0, 1920, 1080, true),
            display(2, 1920.0, 0.0, 1080.0, 1920.0, 1080, 1920, false),
        ])
        .unwrap();

        assert_eq!(layouts[1], PhysicalRect::new(1920, 0, 1080, 1920).unwrap());
    }

    #[test]
    fn physical_layout_rejects_ambiguous_cross_scale_offset() {
        let error = physical_display_layout(&[
            display(1, 0.0, 0.0, 1440.0, 900.0, 2880, 1800, true),
            display(2, 1440.0, 100.0, 1920.0, 1080.0, 1920, 1080, false),
        ])
        .unwrap_err();

        assert!(matches!(error, CaptureError::Backend(_)));
    }

    #[test]
    fn compositor_keeps_first_provider_for_overlapping_pixels() {
        let mut output = vec![0; 3 * 4];
        let mut written = vec![false; 3];
        composite_first_writer_masked(
            &mut output,
            &mut written,
            3,
            1,
            0,
            0,
            2,
            1,
            &[1, 2, 3, 4, 5, 6, 7, 8],
        )
        .unwrap();
        composite_first_writer_masked(
            &mut output,
            &mut written,
            3,
            1,
            1,
            0,
            2,
            1,
            &[9, 10, 11, 12, 13, 14, 15, 16],
        )
        .unwrap();

        assert_eq!(output, vec![1, 2, 3, 4, 5, 6, 7, 8, 13, 14, 15, 16]);
        assert_eq!(written, vec![true, true, true]);
    }
    #[test]
    fn ordinary_error_is_not_permission_error() {
        assert!(matches!(
            classify_screen_capture_error("transport failed"),
            CaptureError::Backend(_)
        ));
    }
}
