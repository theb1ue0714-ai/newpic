use newpic_core::settings::Size;
use newpic_desktop_lib::app_state::{SelectionMode, SelectionRequest};
use newpic_desktop_lib::hotkeys::CaptureAction;
use newpic_desktop_lib::overlay::{OverlayCoordinator, OverlayError};
use newpic_platform::geometry::{
    Display, DisplayId, DisplayOrientation, PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor,
};

fn test_display(id: &str, x: i32, y: i32, width: u32, height: u32) -> Display {
    Display::new(
        DisplayId::new(id).unwrap(),
        PhysicalRect::new(x, y, width, height).unwrap(),
        ScaleFactor::new(1.0).unwrap(),
        true,
        DisplayOrientation::Landscape,
    )
}

#[test]
fn overlay_coordinator_lifecycle_and_concurrency() {
    let mut coordinator = OverlayCoordinator::new();

    assert!(!coordinator.is_active());
    assert_eq!(coordinator.last_region(), None);

    let request = SelectionRequest {
        action: CaptureAction::Region,
        mode: SelectionMode::FreeRegion,
        fixed_size: None,
        fallback_from: None,
    };

    // Starting selection activates the coordinator
    coordinator.start_selection(request.clone()).unwrap();
    assert!(coordinator.is_active());

    // Concurrent request is rejected while active
    let second_req = SelectionRequest {
        action: CaptureAction::FixedRegion,
        mode: SelectionMode::FixedSize,
        fixed_size: Some(Size::new(800, 600)),
        fallback_from: None,
    };
    let err = coordinator.start_selection(second_req).unwrap_err();
    assert_eq!(err, OverlayError::AlreadyActive);

    // Cancel selection clears active state and keeps last_region as None
    coordinator.cancel();
    assert!(!coordinator.is_active());
    assert_eq!(coordinator.last_region(), None);

    // Start again, then complete with a region
    coordinator.start_selection(request).unwrap();
    assert!(coordinator.is_active());

    let selected_rect = PhysicalRect::new(100, 150, 400, 300).unwrap();
    coordinator.complete(selected_rect).unwrap();

    assert!(!coordinator.is_active());
    assert_eq!(coordinator.last_region(), Some(selected_rect));

    // Cancel afterwards does not erase last_region (F8 preservation)
    coordinator.cancel();
    assert_eq!(coordinator.last_region(), Some(selected_rect));
}

#[test]
fn fixed_region_clamping_to_display_bounds() {
    let display = test_display("disp1", 0, 0, 1920, 1080);
    let fixed_size = PhysicalSize::new(800, 600).unwrap();

    // Centered at pointer within bounds
    let rect =
        OverlayCoordinator::clamp_fixed_region(PhysicalPoint::new(960, 540), fixed_size, &display)
            .unwrap();
    assert_eq!(rect, PhysicalRect::new(560, 240, 800, 600).unwrap());

    // Pointer near top-left: clamped to display origin
    let rect_top_left =
        OverlayCoordinator::clamp_fixed_region(PhysicalPoint::new(50, 50), fixed_size, &display)
            .unwrap();
    assert_eq!(rect_top_left, PhysicalRect::new(0, 0, 800, 600).unwrap());

    // Pointer near bottom-right: clamped to display bounds
    let rect_bottom_right = OverlayCoordinator::clamp_fixed_region(
        PhysicalPoint::new(1900, 1050),
        fixed_size,
        &display,
    )
    .unwrap();
    assert_eq!(
        rect_bottom_right,
        PhysicalRect::new(1120, 480, 800, 600).unwrap()
    );
}

#[test]
fn f8_repeat_region_validation_against_displays() {
    let mut coordinator = OverlayCoordinator::new();
    let display1 = test_display("disp1", 0, 0, 1920, 1080);

    let region = PhysicalRect::new(100, 100, 300, 200).unwrap();
    coordinator.complete(region).unwrap();

    // Valid when intersecting current display
    let validated = coordinator.validate_repeat_region(std::slice::from_ref(&display1));
    assert_eq!(validated, Some(region));

    // When no display intersects the region, returns None (falls back to F7 selection)
    let display_far = test_display("disp2", 2000, 2000, 1920, 1080);
    let fallback = coordinator.validate_repeat_region(&[display_far]);
    assert_eq!(fallback, None);
}
