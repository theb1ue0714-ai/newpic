use std::fs;
use std::path::Path;

use newpic_core::settings::{
    load_or_recover, migrate_json, ClipboardSettings, Hotkeys, ImageFormat,
    JsonFileSettingsRepository, Language, NamingSettings, RecoveryNotice, SaveDirectory, Settings,
    SettingsRepository, SettingsValidationError, Size, CURRENT_SCHEMA_VERSION,
};

#[test]
fn default_preset_matches_every_approved_design_value() {
    let value = Settings::default();

    assert_eq!(value.schema_version, 1);
    assert_eq!(value.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(value.autostart);
    assert!(value.run_in_tray);
    assert!(value.save.enabled);
    assert_eq!(value.save.directory, SaveDirectory::DownloadsNewPic);
    assert_eq!(value.save.format, ImageFormat::Png);
    assert_eq!(value.naming.pattern, "%c");
    assert_eq!(value.naming.minimum_width, 3);
    assert!(value.clipboard.enabled);
    assert!(!value.open_editor_after_capture);
    assert!(value.success_notifications);
    assert!(!value.include_pointer);
    assert!(!value.check_updates);
    assert_eq!(value.fixed_region, Size::new(800, 600));
    assert_eq!(value.hotkeys.region, "F7");
    assert_eq!(value.hotkeys.repeat_region, "F8");
    assert_eq!(value.hotkeys.fixed_region, "F9");
    assert_eq!(value.hotkeys.active_window, "F10");
    assert_eq!(value.language, Language::Korean);
    assert_eq!(value.save.jpeg_quality, 90);
}

#[test]
fn current_settings_are_serializable_and_round_trip_without_loss() {
    let settings = Settings::default();

    let json = serde_json::to_string_pretty(&settings).expect("settings serialize");
    let decoded: Settings = serde_json::from_str(&json).expect("settings deserialize");

    assert_eq!(decoded, settings);
    assert!(json.contains("\"schema_version\": 1"));
}

#[test]
fn validation_rejects_zero_width_and_height_with_typed_errors() {
    let zero_width = Settings {
        fixed_region: Size::new(0, 600),
        ..Settings::default()
    };
    assert_eq!(
        zero_width.validate(),
        Err(SettingsValidationError::InvalidFixedRegion(Size::new(
            0, 600
        )))
    );

    let zero_height = Settings {
        fixed_region: Size::new(800, 0),
        ..Settings::default()
    };
    assert_eq!(
        zero_height.validate(),
        Err(SettingsValidationError::InvalidFixedRegion(Size::new(
            800, 0
        )))
    );
}

#[test]
fn validation_rejects_empty_shortcuts_and_save_pattern() {
    let mut settings = Settings::default();
    settings.hotkeys.fixed_region.clear();
    assert_eq!(
        settings.validate(),
        Err(SettingsValidationError::EmptyShortcut("fixed_region"))
    );

    let mut settings = Settings::default();
    settings.naming.pattern.clear();
    assert_eq!(
        settings.validate(),
        Err(SettingsValidationError::EmptyNamingPattern)
    );
}

#[test]
fn validation_rejects_unsupported_filename_tokens() {
    let mut settings = Settings::default();
    settings.naming.pattern = "capture-%c-%q".to_owned();

    assert_eq!(
        settings.validate(),
        Err(SettingsValidationError::UnsupportedNamingToken(
            "%q".to_owned()
        ))
    );

    settings.naming.pattern = "%Y-%m-%d_%H-%M-%S_%h_%c_%%".to_owned();
    assert_eq!(settings.validate(), Ok(()));
}

#[test]
fn schema_zero_fixture_migrates_values_and_adds_current_defaults() {
    let fixture = include_str!("fixtures/settings-schema-0.json");

    let migrated = migrate_json(fixture).expect("schema zero fixture migrates");

    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(!migrated.autostart);
    assert!(!migrated.save.enabled);
    assert_eq!(migrated.save.directory, SaveDirectory::DownloadsNewPic);
    assert_eq!(migrated.save.format, ImageFormat::Jpeg);
    assert_eq!(migrated.save.jpeg_quality, 72);
    assert_eq!(
        migrated.naming,
        NamingSettings {
            pattern: "archive-%Y-%m-%d-%c".to_owned(),
            minimum_width: 5,
        }
    );
    assert_eq!(migrated.clipboard, ClipboardSettings { enabled: false });
    assert_eq!(migrated.fixed_region, Size::new(640, 480));
    assert_eq!(
        migrated.hotkeys,
        Hotkeys {
            region: "Ctrl+Shift+7".to_owned(),
            repeat_region: "Ctrl+Shift+8".to_owned(),
            fixed_region: "Ctrl+Shift+9".to_owned(),
            active_window: "Ctrl+Shift+0".to_owned(),
        }
    );
    assert!(migrated.run_in_tray);
    assert!(migrated.success_notifications);
    assert!(!migrated.open_editor_after_capture);
    assert!(!migrated.include_pointer);
    assert!(!migrated.check_updates);
}

#[test]
fn missing_settings_load_the_approved_preset_and_persist_it() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let repository = JsonFileSettingsRepository::new(&path);
    assert_repository(&repository);

    let outcome = load_or_recover(&repository).expect("missing settings recover to defaults");

    assert_eq!(outcome.settings, Settings::default());
    assert_eq!(outcome.notice, None);
    assert_eq!(read_settings(&path), Settings::default());
}

#[test]
fn corrupted_json_is_backed_up_with_collision_free_names_before_recovery() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    fs::write(&path, "{ definitely not json").expect("write corrupt settings");
    let repository = JsonFileSettingsRepository::new(&path);

    let first = load_or_recover(&repository).expect("first recovery succeeds");
    let first_backup = temp.path().join("settings.corrupt.json");
    assert_eq!(
        first.notice,
        Some(RecoveryNotice::CorruptedSettings {
            backup_path: first_backup.clone()
        })
    );
    assert_eq!(
        fs::read_to_string(&first_backup).expect("read first backup"),
        "{ definitely not json"
    );
    assert_eq!(read_settings(&path), Settings::default());

    fs::write(&path, "[ still invalid").expect("write second corrupt settings");
    let second = load_or_recover(&repository).expect("second recovery succeeds");
    let second_backup = temp.path().join("settings.corrupt-1.json");
    assert_eq!(
        second.notice,
        Some(RecoveryNotice::CorruptedSettings {
            backup_path: second_backup.clone()
        })
    );
    assert_eq!(
        fs::read_to_string(second_backup).expect("read second backup"),
        "[ still invalid"
    );
}

fn assert_repository<R: SettingsRepository>(_repository: &R) {}

fn read_settings(path: &Path) -> Settings {
    let json = fs::read_to_string(path).expect("read persisted settings");
    serde_json::from_str(&json).expect("persisted settings are valid JSON")
}
