use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use newpic_core::settings::{
    load_or_recover, migrate_json, ClipboardSettings, CorruptionRecovery, Hotkeys, ImageFormat,
    JsonFileSettingsRepository, Language, LoadSettingsError, NamingSettings, RecoveryNotice,
    SaveDirectory, Settings, SettingsMigrationError, SettingsRepository, SettingsRepositoryError,
    SettingsValidationError, Size, CURRENT_SCHEMA_VERSION,
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
fn validation_enforces_sequence_width_and_jpeg_quality_bounds() {
    let mut settings = Settings::default();
    settings.naming.minimum_width = 0;
    assert_eq!(
        settings.validate(),
        Err(SettingsValidationError::InvalidMinimumWidth(0))
    );

    let mut settings = Settings::default();
    settings.save.jpeg_quality = 0;
    assert_eq!(
        settings.validate(),
        Err(SettingsValidationError::InvalidJpegQuality(0))
    );

    settings.save.jpeg_quality = 101;
    assert_eq!(
        settings.validate(),
        Err(SettingsValidationError::InvalidJpegQuality(101))
    );

    settings.save.jpeg_quality = 100;
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

#[test]
fn current_schema_load_is_read_only_and_preserves_unknown_fields() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let mut json = serde_json::to_value(Settings::default()).expect("settings value");
    json.as_object_mut().expect("settings object").insert(
        "future_metadata".to_owned(),
        serde_json::json!({ "keep": true }),
    );
    let original = serde_json::to_string_pretty(&json).expect("settings JSON");
    fs::write(&path, &original).expect("write current settings");
    let mut permissions = fs::metadata(&path)
        .expect("settings metadata")
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).expect("make settings read only");

    let outcome = load_or_recover(&JsonFileSettingsRepository::new(&path))
        .expect("current schema loads without a write");

    assert_eq!(outcome.settings, Settings::default());
    assert_eq!(outcome.notice, None);
    assert_eq!(fs::read_to_string(&path).expect("read settings"), original);

    make_writable(&path);
}

#[test]
fn future_schema_is_preserved_without_save_or_backup() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let original = r#"{"schema_version":2,"future":"untouched"}"#;
    fs::write(&path, original).expect("write future settings");

    let result = load_or_recover(&JsonFileSettingsRepository::new(&path));

    assert!(matches!(
        result,
        Err(LoadSettingsError::Migration(
            SettingsMigrationError::UnsupportedSchema(2)
        ))
    ));
    assert_eq!(
        fs::read_to_string(&path).expect("read future settings"),
        original
    );
    assert_eq!(backup_files(temp.path()).len(), 0);
}

#[test]
fn schema_zero_is_persisted_only_after_successful_migration() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    fs::write(&path, include_str!("fixtures/settings-schema-0.json"))
        .expect("write schema zero settings");

    let outcome = load_or_recover(&JsonFileSettingsRepository::new(&path))
        .expect("schema zero settings migrate");

    assert_eq!(outcome.settings.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(read_settings(&path), outcome.settings);
    assert!(fs::read_to_string(&path)
        .expect("read migrated settings")
        .contains("\"schema_version\": 1"));
}

#[test]
fn semantically_invalid_json_is_backed_up_before_default_recovery() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let invalid = Settings {
        fixed_region: Size::new(0, 600),
        ..Settings::default()
    };
    let invalid_json = serde_json::to_string_pretty(&invalid).expect("invalid settings JSON");
    fs::write(&path, &invalid_json).expect("write semantically invalid settings");

    let outcome =
        load_or_recover(&JsonFileSettingsRepository::new(&path)).expect("invalid settings recover");
    let backup = temp.path().join("settings.corrupt.json");

    assert_eq!(
        outcome.notice,
        Some(RecoveryNotice::CorruptedSettings {
            backup_path: backup.clone()
        })
    );
    assert_eq!(
        fs::read_to_string(backup).expect("read backup"),
        invalid_json
    );
    assert_eq!(read_settings(&path), Settings::default());
}

#[cfg(unix)]
#[test]
fn atomic_save_replaces_a_read_only_file_and_leaves_no_temporary_sibling() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let repository = JsonFileSettingsRepository::new(&path);
    repository
        .save(&Settings::default())
        .expect("write initial settings");
    let mut permissions = fs::metadata(&path)
        .expect("settings metadata")
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).expect("make old settings read only");

    let replacement = Settings {
        autostart: false,
        ..Settings::default()
    };
    repository
        .save(&replacement)
        .expect("atomically replace read-only destination");

    assert_eq!(read_settings(&path), replacement);
    assert_eq!(temporary_settings_files(temp.path()).len(), 0);
}

#[cfg(unix)]
#[test]
fn failed_pre_rename_save_preserves_existing_valid_settings_and_cleans_temp() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let repository = JsonFileSettingsRepository::new(&path);
    repository
        .save(&Settings::default())
        .expect("write initial settings");
    let original = fs::read(&path).expect("read original settings");
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o555))
        .expect("make parent read only");

    let replacement = Settings {
        autostart: false,
        ..Settings::default()
    };
    let result = repository.save(&replacement);

    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755))
        .expect("restore parent permissions");
    assert!(result.is_err());
    assert_eq!(fs::read(&path).expect("read preserved settings"), original);
    assert_eq!(temporary_settings_files(temp.path()).len(), 0);
}

#[test]
fn concurrent_backup_callers_never_overwrite_an_existing_backup() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let sentinel_path = temp.path().join("settings.corrupt.json");
    fs::write(&path, "corrupt source").expect("write corrupt source");
    fs::write(&sentinel_path, "existing backup").expect("write sentinel backup");
    let repository = Arc::new(JsonFileSettingsRepository::new(&path));
    let barrier = Arc::new(Barrier::new(8));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let repository = Arc::clone(&repository);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                repository.recover_corrupted_if_unchanged("corrupt source", &Settings::default())
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("backup thread"))
        .collect();

    assert_eq!(
        fs::read_to_string(&sentinel_path).expect("read sentinel"),
        "existing backup"
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(CorruptionRecovery::Recovered { .. })))
            .count(),
        1
    );
    let backups = backup_files(temp.path());
    assert_eq!(backups.len(), 2);
    let created = backups
        .into_iter()
        .find(|candidate| candidate != &sentinel_path)
        .expect("new backup");
    assert_eq!(
        fs::read_to_string(created).expect("read claimed backup"),
        "corrupt source"
    );
}

#[test]
fn backup_failure_does_not_create_an_empty_claim_file() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let repository = JsonFileSettingsRepository::new(temp.path().join("missing.json"));

    assert!(repository
        .recover_corrupted_if_unchanged("missing", &Settings::default())
        .is_err());
    assert_eq!(backup_files(temp.path()).len(), 0);
}

#[test]
fn changed_source_after_corrupt_read_is_reloaded_without_backup_or_overwrite() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    fs::write(&path, "{ corrupt original").expect("write corrupt source");
    let replacement = Settings {
        autostart: false,
        ..Settings::default()
    };
    let repository = InterleavingRepository {
        inner: JsonFileSettingsRepository::new(&path),
        replacement: replacement.clone(),
        replace_after_first_load: AtomicBool::new(true),
    };

    let outcome = load_or_recover(&repository).expect("replacement is reloaded");

    assert_eq!(outcome.settings, replacement);
    assert_eq!(outcome.notice, None);
    assert_eq!(read_settings(&path), replacement);
    assert_eq!(backup_files(temp.path()).len(), 0);
}

#[cfg(unix)]
#[test]
fn failed_backup_claim_preserves_source_and_existing_backup() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("settings.json");
    let existing_backup = temp.path().join("settings.corrupt.json");
    fs::write(&path, "{ corrupt source").expect("write corrupt source");
    fs::write(&existing_backup, "existing backup").expect("write existing backup");
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o555))
        .expect("make parent read only");

    let result = load_or_recover(&JsonFileSettingsRepository::new(&path));

    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755))
        .expect("restore parent permissions");
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(&path).expect("read preserved source"),
        "{ corrupt source"
    );
    assert_eq!(
        fs::read_to_string(existing_backup).expect("read existing backup"),
        "existing backup"
    );
    assert!(!temp.path().join("settings.corrupt-1.json").exists());
}

struct InterleavingRepository {
    inner: JsonFileSettingsRepository,
    replacement: Settings,
    replace_after_first_load: AtomicBool,
}

impl SettingsRepository for InterleavingRepository {
    fn load(&self) -> Result<Option<String>, SettingsRepositoryError> {
        let loaded = self.inner.load()?;
        if self.replace_after_first_load.swap(false, Ordering::SeqCst) {
            self.inner.save(&self.replacement)?;
        }
        Ok(loaded)
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsRepositoryError> {
        self.inner.save(settings)
    }

    fn recover_corrupted_if_unchanged(
        &self,
        expected: &str,
        replacement: &Settings,
    ) -> Result<CorruptionRecovery, SettingsRepositoryError> {
        self.inner
            .recover_corrupted_if_unchanged(expected, replacement)
    }
}

fn assert_repository<R: SettingsRepository>(_repository: &R) {}

fn read_settings(path: &Path) -> Settings {
    let json = fs::read_to_string(path).expect("read persisted settings");
    serde_json::from_str(&json).expect("persisted settings are valid JSON")
}

fn backup_files(directory: &Path) -> Vec<std::path::PathBuf> {
    directory_entries(directory, ".corrupt")
}

fn temporary_settings_files(directory: &Path) -> Vec<std::path::PathBuf> {
    directory_entries(directory, ".newpic-settings-")
}

fn directory_entries(directory: &Path, needle: &str) -> Vec<std::path::PathBuf> {
    fs::read_dir(directory)
        .expect("read directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(needle))
        })
        .collect()
}

#[cfg(unix)]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).expect("settings metadata").permissions();
    permissions.set_mode(permissions.mode() | 0o200);
    fs::set_permissions(path, permissions).expect("restore writable settings");
}

#[cfg(not(unix))]
fn make_writable(path: &Path) {
    let mut permissions = fs::metadata(path).expect("settings metadata").permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).expect("restore writable settings");
}
