use newpic_linux::{LinuxCaptureProvider, LinuxSession};
use newpic_platform::{
    capabilities::{CapabilityStatus, CaptureBackend},
    capture::{CaptureOptions, CaptureProvider, CaptureRequest, CaptureSource},
};

#[test]
fn x11_capabilities_are_explicit_and_pointer_request_is_not_silently_ignored() {
    let provider = LinuxCaptureProvider::for_session(LinuxSession::X11);
    let capabilities = provider.capabilities();

    assert_eq!(capabilities.backend(), CaptureBackend::X11);
    assert_eq!(capabilities.region_capture(), &CapabilityStatus::Supported);
    assert!(matches!(
        capabilities.pointer_capture(),
        CapabilityStatus::Unsupported { .. } | CapabilityStatus::Limited { .. }
    ));
}

#[test]
fn wayland_capabilities_report_runtime_backend_consent_and_active_window_limit() {
    let provider = LinuxCaptureProvider::for_session(LinuxSession::Wayland);
    let capabilities = provider.capabilities();

    assert_eq!(capabilities.backend(), CaptureBackend::XdgDesktopPortal);
    assert!(matches!(
        capabilities.unattended_capture(),
        CapabilityStatus::Limited { .. }
    ));
    assert!(matches!(
        capabilities.active_window_capture(),
        CapabilityStatus::Unsupported { .. }
    ));
}

#[test]
fn wayland_portal_cancellation_is_user_cancellation() {
    assert_eq!(
        newpic_linux::classify_portal_error("Portal request was cancelled"),
        newpic_platform::capture::CaptureError::UserCancelled
    );
}

#[test]
fn wayland_portal_denial_is_permission_denied() {
    assert!(matches!(
        newpic_linux::classify_portal_error("Portal request failed: Not allowed"),
        newpic_platform::capture::CaptureError::PermissionDenied { .. }
    ));
}

/// Opt in on a graphical Linux desktop with:
/// `cargo test -p newpic-linux --test contract interactive_ -- --ignored --nocapture`
#[test]
#[ignore = "requires a real interactive X11/Wayland session and may show a portal"]
fn interactive_provider_lists_displays_and_returns_valid_rgba() {
    let provider = LinuxCaptureProvider::detect();
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
