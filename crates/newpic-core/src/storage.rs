#[cfg(unix)]
use std::fs::File;
use std::fs::{self};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

use crate::naming::{NameAllocator, NamingError};
use tempfile::NamedTempFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredImage {
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct RecoveredImage {
    pub path: PathBuf,
    pub primary_error: StorageError,
}

#[derive(Debug)]
pub enum StorageOutcome {
    Stored(StoredImage),
    Recovered(RecoveredImage),
}

#[derive(Clone)]
pub struct AtomicImageStore {
    primary_directory: PathBuf,
    recovery_directory: PathBuf,
    publisher: Arc<dyn NoReplacePublisher>,
}

impl std::fmt::Debug for AtomicImageStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AtomicImageStore")
            .field("primary_directory", &self.primary_directory)
            .field("recovery_directory", &self.recovery_directory)
            .finish_non_exhaustive()
    }
}

impl AtomicImageStore {
    pub fn new(
        primary_directory: impl Into<PathBuf>,
        recovery_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            primary_directory: primary_directory.into(),
            recovery_directory: recovery_directory.into(),
            publisher: Arc::new(FilesystemPublisher),
        }
    }

    pub fn with_publisher(
        primary_directory: impl Into<PathBuf>,
        recovery_directory: impl Into<PathBuf>,
        publisher: Arc<dyn NoReplacePublisher>,
    ) -> Self {
        Self {
            primary_directory: primary_directory.into(),
            recovery_directory: recovery_directory.into(),
            publisher,
        }
    }

    pub fn store(
        &self,
        allocator: &NameAllocator,
        bytes: &[u8],
    ) -> Result<StorageOutcome, StorageError> {
        match store_in(
            &self.primary_directory,
            allocator,
            bytes,
            self.publisher.as_ref(),
        ) {
            Ok(stored) => Ok(StorageOutcome::Stored(stored)),
            Err(primary_error @ StorageError::NameUnavailable { .. })
            | Err(primary_error @ StorageError::Naming(_)) => Err(primary_error),
            Err(primary_error) => match store_in(
                &self.recovery_directory,
                allocator,
                bytes,
                self.publisher.as_ref(),
            ) {
                Ok(stored) => Ok(StorageOutcome::Recovered(RecoveredImage {
                    path: stored.path,
                    primary_error,
                })),
                Err(recovery_error) => Err(StorageError::RecoveryFailed {
                    primary: Box::new(primary_error),
                    recovery: Box::new(recovery_error),
                }),
            },
        }
    }
}

fn store_in(
    directory: &Path,
    allocator: &NameAllocator,
    bytes: &[u8],
    publisher: &dyn NoReplacePublisher,
) -> Result<StoredImage, StorageError> {
    fs::create_dir_all(directory).map_err(|source| StorageError::CreateDirectory {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".newpic-image-")
        .tempfile_in(directory)
        .map_err(|source| StorageError::Write {
            path: directory.to_path_buf(),
            source,
        })?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| StorageError::Write {
            path: directory.to_path_buf(),
            source,
        })?;

    let mut sequence = 1_u64;
    loop {
        let target = allocator.path_for(directory, sequence)?;
        match publisher.publish(temporary, &target) {
            Ok(()) => {
                sync_directory_best_effort(directory);
                return Ok(StoredImage { path: target });
            }
            Err(failure) if failure.is_collision() => {
                temporary = failure.into_temporary();
                if !allocator.has_sequence() {
                    return Err(StorageError::NameUnavailable { path: target });
                }
                sequence = sequence
                    .checked_add(1)
                    .ok_or(NamingError::SequenceExhausted)?;
            }
            Err(failure) => {
                let (temporary_file, source) = failure.into_io_parts();
                drop(temporary_file);
                return Err(StorageError::Publish {
                    path: target,
                    source,
                });
            }
        }
    }
}

pub trait NoReplacePublisher: std::fmt::Debug + Send + Sync {
    /// Atomically publishes `temporary` at `target` without replacing a target
    /// that already exists. The temporary file is returned on every failure.
    fn publish(&self, temporary: NamedTempFile, target: &Path) -> Result<(), PublishFailure>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FilesystemPublisher;

impl NoReplacePublisher for FilesystemPublisher {
    fn publish(&self, temporary: NamedTempFile, target: &Path) -> Result<(), PublishFailure> {
        // Pinned tempfile 3.27 uses no-replace rename primitives on supported
        // Linux/macOS/Windows targets, with an atomic hard-link publication
        // fallback on Unix. In every supported path the completed temporary
        // file becomes visible without replacing an existing target.
        temporary
            .persist_noclobber(target)
            .map(|_| ())
            .map_err(|error| {
                if error.error.kind() == io::ErrorKind::AlreadyExists {
                    PublishFailure::collision(error.file)
                } else {
                    PublishFailure::io(error.file, error.error)
                }
            })
    }
}

#[derive(Debug)]
pub struct PublishFailure {
    temporary: NamedTempFile,
    kind: PublishFailureKind,
}

#[derive(Debug)]
enum PublishFailureKind {
    Collision,
    Io(io::Error),
}

impl PublishFailure {
    pub fn collision(temporary: NamedTempFile) -> Self {
        Self {
            temporary,
            kind: PublishFailureKind::Collision,
        }
    }

    pub fn io(temporary: NamedTempFile, source: io::Error) -> Self {
        Self {
            temporary,
            kind: PublishFailureKind::Io(source),
        }
    }

    fn is_collision(&self) -> bool {
        matches!(self.kind, PublishFailureKind::Collision)
    }

    fn into_temporary(self) -> NamedTempFile {
        self.temporary
    }

    fn into_io_parts(self) -> (NamedTempFile, io::Error) {
        match self.kind {
            PublishFailureKind::Io(source) => (self.temporary, source),
            PublishFailureKind::Collision => {
                unreachable!("collision failures are handled before I/O extraction")
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("could not create storage directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Naming(#[from] NamingError),
    #[error("filename is already in use and has no sequence token: {path}")]
    NameUnavailable { path: PathBuf },
    #[error("could not write temporary image for {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not publish image without overwriting {path}: {source}")]
    Publish {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("primary storage failed ({primary}); recovery storage also failed ({recovery})")]
    RecoveryFailed {
        primary: Box<StorageError>,
        recovery: Box<StorageError>,
    },
}

#[cfg(unix)]
fn sync_directory_best_effort(path: &Path) {
    let _ = File::open(path).and_then(|directory| directory.sync_all());
}

#[cfg(not(unix))]
fn sync_directory_best_effort(_path: &Path) {}
