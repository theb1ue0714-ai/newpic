use thiserror::Error;

use crate::capabilities::CaptureCapabilities;
use crate::geometry::{Display, DisplayId, GeometryError, PhysicalRect};

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

/// Pixel density metadata retained with captured physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelScale {
    horizontal: f64,
    vertical: f64,
}

impl PixelScale {
    pub const ONE: Self = Self {
        horizontal: 1.0,
        vertical: 1.0,
    };

    pub fn new(horizontal: f64, vertical: f64) -> Result<Self, CaptureError> {
        if !horizontal.is_finite() || horizontal <= 0.0 || !vertical.is_finite() || vertical <= 0.0
        {
            return Err(CaptureError::InvalidPixelScale {
                horizontal,
                vertical,
            });
        }
        Ok(Self {
            horizontal,
            vertical,
        })
    }

    pub const fn horizontal(self) -> f64 {
        self.horizontal
    }

    pub const fn vertical(self) -> f64 {
        self.vertical
    }
}

/// An owned, row-major RGBA8 image returned by every capture backend.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedImage {
    width: u32,
    height: u32,
    row_stride: usize,
    rgba: Vec<u8>,
    scale: PixelScale,
    provenance: CaptureProvenance,
}

impl CapturedImage {
    pub fn new(
        width: u32,
        height: u32,
        row_stride: usize,
        rgba: Vec<u8>,
        scale: PixelScale,
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
        Ok(Self {
            width,
            height,
            row_stride,
            rgba,
            scale,
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

    pub const fn scale(&self) -> PixelScale {
        self.scale
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
    #[error("pixel scale must be finite and positive, got {horizontal} by {vertical}")]
    InvalidPixelScale { horizontal: f64, vertical: f64 },
}

fn format_hint(hint: &Option<String>) -> String {
    hint.as_ref()
        .map(|hint| format!(": {hint}"))
        .unwrap_or_default()
}
