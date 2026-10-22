use newpic_platform::capture::{CaptureProvenance, CaptureProvider, CaptureRequest};
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
        .capture(CaptureRequest::Region(region))
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
        vec![CaptureRequest::Region(region)]
    );
    assert!(!provider.displays().expect("fake displays").is_empty());
    assert!(provider.capabilities().region_capture());
}

#[test]
fn fake_provider_records_distinct_observable_requests_in_order() {
    let provider = FakeCaptureProvider::default();

    provider
        .capture(CaptureRequest::ActiveWindow)
        .expect("active window fixture");
    let display_id = provider.displays().unwrap()[0].id().clone();
    provider
        .capture(CaptureRequest::Display(display_id.clone()))
        .expect("display fixture");

    assert_eq!(
        provider.recorded_requests(),
        vec![
            CaptureRequest::ActiveWindow,
            CaptureRequest::Display(display_id)
        ]
    );
}
