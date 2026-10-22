use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use newpic_core::naming::{NameAllocator, NamingContext};
use newpic_core::storage::{AtomicImageStore, StorageOutcome};

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

fn reservation_files(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(directory)
        .expect("read storage directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".reserve"))
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
    assert!(reservation_files(&primary).is_empty());
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
    assert!(reservation_files(&recovery).is_empty());
}

#[test]
fn existing_target_bytes_are_preserved_and_next_name_is_used() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let primary = temp.path().join("captures");
    fs::create_dir_all(&primary).expect("create destination");
    fs::write(primary.join("001.png"), b"existing bytes").expect("write existing target");
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
}

#[test]
fn failed_publish_cleans_its_temporary_sibling_and_preserves_existing_bytes() {
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

    let outcome = store
        .store(&fixed, b"replacement bytes")
        .expect("recover after occupied fixed target");

    assert!(matches!(outcome, StorageOutcome::Recovered(_)));
    assert_eq!(fs::read(target).unwrap(), b"sentinel bytes");
    assert!(temporary_image_files(&primary).is_empty());
    assert!(temporary_image_files(&recovery).is_empty());
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
    assert!(reservation_files(&primary).is_empty());
}
