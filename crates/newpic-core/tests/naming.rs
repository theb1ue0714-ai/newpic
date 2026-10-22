use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use newpic_core::naming::{render_sequence, NameAllocator, NamingContext, NamingError};

fn context() -> NamingContext {
    NamingContext::new(2026, 10, 22, 9, 7, 5, "studio-pc")
}

fn allocator(pattern: &str) -> Result<NameAllocator, NamingError> {
    NameAllocator::new(pattern, 3, "png", context())
}

#[test]
fn sequence_starts_at_three_digits_and_expands_beyond_minimum_width() {
    assert_eq!(render_sequence(1, 3), "001");
    assert_eq!(render_sequence(999, 3), "999");
    assert_eq!(render_sequence(1000, 3), "1000");
}

#[test]
fn allocator_skips_existing_targets_and_sequence_gaps_are_reusable() {
    let temp = tempfile::tempdir().expect("temporary directory");
    fs::write(temp.path().join("001.png"), b"first").expect("write first capture");
    fs::write(temp.path().join("003.png"), b"third").expect("write third capture");

    let reservation = allocator("%c")
        .expect("valid allocator")
        .reserve(temp.path())
        .expect("reserve gap");

    assert_eq!(reservation.path(), temp.path().join("002.png"));
    assert_eq!(fs::read(temp.path().join("001.png")).unwrap(), b"first");
    assert_eq!(fs::read(temp.path().join("003.png")).unwrap(), b"third");
}

#[test]
fn concurrent_reservations_are_exclusive_and_unique() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let directory = temp.path().to_path_buf();
    let allocator = Arc::new(allocator("%c").expect("valid allocator"));
    let barrier = Arc::new(Barrier::new(16));

    let handles: Vec<_> = (0..16)
        .map(|_| {
            let directory = directory.clone();
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let reservation = allocator.reserve(&directory).expect("reserve name");
                let path = reservation.path().to_path_buf();
                barrier.wait();
                (path, reservation)
            })
        })
        .collect();

    let reservations: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("reservation thread"))
        .collect();
    let paths: HashSet<_> = reservations.iter().map(|(path, _)| path.clone()).collect();

    assert_eq!(paths.len(), 16);
}

#[test]
fn documented_date_time_host_and_percent_tokens_render_deterministically() {
    let rendered = allocator("capture-%Y-%m-%d_%H-%M-%S_%h_%%_%c")
        .expect("documented tokens")
        .render_filename(42)
        .expect("safe filename");

    assert_eq!(rendered, "capture-2026-10-22_09-07-05_studio-pc_%_042.png");
}

#[test]
fn unsupported_tokens_are_rejected() {
    assert!(matches!(
        allocator("capture-%q"),
        Err(NamingError::UnsupportedToken(token)) if token == "%q"
    ));
}

#[test]
fn path_separators_dot_names_controls_and_platform_invalid_names_are_rejected() {
    let invalid_patterns = [
        "folder/%c",
        "folder\\%c",
        ".",
        "..",
        "bad\0%c",
        "bad\n%c",
        "bad<%c",
        "bad>%c",
        "bad:%c",
        "bad\"%c",
        "bad|%c",
        "bad?%c",
        "bad*%c",
        "trailing.%c.",
        "trailing-%c ",
        "CON",
        "nul",
        "COM1",
        "LPT9.txt",
        "   ",
    ];

    for pattern in invalid_patterns {
        assert!(
            allocator(pattern).is_err(),
            "pattern {pattern:?} must not produce a portable filename"
        );
    }
}
#[test]
fn an_existing_fixed_target_is_never_reserved_or_overwritten() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let target = temp.path().join("capture.png");
    fs::write(&target, b"sentinel bytes").expect("write sentinel");

    let result = allocator("capture")
        .expect("valid fixed name")
        .reserve(temp.path());

    assert!(matches!(result, Err(NamingError::NameUnavailable)));
    assert_eq!(fs::read(target).unwrap(), b"sentinel bytes");
}
