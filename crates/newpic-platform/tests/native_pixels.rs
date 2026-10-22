use newpic_platform::capture::{
    capture_scale_metadata_for_region, normalize_native_rgba8, CaptureScaleSource,
    NativePixelFormat,
};
use newpic_platform::geometry::{
    Display, DisplayId, DisplayOrientation, PhysicalPoint, PhysicalRect, ScaleFactor,
};

fn display(id: &str, rect: PhysicalRect, scale: f64) -> Display {
    Display::new(
        DisplayId::new(id).unwrap(),
        rect,
        ScaleFactor::new(scale).unwrap(),
        false,
        DisplayOrientation::Landscape,
    )
}

#[test]
fn one_reviewed_helper_converts_bgra_and_removes_row_padding() {
    let source = vec![
        30, 20, 10, 40, 70, 60, 50, 80, 99, 99, 99, 99, 3, 2, 1, 4, 7, 6, 5, 8, 88, 88, 88, 88,
    ];

    let rgba = normalize_native_rgba8(2, 2, 12, source, NativePixelFormat::Bgra8).unwrap();

    assert_eq!(
        rgba,
        vec![10, 20, 30, 40, 50, 60, 70, 80, 1, 2, 3, 4, 5, 6, 7, 8]
    );
}

#[test]
fn reviewed_helper_preserves_tight_rgba() {
    let source = vec![1, 2, 3, 4, 5, 6, 7, 8];
    assert_eq!(
        normalize_native_rgba8(2, 1, 8, source.clone(), NativePixelFormat::Rgba8).unwrap(),
        source
    );
}

#[test]
fn scale_metadata_marks_gaps_and_resolves_overlaps_by_provider_order() {
    let region = PhysicalRect::new(0, 0, 5, 2).unwrap();
    let lower_z = display("front", PhysicalRect::new(0, 0, 2, 2).unwrap(), 1.0);
    let overlapping = display("behind", PhysicalRect::new(1, 0, 2, 2).unwrap(), 2.0);
    let right = display("right", PhysicalRect::new(4, 0, 1, 2).unwrap(), 1.5);

    let metadata = capture_scale_metadata_for_region(region, &[lower_z, overlapping, right])
        .expect("truthful tiling");
    let segments = metadata
        .segments()
        .expect("mixed because scales and gap differ");

    assert!(segments.iter().any(|segment| {
        segment.image_rect() == PhysicalRect::new(3, 0, 1, 2).unwrap()
            && matches!(segment.source(), CaptureScaleSource::Gap)
    }));
    assert!(segments.iter().any(|segment| match segment.source() {
        CaptureScaleSource::Display(source) => {
            segment.image_rect().contains(PhysicalPoint::new(1, 0))
                && source.display_id().as_str() == "front"
                && source.provider_z_order() == 0
        }
        CaptureScaleSource::Gap => false,
    }));
}

#[test]
fn same_scale_full_coverage_is_reported_as_uniform() {
    let region = PhysicalRect::new(-2, 0, 4, 2).unwrap();
    let left = display("left", PhysicalRect::new(-2, 0, 2, 2).unwrap(), 1.25);
    let right = display("right", PhysicalRect::new(0, 0, 2, 2).unwrap(), 1.25);

    let metadata = capture_scale_metadata_for_region(region, &[left, right]).unwrap();

    assert_eq!(
        metadata.uniform_scale(),
        Some(ScaleFactor::new(1.25).unwrap())
    );
}
