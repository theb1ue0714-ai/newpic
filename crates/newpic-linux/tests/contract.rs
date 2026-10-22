use newpic_linux::{LinuxCaptureProvider, LinuxSession};
use newpic_platform::{
    capabilities::{CapabilityStatus, CaptureBackend},
    capture::{
        CaptureError, CaptureOptions, CaptureProvenance, CaptureProvider, CaptureRequest,
        CaptureSource,
    },
    geometry::PhysicalRect,
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
    assert!(matches!(
        capabilities.display_capture(),
        CapabilityStatus::Unsupported { .. }
    ));
    assert!(matches!(
        capabilities.pointer_capture(),
        CapabilityStatus::Limited { .. }
    ));
}

#[test]
fn wayland_rejects_specific_display_without_opening_the_portal() {
    let provider = LinuxCaptureProvider::for_session(LinuxSession::Wayland);
    let request = CaptureRequest::new(
        CaptureSource::Display(newpic_platform::geometry::DisplayId::new("display").unwrap()),
        CaptureOptions::default(),
    );
    assert!(matches!(
        provider.capture(request),
        Err(newpic_platform::capture::CaptureError::Unsupported {
            operation: "specific-display capture",
            ..
        })
    ));
}

#[test]
fn wayland_display_enumeration_reports_portal_owned_selection() {
    let provider = LinuxCaptureProvider::for_session(LinuxSession::Wayland);
    assert!(matches!(
        provider.displays(),
        Err(CaptureError::PortalOwnedSelection {
            operation: "display enumeration",
            ..
        })
    ));
}

/// Opt in on a graphical Linux desktop with:
/// `cargo test -p newpic-linux --test contract interactive_ -- --ignored --nocapture`
#[test]
#[ignore = "requires a real interactive X11/Wayland session and may show a portal"]
fn interactive_provider_lists_displays_and_returns_valid_rgba() {
    let provider = LinuxCaptureProvider::detect();
    let image = match provider.session() {
        LinuxSession::X11 => {
            let displays = provider
                .displays()
                .expect("interactive X11 display enumeration");
            assert!(!displays.is_empty(), "X11 must expose a display");
            provider
                .capture(CaptureRequest::new(
                    CaptureSource::Display(displays[0].id().clone()),
                    CaptureOptions::default(),
                ))
                .expect("interactive X11 display capture")
        }
        LinuxSession::Wayland => {
            let requested = PhysicalRect::new(0, 0, 1, 1).unwrap();
            let image = provider
                .capture(CaptureRequest::new(
                    CaptureSource::Region(requested),
                    CaptureOptions::default(),
                ))
                .expect("interactive portal-owned area capture");
            assert!(matches!(
                image.provenance(),
                CaptureProvenance::PortalOwnedSelection {
                    requested_region,
                    ..
                } if *requested_region == requested
            ));
            assert!(image
                .execution()
                .and_then(|execution| execution.fallback_reason())
                .is_some());
            image
        }
    };
    assert_eq!(
        image.rgba().len(),
        image.row_stride() * image.height() as usize
    );
}
