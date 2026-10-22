use newpic_platform::capabilities::{
    CapabilityStatus, CaptureBackend, CaptureCapabilities, CaptureOperation,
};
use newpic_platform::capture::{
    CaptureError, CaptureFallback, CaptureProvenance, CapturedImage, PixelScale,
};
use newpic_platform::geometry::{
    fixed_region_at, intersections_with_displays, Display, DisplayId, DisplayOrientation,
    LogicalPoint, PhysicalPoint, PhysicalRect, PhysicalSize, SavedRegion, SavedRegionDecision,
    SavedRegionInvalidation, ScaleFactor,
};

fn display(id: &str, x: i32, y: i32, width: u32, height: u32, scale: f64) -> Display {
    Display::new(
        DisplayId::new(id),
        PhysicalRect::new(x, y, width, height).expect("valid display bounds"),
        ScaleFactor::new(scale).expect("valid scale factor"),
        false,
        if width >= height {
            DisplayOrientation::Landscape
        } else {
            DisplayOrientation::Portrait
        },
    )
}

#[test]
fn negative_monitor_origins_remain_in_virtual_desktop_coordinates() {
    let left = display("left", -1920, -200, 1920, 1080, 1.0);

    assert_eq!(left.bounds().origin(), PhysicalPoint::new(-1920, -200));
    assert_eq!(left.bounds().right(), 0);
    assert_eq!(left.bounds().bottom(), 880);
    assert!(left.bounds().contains(PhysicalPoint::new(-1, 879)));
    assert!(!left.bounds().contains(PhysicalPoint::new(0, 879)));
}

#[test]
fn logical_coordinates_use_each_displays_scale_and_physical_origin() {
    let standard = display("standard", -1920, 0, 1920, 1080, 1.0);
    let hidpi = display("hidpi", 0, 0, 3000, 2000, 1.5);

    assert_eq!(
        standard.logical_to_physical(LogicalPoint::new(100.0, 40.0)),
        Ok(PhysicalPoint::new(-1820, 40))
    );
    assert_eq!(
        hidpi.logical_to_physical(LogicalPoint::new(100.0, 40.0)),
        Ok(PhysicalPoint::new(150, 60))
    );
}

#[test]
fn fixed_800_by_600_region_clamps_to_a_smaller_display() {
    let small = display("small", -640, 50, 640, 480, 1.0);

    let region = fixed_region_at(
        PhysicalPoint::new(-400, 250),
        PhysicalSize::new(800, 600).expect("valid requested size"),
        &small,
    )
    .expect("fixed selection");

    assert_eq!(
        region,
        PhysicalRect::new(-640, 50, 640, 480).expect("expected bounds")
    );
}

#[test]
fn fixed_region_position_is_clamped_inside_the_selected_display() {
    let primary = display("primary", 100, -300, 1920, 1080, 1.0);
    let requested = PhysicalSize::new(800, 600).unwrap();

    let near_bottom_right = fixed_region_at(PhysicalPoint::new(1950, 700), requested, &primary)
        .expect("fixed selection");

    assert_eq!(
        near_bottom_right,
        PhysicalRect::new(1220, 180, 800, 600).unwrap()
    );
}

#[test]
fn region_is_split_at_adjacent_monitor_boundaries() {
    let left = display("left", -1280, 0, 1280, 1024, 1.0);
    let right = display("right", 0, 0, 1920, 1080, 1.5);
    let spanning = PhysicalRect::new(-100, 20, 250, 200).unwrap();

    let intersections = intersections_with_displays(spanning, &[left, right]);

    assert_eq!(intersections.len(), 2);
    assert_eq!(intersections[0].display_id(), &DisplayId::new("left"));
    assert_eq!(
        intersections[0].rect(),
        PhysicalRect::new(-100, 20, 100, 200).unwrap()
    );
    assert_eq!(intersections[1].display_id(), &DisplayId::new("right"));
    assert_eq!(
        intersections[1].rect(),
        PhysicalRect::new(0, 20, 150, 200).unwrap()
    );
}

#[test]
fn captured_image_accepts_padded_rgba_rows_and_exposes_metadata() {
    let provenance = CaptureProvenance::ActiveWindow {
        fallback: Some(CaptureFallback::WholeWindow),
    };
    let pixels = vec![7; 24];

    let image = CapturedImage::new(
        2,
        2,
        12,
        pixels.clone(),
        PixelScale::new(1.5, 1.25).unwrap(),
        provenance.clone(),
    )
    .expect("valid padded RGBA image");

    assert_eq!(image.width(), 2);
    assert_eq!(image.height(), 2);
    assert_eq!(image.row_stride(), 12);
    assert_eq!(image.rgba(), pixels.as_slice());
    assert_eq!(image.scale(), PixelScale::new(1.5, 1.25).unwrap());
    assert_eq!(image.provenance(), &provenance);
}

#[test]
fn captured_image_rejects_invalid_rgba_layouts_with_typed_errors() {
    assert_eq!(
        CapturedImage::new(
            2,
            2,
            7,
            vec![0; 14],
            PixelScale::ONE,
            CaptureProvenance::Region,
        ),
        Err(CaptureError::RowStrideTooSmall {
            minimum: 8,
            actual: 7,
        })
    );
    assert_eq!(
        CapturedImage::new(
            2,
            2,
            8,
            vec![0; 15],
            PixelScale::ONE,
            CaptureProvenance::Region,
        ),
        Err(CaptureError::PixelByteLengthMismatch {
            expected: 16,
            actual: 15,
        })
    );
}

#[test]
fn capabilities_explicitly_describe_backend_and_supported_operations() {
    let capabilities = CaptureCapabilities::new(CaptureBackend::XdgDesktopPortal)
        .with_region_capture(CapabilityStatus::supported())
        .with_active_window_capture(CapabilityStatus::unsupported(
            "the portal cannot select an active window",
            Some("use region capture instead"),
        ))
        .with_display_capture(CapabilityStatus::supported())
        .with_unattended_capture(CapabilityStatus::limited(
            "the compositor shows a chooser",
            None::<String>,
        ))
        .with_pointer_capture(CapabilityStatus::unsupported(
            "pointer inclusion is compositor-controlled",
            None::<String>,
        ));

    assert_eq!(capabilities.backend(), CaptureBackend::XdgDesktopPortal);
    assert!(capabilities.region_capture().is_available());
    assert!(capabilities.display_capture().is_available());
    assert!(capabilities.unattended_capture().is_available());
    assert_eq!(
        capabilities.status(CaptureOperation::ActiveWindow),
        &CapabilityStatus::unsupported(
            "the portal cannot select an active window",
            Some("use region capture instead"),
        )
    );
    assert_eq!(
        capabilities.pointer_capture().reason(),
        Some("pointer inclusion is compositor-controlled")
    );
    assert!(!capabilities.pointer_capture().is_available());
}

#[test]
fn malformed_geometry_and_scale_values_return_typed_errors() {
    assert!(PhysicalRect::new(0, 0, 0, 10).is_err());
    assert!(PhysicalSize::new(10, 0).is_err());
    assert!(ScaleFactor::new(0.0).is_err());
    assert!(ScaleFactor::new(f64::NAN).is_err());
    assert!(PixelScale::new(f64::INFINITY, 1.0).is_err());
}

#[test]
fn physical_size_accessors_preserve_non_zero_invariants() {
    let size = PhysicalSize::new(800, 600).unwrap();

    assert_eq!(size.width(), 800);
    assert_eq!(size.height(), 600);
    assert_eq!(
        PhysicalSize::new(0, 600),
        Err(newpic_platform::geometry::GeometryError::ZeroSize {
            width: 0,
            height: 600
        })
    );
    assert_eq!(
        PhysicalSize::new(800, 0),
        Err(newpic_platform::geometry::GeometryError::ZeroSize {
            width: 800,
            height: 0
        })
    );
}

#[test]
fn physical_rect_rejects_coordinate_extent_overflow() {
    assert!(matches!(
        PhysicalRect::new(i32::MAX, 0, 2, 1),
        Err(newpic_platform::geometry::GeometryError::CoordinateExtentOverflow { .. })
    ));
    assert!(matches!(
        PhysicalRect::new(0, i32::MAX, 1, 2),
        Err(newpic_platform::geometry::GeometryError::CoordinateExtentOverflow { .. })
    ));
}

#[test]
fn saved_region_reuses_unchanged_topology() {
    let left = display("left", -1280, 0, 1280, 1024, 1.0);
    let right = display("right", 0, 0, 1920, 1080, 1.5);
    let rect = PhysicalRect::new(-100, 50, 300, 200).unwrap();
    let saved = SavedRegion::capture(rect, &[left.clone(), right.clone()]).unwrap();

    assert_eq!(
        saved.revalidate(&[left, right]),
        SavedRegionDecision::Reuse(rect)
    );
    assert_eq!(saved.participating_displays().len(), 2);
}

#[test]
fn saved_region_translates_when_all_participating_displays_move_together() {
    let original = display("external", -1920, 0, 1920, 1080, 1.0);
    let saved = SavedRegion::capture(
        PhysicalRect::new(-800, 100, 500, 400).unwrap(),
        std::slice::from_ref(&original),
    )
    .unwrap();
    let moved = display("external", -1600, 200, 1920, 1080, 1.0);

    assert_eq!(
        saved.revalidate(&[moved]),
        SavedRegionDecision::Reuse(PhysicalRect::new(-480, 300, 500, 400).unwrap())
    );
}

#[test]
fn saved_region_falls_back_after_removal_scale_resolution_or_orientation_change() {
    let original = Display::new(
        DisplayId::new("external"),
        PhysicalRect::new(-1920, 0, 1920, 1080).unwrap(),
        ScaleFactor::new(1.0).unwrap(),
        false,
        DisplayOrientation::Landscape,
    );
    let saved = SavedRegion::capture(
        PhysicalRect::new(-800, 100, 500, 400).unwrap(),
        std::slice::from_ref(&original),
    )
    .unwrap();

    assert_eq!(
        saved.revalidate(&[]),
        SavedRegionDecision::FallbackToSelection(SavedRegionInvalidation::DisplayRemoved(
            DisplayId::new("external")
        ))
    );

    let scaled = Display::new(
        original.id().clone(),
        original.bounds(),
        ScaleFactor::new(1.25).unwrap(),
        false,
        DisplayOrientation::Landscape,
    );
    assert_eq!(
        saved.revalidate(&[scaled]),
        SavedRegionDecision::FallbackToSelection(SavedRegionInvalidation::ScaleChanged(
            DisplayId::new("external")
        ))
    );

    let resized = Display::new(
        original.id().clone(),
        PhysicalRect::new(-1920, 0, 1600, 900).unwrap(),
        ScaleFactor::new(1.0).unwrap(),
        false,
        DisplayOrientation::Landscape,
    );
    assert_eq!(
        saved.revalidate(&[resized]),
        SavedRegionDecision::FallbackToSelection(SavedRegionInvalidation::ResolutionChanged(
            DisplayId::new("external")
        ))
    );

    let rotated = Display::new(
        original.id().clone(),
        original.bounds(),
        ScaleFactor::new(1.0).unwrap(),
        false,
        DisplayOrientation::LandscapeFlipped,
    );
    assert_eq!(
        saved.revalidate(&[rotated]),
        SavedRegionDecision::FallbackToSelection(SavedRegionInvalidation::OrientationChanged(
            DisplayId::new("external")
        ))
    );
}

#[test]
fn saved_region_rejects_inconsistent_multi_display_movement() {
    let left = display("left", -1280, 0, 1280, 1024, 1.0);
    let right = display("right", 0, 0, 1920, 1080, 1.0);
    let saved = SavedRegion::capture(
        PhysicalRect::new(-100, 50, 300, 200).unwrap(),
        &[left, right],
    )
    .unwrap();
    let moved_left = display("left", -1400, 0, 1280, 1024, 1.0);
    let unchanged_right = display("right", 0, 0, 1920, 1080, 1.0);

    assert_eq!(
        saved.revalidate(&[moved_left, unchanged_right]),
        SavedRegionDecision::FallbackToSelection(
            SavedRegionInvalidation::InconsistentDisplayMovement
        )
    );
}
