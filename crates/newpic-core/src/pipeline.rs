//! Delivery of a capture to file storage and the clipboard.
//!
//! Both outputs are always attempted. Each outcome is reported
//! independently so that a clipboard failure never hides a saved file and a
//! storage failure never hides a successful clipboard write.

use std::error::Error as StdError;
use std::fmt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use newpic_platform::capture::CapturedImage;
use newpic_platform::clipboard::{
    ClipboardError, ClipboardErrorKind, ClipboardImage, ImageClipboard,
};
use thiserror::Error;

use crate::image_output::{encode_image, packed_rgba, ImageEncoding, ImageOutputError};
use crate::naming::NameAllocator;
use crate::storage::{AtomicImageStore, RecoveredImage, StorageError, StorageOutcome};

/// Where the encoded bytes were written.
#[derive(Debug)]
pub enum StoredOutput {
    Stored {
        path: PathBuf,
    },
    /// The primary location failed and the recovery folder was used.
    Recovered(RecoveredImage),
}

impl StoredOutput {
    /// The path of the file that now holds the capture.
    pub fn path(&self) -> &Path {
        match self {
            Self::Stored { path } => path,
            Self::Recovered(recovered) => &recovered.path,
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ImageStoreError {
    message: String,
    #[source]
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl ImageStoreError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        message: impl Into<String>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl From<StorageError> for ImageStoreError {
    fn from(error: StorageError) -> Self {
        Self::with_source(error.to_string(), error)
    }
}

/// Output port for persisting encoded image bytes.
pub trait ImageStore {
    fn store(&self, bytes: &[u8]) -> Result<StoredOutput, ImageStoreError>;
}

/// [`ImageStore`] backed by the atomic, no-overwrite filesystem store.
#[derive(Debug, Clone)]
pub struct FileImageStore {
    store: AtomicImageStore,
    allocator: NameAllocator,
}

impl FileImageStore {
    pub fn new(store: AtomicImageStore, allocator: NameAllocator) -> Self {
        Self { store, allocator }
    }
}

impl ImageStore for FileImageStore {
    fn store(&self, bytes: &[u8]) -> Result<StoredOutput, ImageStoreError> {
        match self.store.store(&self.allocator, bytes)? {
            StorageOutcome::Stored(stored) => Ok(StoredOutput::Stored { path: stored.path }),
            StorageOutcome::Recovered(recovered) => Ok(StoredOutput::Recovered(recovered)),
        }
    }
}

/// Why the file output did not produce a file.
#[derive(Debug, Error)]
pub enum StorageFailure {
    #[error(transparent)]
    Encode(#[from] ImageOutputError),
    #[error("could not store image: {0}")]
    Store(#[from] ImageStoreError),
}

/// A clipboard write that did not succeed within the retry bound.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{error} after {attempts} attempt(s)")]
pub struct ClipboardFailure {
    error: ClipboardError,
    attempts: u32,
}

impl ClipboardFailure {
    pub const fn error(&self) -> &ClipboardError {
        &self.error
    }

    pub const fn attempts(&self) -> u32 {
        self.attempts
    }
}

/// Independent outcomes of one delivery where at least one output succeeded.
#[derive(Debug)]
pub struct DeliveryReport {
    encoding: ImageEncoding,
    storage: Result<StoredOutput, StorageFailure>,
    clipboard: Result<u32, ClipboardFailure>,
}

impl DeliveryReport {
    /// True when both outputs succeeded. A recovery-folder save counts as a
    /// successful save; inspect [`Self::recovered`] to warn the user.
    pub fn is_complete(&self) -> bool {
        self.storage.is_ok() && self.clipboard.is_ok()
    }

    pub const fn encoding(&self) -> ImageEncoding {
        self.encoding
    }

    pub fn stored(&self) -> Option<&StoredOutput> {
        self.storage.as_ref().ok()
    }

    /// The file holding the capture, including a recovery-folder path.
    pub fn stored_path(&self) -> Option<&Path> {
        self.stored().map(StoredOutput::path)
    }

    pub fn recovered(&self) -> Option<&RecoveredImage> {
        match self.stored() {
            Some(StoredOutput::Recovered(recovered)) => Some(recovered),
            _ => None,
        }
    }

    pub fn storage_error(&self) -> Option<&StorageFailure> {
        self.storage.as_ref().err()
    }

    /// Attempts used by a successful clipboard write.
    pub fn clipboard_attempts(&self) -> Option<u32> {
        self.clipboard.as_ref().ok().copied()
    }

    pub fn clipboard_failure(&self) -> Option<&ClipboardFailure> {
        self.clipboard.as_ref().err()
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DeliveryError {
    /// Both outputs were attempted and both failed.
    #[error("capture was not delivered: {storage}; {clipboard}")]
    Outputs {
        storage: StorageFailure,
        clipboard: ClipboardFailure,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("clipboard retry policy needs at least one attempt")]
pub struct InvalidRetryPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RetryPolicy {
    max_attempts: u32,
    delay: Duration,
}

const DEFAULT_RETRY: RetryPolicy = RetryPolicy {
    max_attempts: 3,
    delay: Duration::from_millis(50),
};

/// Delivers captures to an [`ImageStore`] and an [`ImageClipboard`].
pub struct CapturePipeline<'a> {
    store: &'a dyn ImageStore,
    clipboard: &'a dyn ImageClipboard,
    retry: RetryPolicy,
}

impl fmt::Debug for CapturePipeline<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapturePipeline")
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl<'a> CapturePipeline<'a> {
    pub fn new(store: &'a dyn ImageStore, clipboard: &'a dyn ImageClipboard) -> Self {
        Self {
            store,
            clipboard,
            retry: DEFAULT_RETRY,
        }
    }

    /// Bounds transient (`Busy`) clipboard retries to `max_attempts` total
    /// writes, sleeping `delay` between attempts. Permanent errors are never
    /// retried.
    pub fn with_clipboard_retry(
        mut self,
        max_attempts: u32,
        delay: Duration,
    ) -> Result<Self, InvalidRetryPolicy> {
        if max_attempts == 0 {
            return Err(InvalidRetryPolicy);
        }
        self.retry = RetryPolicy {
            max_attempts,
            delay,
        };
        Ok(self)
    }

    /// Attempts both outputs and reports each outcome. Returns an error only
    /// when neither output succeeded. The capture buffer is never mutated.
    pub fn deliver(
        &self,
        image: &CapturedImage,
        encoding: ImageEncoding,
    ) -> Result<DeliveryReport, DeliveryError> {
        let storage = self.deliver_to_storage(image, encoding);
        let clipboard = self.deliver_to_clipboard(image);
        match (storage, clipboard) {
            (Err(storage), Err(clipboard)) => Err(DeliveryError::Outputs { storage, clipboard }),
            (storage, clipboard) => Ok(DeliveryReport {
                encoding,
                storage,
                clipboard,
            }),
        }
    }

    fn deliver_to_storage(
        &self,
        image: &CapturedImage,
        encoding: ImageEncoding,
    ) -> Result<StoredOutput, StorageFailure> {
        let encoded = encode_image(image, encoding)?;
        Ok(self.store.store(encoded.bytes())?)
    }

    fn deliver_to_clipboard(&self, image: &CapturedImage) -> Result<u32, ClipboardFailure> {
        let clipboard_image = ClipboardImage::new(
            image.width(),
            image.height(),
            packed_rgba(image).into_owned(),
        )
        .map_err(|error| ClipboardFailure {
            error: ClipboardError::permanent(ClipboardErrorKind::Conversion, error.to_string()),
            attempts: 0,
        })?;

        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.clipboard.write_image(&clipboard_image) {
                Ok(()) => return Ok(attempts),
                Err(error) if error.is_transient() && attempts < self.retry.max_attempts => {
                    if !self.retry.delay.is_zero() {
                        thread::sleep(self.retry.delay);
                    }
                }
                Err(error) => return Err(ClipboardFailure { error, attempts }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::NamingContext;

    #[test]
    fn zero_attempt_retry_policy_is_rejected() {
        struct Never;
        impl ImageStore for Never {
            fn store(&self, _: &[u8]) -> Result<StoredOutput, ImageStoreError> {
                unreachable!()
            }
        }
        impl ImageClipboard for Never {
            fn write_image(&self, _: &ClipboardImage) -> Result<(), ClipboardError> {
                unreachable!()
            }
        }
        let result = CapturePipeline::new(&Never, &Never).with_clipboard_retry(0, Duration::ZERO);
        assert_eq!(result.err(), Some(InvalidRetryPolicy));
    }

    #[test]
    fn file_image_store_publishes_bytes_with_allocated_name() {
        let root = tempfile::tempdir().unwrap();
        let allocator = NameAllocator::new(
            "%c",
            3,
            "png",
            NamingContext::new(2026, 10, 22, 9, 7, 5, "studio-pc"),
        )
        .unwrap();
        let store = FileImageStore::new(
            AtomicImageStore::new(root.path().join("primary"), root.path().join("recovery")),
            allocator,
        );

        let output = ImageStore::store(&store, b"bytes").unwrap();

        assert!(matches!(output, StoredOutput::Stored { .. }));
        assert_eq!(output.path(), root.path().join("primary").join("001.png"));
        assert_eq!(std::fs::read(output.path()).unwrap(), b"bytes");
    }
}
