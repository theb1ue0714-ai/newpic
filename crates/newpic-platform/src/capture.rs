use thiserror::Error;

use crate::capabilities::CaptureCapabilities;
use crate::geometry::{Display, DisplayId, GeometryError, PhysicalRect, ScaleFactor};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureScaleSegment {
    image_rect: PhysicalRect,
    source_display: DisplayId,
    scale_factor: ScaleFactor,
}

impl CaptureScaleSegment {
    pub const fn new(
        image_rect: PhysicalRect,
        source_display: DisplayId,
        scale_factor: ScaleFactor,
    ) -> Self {
        Self {
            image_rect,
            source_display,
            scale_factor,
        }
    }

    pub const fn image_rect(&self) -> PhysicalRect {
        self.image_rect
    }
    pub const fn source_display(&self) -> &DisplayId {
        &self.source_display
    }
    pub const fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureScaleKind {
    Uniform(ScaleFactor),
    Mixed(Vec<CaptureScaleSegment>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureScaleMetadata {
    kind: CaptureScaleKind,
}

impl CaptureScaleMetadata {
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
        if segments
            .iter()
            .all(|segment| segment.scale_factor == segments[0].scale_factor)
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
            CaptureScaleKind::Uniform(_) => None,
            CaptureScaleKind::Mixed(segments) => Some(segments),
        }
    }

    pub const fn uniform_scale(&self) -> Option<ScaleFactor> {
        match &self.kind {
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
        if segments[..index].iter().any(|earlier| {
            earlier.source_display() == segment.source_display()
                && earlier.scale_factor() != segment.scale_factor()
        }) {
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
