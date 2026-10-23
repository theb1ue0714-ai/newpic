use newpic_core::settings::Size;
use newpic_desktop_lib::app_state::{SelectionMode, SelectionRequest};
use newpic_desktop_lib::hotkeys::CaptureAction;
use newpic_desktop_lib::overlay::{
    close_overlay_windows as close_session_windows,
    create_overlay_windows as create_session_windows,
    overlay_info_for_label as session_overlay_info, OverlayError, OverlayWindowCloser,
    OverlayWindowFactory, PhysicalRectDto,
};

fn create_overlay_windows(
    request: &SelectionRequest,
    displays: &[Display],
    factory: &mut impl OverlayWindowFactory,
) -> Result<(), OverlayError> {
    create_session_windows(1, request, displays, factory)
}

fn overlay_info_for_label(
    label: &str,
    request: &SelectionRequest,
    displays: &[Display],
) -> Result<newpic_desktop_lib::overlay::OverlayInfo, OverlayError> {
    let index = label.strip_prefix("overlay-").unwrap_or(label);
    session_overlay_info(&format!("overlay-1-{index}"), 1, request, displays)
}

fn close_overlay_windows(closer: &mut RecordingCloser) -> Result<(), OverlayError> {
    let labels = closer
        .visible
        .iter()
        .filter(|label| label.starts_with("overlay-"))
        .cloned()
        .collect::<Vec<_>>();
    let active = labels.first().map(String::as_str);
    close_session_windows(closer, &labels, active)
}
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
    visible: Vec<String>,
    events: Vec<String>,
    fail_hide: Option<String>,
    close_error: bool,
    stays_visible: bool,
    waits: usize,
}

impl OverlayWindowCloser for RecordingCloser {
    fn hide(&mut self, label: &str) -> Result<(), OverlayError> {
        self.events.push(format!("hide:{label}"));
        if self.fail_hide.as_deref() == Some(label) {
            return Err(OverlayError::Window("hide failed".into()));
        }
        if !self.stays_visible {
            self.visible.retain(|item| item != label);
        }
        Ok(())
    }
    fn show(&mut self, label: &str) -> Result<(), OverlayError> {
        self.events.push(format!("show:{label}"));
        if !self.visible.iter().any(|item| item == label) {
            self.visible.push(label.to_owned());
        }
        Ok(())
    }
    fn focus(&mut self, label: &str) -> Result<(), OverlayError> {
        self.events.push(format!("refocus:{label}"));
        Ok(())
    }
    fn is_visible(&self, label: &str) -> bool {
        self.visible.iter().any(|item| item == label)
    }
    fn close_best_effort(&mut self, label: &str) {
        self.events.push(format!("close:{label}"));
        if !self.close_error {
            self.visible.retain(|item| item != label);
        }
    }
    fn wait_for_state_change(&mut self) {
        self.waits += 1;
    }
}

#[test]
fn close_waits_for_verified_removal_and_reports_failures() {
    let mut success = RecordingCloser {
        visible: vec!["overlay-0".into(), "main".into()],
        close_error: true,
        ..Default::default()
    };
    close_overlay_windows(&mut success).unwrap();
    assert_eq!(success.visible, ["main"]);
    assert_eq!(success.events, ["hide:overlay-0", "close:overlay-0"]);

    let mut failed = RecordingCloser {
        visible: vec!["overlay-0".into(), "overlay-1".into()],
        fail_hide: Some("overlay-1".into()),
        ..Default::default()
    };
    assert!(matches!(
        close_overlay_windows(&mut failed),
        Err(OverlayError::Window(_))
    ));
    assert_eq!(failed.visible, ["overlay-1", "overlay-0"]);
    assert!(failed.events.contains(&"show:overlay-0".into()));
    assert!(failed.events.contains(&"refocus:overlay-0".into()));

    let mut timeout = RecordingCloser {
        visible: vec!["overlay-0".into()],
        stays_visible: true,
        ..Default::default()
    };
    assert!(matches!(
        close_overlay_windows(&mut timeout),
        Err(OverlayError::CloseTimeout(_))
    ));
    assert_eq!(timeout.waits, 100);
}

#[test]
fn hide_failure_restores_hidden_windows_then_focuses_only_active_once() {
    let labels = vec![
        "overlay-7-0".into(),
        "overlay-7-1".into(),
        "overlay-7-2".into(),
    ];
    for failing in &labels {
        let mut closer = RecordingCloser {
            visible: labels.clone(),
            fail_hide: Some(failing.clone()),
            ..Default::default()
        };
        assert!(close_session_windows(&mut closer, &labels, Some("overlay-7-1")).is_err());
        let focuses = closer
            .events
            .iter()
            .filter(|event| event.starts_with("refocus:"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(focuses, ["refocus:overlay-7-1"]);
        let focus_index = closer
            .events
            .iter()
            .position(|event| event == "refocus:overlay-7-1")
            .unwrap();
        assert!(!closer.events[focus_index + 1..]
            .iter()
            .any(|event| event.starts_with("show:")));
        assert!(!closer
            .events
            .iter()
            .any(|event| event == "refocus:overlay-7-0" || event == "refocus:overlay-7-2"));
    }
}

#[test]
fn session_unique_labels_do_not_collide_after_best_effort_close_failure() {
    let displays = vec![display("main", 0, 1920, true)];
    let request = request(
        CaptureAction::FixedRegion,
        Some(PhysicalRectDto {
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        }),
    );
    let mut first = RecordingFactory::default();
    create_session_windows(41, &request, &displays, &mut first).unwrap();
    let mut second = RecordingFactory::default();
    create_session_windows(42, &request, &displays, &mut second).unwrap();
    assert!(first
        .events
        .iter()
        .any(|event| event == "build:overlay-41-0"));
    assert!(second
        .events
        .iter()
        .any(|event| event == "build:overlay-42-0"));
    assert!(first
        .events
        .iter()
        .all(|event| !event.contains("overlay-42-")));
}

fn request(action: CaptureAction, initial_rect: Option<PhysicalRectDto>) -> SelectionRequest {
    let selected_display_id =
        initial_rect.map(|rect| if rect.x < 0 { "left" } else { "main" }.to_owned());
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
        selected_display_id,
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
    let mut request = request(
        CaptureAction::FixedRegion,
        Some(PhysicalRectDto {
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        }),
    );
    request.selected_display_id = Some("first".into());
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
    fail_cleanup: bool,
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
        if self.fail_cleanup {
            Err(OverlayError::Window(format!("cleanup failed {label}")))
        } else {
            Ok(())
        }
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
        [
            "build:overlay-1-0",
            "build:overlay-1-1",
            "focus:overlay-1-0"
        ]
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
            Some("overlay-1-0"),
            vec!["build:overlay-1-0", "cleanup:overlay-1-0"],
        ),
        (
            Some("overlay-1-1"),
            vec![
                "build:overlay-1-0",
                "build:overlay-1-1",
                "cleanup:overlay-1-1",
                "cleanup:overlay-1-0",
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
            "build:overlay-1-0",
            "build:overlay-1-1",
            "focus:overlay-1-0",
            "cleanup:overlay-1-1",
            "cleanup:overlay-1-0"
        ]
    );
}

#[test]
fn startup_failure_attempts_every_cleanup_and_aggregates_cleanup_errors() {
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
    let mut factory = RecordingFactory {
        fail_focus: true,
        fail_cleanup: true,
        ..Default::default()
    };
    assert!(matches!(
        create_overlay_windows(&request, &displays, &mut factory),
        Err(OverlayError::CleanupFailures { .. })
    ));
    assert!(factory.events.contains(&"cleanup:overlay-1-0".into()));
    assert!(factory.events.contains(&"cleanup:overlay-1-1".into()));
}
