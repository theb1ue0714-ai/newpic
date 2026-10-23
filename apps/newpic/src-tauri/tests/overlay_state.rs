use std::cell::{Cell, RefCell};
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use newpic_core::settings::{
    ConditionalSave, CorruptionRecovery, ExpectedSettingsState, JsonFileSettingsRepository,
    Settings, SettingsRepository, SettingsRepositoryError, Size,
};
use newpic_desktop_lib::app_state::{
    plan_capture, AppPaths, AppState, CaptureGate, CapturePlan, SelectionMode, SelectionRequest,
};
use newpic_desktop_lib::commands::{
    complete_selection_with_effects, merge_read_only_saved_geometry, persist_settings_and_replace,
    prepare_settings_for_save, with_capture_reservation,
};
use newpic_desktop_lib::hotkeys::CaptureAction;
use newpic_desktop_lib::overlay::{
    overlay_info_for_label, start_overlay_session, OverlayCoordinator, OverlayError,
    PhysicalRectDto,
};
use newpic_platform::clipboard::{ClipboardError, ClipboardImage, ImageClipboard};
use newpic_platform::geometry::{
    Display, DisplayId, DisplayOrientation, PhysicalRect, SavedFixedRegion, ScaleFactor,
};
use newpic_testing::FakeCaptureProvider;

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
    let selected_display_id =
        initial_rect.map(|rect| if rect.origin().x < 0 { "left" } else { "main" }.to_owned());
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
        selected_display_id,
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

struct NoopClipboard;

impl ImageClipboard for NoopClipboard {
    fn write_image(&self, _image: &ClipboardImage) -> Result<(), ClipboardError> {
        Ok(())
    }
}

fn real_state(root: &std::path::Path, settings: Settings) -> AppState {
    let settings_file = root.join("settings.json");
    AppState::new(
        AppPaths {
            settings_file: settings_file.clone(),
            downloads_dir: root.join("downloads"),
            recovery_dir: root.join("recovery"),
        },
        JsonFileSettingsRepository::new(settings_file),
        settings,
        None,
        Box::new(FakeCaptureProvider::default()),
        Box::new(NoopClipboard),
    )
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
    assert_eq!(request.selected_display_id.as_deref(), Some("main"));
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
            || {
                closed.set(true);
                Ok(())
            },
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
            || {
                repository.events.borrow_mut().push("close");
                Ok(())
            },
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
fn close_failure_blocks_capture_and_keeps_selection_retryable() {
    let displays = vec![test_display("main", 0, 0, 1920, 1080, 1.0, true)];
    let rect = PhysicalRect::new(160, 180, 700, 500).unwrap();
    let mut coordinator = OverlayCoordinator::new();
    coordinator
        .start_selection(request(CaptureAction::FixedRegion, Some(rect)))
        .unwrap();
    let repository = RecordingRepository::default();
    let captured = Cell::new(false);

    let result = complete_selection_with_effects(
        &mut coordinator,
        &Settings::default(),
        &repository,
        &displays,
        rect,
        (
            |_| {},
            || {
                Err(newpic_desktop_lib::commands::CommandError {
                    message: "close failed".into(),
                })
            },
            || captured.set(true),
        ),
    );
    assert!(result.is_err());
    assert!(coordinator.is_active());
    assert!(!captured.get());
}

#[test]
fn busy_capture_reservation_prevents_every_completion_side_effect() {
    let gate = CaptureGate::default();
    let held = gate.try_reserve().unwrap();
    let touched = Cell::new(false);
    let result = with_capture_reservation(gate.try_reserve(), |_| {
        touched.set(true);
        Ok(())
    });
    assert!(result.is_err());
    assert!(!touched.get());
    drop(held);
    assert!(gate.try_reserve().is_some());
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
    let state = real_state(temp.path(), previous.clone());
    state.repository.save(&previous).unwrap();
    let reset = persist_settings_and_replace(
        &state.repository,
        &mut state.settings_transaction(),
        Settings::default(),
        &displays,
    )
    .unwrap();

    assert_eq!(reset.fixed_region, Size::new(800, 600));
    assert_eq!(reset.saved_fixed_region, None);
    assert_eq!(fs::read(&image).unwrap(), b"captured pixels");
    assert_eq!(state.settings(), reset);
    let disk: Settings =
        serde_json::from_str(&fs::read_to_string(&state.paths.settings_file).unwrap()).unwrap();
    assert_eq!(disk, reset);
}

#[test]
fn settings_transaction_serializes_interleaving_without_lost_f9_or_disk_drift() {
    let temp = tempfile::tempdir().unwrap();
    let state = Arc::new(real_state(temp.path(), Settings::default()));
    state.repository.save(&Settings::default()).unwrap();
    let barrier = Arc::new(Barrier::new(2));

    let first_state = Arc::clone(&state);
    let first_barrier = Arc::clone(&barrier);
    let first = thread::spawn(move || {
        let displays = first_state.displays().unwrap();
        let mut guard = first_state.settings_transaction();
        let rect = PhysicalRect::new(100, 100, 700, 500).unwrap();
        guard.fixed_region = Size::new(700, 500);
        guard.saved_fixed_region = Some(SavedFixedRegion::capture(rect, &displays).unwrap());
        first_state.repository.save(&guard).unwrap();
        first_barrier.wait();
    });
    let second_state = Arc::clone(&state);
    let second = thread::spawn(move || {
        barrier.wait();
        let mut guard = second_state.settings_transaction();
        guard.include_pointer = true;
        second_state.repository.save(&guard).unwrap();
    });
    first.join().unwrap();
    second.join().unwrap();

    let memory = state.settings();
    let disk: Settings =
        serde_json::from_str(&fs::read_to_string(&state.paths.settings_file).unwrap()).unwrap();
    assert!(memory.include_pointer);
    assert!(memory.saved_fixed_region.is_some());
    assert_eq!(disk, memory);
}

#[test]
fn stale_settings_form_cannot_undo_a_concurrently_completed_f9() {
    let displays = vec![test_display("main", 0, 0, 1920, 1080, 1.0, true)];
    let stale = Settings::default();
    let rect = PhysicalRect::new(100, 100, 700, 500).unwrap();
    let current = Settings {
        fixed_region: Size::new(700, 500),
        saved_fixed_region: Some(SavedFixedRegion::capture(rect, &displays).unwrap()),
        ..Settings::default()
    };
    let mut unrelated_form_save = Settings {
        include_pointer: true,
        ..stale
    };
    merge_read_only_saved_geometry(&current, &mut unrelated_form_save);
    assert!(unrelated_form_save.include_pointer);
    assert_eq!(unrelated_form_save.fixed_region, Size::new(700, 500));
    assert_eq!(
        unrelated_form_save.saved_fixed_region,
        current.saved_fixed_region
    );
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

#[test]
fn selection_session_keeps_one_display_snapshot_and_cancels_on_open_failure() {
    let original = vec![
        test_display("left", -1280, 0, 1280, 1024, 1.25, false),
        test_display("main", 0, 0, 1920, 1080, 1.0, true),
    ];
    let reordered = [original[1].clone(), original[0].clone()];
    let rect = PhysicalRect::new(-1100, 100, 800, 600).unwrap();
    let request = request(CaptureAction::FixedRegion, Some(rect));
    let mut coordinator = OverlayCoordinator::new();
    start_overlay_session(
        &mut coordinator,
        request.clone(),
        original.clone(),
        |_, _| Ok(()),
    )
    .unwrap();
    let info = overlay_info_for_label(
        "overlay-0",
        coordinator.active_request().unwrap(),
        coordinator.active_displays(),
    )
    .unwrap();
    assert_eq!(info.display_bounds, original[0].bounds().into());
    assert_ne!(info.display_bounds, reordered[0].bounds().into());
    coordinator.cancel();

    let error = start_overlay_session(&mut coordinator, request, original, |_, _| {
        Err(OverlayError::Window("focus failed".into()))
    });
    assert!(error.is_err());
    assert!(!coordinator.is_active());
    assert!(coordinator.active_displays().is_empty());
}

#[test]
fn completion_revalidates_current_topology_after_snapshotted_display_is_removed() {
    let original = vec![test_display("left", -1280, 0, 1280, 1024, 1.25, true)];
    let current = vec![test_display("main", 0, 0, 1920, 1080, 1.0, true)];
    let rect = PhysicalRect::new(-1100, 100, 800, 600).unwrap();
    let mut coordinator = OverlayCoordinator::new();
    start_overlay_session(
        &mut coordinator,
        request(CaptureAction::FixedRegion, Some(rect)),
        original,
        |_, _| Ok(()),
    )
    .unwrap();
    let repository = RecordingRepository::default();
    let result = complete_selection_with_effects(
        &mut coordinator,
        &Settings::default(),
        &repository,
        &current,
        rect,
        (|_| {}, || Ok(()), || {}),
    );
    assert!(result.is_err());
    assert!(coordinator.is_active());
    assert!(repository.saves.borrow().is_empty());
}

#[test]
fn mirrored_f9_persists_the_explicitly_selected_display() {
    let first = test_display("first", 0, 0, 1920, 1080, 1.0, false);
    let second = test_display("second", 0, 0, 1920, 1080, 1.0, true);
    let displays = vec![first, second.clone()];
    let rect = PhysicalRect::new(100, 100, 700, 500).unwrap();
    let request = SelectionRequest {
        action: CaptureAction::FixedRegion,
        mode: SelectionMode::FixedSize,
        fixed_size: Some(Size::new(700, 500)),
        fallback_from: None,
        initial_rect: Some(rect.into()),
        selected_display_id: Some("second".into()),
    };
    let mut coordinator = OverlayCoordinator::new();
    start_overlay_session(&mut coordinator, request, displays.clone(), |_, _| Ok(())).unwrap();
    let repository = RecordingRepository::default();
    let committed = complete_selection_with_effects(
        &mut coordinator,
        &Settings::default(),
        &repository,
        &displays,
        rect,
        (|_| {}, || Ok(()), || {}),
    )
    .unwrap();
    assert_eq!(
        committed.settings.saved_fixed_region.unwrap().display_id(),
        second.id()
    );
}
