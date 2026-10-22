//! Encoding of captured RGBA8 pixels into saved image formats.

use std::borrow::Cow;
use std::fmt;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use newpic_platform::capture::CapturedImage;
use thiserror::Error;

/// JPEG quality validated to the inclusive range 1 through 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegQuality(u8);

impl JpegQuality {
    pub fn new(quality: u8) -> Result<Self, ImageOutputError> {
        if (1..=100).contains(&quality) {
            Ok(Self(quality))
        } else {
            Err(ImageOutputError::InvalidJpegQuality(quality))
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEncoding {
    Png,
    Jpeg(JpegQuality),
}

impl ImageEncoding {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg(_) => "jpg",
        }
    }
}

/// Encoded file bytes together with the encoding that produced them.
#[derive(Clone, PartialEq, Eq)]
pub struct EncodedImage {
    bytes: Vec<u8>,
    encoding: ImageEncoding,
}

impl EncodedImage {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub const fn encoding(&self) -> ImageEncoding {
        self.encoding
    }

    pub const fn extension(&self) -> &'static str {
        self.encoding.extension()
    }
}

impl fmt::Debug for EncodedImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedImage")
            .field("encoding", &self.encoding)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ImageOutputError {
    #[error("JPEG quality must be in the range 1 through 100, got {0}")]
    InvalidJpegQuality(u8),
    #[error("could not encode {format} image: {source}")]
    Encode {
        format: &'static str,
        #[source]
        source: image::ImageError,
    },
}

/// Returns the image's pixels as tightly packed rows, dropping any row
/// padding. Borrows when the capture has no padding; never mutates it.
pub fn packed_rgba(image: &CapturedImage) -> Cow<'_, [u8]> {
    let row_bytes = image.width() as usize * 4;
    let stride = image.row_stride();
    if stride == row_bytes {
        return Cow::Borrowed(image.rgba());
    }
    let mut packed = Vec::with_capacity(row_bytes * image.height() as usize);
    for row in image.rgba().chunks_exact(stride) {
        packed.extend_from_slice(&row[..row_bytes]);
    }
    Cow::Owned(packed)
}

/// Encodes a capture. JPEG has no alpha channel, so alpha is discarded;
/// captured screen pixels are opaque in practice.
pub fn encode_image(
    image: &CapturedImage,
    encoding: ImageEncoding,
) -> Result<EncodedImage, ImageOutputError> {
    let pixels = packed_rgba(image);
    let (width, height) = (image.width(), image.height());
    let mut bytes = Vec::new();
    match encoding {
        ImageEncoding::Png => PngEncoder::new(&mut bytes)
            .write_image(&pixels, width, height, ExtendedColorType::Rgba8)
            .map_err(|source| ImageOutputError::Encode {
                format: "PNG",
                source,
            })?,
        ImageEncoding::Jpeg(quality) => {
            let rgb: Vec<u8> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|&[red, green, blue, _alpha]| [red, green, blue])
                .collect();
            JpegEncoder::new_with_quality(&mut bytes, quality.get())
                .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
                .map_err(|source| ImageOutputError::Encode {
                    format: "JPEG",
                    source,
                })?
        }
    }
    Ok(EncodedImage { bytes, encoding })
}
