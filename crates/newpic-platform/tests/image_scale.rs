use newpic_platform::capture::{
    CaptureError, CaptureProvenance, CaptureScaleMetadata, CaptureScaleSegment, CaptureScaleSource,
    CapturedImage, DisplayScaleSource,
};
use newpic_platform::geometry::{DisplayId, PhysicalRect, ScaleFactor};

fn segment(id: &str, x: i32, y: i32, width: u32, height: u32, scale: f64) -> CaptureScaleSegment {
    CaptureScaleSegment::new(
        PhysicalRect::new(x, y, width, height).unwrap(),
        CaptureScaleSource::Display(DisplayScaleSource::new(
            DisplayId::new(id).unwrap(),
            ScaleFactor::new(scale).unwrap(),
            0,
        )),
    )
}

#[test]
fn gap_segments_truthfully_cover_monitor_gap_pixels() {
    let metadata = CaptureScaleMetadata::mixed(vec![
        segment("left", 0, 0, 1, 2, 1.0),
        CaptureScaleSegment::new(
            PhysicalRect::new(1, 0, 1, 2).unwrap(),
            CaptureScaleSource::Gap,
        ),
        segment("right", 2, 0, 1, 2, 1.5),
    ])
    .unwrap();

    assert!(
        CapturedImage::new(3, 2, 12, vec![0; 24], metadata, CaptureProvenance::Region,).is_ok()
    );
}

#[test]
fn mirrored_display_winner_is_lowest_provider_z_order_then_display_id() {
    let a = DisplayScaleSource::new(
        DisplayId::new("a").unwrap(),
        ScaleFactor::new(1.0).unwrap(),
        4,
    );
    let b = DisplayScaleSource::new(
        DisplayId::new("b").unwrap(),
        ScaleFactor::new(1.5).unwrap(),
        2,
    );
    let c = DisplayScaleSource::new(
        DisplayId::new("c").unwrap(),
        ScaleFactor::new(2.0).unwrap(),
        2,
    );

    assert!(b.precedes(&a));
    assert!(b.precedes(&c));
}

#[test]
fn uniform_and_truthful_mixed_scale_metadata_are_accepted() {
    let uniform = CapturedImage::new(
        2,
        2,
        8,
        vec![0; 16],
        CaptureScaleMetadata::uniform(ScaleFactor::new(1.25).unwrap()),
        CaptureProvenance::Region,
    )
    .unwrap();
    assert_eq!(
        uniform.scale_metadata().uniform_scale(),
        Some(ScaleFactor::new(1.25).unwrap())
    );

    let mixed = CaptureScaleMetadata::mixed(vec![
        segment("left", 0, 0, 1, 2, 1.0),
        segment("right", 1, 0, 1, 2, 1.5),
    ])
    .unwrap();
    let image = CapturedImage::new(2, 2, 8, vec![0; 16], mixed, CaptureProvenance::Region).unwrap();
    assert_eq!(image.scale_metadata().segments().unwrap().len(), 2);
}

#[test]
fn mixed_scale_metadata_rejects_gaps_overlaps_and_out_of_bounds_segments() {
    let cases = [
        vec![
            segment("left", 0, 0, 1, 1, 1.0),
            segment("right", 1, 0, 1, 2, 1.5),
        ],
        vec![
            segment("left", 0, 0, 2, 2, 1.0),
            segment("right", 1, 0, 1, 2, 1.5),
        ],
        vec![
            segment("left", 0, 0, 1, 2, 1.0),
            segment("right", 2, 0, 1, 2, 1.5),
        ],
    ];
    for segments in cases {
        let metadata = CaptureScaleMetadata::mixed(segments).unwrap();
        assert!(matches!(
            CapturedImage::new(2, 2, 8, vec![0; 16], metadata, CaptureProvenance::Region),
            Err(CaptureError::InvalidScaleSegments(_))
        ));
    }
}

#[test]
fn mixed_metadata_rejects_segments_that_all_have_the_same_scale() {
    assert!(matches!(
        CaptureScaleMetadata::mixed(vec![
            segment("left", 0, 0, 1, 2, 1.0),
            segment("right", 1, 0, 1, 2, 1.0),
        ]),
        Err(CaptureError::InvalidScaleSegments(_))
    ));
}

#[test]
fn mixed_metadata_rejects_conflicting_scales_for_one_source_display() {
    let metadata = CaptureScaleMetadata::mixed(vec![
        segment("same", 0, 0, 1, 2, 1.0),
        segment("same", 1, 0, 1, 2, 1.5),
    ])
    .unwrap();
    assert!(matches!(
        CapturedImage::new(2, 2, 8, vec![0; 16], metadata, CaptureProvenance::Region),
        Err(CaptureError::InvalidScaleSegments(_))
    ));
}
