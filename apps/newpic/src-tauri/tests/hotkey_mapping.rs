use std::collections::HashSet;
use std::str::FromStr;

use newpic_core::settings::{Hotkeys, Settings};
use newpic_desktop_lib::hotkeys::{
    CaptureAction, HotkeyBindings, HotkeyError, HotkeyManager, RegistrationStatus,
    ShortcutRegistrar,
};
use tauri_plugin_global_shortcut::Shortcut;

#[derive(Debug, Default)]
struct FakeRegistrar {
    registered: HashSet<Shortcut>,
    taken_by_other_apps: HashSet<Shortcut>,
    register_calls: usize,
    unregister_calls: usize,
}

impl FakeRegistrar {
    fn with_taken(accelerators: &[&str]) -> Self {
        Self {
            taken_by_other_apps: accelerators.iter().map(|value| shortcut(value)).collect(),
            ..Self::default()
        }
    }

    fn has(&self, accelerator: &str) -> bool {
        self.registered.contains(&shortcut(accelerator))
    }
}

impl ShortcutRegistrar for FakeRegistrar {
    fn register(&mut self, shortcut: &Shortcut) -> Result<(), String> {
        self.register_calls += 1;
        if self.taken_by_other_apps.contains(shortcut) {
            return Err("already registered by another application".into());
        }
        if !self.registered.insert(*shortcut) {
            return Err("already registered".into());
        }
        Ok(())
    }

    fn unregister(&mut self, shortcut: &Shortcut) -> Result<(), String> {
        self.unregister_calls += 1;
        if self.registered.remove(shortcut) {
            Ok(())
        } else {
            Err("not registered".into())
        }
    }
}

fn shortcut(accelerator: &str) -> Shortcut {
    Shortcut::from_str(accelerator).unwrap()
}

fn registered(accelerator: &str) -> RegistrationStatus {
    RegistrationStatus::Registered {
        accelerator: accelerator.to_owned(),
    }
}

fn started(registrar: &mut FakeRegistrar) -> HotkeyManager {
    let mut manager = HotkeyManager::new();
    manager.register_all(registrar, &Hotkeys::default());
    manager
}

#[test]
fn approved_preset_maps_f7_through_f10_to_the_four_capture_actions() {
    let bindings = HotkeyBindings::from_settings(&Settings::default().hotkeys);

    assert_eq!(
        bindings.entries(),
        [
            (CaptureAction::Region, "F7"),
            (CaptureAction::RepeatRegion, "F8"),
            (CaptureAction::FixedRegion, "F9"),
            (CaptureAction::ActiveWindow, "F10"),
        ]
    );
    assert_eq!(CaptureAction::Region.label(), "영역 캡처");
    assert_eq!(CaptureAction::RepeatRegion.label(), "마지막 영역 다시 캡처");
    assert_eq!(CaptureAction::FixedRegion.label(), "고정 크기 캡처");
    assert_eq!(CaptureAction::ActiveWindow.label(), "활성 창 캡처");
}

#[test]
fn every_approved_shortcut_registers_and_dispatches_to_its_action() {
    let mut registrar = FakeRegistrar::default();
    let manager = started(&mut registrar);

    for (action, accelerator) in [
        (CaptureAction::Region, "F7"),
        (CaptureAction::RepeatRegion, "F8"),
        (CaptureAction::FixedRegion, "F9"),
        (CaptureAction::ActiveWindow, "F10"),
    ] {
        assert_eq!(manager.status(action), &registered(accelerator));
        assert!(registrar.has(accelerator));
        assert_eq!(manager.action_for(&shortcut(accelerator)), Some(action));
    }
    assert_eq!(manager.action_for(&shortcut("F11")), None);
}

#[test]
fn one_failed_registration_disables_only_that_action() {
    let mut registrar = FakeRegistrar::with_taken(&["F9"]);
    let manager = started(&mut registrar);

    let RegistrationStatus::Failed {
        accelerator,
        reason,
    } = manager.status(CaptureAction::FixedRegion)
    else {
        panic!("F9 should fail");
    };
    assert_eq!(accelerator, "F9");
    assert!(reason.contains("another application"), "{reason}");
    assert_eq!(manager.action_for(&shortcut("F9")), None);

    assert_eq!(manager.status(CaptureAction::Region), &registered("F7"));
    assert_eq!(
        manager.status(CaptureAction::RepeatRegion),
        &registered("F8")
    );
    assert_eq!(
        manager.status(CaptureAction::ActiveWindow),
        &registered("F10")
    );
    assert_eq!(registrar.registered.len(), 3);
}

#[test]
fn invalid_and_duplicate_settings_fail_per_action() {
    let hotkeys = Hotkeys {
        region: "F7".into(),
        repeat_region: "NotAKey".into(),
        fixed_region: "f7".into(),
        active_window: "F10".into(),
    };
    let mut registrar = FakeRegistrar::default();
    let mut manager = HotkeyManager::new();

    manager.register_all(&mut registrar, &hotkeys);

    assert_eq!(manager.status(CaptureAction::Region), &registered("F7"));
    assert!(matches!(
        manager.status(CaptureAction::RepeatRegion),
        RegistrationStatus::Failed { .. }
    ));
    let RegistrationStatus::Failed { reason, .. } = manager.status(CaptureAction::FixedRegion)
    else {
        panic!("duplicate F7 should fail");
    };
    assert!(reason.contains("영역 캡처"), "{reason}");
    assert_eq!(
        manager.status(CaptureAction::ActiveWindow),
        &registered("F10")
    );
    assert_eq!(
        manager.action_for(&shortcut("F7")),
        Some(CaptureAction::Region)
    );
}

#[test]
fn successful_replacement_moves_dispatch_to_the_new_shortcut() {
    let mut registrar = FakeRegistrar::default();
    let mut manager = started(&mut registrar);

    manager
        .replace(&mut registrar, CaptureAction::Region, "Ctrl+Shift+S")
        .expect("replacement succeeds");

    assert_eq!(
        manager.status(CaptureAction::Region),
        &registered("Ctrl+Shift+S")
    );
    assert!(!registrar.has("F7"));
    assert!(registrar.has("Ctrl+Shift+S"));
    assert_eq!(
        manager.action_for(&shortcut("Ctrl+Shift+S")),
        Some(CaptureAction::Region)
    );
    assert_eq!(manager.action_for(&shortcut("F7")), None);
}

#[test]
fn replacement_taken_by_the_os_rolls_back_to_the_previous_shortcut() {
    let mut registrar = FakeRegistrar::with_taken(&["Ctrl+Shift+S"]);
    let mut manager = started(&mut registrar);

    let error = manager
        .replace(&mut registrar, CaptureAction::Region, "Ctrl+Shift+S")
        .expect_err("OS conflict");

    assert!(matches!(error, HotkeyError::Registration { .. }));
    assert_eq!(manager.status(CaptureAction::Region), &registered("F7"));
    assert!(registrar.has("F7"));
    assert_eq!(
        manager.action_for(&shortcut("F7")),
        Some(CaptureAction::Region)
    );
    for (action, accelerator) in [
        (CaptureAction::RepeatRegion, "F8"),
        (CaptureAction::FixedRegion, "F9"),
        (CaptureAction::ActiveWindow, "F10"),
    ] {
        assert_eq!(manager.status(action), &registered(accelerator));
        assert!(registrar.has(accelerator));
    }
}

#[test]
fn replacement_conflicting_with_another_action_is_rejected_before_registration() {
    let mut registrar = FakeRegistrar::default();
    let mut manager = started(&mut registrar);
    let calls_before = (registrar.register_calls, registrar.unregister_calls);

    let error = manager
        .replace(&mut registrar, CaptureAction::Region, "F8")
        .expect_err("conflicts with repeat region");

    assert_eq!(
        error,
        HotkeyError::Conflict {
            accelerator: "F8".into(),
            other: CaptureAction::RepeatRegion
        }
    );
    assert_eq!(
        (registrar.register_calls, registrar.unregister_calls),
        calls_before
    );
    assert_eq!(manager.status(CaptureAction::Region), &registered("F7"));
}

#[test]
fn invalid_replacement_is_rejected_and_keeps_the_previous_shortcut() {
    let mut registrar = FakeRegistrar::default();
    let mut manager = started(&mut registrar);

    let error = manager
        .replace(&mut registrar, CaptureAction::ActiveWindow, "Ctrl+")
        .expect_err("invalid accelerator");

    assert!(matches!(error, HotkeyError::Invalid { .. }));
    assert_eq!(
        manager.status(CaptureAction::ActiveWindow),
        &registered("F10")
    );
}

#[test]
fn replacing_a_failed_action_registers_it_when_the_new_shortcut_is_free() {
    let mut registrar = FakeRegistrar::with_taken(&["F9"]);
    let mut manager = started(&mut registrar);

    manager
        .replace(&mut registrar, CaptureAction::FixedRegion, "Shift+F9")
        .unwrap();

    assert_eq!(
        manager.status(CaptureAction::FixedRegion),
        &registered("Shift+F9")
    );
    assert_eq!(
        manager.action_for(&shortcut("Shift+F9")),
        Some(CaptureAction::FixedRegion)
    );
}

#[test]
fn applying_settings_reports_rejections_and_returns_the_effective_bindings() {
    let mut registrar = FakeRegistrar::with_taken(&["Ctrl+F8"]);
    let mut manager = started(&mut registrar);
    let requested = Hotkeys {
        region: "Ctrl+F7".into(),
        repeat_region: "Ctrl+F8".into(),
        fixed_region: "F9".into(),
        active_window: "F10".into(),
    };

    let outcome = manager.apply(&mut registrar, &requested);

    assert_eq!(outcome.rejected.len(), 1);
    assert_eq!(outcome.rejected[0].0, CaptureAction::RepeatRegion);
    assert_eq!(
        outcome.effective,
        Hotkeys {
            region: "Ctrl+F7".into(),
            repeat_region: "F8".into(),
            fixed_region: "F9".into(),
            active_window: "F10".into(),
        }
    );
    assert_eq!(
        manager.status(CaptureAction::Region),
        &registered("Ctrl+F7")
    );
    assert_eq!(
        manager.status(CaptureAction::RepeatRegion),
        &registered("F8")
    );
}

#[test]
fn registration_report_lists_every_action_in_stable_order() {
    let mut registrar = FakeRegistrar::with_taken(&["F10"]);
    let manager = started(&mut registrar);

    let report = manager.report();

    assert_eq!(
        report.iter().map(|entry| entry.action).collect::<Vec<_>>(),
        CaptureAction::ALL
    );
    assert!(report[3].status.is_failed());
    assert!(!report[0].status.is_failed());
}
