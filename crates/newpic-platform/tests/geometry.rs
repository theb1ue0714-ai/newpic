use newpic_platform::capabilities::{
    CapabilityStatus, CaptureBackend, CaptureCapabilities, CaptureOperation,
};
use newpic_platform::capture::{
    CaptureError, CaptureFallback, CaptureProvenance, CaptureScaleMetadata, CapturedImage,
};
use newpic_platform::geometry::{
    fixed_region_at, intersections_with_displays, Display, DisplayId, DisplayOrientation,
    LogicalPoint, PhysicalPoint, PhysicalRect, PhysicalSize, SavedFixedRegion, SavedRegion,
    SavedRegionDecision, SavedRegionInvalidation, ScaleFactor,
};

fn display(id: &str, x: i32, y: i32, width: u32, height: u32, scale: f64) -> Display {
    Display::new(
        DisplayId::new(id).unwrap(),
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

fn primary_display(id: &str, x: i32, y: i32, width: u32, height: u32, scale: f64) -> Display {
    Display::new(
        DisplayId::new(id).unwrap(),
        PhysicalRect::new(x, y, width, height).expect("valid display bounds"),
        ScaleFactor::new(scale).expect("valid scale factor"),
        true,
        if width >= height {
            DisplayOrientation::Landscape
        } else {
            DisplayOrientation::Portrait
        },
    )
}

#[test]
fn saved_fixed_region_restores_exactly_on_unchanged_display() {
    let displays = vec![primary_display("main", 0, 0, 1920, 1080, 1.0)];
    let rect = PhysicalRect::new(220, 140, 900, 520).unwrap();
    let saved = SavedFixedRegion::capture(rect, &displays).unwrap();

    assert_eq!(saved.resolve(&displays).unwrap(), rect);
}

#[test]
fn saved_fixed_region_preserves_negative_origins() {
    let displays = vec![primary_display("left", -1920, -200, 1920, 1080, 1.0)];
    let rect = PhysicalRect::new(-1800, -120, 800, 600).unwrap();
    let saved = SavedFixedRegion::capture(rect, &displays).unwrap();

    assert_eq!(saved.rect(), rect);
    assert_eq!(saved.resolve(&displays).unwrap(), rect);
}

#[test]
fn saved_fixed_region_moves_inside_primary_when_topology_changes() {
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(-1200, 80, 640, 480).unwrap(),
        &[display("left", -1280, 0, 1280, 1024, 1.0)],
    )
    .unwrap();
    let current = vec![primary_display("main", 0, 0, 500, 400, 1.25)];

    assert_eq!(
        saved.resolve(&current).unwrap(),
        PhysicalRect::new(0, 0, 500, 400).unwrap()
    );
}

#[test]
fn saved_fixed_region_centers_on_primary_after_scale_or_orientation_changes() {
    let original = primary_display("main", 0, 0, 1920, 1080, 1.0);
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(100, 80, 800, 600).unwrap(),
        std::slice::from_ref(&original),
    )
    .unwrap();
    let scaled = Display::new(
        original.id().clone(),
        original.bounds(),
        ScaleFactor::new(1.25).unwrap(),
        true,
        DisplayOrientation::Portrait,
    );

    assert_eq!(
        saved.resolve(&[scaled]).unwrap(),
        PhysicalRect::new(560, 240, 800, 600).unwrap()
    );
}

#[test]
fn saved_fixed_region_uses_primary_when_saved_display_is_missing() {
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(-900, 100, 600, 400).unwrap(),
        &[display("removed", -1280, 0, 1280, 1024, 1.0)],
    )
    .unwrap();
    let current = vec![
        display("secondary", 1920, 0, 1280, 1024, 1.0),
        primary_display("main", 0, 0, 1920, 1080, 1.0),
    ];

    assert_eq!(
        saved.resolve(&current).unwrap(),
        PhysicalRect::new(660, 340, 600, 400).unwrap()
    );
}

#[test]
fn saved_fixed_region_rejects_duplicate_ids_and_multi_display_rectangles() {
    let duplicate_a = primary_display("same", 0, 0, 100, 100, 1.0);
    let duplicate_b = display("same", 100, 0, 100, 100, 1.0);
    assert!(SavedFixedRegion::capture(
        PhysicalRect::new(10, 10, 40, 40).unwrap(),
        &[duplicate_a, duplicate_b]
    )
    .is_err());

    let left = display("left", -100, 0, 100, 100, 1.0);
    let right = primary_display("right", 0, 0, 100, 100, 1.0);
    assert!(
        SavedFixedRegion::capture(PhysicalRect::new(-20, 10, 40, 40).unwrap(), &[left, right])
            .is_err()
    );
}

#[test]
fn saved_fixed_region_with_size_preserves_or_clamps_top_left() {
    let displays = vec![primary_display("main", -100, -50, 1000, 700, 1.0)];
    let saved =
        SavedFixedRegion::capture(PhysicalRect::new(100, 100, 300, 200).unwrap(), &displays)
            .unwrap();

    let preserved = saved
        .with_size(PhysicalSize::new(400, 300).unwrap(), &displays)
        .unwrap();
    assert_eq!(
        preserved.rect(),
        PhysicalRect::new(100, 100, 400, 300).unwrap()
    );

    let clamped = saved
        .with_size(PhysicalSize::new(900, 650).unwrap(), &displays)
        .unwrap();
    assert_eq!(clamped.rect(), PhysicalRect::new(0, 0, 900, 650).unwrap());
}

#[test]
fn saved_fixed_region_with_size_keeps_stable_display_after_scale_change() {
    let original = display("external", -1280, 0, 1280, 1024, 1.0);
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(-1100, 100, 500, 400).unwrap(),
        std::slice::from_ref(&original),
    )
    .unwrap();
    let current = vec![
        primary_display("main", 0, 0, 1920, 1080, 1.0),
        Display::new(
            original.id().clone(),
            original.bounds(),
            ScaleFactor::new(1.25).unwrap(),
            false,
            original.orientation(),
        ),
    ];

    let resized = saved
        .with_size(PhysicalSize::new(600, 450).unwrap(), &current)
        .unwrap();

    assert_eq!(
        resized.rect(),
        PhysicalRect::new(-1100, 100, 600, 450).unwrap()
    );
    assert_eq!(resized.resolve(&current).unwrap(), resized.rect());
}

#[test]
fn saved_fixed_region_with_size_keeps_stable_display_after_orientation_change() {
    let original = display("external", -1280, 0, 1280, 1024, 1.0);
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(-1100, 100, 500, 400).unwrap(),
        std::slice::from_ref(&original),
    )
    .unwrap();
    let current = vec![
        primary_display("main", 0, 0, 1920, 1080, 1.0),
        Display::new(
            original.id().clone(),
            original.bounds(),
            original.scale_factor(),
            false,
            DisplayOrientation::LandscapeFlipped,
        ),
    ];

    let resized = saved
        .with_size(PhysicalSize::new(600, 450).unwrap(), &current)
        .unwrap();

    assert_eq!(
        resized.rect(),
        PhysicalRect::new(-1100, 100, 600, 450).unwrap()
    );
    assert_eq!(resized.resolve(&current).unwrap(), resized.rect());
}

#[test]
fn saved_fixed_region_with_size_clamps_stable_display_after_bounds_change() {
    let original = display("external", -1280, 0, 1280, 1024, 1.0);
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(-1200, 80, 500, 400).unwrap(),
        std::slice::from_ref(&original),
    )
    .unwrap();
    let current = vec![
        primary_display("main", 0, 0, 1920, 1080, 1.0),
        display("external", -1000, 50, 900, 700, 1.0),
    ];

    let resized = saved
        .with_size(PhysicalSize::new(850, 650).unwrap(), &current)
        .unwrap();

    assert_eq!(
        resized.rect(),
        PhysicalRect::new(-1000, 80, 850, 650).unwrap()
    );
    assert_eq!(resized.resolve(&current).unwrap(), resized.rect());
}

#[test]
fn saved_fixed_region_results_remain_keyed_to_selected_display_with_a_new_mirror() {
    let original = primary_display("selected", 0, 0, 1000, 800, 1.0);
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(100, 80, 500, 400).unwrap(),
        std::slice::from_ref(&original),
    )
    .unwrap();
    let current = vec![original, display("mirror", 0, 0, 1000, 800, 1.0)];

    assert_eq!(
        saved.resolve(&current).unwrap(),
        PhysicalRect::new(100, 80, 500, 400).unwrap()
    );

    let resized = saved
        .with_size(PhysicalSize::new(700, 600).unwrap(), &current)
        .unwrap();
    assert_eq!(
        resized.rect(),
        PhysicalRect::new(100, 80, 700, 600).unwrap()
    );
    assert_eq!(resized.resolve(&current).unwrap(), resized.rect());
}

#[test]
fn saved_fixed_region_reports_exact_primary_topology_errors() {
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(-900, 100, 600, 400).unwrap(),
        &[display("removed", -1280, 0, 1280, 1024, 1.0)],
    )
    .unwrap();
    let size = PhysicalSize::new(800, 600).unwrap();

    assert_eq!(
        saved.resolve(&[]),
        Err(newpic_platform::geometry::GeometryError::PrimaryDisplayUnavailable)
    );
    assert_eq!(
        saved.with_size(size, &[]),
        Err(newpic_platform::geometry::GeometryError::PrimaryDisplayUnavailable)
    );

    let two_primaries = vec![
        primary_display("one", 0, 0, 1000, 800, 1.0),
        primary_display("two", 1000, 0, 1000, 800, 1.0),
    ];
    assert_eq!(
        saved.resolve(&two_primaries),
        Err(newpic_platform::geometry::GeometryError::MultiplePrimaryDisplays)
    );
    assert_eq!(
        saved.with_size(size, &two_primaries),
        Err(newpic_platform::geometry::GeometryError::MultiplePrimaryDisplays)
    );
}

#[test]
fn saved_fixed_region_serialization_round_trip_preserves_validated_geometry() {
    let displays = vec![primary_display("left", -1920, -200, 1920, 1080, 1.25)];
    let saved =
        SavedFixedRegion::capture(PhysicalRect::new(-1800, -120, 800, 600).unwrap(), &displays)
            .unwrap();

    let json = serde_json::to_string(&saved).unwrap();
    let restored: SavedFixedRegion = serde_json::from_str(&json).unwrap();

    assert_eq!(restored, saved);
}

#[test]
fn saved_fixed_region_deserialization_rejects_invalid_private_geometry() {
    let zero_width = r#"{
        "rect":{"x":0,"y":0,"width":0,"height":10},
        "display":{
            "id":"main",
            "bounds":{"x":0,"y":0,"width":100,"height":100},
            "scale_factor":1.0,
            "orientation":"Landscape"
        }
    }"#;
    let invalid_scale = zero_width
        .replace("\"width\":0", "\"width\":10")
        .replace("\"scale_factor\":1.0", "\"scale_factor\":0.0");
    let overflowing = zero_width.replace(
        "\"x\":0,\"y\":0,\"width\":0",
        "\"x\":2147483647,\"y\":0,\"width\":2",
    );

    assert!(serde_json::from_str::<SavedFixedRegion>(zero_width).is_err());
    assert!(serde_json::from_str::<SavedFixedRegion>(&invalid_scale).is_err());
    assert!(serde_json::from_str::<SavedFixedRegion>(&overflowing).is_err());
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
    assert_eq!(
        intersections[0].display_id(),
        &DisplayId::new("left").unwrap()
    );
    assert_eq!(
        intersections[0].rect(),
        PhysicalRect::new(-100, 20, 100, 200).unwrap()
    );
    assert_eq!(
        intersections[1].display_id(),
        &DisplayId::new("right").unwrap()
    );
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
        CaptureScaleMetadata::uniform(ScaleFactor::new(1.5).unwrap()),
        provenance.clone(),
    )
    .expect("valid padded RGBA image");

    assert_eq!(image.width(), 2);
    assert_eq!(image.height(), 2);
    assert_eq!(image.row_stride(), 12);
    assert_eq!(image.rgba(), pixels.as_slice());
    assert_eq!(
        image.scale_metadata(),
        &CaptureScaleMetadata::uniform(ScaleFactor::new(1.5).unwrap())
    );
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
            CaptureScaleMetadata::uniform(ScaleFactor::new(1.0).unwrap()),
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
            CaptureScaleMetadata::uniform(ScaleFactor::new(1.0).unwrap()),
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
    assert!(ScaleFactor::new(f64::INFINITY).is_err());
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
fn saved_region_falls_back_after_removal_or_orientation_change() {
    let original = Display::new(
        DisplayId::new("external").unwrap(),
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
            DisplayId::new("external").unwrap()
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
            DisplayId::new("external").unwrap()
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
            SavedRegionInvalidation::NonRectangularReconstruction
        )
    );
}

#[test]
fn logical_delta_is_rounded_before_checked_origin_addition() {
    let positive = display("positive", 100, 0, 1000, 800, 1.0);
    let negative = display("negative", -100, 0, 1000, 800, 1.0);

    assert_eq!(
        positive.logical_to_physical(LogicalPoint::new(0.5, 0.0)),
        Ok(PhysicalPoint::new(101, 0))
    );
    assert_eq!(
        negative.logical_to_physical(LogicalPoint::new(-0.5, 0.0)),
        Ok(PhysicalPoint::new(-101, 0))
    );
    assert!(matches!(
        display("edge", i32::MAX, 0, 1, 1, 1.0).logical_to_physical(LogicalPoint::new(0.5, 0.0)),
        Err(newpic_platform::geometry::GeometryError::CoordinateOutOfRange)
    ));
}

#[test]
fn display_ids_are_non_empty_and_saved_topology_rejects_duplicates() {
    assert!(DisplayId::new("").is_err());
    assert!(DisplayId::new("   ").is_err());

    let duplicate_a = display("duplicate", 0, 0, 100, 100, 1.0);
    let duplicate_b = display("duplicate", 100, 0, 100, 100, 1.0);
    assert!(matches!(
        SavedRegion::capture(
            PhysicalRect::new(50, 0, 100, 50).unwrap(),
            &[duplicate_a, duplicate_b]
        ),
        Err(newpic_platform::geometry::GeometryError::DuplicateDisplayId(_))
    ));
}

#[test]
fn scale_factors_are_normalized_against_float_jitter() {
    assert_eq!(
        ScaleFactor::new(1.25).unwrap(),
        ScaleFactor::new(1.25000001).unwrap()
    );
}

#[test]
fn saved_region_reconstructs_single_display_after_origin_and_scale_changes() {
    let original = display("screen", -1000, 0, 1000, 800, 1.0);
    let saved =
        SavedRegion::capture(PhysicalRect::new(-800, 100, 400, 200).unwrap(), &[original]).unwrap();

    let scaled_up = display("screen", 200, 50, 2000, 1600, 2.0);
    assert_eq!(
        saved.revalidate(&[scaled_up]),
        SavedRegionDecision::Reuse(PhysicalRect::new(600, 250, 800, 400).unwrap())
    );
    let scaled_down = display("screen", -500, -20, 500, 400, 0.5);
    assert_eq!(
        saved.revalidate(&[scaled_down]),
        SavedRegionDecision::Reuse(PhysicalRect::new(-400, 30, 200, 100).unwrap())
    );
}

#[test]
fn mixed_dpi_reconstruction_requires_one_exact_rectangle() {
    let left = display("left", -100, 0, 100, 100, 1.0);
    let right = display("right", 0, 0, 100, 100, 2.0);
    let saved =
        SavedRegion::capture(PhysicalRect::new(-50, 10, 100, 40).unwrap(), &[left, right]).unwrap();

    let rectangular_left = display("left", -200, 0, 200, 200, 2.0);
    let rectangular_right = display("right", 0, 0, 400, 400, 4.0);
    assert_eq!(
        saved.revalidate(&[rectangular_left, rectangular_right]),
        SavedRegionDecision::Reuse(PhysicalRect::new(-100, 20, 200, 80).unwrap())
    );

    let gap_left = display("left", -100, 0, 100, 100, 0.5);
    let gap_right = display("right", 0, 0, 100, 100, 2.0);
    assert_eq!(
        saved.revalidate(&[gap_left, gap_right]),
        SavedRegionDecision::FallbackToSelection(
            SavedRegionInvalidation::NonRectangularReconstruction
        )
    );

    let overlap_left = display("left", -100, 0, 100, 100, 1.0);
    let overlap_right = display("right", -25, 0, 100, 100, 2.0);
    assert_eq!(
        saved.revalidate(&[overlap_left, overlap_right]),
        SavedRegionDecision::FallbackToSelection(
            SavedRegionInvalidation::NonRectangularReconstruction
        )
    );
}

#[test]
fn unchanged_full_topology_preserves_outside_display_margins_exactly() {
    let screen = display("screen", 0, 0, 100, 100, 1.0);
    let rect = PhysicalRect::new(-20, 10, 80, 40).unwrap();
    let saved = SavedRegion::capture(rect, std::slice::from_ref(&screen)).unwrap();

    assert_eq!(
        saved.revalidate(std::slice::from_ref(&screen)),
        SavedRegionDecision::Reuse(rect)
    );

    let moved = display("screen", 10, 0, 100, 100, 1.0);
    assert_eq!(
        saved.revalidate(&[moved]),
        SavedRegionDecision::FallbackToSelection(
            SavedRegionInvalidation::UncoveredPixelsCannotTransform
        )
    );
}

#[test]
fn unchanged_full_topology_preserves_monitor_gap_pixels_exactly() {
    let left = display("left", 0, 0, 100, 100, 1.0);
    let right = display("right", 120, 0, 100, 100, 1.0);
    let rect = PhysicalRect::new(80, 10, 60, 40).unwrap();
    let saved = SavedRegion::capture(rect, &[left.clone(), right.clone()]).unwrap();

    assert_eq!(
        saved.revalidate(&[left.clone(), right.clone()]),
        SavedRegionDecision::Reuse(rect)
    );

    let moved_right = display("right", 110, 0, 100, 100, 1.0);
    assert_eq!(
        saved.revalidate(&[left, moved_right]),
        SavedRegionDecision::FallbackToSelection(
            SavedRegionInvalidation::UncoveredPixelsCannotTransform
        )
    );
}

#[test]
fn unchanged_topology_is_order_independent_for_gap_and_mirrored_layouts() {
    let left = display("left", 0, 0, 100, 100, 1.0);
    let right = display("right", 120, 0, 100, 100, 1.5);
    let mirror = display("mirror", 0, 0, 100, 100, 2.0);
    let rect = PhysicalRect::new(80, 10, 60, 40).unwrap();
    let saved = SavedRegion::capture(rect, &[left.clone(), right.clone(), mirror.clone()]).unwrap();

    assert_eq!(
        saved.revalidate(&[mirror, right, left]),
        SavedRegionDecision::Reuse(rect)
    );
}

#[test]
fn changed_topology_detects_a_new_display_participating_in_candidate_region() {
    let source = display("source", 0, 0, 100, 100, 1.0);
    let spare = display("spare", 200, 0, 100, 100, 1.0);
    let saved = SavedRegion::capture(
        PhysicalRect::new(10, 10, 40, 40).unwrap(),
        &[source.clone(), spare],
    )
    .unwrap();
    let moved_spare = display("spare", 20, 20, 100, 100, 1.0);

    assert_eq!(
        saved.revalidate(&[source, moved_spare]),
        SavedRegionDecision::FallbackToSelection(SavedRegionInvalidation::ParticipantSetChanged)
    );
}
