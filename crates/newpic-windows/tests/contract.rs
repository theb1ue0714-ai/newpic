use newpic_platform::{
    capabilities::{CapabilityStatus, CaptureBackend},
    capture::{CaptureOptions, CaptureProvider, CaptureRequest, CaptureSource},
};
use newpic_windows::{WindowsBackendFallback, WindowsCaptureProvider};

#[test]
fn default_provider_reports_wgc_and_truthful_window_fallback_capabilities() {
    let provider = WindowsCaptureProvider::new();
    let capabilities = provider.capabilities();

    assert_eq!(
        capabilities.backend(),
        CaptureBackend::WindowsGraphicsCapture
    );
    assert!(matches!(
        capabilities.active_window_capture(),
        CapabilityStatus::Limited { .. }
    ));
    assert!(matches!(
        capabilities.pointer_capture(),
        CapabilityStatus::Unsupported { .. }
    ));
}

/// Opt in on an unlocked Windows desktop with:
/// `cargo test -p newpic-windows --test contract interactive_ -- --ignored --nocapture`
#[test]
#[ignore = "requires a real interactive Windows desktop"]
fn interactive_provider_lists_displays_and_returns_valid_rgba() {
    let provider = WindowsCaptureProvider::new();
    let displays = provider
        .displays()
        .expect("interactive display enumeration");
    assert!(
        !displays.is_empty(),
        "an interactive session must expose a display"
    );
    let detailed = provider
        .capture_detailed(CaptureRequest::new(
            CaptureSource::Display(displays[0].id().clone()),
            CaptureOptions::default(),
        ))
        .expect("interactive display capture");
    match detailed.backend() {
        CaptureBackend::WindowsGraphicsCapture => assert!(detailed.fallback().is_none()),
        CaptureBackend::WindowsGdi => assert!(matches!(
            detailed.fallback(),
            Some(WindowsBackendFallback::WgcFailed { .. })
        )),
        backend => panic!("unexpected Windows capture backend: {backend:?}"),
    }
    let image = detailed.image();
    assert_eq!(
        image.rgba().len(),
        image.row_stride() * image.height() as usize
    );
}
