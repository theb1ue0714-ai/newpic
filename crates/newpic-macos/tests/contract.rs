use newpic_macos::MacOsCaptureProvider;
use newpic_platform::{
    capabilities::{CapabilityStatus, CaptureBackend},
    capture::{CaptureOptions, CaptureProvider, CaptureRequest, CaptureSource},
};

#[test]
fn capabilities_report_screen_capture_kit_and_whole_window_limit() {
    let provider = MacOsCaptureProvider::new();
    let capabilities = provider.capabilities();

    assert_eq!(
        capabilities.backend(),
        CaptureBackend::MacOsScreenCaptureKit
    );
    assert!(matches!(
        capabilities.active_window_capture(),
        CapabilityStatus::Limited { .. }
    ));
    assert_eq!(capabilities.pointer_capture(), &CapabilityStatus::Supported);
}

#[test]
fn permission_failure_maps_to_typed_error_with_settings_deep_link() {
    let error =
        newpic_macos::classify_screen_capture_error("Screen Recording permission not granted");
    let newpic_platform::capture::CaptureError::PermissionDenied { settings_hint } = error else {
        panic!("expected permission denied");
    };
    assert!(settings_hint.unwrap().contains("x-apple.systempreferences"));
}

/// Opt in on an unlocked macOS desktop after granting Screen Recording with:
/// `cargo test -p newpic-macos --test contract interactive_ -- --ignored --nocapture`
#[test]
#[ignore = "requires a real interactive macOS desktop with Screen Recording permission"]
fn interactive_provider_lists_displays_and_returns_valid_rgba() {
    let provider = MacOsCaptureProvider::new();
    let displays = provider
        .displays()
        .expect("interactive display enumeration");
    assert!(
        !displays.is_empty(),
        "an interactive session must expose a display"
    );
    let image = provider
        .capture(CaptureRequest::new(
            CaptureSource::Display(displays[0].id().clone()),
            CaptureOptions::default(),
        ))
        .expect("interactive display capture");
    assert_eq!(
        image.rgba().len(),
        image.row_stride() * image.height() as usize
    );
}
