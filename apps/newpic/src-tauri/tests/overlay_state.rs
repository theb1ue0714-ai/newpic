use std::cell::{Cell, RefCell};
use std::fs;

use newpic_core::settings::{
    ConditionalSave, CorruptionRecovery, ExpectedSettingsState, Settings, SettingsRepository,
    SettingsRepositoryError, Size,
};
use newpic_desktop_lib::app_state::{plan_capture, CapturePlan, SelectionMode, SelectionRequest};
use newpic_desktop_lib::commands::{
    complete_selection_with_effects, persist_settings_to_repository, prepare_settings_for_save,
};
use newpic_desktop_lib::hotkeys::CaptureAction;
use newpic_desktop_lib::overlay::{OverlayCoordinator, OverlayError, PhysicalRectDto};
use newpic_platform::geometry::{
    Display, DisplayId, DisplayOrientation, PhysicalRect, SavedFixedRegion, ScaleFactor,
};

fn test_display(
    id: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    primary: bool,
) -> Display {
    Display::new(
        DisplayId::new(id).unwrap(),
        PhysicalRect::new(x, y, width, height).unwrap(),
        ScaleFactor::new(scale).unwrap(),
        primary,
        DisplayOrientation::Landscape,
    )
}

fn request(action: CaptureAction, initial_rect: Option<PhysicalRect>) -> SelectionRequest {
    SelectionRequest {
        action,
        mode: if action == CaptureAction::FixedRegion {
            SelectionMode::FixedSize
        } else {
            SelectionMode::FreeRegion
        },
        fixed_size: (action == CaptureAction::FixedRegion).then(|| Size::new(800, 600)),
        fallback_from: None,
        initial_rect: initial_rect.map(PhysicalRectDto::from),
    }
}

#[derive(Default)]
struct RecordingRepository {
    fail: Cell<bool>,
    saves: RefCell<Vec<Settings>>,
    events: RefCell<Vec<&'static str>>,
}

impl SettingsRepository for RecordingRepository {
    fn load(&self) -> Result<Option<String>, SettingsRepositoryError> {
        Ok(None)
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsRepositoryError> {
        if self.fail.get() {
            return Err(std::io::Error::other("scripted save failure").into());
        }
        self.events.borrow_mut().push("save");
        self.saves.borrow_mut().push(settings.clone());
        Ok(())
    }

    fn save_if_unchanged(
        &self,
        _expected: ExpectedSettingsState<'_>,
        settings: &Settings,
    ) -> Result<ConditionalSave, SettingsRepositoryError> {
        self.save(settings)?;
        Ok(ConditionalSave::Saved)
    }

    fn recover_corrupted_if_unchanged(
        &self,
        _expected: &str,
        replacement: &Settings,
    ) -> Result<CorruptionRecovery, SettingsRepositoryError> {
        self.save(replacement)?;
        Ok(CorruptionRecovery::Recovered {
            backup_path: "settings.corrupt.json".into(),
        })
    }
}

#[test]
fn f7_completion_updates_f8_but_f9_completion_never_does() {
    let mut coordinator = OverlayCoordinator::new();
    let f7_rect = PhysicalRect::new(100, 150, 400, 300).unwrap();
    coordinator
        .start_selection(request(CaptureAction::Region, None))
        .unwrap();
    let completed = coordinator.complete(f7_rect).unwrap();
    assert_eq!(completed.action, CaptureAction::Region);
    assert_eq!(coordinator.last_region(), Some(f7_rect));

    let f9_rect = PhysicalRect::new(500, 200, 800, 600).unwrap();
    coordinator
        .start_selection(request(CaptureAction::FixedRegion, Some(f9_rect)))
        .unwrap();
    let completed = coordinator.complete(f9_rect).unwrap();
    assert_eq!(completed.action, CaptureAction::FixedRegion);
    assert_eq!(coordinator.last_region(), Some(f7_rect));
}

#[test]
fn escape_clears_only_the_active_request() {
    let mut coordinator = OverlayCoordinator::new();
    let f8_rect = PhysicalRect::new(100, 150, 400, 300).unwrap();
    coordinator
        .start_selection(request(CaptureAction::Region, None))
        .unwrap();
    coordinator.complete(f8_rect).unwrap();
    coordinator
        .start_selection(request(
            CaptureAction::FixedRegion,
            Some(PhysicalRect::new(500, 200, 800, 600).unwrap()),
        ))
        .unwrap();
    coordinator.cancel();

    assert!(!coordinator.is_active());
    assert_eq!(coordinator.last_region(), Some(f8_rect));
}

#[test]
fn f9_plan_restores_saved_rectangle_after_restart() {
    let displays = vec![test_display("main", 0, 0, 1920, 1080, 1.0, true)];
    let rect = PhysicalRect::new(220, 140, 900, 520).unwrap();
    let settings = Settings {
        fixed_region: Size::new(900, 520),
        saved_fixed_region: Some(SavedFixedRegion::capture(rect, &displays).unwrap()),
        ..Settings::default()
    };

    let CapturePlan::Select(request) =
        plan_capture(CaptureAction::FixedRegion, &settings, &displays).unwrap()
    else {
        panic!("F9 must open selection");
    };
    assert_eq!(request.initial_rect, Some(rect.into()));
}

#[test]
fn f9_plan_falls_back_inside_primary_when_topology_changes() {
    let old = vec![test_display("left", -1280, 0, 1280, 1024, 1.0, true)];
    let settings = Settings {
        fixed_region: Size::new(640, 480),
        saved_fixed_region: Some(
            SavedFixedRegion::capture(PhysicalRect::new(-1200, 80, 640, 480).unwrap(), &old)
                .unwrap(),
        ),
        ..Settings::default()
    };
    let current = vec![test_display("main", 0, 0, 500, 400, 1.25, true)];

    let CapturePlan::Select(request) =
        plan_capture(CaptureAction::FixedRegion, &settings, &current).unwrap()
    else {
        panic!("F9 must open selection");
    };
    assert_eq!(
        request.initial_rect,
        Some(PhysicalRectDto {
            x: 0,
            y: 0,
            width: 500,
            height: 400,
        })
    );
}

#[test]
fn failed_f9_save_retains_active_overlay_and_does_not_dispatch_capture() {
    let displays = vec![test_display("main", 0, 0, 1920, 1080, 1.0, true)];
    let mut coordinator = OverlayCoordinator::new();
    coordinator
        .start_selection(request(
            CaptureAction::FixedRegion,
            Some(PhysicalRect::new(100, 100, 800, 600).unwrap()),
        ))
        .unwrap();
    let repository = RecordingRepository::default();
    repository.fail.set(true);

    let closed = Cell::new(false);
    let captured = Cell::new(false);
    let result = complete_selection_with_effects(
        &mut coordinator,
        &Settings::default(),
        &repository,
        &displays,
        PhysicalRect::new(160, 180, 700, 500).unwrap(),
        (
            |_| panic!("failed save must not apply settings"),
            || closed.set(true),
            || captured.set(true),
        ),
    );

    assert!(result.is_err());
    assert!(
        coordinator.is_active(),
        "window must remain usable for retry"
    );
    assert!(repository.saves.borrow().is_empty());
    assert!(!closed.get());
    assert!(!captured.get());
}

#[test]
fn successful_f9_save_precedes_completion_and_updates_size() {
    let displays = vec![test_display("main", 0, 0, 1920, 1080, 1.0, true)];
    let rect = PhysicalRect::new(160, 180, 700, 500).unwrap();
    let mut coordinator = OverlayCoordinator::new();
    coordinator
        .start_selection(request(CaptureAction::FixedRegion, Some(rect)))
        .unwrap();
    let repository = RecordingRepository::default();

    let committed = complete_selection_with_effects(
        &mut coordinator,
        &Settings::default(),
        &repository,
        &displays,
        rect,
        (
            |_| repository.events.borrow_mut().push("apply"),
            || repository.events.borrow_mut().push("close"),
            || repository.events.borrow_mut().push("capture"),
        ),
    )
    .unwrap();

    assert_eq!(committed.request.action, CaptureAction::FixedRegion);
    assert_eq!(committed.settings.fixed_region, Size::new(700, 500));
    assert_eq!(
        committed
            .settings
            .saved_fixed_region
            .as_ref()
            .unwrap()
            .rect(),
        rect
    );
    assert_eq!(repository.saves.borrow().as_slice(), &[committed.settings]);
    assert_eq!(
        repository.events.borrow().as_slice(),
        &["save", "apply", "close", "capture"]
    );
    assert!(!coordinator.is_active());
}

#[test]
fn settings_size_edit_preserves_or_clamps_saved_position_without_touching_f8() {
    let displays = vec![test_display("main", 0, 0, 1000, 800, 1.0, true)];
    let previous = Settings {
        saved_fixed_region: Some(
            SavedFixedRegion::capture(PhysicalRect::new(300, 200, 600, 400).unwrap(), &displays)
                .unwrap(),
        ),
        ..Settings::default()
    };
    let mut requested = previous.clone();
    requested.fixed_region = Size::new(800, 700);
    let mut coordinator = OverlayCoordinator::new();
    let f8_rect = PhysicalRect::new(10, 20, 300, 200).unwrap();
    coordinator
        .start_selection(request(CaptureAction::Region, None))
        .unwrap();
    coordinator.complete(f8_rect).unwrap();

    let prepared = prepare_settings_for_save(&previous, requested, &displays).unwrap();
    assert_eq!(
        prepared.saved_fixed_region.unwrap().rect(),
        PhysicalRect::new(200, 100, 800, 700).unwrap()
    );
    assert_eq!(coordinator.last_region(), Some(f8_rect));
}

#[test]
fn settings_size_edit_synchronizes_size_when_display_forces_reduction() {
    let displays = vec![test_display("main", 0, 0, 500, 400, 1.0, true)];
    let previous = Settings {
        fixed_region: Size::new(300, 200),
        saved_fixed_region: Some(
            SavedFixedRegion::capture(PhysicalRect::new(100, 100, 300, 200).unwrap(), &displays)
                .unwrap(),
        ),
        ..Settings::default()
    };
    let requested = Settings {
        fixed_region: Size::new(900, 700),
        ..previous.clone()
    };

    let prepared = prepare_settings_for_save(&previous, requested, &displays).unwrap();
    assert_eq!(prepared.fixed_region, Size::new(500, 400));
    assert_eq!(
        prepared.saved_fixed_region.unwrap().rect(),
        PhysicalRect::new(0, 0, 500, 400).unwrap()
    );
}

#[test]
fn command_service_default_reset_clears_f9_and_never_deletes_images() {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join("001.png");
    fs::write(&image, b"captured pixels").unwrap();
    let displays = vec![test_display("main", 0, 0, 1920, 1080, 1.0, true)];
    let previous = Settings {
        fixed_region: Size::new(900, 520),
        saved_fixed_region: Some(
            SavedFixedRegion::capture(PhysicalRect::new(220, 140, 900, 520).unwrap(), &displays)
                .unwrap(),
        ),
        ..Settings::default()
    };
    let repository = RecordingRepository::default();

    let reset =
        persist_settings_to_repository(&repository, &previous, Settings::default(), &displays)
            .unwrap();

    assert_eq!(reset.fixed_region, Size::new(800, 600));
    assert_eq!(reset.saved_fixed_region, None);
    assert_eq!(fs::read(&image).unwrap(), b"captured pixels");
    assert_eq!(repository.saves.borrow().as_slice(), &[reset]);
}

#[test]
fn overlay_coordinator_rejects_concurrent_selection() {
    let mut coordinator = OverlayCoordinator::new();
    coordinator
        .start_selection(request(CaptureAction::Region, None))
        .unwrap();
    assert_eq!(
        coordinator
            .start_selection(request(CaptureAction::FixedRegion, None))
            .unwrap_err(),
        OverlayError::AlreadyActive
    );
}
