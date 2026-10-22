use newpic_platform::capabilities::{CapabilityStatus, CaptureBackend, CaptureCapabilities};
use newpic_platform::capture::{
    CaptureError, CaptureOptions, CaptureProvenance, CaptureProvider, CaptureRequest, CaptureSource,
};
use newpic_platform::geometry::{PhysicalRect, PhysicalSize};
use newpic_testing::FakeCaptureProvider;

#[test]
fn fake_provider_obeys_the_capture_contract_with_deterministic_pixels() {
    let provider = FakeCaptureProvider::default();
    let region = PhysicalRect::from_origin_size(
        -20,
        30,
        PhysicalSize::new(40, 50).expect("valid request size"),
    )
    .expect("valid request region");

    let image = provider
        .capture(CaptureRequest::new(
            CaptureSource::Region(region),
            CaptureOptions::default().with_include_pointer(true),
        ))
        .expect("fake capture");

    assert_eq!(
        (image.width(), image.height(), image.row_stride()),
        (2, 2, 8)
    );
    assert_eq!(
        image.rgba(),
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,]
    );
    assert_eq!(image.provenance(), &CaptureProvenance::Region);
    assert_eq!(
        provider.recorded_requests(),
        vec![CaptureRequest::new(
            CaptureSource::Region(region),
            CaptureOptions::default().with_include_pointer(true),
        )]
    );
    assert!(!provider.displays().expect("fake displays").is_empty());
    assert!(provider.capabilities().region_capture().is_available());
}

#[test]
fn fake_provider_records_distinct_observable_requests_in_order() {
    let provider = FakeCaptureProvider::default();

    provider
        .capture(CaptureRequest::new(
            CaptureSource::ActiveWindow,
            CaptureOptions::default(),
        ))
        .expect("active window fixture");
    let display_id = provider.displays().unwrap()[0].id().clone();
    provider
        .capture(CaptureRequest::new(
            CaptureSource::Display(display_id.clone()),
            CaptureOptions::default(),
        ))
        .expect("display fixture");

    assert_eq!(
        provider.recorded_requests(),
        vec![
            CaptureRequest::new(CaptureSource::ActiveWindow, CaptureOptions::default()),
            CaptureRequest::new(
                CaptureSource::Display(display_id),
                CaptureOptions::default()
            )
        ]
    );
}

#[test]
fn request_options_are_explicit_and_preserved_at_the_provider_boundary() {
    let provider = FakeCaptureProvider::default();
    let request = CaptureRequest::new(
        CaptureSource::ActiveWindow,
        CaptureOptions::default().with_include_pointer(true),
    );

    provider.capture(request.clone()).unwrap();

    assert!(request.options().include_pointer());
    assert_eq!(provider.recorded_requests(), vec![request]);
}

#[test]
fn fake_rejects_unsupported_operations_and_unknown_display_ids() {
    let capabilities = CaptureCapabilities::new(CaptureBackend::Test)
        .with_region_capture(CapabilityStatus::supported())
        .with_active_window_capture(CapabilityStatus::unsupported(
            "not enabled in this fixture",
            None::<String>,
        ))
        .with_display_capture(CapabilityStatus::supported())
        .with_unattended_capture(CapabilityStatus::supported())
        .with_pointer_capture(CapabilityStatus::unsupported(
            "pointer capture is disabled",
            None::<String>,
        ));
    let provider = FakeCaptureProvider::new(
        capabilities,
        FakeCaptureProvider::default().displays().unwrap(),
    )
    .unwrap();

    assert!(matches!(
        provider.capture(CaptureRequest::new(
            CaptureSource::ActiveWindow,
            CaptureOptions::default(),
        )),
        Err(CaptureError::Unsupported {
            operation: "active_window",
            ..
        })
    ));
    assert_eq!(
        provider.capture(CaptureRequest::new(
            CaptureSource::Display(newpic_platform::geometry::DisplayId::new("missing").unwrap()),
            CaptureOptions::default(),
        )),
        Err(CaptureError::DisplayNotFound(
            newpic_platform::geometry::DisplayId::new("missing").unwrap()
        ))
    );
    assert!(matches!(
        provider.capture(CaptureRequest::new(
            CaptureSource::Region(PhysicalRect::new(0, 0, 10, 10).unwrap()),
            CaptureOptions::default().with_include_pointer(true),
        )),
        Err(CaptureError::Unsupported {
            operation: "pointer_capture",
            ..
        })
    ));
}

#[test]
fn fake_supports_thread_safe_scripted_capture_and_display_results() {
    let provider = FakeCaptureProvider::default();
    provider.queue_capture_result(Err(CaptureError::PermissionDenied {
        settings_hint: Some("open settings".into()),
    }));
    provider.queue_capture_result(Err(CaptureError::UserCancelled));
    provider.queue_displays_result(Err(CaptureError::BackendUnavailable("offline".into())));

    let request = CaptureRequest::new(CaptureSource::ActiveWindow, CaptureOptions::default());
    assert!(matches!(
        provider.capture(request.clone()),
        Err(CaptureError::PermissionDenied { .. })
    ));
    assert_eq!(provider.capture(request), Err(CaptureError::UserCancelled));
    assert!(matches!(
        provider.displays(),
        Err(CaptureError::BackendUnavailable(_))
    ));
}

#[test]
fn fake_validation_precedes_and_does_not_consume_queued_capture_results() {
    let capabilities = CaptureCapabilities::new(CaptureBackend::Test)
        .with_region_capture(CapabilityStatus::supported())
        .with_active_window_capture(CapabilityStatus::unsupported("disabled", None::<String>))
        .with_display_capture(CapabilityStatus::supported())
        .with_unattended_capture(CapabilityStatus::supported())
        .with_pointer_capture(CapabilityStatus::supported());
    let provider = FakeCaptureProvider::new(
        capabilities,
        FakeCaptureProvider::default().displays().unwrap(),
    )
    .unwrap();
    provider.queue_capture_result(Err(CaptureError::PermissionDenied {
        settings_hint: None,
    }));

    assert!(matches!(
        provider.capture(CaptureRequest::new(
            CaptureSource::ActiveWindow,
            CaptureOptions::default(),
        )),
        Err(CaptureError::Unsupported { .. })
    ));
    assert!(matches!(
        provider.capture(CaptureRequest::new(
            CaptureSource::Region(PhysicalRect::new(0, 0, 10, 10).unwrap()),
            CaptureOptions::default(),
        )),
        Err(CaptureError::PermissionDenied { .. })
    ));
}
