#[cfg(unix)]
use std::fs::File;
use std::fs::{self};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::naming::{NameAllocator, NamingError};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicImageStore {
    primary_directory: PathBuf,
    recovery_directory: PathBuf,
}

impl AtomicImageStore {
    pub fn new(
        primary_directory: impl Into<PathBuf>,
        recovery_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            primary_directory: primary_directory.into(),
            recovery_directory: recovery_directory.into(),
        }
    }

    pub fn store(
        &self,
        allocator: &NameAllocator,
        bytes: &[u8],
    ) -> Result<StorageOutcome, StorageError> {
        match store_in(&self.primary_directory, allocator, bytes) {
            Ok(stored) => Ok(StorageOutcome::Stored(stored)),
            Err(primary_error) => match store_in(&self.recovery_directory, allocator, bytes) {
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
) -> Result<StoredImage, StorageError> {
    fs::create_dir_all(directory).map_err(|source| StorageError::CreateDirectory {
        path: directory.to_path_buf(),
        source,
    })?;
    let reservation = allocator.reserve(directory)?;
    let target = reservation.path().to_path_buf();
    let mut temporary = tempfile::Builder::new()
        .prefix(".newpic-image-")
        .tempfile_in(directory)
        .map_err(|source| StorageError::Write {
            path: target.clone(),
            source,
        })?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| StorageError::Write {
            path: target.clone(),
            source,
        })?;

    temporary
        .persist_noclobber(&target)
        .map_err(|error| StorageError::Publish {
            path: target.clone(),
            source: error.error,
        })?;
    sync_directory_best_effort(directory);
    drop(reservation);
    Ok(StoredImage { path: target })
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
