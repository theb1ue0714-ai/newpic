use newpic_core::settings::Size;
use newpic_desktop_lib::app_state::{SelectionMode, SelectionRequest};
use newpic_desktop_lib::hotkeys::CaptureAction;
use newpic_desktop_lib::overlay::{
    close_overlay_windows, create_overlay_windows, overlay_info_for_label, OverlayError,
    OverlayWindowCloser, OverlayWindowFactory, PhysicalRectDto,
};
use newpic_platform::geometry::{
    Display, DisplayId, DisplayOrientation, PhysicalRect, ScaleFactor,
};

fn display(id: &str, x: i32, width: u32, primary: bool) -> Display {
    Display::new(
        DisplayId::new(id).unwrap(),
        PhysicalRect::new(x, 0, width, 1080).unwrap(),
        ScaleFactor::new(if x < 0 { 1.25 } else { 1.0 }).unwrap(),
        primary,
        DisplayOrientation::Landscape,
    )
}

#[derive(Default)]
struct RecordingCloser {
    open: Vec<String>,
    close_error: bool,
    stays_open: bool,
    waits: usize,
}

impl OverlayWindowCloser for RecordingCloser {
    fn labels(&self) -> Vec<String> {
        self.open.clone()
    }
    fn close(&mut self, label: &str) -> Result<(), OverlayError> {
        if self.close_error {
            return Err(OverlayError::Window("close failed".into()));
        }
        if !self.stays_open {
            self.open.retain(|item| item != label);
        }
        Ok(())
    }
    fn is_open(&self, label: &str) -> bool {
        self.open.iter().any(|item| item == label)
    }
    fn wait_for_state_change(&mut self) {
        self.waits += 1;
    }
}

#[test]
fn close_waits_for_verified_removal_and_reports_failures() {
    let mut success = RecordingCloser {
        open: vec!["overlay-0".into(), "main".into()],
        ..Default::default()
    };
    close_overlay_windows(&mut success).unwrap();
    assert_eq!(success.open, ["main"]);

    let mut failed = RecordingCloser {
        open: vec!["overlay-0".into()],
        close_error: true,
        ..Default::default()
    };
    assert!(matches!(
        close_overlay_windows(&mut failed),
        Err(OverlayError::Window(_))
    ));

    let mut timeout = RecordingCloser {
        open: vec!["overlay-0".into()],
        stays_open: true,
        ..Default::default()
    };
    assert!(matches!(
        close_overlay_windows(&mut timeout),
        Err(OverlayError::CloseTimeout(_))
    ));
    assert_eq!(timeout.waits, 100);
}

fn request(action: CaptureAction, initial_rect: Option<PhysicalRectDto>) -> SelectionRequest {
    SelectionRequest {
        action,
        mode: if action == CaptureAction::FixedRegion {
            SelectionMode::FixedSize
        } else {
            SelectionMode::FreeRegion
        },
        fixed_size: Some(Size::new(800, 600)),
        fallback_from: None,
        initial_rect,
    }
}

#[test]
fn overlay_info_uses_calling_window_display_and_one_active_f9() {
    let displays = vec![
        display("left", -1280, 1280, false),
        display("main", 0, 1920, true),
    ];
    let rect = PhysicalRectDto {
        x: -1100,
        y: 100,
        width: 800,
        height: 600,
    };
    let request = request(CaptureAction::FixedRegion, Some(rect));

    let left = overlay_info_for_label("overlay-0", &request, &displays).unwrap();
    let main = overlay_info_for_label("overlay-1", &request, &displays).unwrap();
    assert_eq!(left.display_bounds, displays[0].bounds().into());
    assert_eq!(left.scale_factor, 1.25);
    assert!(left.active_display);
    assert_eq!(left.initial_rect, Some(rect));
    assert_eq!(main.display_bounds, displays[1].bounds().into());
    assert!(!main.active_display);
    assert_eq!(main.initial_rect, None);
}

#[test]
fn free_region_makes_every_display_active() {
    let displays = vec![
        display("left", -1280, 1280, false),
        display("main", 0, 1920, true),
    ];
    let request = request(CaptureAction::Region, None);
    assert!(
        overlay_info_for_label("overlay-0", &request, &displays)
            .unwrap()
            .active_display
    );
    assert!(
        overlay_info_for_label("overlay-1", &request, &displays)
            .unwrap()
            .active_display
    );
}

#[test]
fn malformed_or_missing_overlay_labels_are_typed_errors() {
    let displays = vec![display("main", 0, 1920, true)];
    let request = request(CaptureAction::Region, None);
    assert!(matches!(
        overlay_info_for_label("main", &request, &displays),
        Err(OverlayError::InvalidWindowLabel(_))
    ));
    assert!(matches!(
        overlay_info_for_label("overlay-x", &request, &displays),
        Err(OverlayError::InvalidWindowLabel(_))
    ));
    assert!(matches!(
        overlay_info_for_label("overlay-00", &request, &displays),
        Err(OverlayError::InvalidWindowLabel(_))
    ));
    assert!(matches!(
        overlay_info_for_label("overlay-7", &request, &displays),
        Err(OverlayError::DisplayIndexOutOfRange(7))
    ));
}

#[test]
fn overlapping_displays_still_select_exactly_one_active_f9_window() {
    let displays = vec![
        display("first", 0, 1920, false),
        display("second", 0, 1920, true),
    ];
    let request = request(
        CaptureAction::FixedRegion,
        Some(PhysicalRectDto {
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        }),
    );
    let active = ["overlay-0", "overlay-1"]
        .into_iter()
        .filter(|label| {
            overlay_info_for_label(label, &request, &displays)
                .unwrap()
                .active_display
        })
        .collect::<Vec<_>>();
    assert_eq!(active, ["overlay-0"]);
}

#[derive(Default)]
struct RecordingFactory {
    events: Vec<String>,
    fail_build: Option<String>,
    fail_focus: bool,
}

impl OverlayWindowFactory for RecordingFactory {
    fn build(&mut self, label: &str, _display: &Display) -> Result<(), OverlayError> {
        self.events.push(format!("build:{label}"));
        if self.fail_build.as_deref() == Some(label) {
            return Err(OverlayError::Window("scripted build failure".into()));
        }
        Ok(())
    }

    fn focus(&mut self, label: &str) -> Result<(), OverlayError> {
        self.events.push(format!("focus:{label}"));
        if self.fail_focus {
            return Err(OverlayError::Window("scripted focus failure".into()));
        }
        Ok(())
    }

    fn cleanup(&mut self, label: &str) -> Result<(), OverlayError> {
        self.events.push(format!("cleanup:{label}"));
        Ok(())
    }
}

#[test]
fn all_windows_build_before_exactly_one_active_f9_focus() {
    let displays = vec![
        display("left", -1280, 1280, false),
        display("main", 0, 1920, true),
    ];
    let request = request(
        CaptureAction::FixedRegion,
        Some(PhysicalRectDto {
            x: -1100,
            y: 100,
            width: 800,
            height: 600,
        }),
    );
    let mut factory = RecordingFactory::default();

    create_overlay_windows(&request, &displays, &mut factory).unwrap();

    assert_eq!(
        factory.events,
        ["build:overlay-0", "build:overlay-1", "focus:overlay-0"]
    );
}

#[test]
fn first_middle_and_focus_failures_cleanup_every_built_overlay() {
    let displays = vec![
        display("left", -1280, 1280, false),
        display("main", 0, 1920, true),
    ];
    let request = request(
        CaptureAction::FixedRegion,
        Some(PhysicalRectDto {
            x: -1100,
            y: 100,
            width: 800,
            height: 600,
        }),
    );

    for (failure, expected) in [
        (
            Some("overlay-0"),
            vec!["build:overlay-0", "cleanup:overlay-0"],
        ),
        (
            Some("overlay-1"),
            vec![
                "build:overlay-0",
                "build:overlay-1",
                "cleanup:overlay-1",
                "cleanup:overlay-0",
            ],
        ),
    ] {
        let mut factory = RecordingFactory {
            fail_build: failure.map(str::to_owned),
            ..Default::default()
        };
        assert!(create_overlay_windows(&request, &displays, &mut factory).is_err());
        assert_eq!(factory.events, expected);
    }

    let mut factory = RecordingFactory {
        fail_focus: true,
        ..Default::default()
    };
    assert!(create_overlay_windows(&request, &displays, &mut factory).is_err());
    assert_eq!(
        factory.events,
        [
            "build:overlay-0",
            "build:overlay-1",
            "focus:overlay-0",
            "cleanup:overlay-1",
            "cleanup:overlay-0"
        ]
    );
}
