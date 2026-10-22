//! Platform-neutral clipboard output contract.
//!
//! The pipeline only depends on [`ImageClipboard`]. The optional `arboard`
//! feature provides the native adapter used by the desktop application.

use std::fmt;

use thiserror::Error;

/// A tightly packed RGBA8 image ready to hand to a clipboard backend.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl ClipboardImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ClipboardImageError> {
        if width == 0 || height == 0 {
            return Err(ClipboardImageError::InvalidDimensions { width, height });
        }
        let expected = usize::try_from(width)
            .ok()
            .zip(usize::try_from(height).ok())
            .and_then(|(width, height)| width.checked_mul(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(ClipboardImageError::LayoutOverflow)?;
        if rgba.len() != expected {
            return Err(ClipboardImageError::PixelByteLengthMismatch {
                expected,
                actual: rgba.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl fmt::Debug for ClipboardImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Pixels are private capture data; never print them.
        formatter
            .debug_struct("ClipboardImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClipboardImageError {
    #[error("clipboard image dimensions must be non-zero, got {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("clipboard image layout overflows the address space")]
    LayoutOverflow,
    #[error("clipboard image expected {expected} RGBA bytes, got {actual}")]
    PixelByteLengthMismatch { expected: usize, actual: usize },
}

/// Classification used to decide whether a clipboard write may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardErrorKind {
    /// Another party temporarily holds the clipboard. Retrying may succeed.
    Busy,
    /// The session or compositor does not offer an image clipboard.
    Unsupported,
    /// The platform refused clipboard access.
    PermissionDenied,
    /// The image could not be converted to a native clipboard format.
    Conversion,
    /// Any other backend failure.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("clipboard write failed ({kind:?}): {message}")]
pub struct ClipboardError {
    kind: ClipboardErrorKind,
    message: String,
}

impl ClipboardError {
    /// A transient failure that the pipeline may retry within its bound.
    pub fn busy(message: impl Into<String>) -> Self {
        Self {
            kind: ClipboardErrorKind::Busy,
            message: message.into(),
        }
    }

    /// A failure that retrying cannot fix. `Busy` is normalized to `Other`
    /// so a permanent error is never retried by accident.
    pub fn permanent(kind: ClipboardErrorKind, message: impl Into<String>) -> Self {
        let kind = match kind {
            ClipboardErrorKind::Busy => ClipboardErrorKind::Other,
            kind => kind,
        };
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> ClipboardErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn is_transient(&self) -> bool {
        matches!(self.kind, ClipboardErrorKind::Busy)
    }
}

/// Output port for placing a captured image on the system clipboard.
pub trait ImageClipboard {
    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardError>;
}

#[cfg(feature = "arboard")]
pub use native::ArboardClipboard;

#[cfg(feature = "arboard")]
mod native {
    use std::borrow::Cow;
    use std::sync::Mutex;

    use super::{ClipboardError, ClipboardErrorKind, ClipboardImage, ImageClipboard};

    /// Native clipboard adapter backed by `arboard`.
    ///
    /// The connection is opened lazily and kept alive so that X11 and
    /// Wayland selections continue to be served after the write returns.
    #[derive(Default)]
    pub struct ArboardClipboard {
        connection: Mutex<Option<arboard::Clipboard>>,
    }

    impl ArboardClipboard {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl std::fmt::Debug for ArboardClipboard {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("ArboardClipboard")
                .finish_non_exhaustive()
        }
    }

    impl ImageClipboard for ArboardClipboard {
        fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardError> {
            let mut connection = self
                .connection
                .lock()
                .map_err(|_| ClipboardError::busy("clipboard adapter lock was poisoned"))?;
            if connection.is_none() {
                *connection = Some(arboard::Clipboard::new().map_err(map_error)?);
            }
            let (width, height) = image.dimensions();
            let data = arboard::ImageData {
                width: usize::try_from(width).map_err(|_| conversion_overflow())?,
                height: usize::try_from(height).map_err(|_| conversion_overflow())?,
                bytes: Cow::Borrowed(image.rgba()),
            };
            let result = connection
                .as_mut()
                .expect("connection initialized above")
                .set_image(data)
                .map_err(map_error);
            if result.is_err() {
                // Reopen on the next attempt in case the connection went stale.
                *connection = None;
            }
            result
        }
    }

    fn conversion_overflow() -> ClipboardError {
        ClipboardError::permanent(
            ClipboardErrorKind::Conversion,
            "image dimensions exceed the platform size type",
        )
    }

    fn map_error(error: arboard::Error) -> ClipboardError {
        match error {
            arboard::Error::ClipboardOccupied => ClipboardError::busy(error.to_string()),
            arboard::Error::ClipboardNotSupported => {
                ClipboardError::permanent(ClipboardErrorKind::Unsupported, error.to_string())
            }
            arboard::Error::ConversionFailure => {
                ClipboardError::permanent(ClipboardErrorKind::Conversion, error.to_string())
            }
            other => ClipboardError::permanent(ClipboardErrorKind::Other, other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_image_requires_tightly_packed_rgba() {
        assert!(ClipboardImage::new(2, 1, vec![0; 8]).is_ok());
        assert_eq!(
            ClipboardImage::new(2, 1, vec![0; 12]),
            Err(ClipboardImageError::PixelByteLengthMismatch {
                expected: 8,
                actual: 12
            })
        );
        assert_eq!(
            ClipboardImage::new(0, 1, Vec::new()),
            Err(ClipboardImageError::InvalidDimensions {
                width: 0,
                height: 1
            })
        );
    }

    #[test]
    fn permanent_constructor_never_yields_a_retryable_error() {
        let error = ClipboardError::permanent(ClipboardErrorKind::Busy, "held");
        assert_eq!(error.kind(), ClipboardErrorKind::Other);
        assert!(!error.is_transient());
        assert!(ClipboardError::busy("held").is_transient());
    }

    #[test]
    fn debug_output_omits_pixel_data() {
        let image = ClipboardImage::new(1, 1, vec![11, 22, 33, 44]).unwrap();
        let debug = format!("{image:?}");
        assert!(!debug.contains("22"), "{debug}");
        assert!(debug.contains("bytes: 4"), "{debug}");
    }
}
