use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use newpic_core::naming::{NameAllocator, NamingContext};
use newpic_core::storage::{
    AtomicImageStore, FilesystemPublisher, NoReplacePublisher, PublishFailure, StorageError,
    StorageOutcome,
};
use tempfile::NamedTempFile;

fn allocator() -> NameAllocator {
    NameAllocator::new(
        "%c",
        3,
        "png",
        NamingContext::new(2026, 10, 22, 9, 7, 5, "studio-pc"),
    )
    .expect("valid allocator")
}

fn temporary_image_files(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(directory)
        .expect("read storage directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".newpic-image-"))
        })
        .collect()
}

#[test]
fn store_creates_the_destination_and_atomically_publishes_bytes() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("missing").join("NewPic");
    let recovery = temp.path().join("recovery");
    let store = AtomicImageStore::new(&primary, &recovery);

    let outcome = store
        .store(&allocator(), b"encoded png bytes")
        .expect("store image");

    let stored = match outcome {
        StorageOutcome::Stored(stored) => stored,
        StorageOutcome::Recovered(_) => panic!("primary directory should be writable"),
    };
    assert_eq!(stored.path, primary.join("001.png"));
    assert_eq!(fs::read(&stored.path).unwrap(), b"encoded png bytes");
    assert!(temporary_image_files(&primary).is_empty());
}

#[test]
fn primary_failure_falls_back_to_the_recovery_directory() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("not-a-directory");
    fs::write(&primary, b"blocking file").expect("write blocking file");
    let recovery = temp.path().join("recovery");
    let store = AtomicImageStore::new(&primary, &recovery);

    let outcome = store
        .store(&allocator(), b"recover this image")
        .expect("recover image");

    let recovered = match outcome {
        StorageOutcome::Recovered(recovered) => recovered,
        StorageOutcome::Stored(_) => panic!("invalid primary must use recovery"),
    };
    assert_eq!(recovered.path, recovery.join("001.png"));
    assert_eq!(fs::read(&recovered.path).unwrap(), b"recover this image");
    assert!(recovered
        .primary_error
        .to_string()
        .contains("not-a-directory"));
    assert!(temporary_image_files(&recovery).is_empty());
}

#[test]
fn existing_target_bytes_are_preserved_and_next_name_is_used() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    fs::create_dir_all(&primary).expect("create destination");
    fs::write(primary.join("001.png"), b"existing bytes").expect("write existing target");
    fs::write(primary.join("003.png"), b"later bytes").expect("write later target");
    let store = AtomicImageStore::new(&primary, temp.path().join("recovery"));

    let outcome = store
        .store(&allocator(), b"new bytes")
        .expect("store next image");

    assert!(matches!(outcome, StorageOutcome::Stored(_)));
    assert_eq!(
        fs::read(primary.join("001.png")).unwrap(),
        b"existing bytes"
    );
    assert_eq!(fs::read(primary.join("002.png")).unwrap(), b"new bytes");
    assert_eq!(fs::read(primary.join("003.png")).unwrap(), b"later bytes");
}

#[test]
fn occupied_fixed_target_is_typed_unavailable_and_never_uses_recovery() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    let recovery = temp.path().join("recovery");
    fs::create_dir_all(&primary).expect("create destination");
    let target = primary.join("capture.png");
    fs::write(&target, b"sentinel bytes").expect("write sentinel");
    let fixed = NameAllocator::new(
        "capture",
        3,
        "png",
        NamingContext::new(2026, 10, 22, 9, 7, 5, "studio-pc"),
    )
    .expect("valid allocator");
    let store = AtomicImageStore::new(&primary, &recovery);

    let outcome = store.store(&fixed, b"replacement bytes");

    assert!(matches!(
        outcome,
        Err(StorageError::NameUnavailable { path }) if path == target
    ));
    assert_eq!(fs::read(target).unwrap(), b"sentinel bytes");
    assert!(!recovery.exists());
    assert!(temporary_image_files(&primary).is_empty());
}

#[test]
fn occupied_date_only_target_is_typed_unavailable() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    fs::create_dir_all(&primary).expect("create destination");
    let target = primary.join("capture-2026-10-22.png");
    fs::write(&target, b"existing").expect("write target");
    let date_only = NameAllocator::new(
        "capture-%Y-%m-%d",
        3,
        "png",
        NamingContext::new(2026, 10, 22, 9, 7, 5, "studio-pc"),
    )
    .expect("valid allocator");

    let result = AtomicImageStore::new(&primary, temp.path().join("recovery"))
        .store(&date_only, b"replacement");

    assert!(matches!(
        result,
        Err(StorageError::NameUnavailable { path }) if path == target
    ));
    assert_eq!(fs::read(target).unwrap(), b"existing");
}

#[test]
fn stale_legacy_claim_does_not_block_sequence_fixed_or_date_names() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    fs::create_dir_all(&primary).expect("create destination");
    fs::write(primary.join(".newpic-001.png.reserve"), b"stale").unwrap();
    fs::write(primary.join(".newpic-capture.png.reserve"), b"stale").unwrap();
    fs::write(
        primary.join(".newpic-capture-2026-10-22.png.reserve"),
        b"stale",
    )
    .unwrap();
    let store = AtomicImageStore::new(&primary, temp.path().join("recovery"));

    let sequence = store.store(&allocator(), b"sequence").unwrap();
    let fixed = NameAllocator::new(
        "capture",
        3,
        "png",
        NamingContext::new(2026, 10, 22, 9, 7, 5, "studio-pc"),
    )
    .unwrap();
    let fixed = store.store(&fixed, b"fixed").unwrap();
    let date = NameAllocator::new(
        "capture-%Y-%m-%d",
        3,
        "png",
        NamingContext::new(2026, 10, 22, 9, 7, 5, "studio-pc"),
    )
    .unwrap();
    let date = store.store(&date, b"date").unwrap();

    assert!(matches!(sequence, StorageOutcome::Stored(_)));
    assert!(matches!(fixed, StorageOutcome::Stored(_)));
    assert!(matches!(date, StorageOutcome::Stored(_)));
    assert_eq!(fs::read(primary.join("001.png")).unwrap(), b"sequence");
    assert_eq!(fs::read(primary.join("capture.png")).unwrap(), b"fixed");
    assert_eq!(
        fs::read(primary.join("capture-2026-10-22.png")).unwrap(),
        b"date"
    );
}

#[derive(Debug)]
struct CollisionOncePublisher {
    calls: AtomicUsize,
}

impl NoReplacePublisher for CollisionOncePublisher {
    fn publish(&self, temporary: NamedTempFile, target: &Path) -> Result<(), PublishFailure> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            fs::write(target, b"racing writer").expect("inject target collision");
            Err(PublishFailure::collision(temporary))
        } else {
            FilesystemPublisher.publish(temporary, target)
        }
    }
}

#[test]
fn publish_collision_retries_the_next_sequence_in_primary() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    let store = AtomicImageStore::with_publisher(
        &primary,
        temp.path().join("recovery"),
        Arc::new(CollisionOncePublisher {
            calls: AtomicUsize::new(0),
        }),
    );

    let outcome = store
        .store(&allocator(), b"new bytes")
        .expect("retry collision");

    let stored = match outcome {
        StorageOutcome::Stored(stored) => stored,
        StorageOutcome::Recovered(_) => panic!("collision must not use recovery"),
    };
    assert_eq!(stored.path, primary.join("002.png"));
    assert_eq!(fs::read(primary.join("001.png")).unwrap(), b"racing writer");
    assert_eq!(fs::read(primary.join("002.png")).unwrap(), b"new bytes");
    assert!(temporary_image_files(&primary).is_empty());
}

#[derive(Debug)]
struct IoFailureOncePublisher {
    calls: AtomicUsize,
}

impl NoReplacePublisher for IoFailureOncePublisher {
    fn publish(&self, temporary: NamedTempFile, target: &Path) -> Result<(), PublishFailure> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(PublishFailure::io(
                temporary,
                io::Error::new(io::ErrorKind::PermissionDenied, "injected publish failure"),
            ))
        } else {
            FilesystemPublisher.publish(temporary, target)
        }
    }
}

#[test]
fn post_temp_publish_io_failure_cleans_primary_temp_and_uses_recovery() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    let recovery = temp.path().join("recovery");
    let store = AtomicImageStore::with_publisher(
        &primary,
        &recovery,
        Arc::new(IoFailureOncePublisher {
            calls: AtomicUsize::new(0),
        }),
    );

    let outcome = store
        .store(&allocator(), b"recover me")
        .expect("recover image");

    assert!(matches!(outcome, StorageOutcome::Recovered(_)));
    assert!(temporary_image_files(&primary).is_empty());
    assert!(temporary_image_files(&recovery).is_empty());
    assert_eq!(fs::read(recovery.join("001.png")).unwrap(), b"recover me");
}

#[test]
fn concurrent_stores_publish_distinct_complete_files() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    let store = Arc::new(AtomicImageStore::new(
        &primary,
        temp.path().join("recovery"),
    ));
    let allocator = Arc::new(allocator());
    let barrier = Arc::new(Barrier::new(16));

    let handles: Vec<_> = (0..16)
        .map(|index| {
            let store = Arc::clone(&store);
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let bytes = format!("complete image {index}").into_bytes();
                barrier.wait();
                let outcome = store.store(&allocator, &bytes).expect("store image");
                (outcome, bytes)
            })
        })
        .collect();

    let mut paths = std::collections::HashSet::new();
    for handle in handles {
        let (outcome, bytes) = handle.join().expect("storage thread");
        let path = match outcome {
            StorageOutcome::Stored(stored) => stored.path,
            StorageOutcome::Recovered(_) => panic!("primary directory should remain writable"),
        };
        assert!(paths.insert(path.clone()), "published paths must be unique");
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    assert!(temporary_image_files(&primary).is_empty());
}

#[test]
fn subprocess_store_worker() {
    let Ok(directory) = std::env::var("NEWPIC_SUBPROCESS_STORE_DIR") else {
        return;
    };
    AtomicImageStore::new(&directory, format!("{directory}-recovery"))
        .store(&allocator(), std::process::id().to_string().as_bytes())
        .expect("subprocess store");
}

#[test]
fn concurrent_subprocesses_publish_distinct_files() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let directory = temp.path().join("captures");
    let current_exe = std::env::current_exe().expect("current test executable");
    let children: Vec<_> = (0..8)
        .map(|_| {
            Command::new(&current_exe)
                .args(["--exact", "subprocess_store_worker"])
                .env("NEWPIC_SUBPROCESS_STORE_DIR", &directory)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn storage worker")
        })
        .collect();

    for mut child in children {
        assert!(child.wait().expect("wait for worker").success());
    }
    let published: Vec<_> = fs::read_dir(&directory)
        .expect("read captures")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .collect();
    assert_eq!(published.len(), 8);
    assert!(temporary_image_files(&directory).is_empty());
}
