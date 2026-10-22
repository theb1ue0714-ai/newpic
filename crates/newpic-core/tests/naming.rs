use std::collections::HashSet;

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
fn candidate_paths_are_deterministic_and_unique() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let allocator = allocator("%c").expect("valid allocator");
    let paths: HashSet<_> = (1..=16)
        .map(|sequence| allocator.path_for(temp.path(), sequence).unwrap())
        .collect();

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
        "COM¹",
        "com².txt",
        "CoM³",
        "LPT¹",
        "lpt².log",
        "LpT³",
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
fn filename_length_uses_the_target_limit_not_internal_temporary_prefixes() {
    let allowed = "a".repeat(251);
    let too_long = "a".repeat(252);

    assert!(allocator(&allowed).is_ok());
    assert!(matches!(
        allocator(&too_long),
        Err(NamingError::InvalidFilename(_))
    ));
}
