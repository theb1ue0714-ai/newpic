use thiserror::Error;

use crate::capabilities::{CaptureBackend, CaptureCapabilities};
use crate::geometry::{Display, DisplayId, GeometryError, PhysicalRect, ScaleFactor};

/// Byte order supplied by a native capture API before NewPic normalizes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePixelFormat {
    Rgba8,
    Bgra8,
}

/// Converts a possibly padded native row buffer into tightly packed RGBA8.
///
/// This is the single byte-order conversion path used by native adapters. It
/// deliberately owns the source bytes so captured pixels are not borrowed
/// across platform API boundaries.
pub fn normalize_native_rgba8(
    width: u32,
    height: u32,
    source_row_stride: usize,
    source: Vec<u8>,
    format: NativePixelFormat,
) -> Result<Vec<u8>, CaptureError> {
    if width == 0 || height == 0 {
        return Err(CaptureError::InvalidImageDimensions { width, height });
    }
    let tight_stride = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(CaptureError::ImageLayoutOverflow)?;
    if source_row_stride < tight_stride {
        return Err(CaptureError::RowStrideTooSmall {
            minimum: tight_stride,
            actual: source_row_stride,
        });
    }
    let source_len = source_row_stride
        .checked_mul(usize::try_from(height).map_err(|_| CaptureError::ImageLayoutOverflow)?)
        .ok_or(CaptureError::ImageLayoutOverflow)?;
    if source.len() != source_len {
        return Err(CaptureError::PixelByteLengthMismatch {
            expected: source_len,
            actual: source.len(),
        });
    }
    let output_len = tight_stride
        .checked_mul(usize::try_from(height).map_err(|_| CaptureError::ImageLayoutOverflow)?)
        .ok_or(CaptureError::ImageLayoutOverflow)?;
    let mut rgba = Vec::with_capacity(output_len);
    for source_row in source.chunks_exact(source_row_stride) {
        rgba.extend_from_slice(&source_row[..tight_stride]);
    }
    if format == NativePixelFormat::Bgra8 {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }
    Ok(rgba)
}

/// Describes which display supplied each captured pixel. Display slice order
/// is provider z-order: lower indices win overlaps, with display ID as the
/// deterministic tie-break described by [`DisplayScaleSource::precedes`].
pub fn capture_scale_metadata_for_region(
    region: PhysicalRect,
    displays: &[Display],
) -> Result<CaptureScaleMetadata, CaptureError> {
    let intersections: Vec<_> = displays
        .iter()
        .enumerate()
        .filter_map(|(z_order, display)| {
            region
                .intersection(display.bounds())
                .map(|intersection| (z_order, display, intersection))
        })
        .collect();
    if intersections.is_empty() {
        return Err(CaptureError::InvalidScaleSegments(
            "capture region does not intersect a display".into(),
        ));
    }

    let mut x_edges = vec![region.left(), region.right()];
    let mut y_edges = vec![region.top(), region.bottom()];
    for (_, _, intersection) in &intersections {
        x_edges.extend([intersection.left(), intersection.right()]);
        y_edges.extend([intersection.top(), intersection.bottom()]);
    }
    x_edges.sort_unstable();
    x_edges.dedup();
    y_edges.sort_unstable();
    y_edges.dedup();

    let mut segments: Vec<CaptureScaleSegment> = Vec::new();
    for y_pair in y_edges.windows(2) {
        for x_pair in x_edges.windows(2) {
            let width = u32::try_from(x_pair[1] - x_pair[0])
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let height = u32::try_from(y_pair[1] - y_pair[0])
                .map_err(|_| CaptureError::ImageLayoutOverflow)?;
            let desktop_cell = PhysicalRect::new(
                i32::try_from(x_pair[0]).map_err(|_| CaptureError::ImageLayoutOverflow)?,
                i32::try_from(y_pair[0]).map_err(|_| CaptureError::ImageLayoutOverflow)?,
                width,
                height,
            )?;
            let winner = intersections
                .iter()
                .filter(|(_, _, covered)| covered.intersection(desktop_cell) == Some(desktop_cell))
                .min_by_key(|(z_order, display, _)| (*z_order, display.id().as_str()));
            let source = winner.map_or(CaptureScaleSource::Gap, |(z_order, display, _)| {
                CaptureScaleSource::Display(DisplayScaleSource::new(
                    display.id().clone(),
                    display.scale_factor(),
                    u32::try_from(*z_order).unwrap_or(u32::MAX),
                ))
            });
            let image_cell = PhysicalRect::new(
                i32::try_from(desktop_cell.left() - region.left())
                    .map_err(|_| CaptureError::ImageLayoutOverflow)?,
                i32::try_from(desktop_cell.top() - region.top())
                    .map_err(|_| CaptureError::ImageLayoutOverflow)?,
                width,
                height,
            )?;

            if let Some(previous) = segments.last_mut() {
                let previous_rect = previous.image_rect();
                if previous.source() == &source
                    && previous_rect.top() == image_cell.top()
                    && previous_rect.bottom() == image_cell.bottom()
                    && previous_rect.right() == image_cell.left()
                {
                    *previous = CaptureScaleSegment::new(
                        PhysicalRect::new(
                            previous_rect.origin().x,
                            previous_rect.origin().y,
                            previous_rect.size().width() + image_cell.size().width(),
                            previous_rect.size().height(),
                        )?,
                        source,
                    );
                    continue;
                }
            }
            segments.push(CaptureScaleSegment::new(image_cell, source));
        }
    }

    let mut display_scales = segments
        .iter()
        .filter_map(|segment| match segment.source() {
            CaptureScaleSource::Display(source) => Some(source.scale_factor()),
            CaptureScaleSource::Gap => None,
        });
    let first_scale = display_scales.next().ok_or_else(|| {
        CaptureError::InvalidScaleSegments("capture region contains no display pixels".into())
    })?;
    let has_gap = segments
        .iter()
        .any(|segment| matches!(segment.source(), CaptureScaleSource::Gap));
    if !has_gap && display_scales.all(|scale| scale == first_scale) {
        Ok(CaptureScaleMetadata::uniform(first_scale))
    } else {
        CaptureScaleMetadata::mixed(segments)
    }
}

pub trait CaptureProvider: Send + Sync {
    fn capabilities(&self) -> CaptureCapabilities;
    fn displays(&self) -> Result<Vec<Display>, CaptureError>;
    fn capture(&self, request: CaptureRequest) -> Result<CapturedImage, CaptureError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSource {
    Region(PhysicalRect),
    ActiveWindow,
    Display(DisplayId),
}

/// Options are part of every request so providers never depend on hidden
/// mutable capture settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureOptions {
    include_pointer: bool,
}

impl CaptureOptions {
    pub const fn with_include_pointer(mut self, include_pointer: bool) -> Self {
        self.include_pointer = include_pointer;
        self
    }

    pub const fn include_pointer(self) -> bool {
        self.include_pointer
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRequest {
    source: CaptureSource,
    options: CaptureOptions,
}

impl CaptureRequest {
    pub const fn new(source: CaptureSource, options: CaptureOptions) -> Self {
        Self { source, options }
    }

    pub const fn source(&self) -> &CaptureSource {
        &self.source
    }

    pub const fn options(&self) -> CaptureOptions {
        self.options
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureFallback {
    WholeWindow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureProvenance {
    Region,
    ActiveWindow { fallback: Option<CaptureFallback> },
    Display(DisplayId),
}

/// The backend that actually produced a capture and any runtime fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureExecution {
    backend: CaptureBackend,
    fallback_reason: Option<String>,
}

impl CaptureExecution {
    pub fn new(backend: CaptureBackend, fallback_reason: Option<impl Into<String>>) -> Self {
        Self {
            backend,
            fallback_reason: fallback_reason.map(Into::into),
        }
    }

    pub const fn backend(&self) -> CaptureBackend {
        self.backend
    }

    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayScaleSource {
    display_id: DisplayId,
    scale_factor: ScaleFactor,
    provider_z_order: u32,
}

impl DisplayScaleSource {
    pub const fn new(
        display_id: DisplayId,
        scale_factor: ScaleFactor,
        provider_z_order: u32,
    ) -> Self {
        Self {
            display_id,
            scale_factor,
            provider_z_order,
        }
    }

    pub const fn display_id(&self) -> &DisplayId {
        &self.display_id
    }
    pub const fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }
    pub const fn provider_z_order(&self) -> u32 {
        self.provider_z_order
    }

    /// Overlapping/mirrored sources are resolved by lower provider z-order,
    /// then lexicographically by stable display ID.
    pub fn precedes(&self, other: &Self) -> bool {
        (self.provider_z_order, self.display_id.as_str())
            < (other.provider_z_order, other.display_id.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureScaleSource {
    Display(DisplayScaleSource),
    /// Pixels in the rectangular capture that are not sourced from a display.
    Gap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureScaleSegment {
    image_rect: PhysicalRect,
    source: CaptureScaleSource,
}

impl CaptureScaleSegment {
    pub const fn new(image_rect: PhysicalRect, source: CaptureScaleSource) -> Self {
        Self { image_rect, source }
    }

    pub const fn image_rect(&self) -> PhysicalRect {
        self.image_rect
    }
    pub const fn source(&self) -> &CaptureScaleSource {
        &self.source
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureScaleKind {
    Unknown,
    Uniform(ScaleFactor),
    Mixed(Vec<CaptureScaleSegment>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureScaleMetadata {
    kind: CaptureScaleKind,
}

impl CaptureScaleMetadata {
    /// Scale is intentionally unknown because a compositor owns the source
    /// selection and does not return its desktop-space geometry.
    pub const fn unknown() -> Self {
        Self {
            kind: CaptureScaleKind::Unknown,
        }
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self.kind, CaptureScaleKind::Unknown)
    }

    pub const fn uniform(scale: ScaleFactor) -> Self {
        Self {
            kind: CaptureScaleKind::Uniform(scale),
        }
    }

    pub fn mixed(segments: Vec<CaptureScaleSegment>) -> Result<Self, CaptureError> {
        if segments.len() < 2 {
            return Err(CaptureError::InvalidScaleSegments(
                "mixed scale metadata requires at least two segments".into(),
            ));
        }
        let display_scales: Vec<_> = segments
            .iter()
            .filter_map(|segment| match segment.source() {
                CaptureScaleSource::Display(source) => Some(source.scale_factor()),
                CaptureScaleSource::Gap => None,
            })
            .collect();
        let has_gap = display_scales.len() != segments.len();
        if display_scales.is_empty() {
            return Err(CaptureError::InvalidScaleSegments(
                "mixed scale metadata requires at least one display source".into(),
            ));
        }
        if !has_gap
            && display_scales
                .iter()
                .all(|scale| *scale == display_scales[0])
        {
            return Err(CaptureError::InvalidScaleSegments(
                "mixed scale metadata must contain distinct scale factors".into(),
            ));
        }
        Ok(Self {
            kind: CaptureScaleKind::Mixed(segments),
        })
    }

    pub fn segments(&self) -> Option<&[CaptureScaleSegment]> {
        match &self.kind {
            CaptureScaleKind::Unknown => None,
            CaptureScaleKind::Uniform(_) => None,
            CaptureScaleKind::Mixed(segments) => Some(segments),
        }
    }

    pub const fn uniform_scale(&self) -> Option<ScaleFactor> {
        match &self.kind {
            CaptureScaleKind::Unknown => None,
            CaptureScaleKind::Uniform(scale) => Some(*scale),
            CaptureScaleKind::Mixed(_) => None,
        }
    }
}

/// An owned, row-major RGBA8 image returned by every capture backend.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedImage {
    width: u32,
    height: u32,
    row_stride: usize,
    rgba: Vec<u8>,
    scale_metadata: CaptureScaleMetadata,
    provenance: CaptureProvenance,
    execution: Option<CaptureExecution>,
}

impl CapturedImage {
    pub fn new(
        width: u32,
        height: u32,
        row_stride: usize,
        rgba: Vec<u8>,
        scale_metadata: CaptureScaleMetadata,
        provenance: CaptureProvenance,
    ) -> Result<Self, CaptureError> {
        if width == 0 || height == 0 {
            return Err(CaptureError::InvalidImageDimensions { width, height });
        }
        let minimum = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        if row_stride < minimum {
            return Err(CaptureError::RowStrideTooSmall {
                minimum,
                actual: row_stride,
            });
        }
        let expected = row_stride
            .checked_mul(usize::try_from(height).map_err(|_| CaptureError::ImageLayoutOverflow)?)
            .ok_or(CaptureError::ImageLayoutOverflow)?;
        if rgba.len() != expected {
            return Err(CaptureError::PixelByteLengthMismatch {
                expected,
                actual: rgba.len(),
            });
        }
        validate_scale_metadata(width, height, &scale_metadata)?;
        Ok(Self {
            width,
            height,
            row_stride,
            rgba,
            scale_metadata,
            provenance,
            execution: None,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn row_stride(&self) -> usize {
        self.row_stride
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub const fn scale_metadata(&self) -> &CaptureScaleMetadata {
        &self.scale_metadata
    }

    pub const fn provenance(&self) -> &CaptureProvenance {
        &self.provenance
    }

    pub const fn execution(&self) -> Option<&CaptureExecution> {
        self.execution.as_ref()
    }

    pub fn with_execution(mut self, execution: CaptureExecution) -> Self {
        self.execution = Some(execution);
        self
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum CaptureError {
    #[error("capture permission was denied{hint}", hint = format_hint(.settings_hint))]
    PermissionDenied { settings_hint: Option<String> },
    #[error("capture operation {operation} is unsupported: {reason}")]
    Unsupported {
        operation: &'static str,
        reason: String,
    },
    #[error("capture was cancelled by the user")]
    UserCancelled,
    #[error("display {0:?} is no longer attached")]
    DisplayNotFound(DisplayId),
    #[error("capture backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("capture backend failed: {0}")]
    Backend(String),
    #[error(transparent)]
    InvalidGeometry(#[from] GeometryError),
    #[error("captured image dimensions must be non-zero, got {width} by {height}")]
    InvalidImageDimensions { width: u32, height: u32 },
    #[error("captured RGBA row stride is too small: need at least {minimum}, got {actual}")]
    RowStrideTooSmall { minimum: usize, actual: usize },
    #[error("captured RGBA byte length mismatch: expected {expected}, got {actual}")]
    PixelByteLengthMismatch { expected: usize, actual: usize },
    #[error("captured image layout overflows the supported address space")]
    ImageLayoutOverflow,
    #[error("invalid capture scale segments: {0}")]
    InvalidScaleSegments(String),
}

fn validate_scale_metadata(
    width: u32,
    height: u32,
    metadata: &CaptureScaleMetadata,
) -> Result<(), CaptureError> {
    let CaptureScaleKind::Mixed(segments) = &metadata.kind else {
        return Ok(());
    };
    let mut covered = 0_u128;
    for (index, segment) in segments.iter().enumerate() {
        let rect = segment.image_rect();
        if rect.origin().x < 0
            || rect.origin().y < 0
            || rect.right() > i64::from(width)
            || rect.bottom() > i64::from(height)
        {
            return Err(CaptureError::InvalidScaleSegments(
                "segment is outside image bounds".into(),
            ));
        }
        if segments[..index]
            .iter()
            .any(|earlier| earlier.image_rect().intersection(rect).is_some())
        {
            return Err(CaptureError::InvalidScaleSegments(
                "segments overlap".into(),
            ));
        }
        if segments[..index]
            .iter()
            .any(|earlier| match (earlier.source(), segment.source()) {
                (CaptureScaleSource::Display(a), CaptureScaleSource::Display(b)) => {
                    a.display_id() == b.display_id() && a.scale_factor() != b.scale_factor()
                }
                _ => false,
            })
        {
            return Err(CaptureError::InvalidScaleSegments(
                "one source display reports conflicting scale factors".into(),
            ));
        }
        covered += u128::from(rect.size().width()) * u128::from(rect.size().height());
    }
    if covered != u128::from(width) * u128::from(height) {
        return Err(CaptureError::InvalidScaleSegments(
            "segments do not cover the image".into(),
        ));
    }
    Ok(())
}

fn format_hint(hint: &Option<String>) -> String {
    hint.as_ref()
        .map(|hint| format!(": {hint}"))
        .unwrap_or_default()
}
