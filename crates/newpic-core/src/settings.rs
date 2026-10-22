#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use newpic_platform::geometry::SavedFixedRegion;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 2;
const MAX_SOURCE_CHANGES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub schema_version: u32,
    pub autostart: bool,
    pub run_in_tray: bool,
    pub success_notifications: bool,
    pub language: Language,
    pub include_pointer: bool,
    pub open_editor_after_capture: bool,
    pub check_updates: bool,
    pub save: SaveSettings,
    pub naming: NamingSettings,
    pub clipboard: ClipboardSettings,
    pub fixed_region: Size,
    pub saved_fixed_region: Option<SavedFixedRegion>,
    pub hotkeys: Hotkeys,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            autostart: true,
            run_in_tray: true,
            success_notifications: true,
            language: Language::Korean,
            include_pointer: false,
            open_editor_after_capture: false,
            check_updates: false,
            save: SaveSettings::default(),
            naming: NamingSettings::default(),
            clipboard: ClipboardSettings::default(),
            fixed_region: Size::new(800, 600),
            saved_fixed_region: None,
            hotkeys: Hotkeys::default(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), SettingsValidationError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(SettingsValidationError::UnexpectedSchemaVersion(
                self.schema_version,
            ));
        }

        if self.fixed_region.width == 0 || self.fixed_region.height == 0 {
            return Err(SettingsValidationError::InvalidFixedRegion(
                self.fixed_region,
            ));
        }

        for (name, shortcut) in self.hotkeys.named_values() {
            if shortcut.trim().is_empty() {
                return Err(SettingsValidationError::EmptyShortcut(name));
            }
        }

        if self.naming.pattern.trim().is_empty() {
            return Err(SettingsValidationError::EmptyNamingPattern);
        }
        validate_naming_pattern(&self.naming.pattern)?;

        if self.naming.minimum_width == 0 {
            return Err(SettingsValidationError::InvalidMinimumWidth(0));
        }

        if !(1..=100).contains(&self.save.jpeg_quality) {
            return Err(SettingsValidationError::InvalidJpegQuality(
                self.save.jpeg_quality,
            ));
        }

        if let SaveDirectory::Custom(path) = &self.save.directory {
            validate_custom_save_directory(path)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Korean,
    English,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSettings {
    pub enabled: bool,
    pub directory: SaveDirectory,
    pub format: ImageFormat,
    pub jpeg_quality: u8,
}

impl Default for SaveSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: SaveDirectory::DownloadsNewPic,
            format: ImageFormat::Png,
            jpeg_quality: 90,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveDirectory {
    DownloadsNewPic,
    Custom(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamingSettings {
    pub pattern: String,
    pub minimum_width: u8,
}

impl Default for NamingSettings {
    fn default() -> Self {
        Self {
            pattern: "%c".to_owned(),
            minimum_width: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardSettings {
    pub enabled: bool,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkeys {
    pub region: String,
    pub repeat_region: String,
    pub fixed_region: String,
    pub active_window: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            region: "F7".to_owned(),
            repeat_region: "F8".to_owned(),
            fixed_region: "F9".to_owned(),
            active_window: "F10".to_owned(),
        }
    }
}

impl Hotkeys {
    fn named_values(&self) -> [(&'static str, &str); 4] {
        [
            ("region", &self.region),
            ("repeat_region", &self.repeat_region),
            ("fixed_region", &self.fixed_region),
            ("active_window", &self.active_window),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsValidationError {
    #[error("settings schema version {0} is not current")]
    UnexpectedSchemaVersion(u32),
    #[error("fixed capture region must have non-zero dimensions, got {0:?}")]
    InvalidFixedRegion(Size),
    #[error("shortcut {0} cannot be empty")]
    EmptyShortcut(&'static str),
    #[error("filename pattern cannot be empty")]
    EmptyNamingPattern,
    #[error("filename token {0} is not supported")]
    UnsupportedNamingToken(String),
    #[error("minimum sequence width must be greater than zero, got {0}")]
    InvalidMinimumWidth(u8),
    #[error("JPEG quality must be in the range 1 through 100, got {0}")]
    InvalidJpegQuality(u8),
    #[error("custom save directory cannot be empty")]
    EmptyCustomSaveDirectory,
    #[error("custom save directory must be absolute: {0}")]
    CustomSaveDirectoryMustBeAbsolute(PathBuf),
    #[error("custom save directory contains unsafe path components: {0}")]
    UnsafeCustomSaveDirectory(PathBuf),
}

fn validate_custom_save_directory(path: &Path) -> Result<(), SettingsValidationError> {
    let text = path
        .to_str()
        .ok_or_else(|| SettingsValidationError::UnsafeCustomSaveDirectory(path.to_path_buf()))?;
    #[cfg(windows)]
    let platform = SavePathPlatform::Windows;
    #[cfg(not(windows))]
    let platform = SavePathPlatform::Native;
    validate_custom_save_directory_for_platform(text, platform)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePathPlatform {
    Native,
    Windows,
}

pub fn validate_custom_save_directory_for_platform(
    path: &str,
    platform: SavePathPlatform,
) -> Result<(), SettingsValidationError> {
    match platform {
        SavePathPlatform::Native => validate_native_save_directory(path),
        SavePathPlatform::Windows => validate_windows_save_directory(path),
    }
}

fn validate_native_save_directory(path: &str) -> Result<(), SettingsValidationError> {
    use std::path::Component;

    let path = Path::new(path);
    if path.as_os_str().is_empty() {
        return Err(SettingsValidationError::EmptyCustomSaveDirectory);
    }
    if !path.is_absolute() {
        return Err(SettingsValidationError::CustomSaveDirectoryMustBeAbsolute(
            path.to_path_buf(),
        ));
    }
    if path
        .as_os_str()
        .to_string_lossy()
        .chars()
        .any(char::is_control)
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(SettingsValidationError::UnsafeCustomSaveDirectory(
            path.to_path_buf(),
        ));
    }
    Ok(())
}

fn validate_windows_save_directory(path: &str) -> Result<(), SettingsValidationError> {
    let rejected = || SettingsValidationError::UnsafeCustomSaveDirectory(PathBuf::from(path));
    if path.is_empty() {
        return Err(SettingsValidationError::EmptyCustomSaveDirectory);
    }
    let normalized = path.replace('/', "\\");
    let uppercase = normalized.to_ascii_uppercase();
    if uppercase.starts_with(r"\\?\")
        || uppercase.starts_with(r"\\.\")
        || uppercase.starts_with(r"\??\")
    {
        return Err(rejected());
    }

    let components: Vec<&str> = if let Some(unc_path) = normalized.strip_prefix(r"\\") {
        let raw_components: Vec<_> = unc_path.split('\\').collect();
        if raw_components.len() < 2 || raw_components[0].is_empty() || raw_components[1].is_empty()
        {
            return Err(SettingsValidationError::CustomSaveDirectoryMustBeAbsolute(
                PathBuf::from(path),
            ));
        }
        raw_components
            .into_iter()
            .filter(|component| !component.is_empty())
            .collect()
    } else {
        let bytes = normalized.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || bytes[2] != b'\\'
        {
            return Err(SettingsValidationError::CustomSaveDirectoryMustBeAbsolute(
                PathBuf::from(path),
            ));
        }
        normalized[3..]
            .split('\\')
            .filter(|component| !component.is_empty())
            .collect()
    };

    for component in components {
        if component.is_empty() || matches!(component, "." | "..") {
            return Err(rejected());
        }
        if component.ends_with(['.', ' '])
            || component.chars().any(|character| {
                character.is_control()
                    || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            })
            || crate::naming::is_windows_reserved_component(component)
        {
            return Err(rejected());
        }
    }
    Ok(())
}

fn validate_naming_pattern(pattern: &str) -> Result<(), SettingsValidationError> {
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            continue;
        }

        let Some(token) = characters.next() else {
            return Err(SettingsValidationError::UnsupportedNamingToken(
                "%".to_owned(),
            ));
        };
        if !matches!(token, '%' | 'c' | 'Y' | 'm' | 'd' | 'H' | 'M' | 'S' | 'h') {
            return Err(SettingsValidationError::UnsupportedNamingToken(format!(
                "%{token}"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum SettingsMigrationError {
    #[error("settings JSON is malformed: {0}")]
    MalformedJson(#[from] serde_json::Error),
    #[error("settings schema version {0} is not supported")]
    UnsupportedSchema(u64),
    #[error("settings validation failed: {0}")]
    Invalid(#[from] SettingsValidationError),
}

#[derive(Deserialize)]
struct SchemaEnvelope {
    schema_version: u64,
}

#[derive(Deserialize)]
struct SchemaZeroSettings {
    schema_version: u32,
    autostart: bool,
    save_enabled: bool,
    save_directory: SaveDirectory,
    image_format: ImageFormat,
    jpeg_quality: u8,
    naming_pattern: String,
    minimum_width: u8,
    clipboard_enabled: bool,
    fixed_region_width: u32,
    fixed_region_height: u32,
    region_hotkey: String,
    repeat_region_hotkey: String,
    fixed_region_hotkey: String,
    active_window_hotkey: String,
}

#[derive(Deserialize)]
struct SchemaOneSettings {
    schema_version: u32,
    autostart: bool,
    run_in_tray: bool,
    success_notifications: bool,
    language: Language,
    include_pointer: bool,
    open_editor_after_capture: bool,
    check_updates: bool,
    save: SaveSettings,
    naming: NamingSettings,
    clipboard: ClipboardSettings,
    fixed_region: Size,
    hotkeys: Hotkeys,
}

pub fn migrate_json(json: &str) -> Result<Settings, SettingsMigrationError> {
    decode_json(json).map(|decoded| decoded.settings)
}

struct DecodedSettings {
    settings: Settings,
    migrated: bool,
}

fn decode_json(json: &str) -> Result<DecodedSettings, SettingsMigrationError> {
    let envelope: SchemaEnvelope = serde_json::from_str(json)?;
    let (settings, migrated) = match envelope.schema_version {
        0 => (migrate_schema_zero(serde_json::from_str(json)?), true),
        1 => (migrate_schema_one(serde_json::from_str(json)?), true),
        2 => (serde_json::from_str(json)?, false),
        version => return Err(SettingsMigrationError::UnsupportedSchema(version)),
    };
    settings.validate()?;
    Ok(DecodedSettings { settings, migrated })
}

fn migrate_schema_one(old: SchemaOneSettings) -> Settings {
    debug_assert_eq!(old.schema_version, 1);
    Settings {
        schema_version: CURRENT_SCHEMA_VERSION,
        autostart: old.autostart,
        run_in_tray: old.run_in_tray,
        success_notifications: old.success_notifications,
        language: old.language,
        include_pointer: old.include_pointer,
        open_editor_after_capture: old.open_editor_after_capture,
        check_updates: old.check_updates,
        save: old.save,
        naming: old.naming,
        clipboard: old.clipboard,
        fixed_region: old.fixed_region,
        saved_fixed_region: None,
        hotkeys: old.hotkeys,
    }
}

fn migrate_schema_zero(old: SchemaZeroSettings) -> Settings {
    debug_assert_eq!(old.schema_version, 0);
    Settings {
        autostart: old.autostart,
        save: SaveSettings {
            enabled: old.save_enabled,
            directory: old.save_directory,
            format: old.image_format,
            jpeg_quality: old.jpeg_quality,
        },
        naming: NamingSettings {
            pattern: old.naming_pattern,
            minimum_width: old.minimum_width,
        },
        clipboard: ClipboardSettings {
            enabled: old.clipboard_enabled,
        },
        fixed_region: Size::new(old.fixed_region_width, old.fixed_region_height),
        hotkeys: Hotkeys {
            region: old.region_hotkey,
            repeat_region: old.repeat_region_hotkey,
            fixed_region: old.fixed_region_hotkey,
            active_window: old.active_window_hotkey,
        },
        ..Settings::default()
    }
}

pub trait SettingsRepository {
    fn load(&self) -> Result<Option<String>, SettingsRepositoryError>;
    fn save(&self, settings: &Settings) -> Result<(), SettingsRepositoryError>;
    /// Creates or migrates settings only while the on-disk state still equals
    /// `expected`. The comparison and save must share one cross-process lock.
    fn save_if_unchanged(
        &self,
        expected: ExpectedSettingsState<'_>,
        settings: &Settings,
    ) -> Result<ConditionalSave, SettingsRepositoryError>;
    /// Replaces a corrupt file only when its current bytes still equal
    /// `expected`. Implementations must make the comparison, backup claim, and
    /// replacement one cross-process critical section.
    fn recover_corrupted_if_unchanged(
        &self,
        expected: &str,
        replacement: &Settings,
    ) -> Result<CorruptionRecovery, SettingsRepositoryError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedSettingsState<'a> {
    Missing,
    Contents(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalSave {
    Saved,
    SourceChanged { contents: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorruptionRecovery {
    Recovered { backup_path: PathBuf },
    SourceChanged { contents: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonFileSettingsRepository {
    path: PathBuf,
}

impl JsonFileSettingsRepository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn corrupted_backup_path(&self, sequence: u64) -> PathBuf {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let stem = self
            .path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("settings");
        let extension = self.path.extension().and_then(|value| value.to_str());

        let suffix = if sequence == 0 {
            String::new()
        } else {
            format!("-{sequence}")
        };
        let name = match extension {
            Some(extension) => format!("{stem}.corrupt{suffix}.{extension}"),
            None => format!("{stem}.corrupt{suffix}"),
        };
        parent.join(name)
    }

    fn lock_path(&self) -> PathBuf {
        let parent = nonempty_parent(&self.path);
        let file_name = self
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("settings");
        parent.join(format!(".{file_name}.lock"))
    }

    fn with_exclusive_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, SettingsRepositoryError>,
    ) -> Result<T, SettingsRepositoryError> {
        let parent = nonempty_parent(&self.path);
        fs::create_dir_all(parent)?;
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.lock_path())?;
        lock_file.lock_exclusive()?;
        let result = operation();
        drop(lock_file);
        result
    }

    fn serialize(settings: &Settings) -> Result<Vec<u8>, SettingsRepositoryError> {
        settings.validate()?;
        let mut json = serde_json::to_vec_pretty(settings)?;
        json.push(b'\n');
        Ok(json)
    }

    fn save_bytes_unlocked(&self, bytes: &[u8]) -> Result<(), SettingsRepositoryError> {
        let parent = nonempty_parent(&self.path);
        let mut temporary = tempfile::Builder::new()
            .prefix(".newpic-settings-")
            .tempfile_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary.persist(&self.path).map_err(|error| error.error)?;
        sync_directory_best_effort(parent);
        Ok(())
    }

    fn claim_corrupted_backup(&self, contents: &[u8]) -> Result<PathBuf, SettingsRepositoryError> {
        for sequence in 0_u64.. {
            let backup_path = self.corrupted_backup_path(sequence);
            let backup = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup_path);
            let mut backup = match backup {
                Ok(backup) => backup,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            };

            let write_result = (|| -> io::Result<()> {
                backup.write_all(contents)?;
                backup.flush()?;
                backup.sync_all()
            })();
            if let Err(error) = write_result {
                drop(backup);
                let _ = fs::remove_file(&backup_path);
                return Err(error.into());
            }
            return Ok(backup_path);
        }
        unreachable!("the backup sequence is unbounded")
    }

    fn read_current(&self) -> Result<Option<String>, SettingsRepositoryError> {
        match fs::read_to_string(&self.path) {
            Ok(json) => Ok(Some(json)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

impl SettingsRepository for JsonFileSettingsRepository {
    fn load(&self) -> Result<Option<String>, SettingsRepositoryError> {
        self.read_current()
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsRepositoryError> {
        let json = Self::serialize(settings)?;
        self.with_exclusive_lock(|| self.save_bytes_unlocked(&json))
    }

    fn save_if_unchanged(
        &self,
        expected: ExpectedSettingsState<'_>,
        settings: &Settings,
    ) -> Result<ConditionalSave, SettingsRepositoryError> {
        let replacement = Self::serialize(settings)?;
        self.with_exclusive_lock(|| {
            let current = self.read_current()?;
            let unchanged = match (expected, current.as_deref()) {
                (ExpectedSettingsState::Missing, None) => true,
                (ExpectedSettingsState::Contents(expected), Some(current)) => expected == current,
                _ => false,
            };
            if !unchanged {
                return Ok(ConditionalSave::SourceChanged { contents: current });
            }

            self.save_bytes_unlocked(&replacement)?;
            Ok(ConditionalSave::Saved)
        })
    }

    fn recover_corrupted_if_unchanged(
        &self,
        expected: &str,
        replacement: &Settings,
    ) -> Result<CorruptionRecovery, SettingsRepositoryError> {
        let replacement = Self::serialize(replacement)?;
        self.with_exclusive_lock(|| {
            let current = fs::read_to_string(&self.path)?;
            if current != expected {
                return Ok(CorruptionRecovery::SourceChanged { contents: current });
            }

            let backup_path = self.claim_corrupted_backup(current.as_bytes())?;
            if let Err(error) = self.save_bytes_unlocked(&replacement) {
                let _ = fs::remove_file(&backup_path);
                return Err(error);
            }
            Ok(CorruptionRecovery::Recovered { backup_path })
        })
    }
}

fn nonempty_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Directory metadata sync is best-effort after the file itself has been
/// synced and atomically persisted. Some supported filesystems reject
/// directory fsync; that must not turn an already-committed save into a
/// reported failure.
#[cfg(unix)]
fn sync_directory_best_effort(path: &Path) {
    let _ = File::open(path).and_then(|directory| directory.sync_all());
}

#[cfg(not(unix))]
fn sync_directory_best_effort(_path: &Path) {}

#[derive(Debug, Error)]
pub enum SettingsRepositoryError {
    #[error("settings file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("settings serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("settings validation failed: {0}")]
    Validation(#[from] SettingsValidationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSettingsOutcome {
    pub settings: Settings,
    pub notice: Option<RecoveryNotice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryNotice {
    CorruptedSettings { backup_path: PathBuf },
}

#[derive(Debug, Error)]
pub enum LoadSettingsError {
    #[error(transparent)]
    Repository(#[from] SettingsRepositoryError),
    #[error(transparent)]
    Migration(#[from] SettingsMigrationError),
    #[error("settings changed repeatedly during loading")]
    ConcurrentModification,
}

pub fn load_or_recover<R: SettingsRepository>(
    repository: &R,
) -> Result<LoadSettingsOutcome, LoadSettingsError> {
    enum LoadedState {
        Missing,
        Contents(String),
    }

    let mut state = match repository.load()? {
        Some(json) => LoadedState::Contents(json),
        None => LoadedState::Missing,
    };
    let mut source_changes = 0;

    loop {
        let changed = match state {
            LoadedState::Missing => {
                let settings = Settings::default();
                match repository.save_if_unchanged(ExpectedSettingsState::Missing, &settings)? {
                    ConditionalSave::Saved => {
                        return Ok(LoadSettingsOutcome {
                            settings,
                            notice: None,
                        });
                    }
                    ConditionalSave::SourceChanged { contents } => contents,
                }
            }
            LoadedState::Contents(json) => match decode_json(&json) {
                Ok(decoded) if !decoded.migrated => {
                    return Ok(LoadSettingsOutcome {
                        settings: decoded.settings,
                        notice: None,
                    });
                }
                Ok(decoded) => match repository
                    .save_if_unchanged(ExpectedSettingsState::Contents(&json), &decoded.settings)?
                {
                    ConditionalSave::Saved => {
                        return Ok(LoadSettingsOutcome {
                            settings: decoded.settings,
                            notice: None,
                        });
                    }
                    ConditionalSave::SourceChanged { contents } => contents,
                },
                Err(
                    SettingsMigrationError::MalformedJson(_) | SettingsMigrationError::Invalid(_),
                ) => {
                    let settings = Settings::default();
                    match repository.recover_corrupted_if_unchanged(&json, &settings)? {
                        CorruptionRecovery::Recovered { backup_path } => {
                            return Ok(LoadSettingsOutcome {
                                settings,
                                notice: Some(RecoveryNotice::CorruptedSettings { backup_path }),
                            });
                        }
                        CorruptionRecovery::SourceChanged { contents } => Some(contents),
                    }
                }
                Err(error @ SettingsMigrationError::UnsupportedSchema(_)) => {
                    return Err(error.into());
                }
            },
        };

        source_changes += 1;
        if source_changes > MAX_SOURCE_CHANGES {
            return Err(LoadSettingsError::ConcurrentModification);
        }
        state = match changed {
            Some(json) => LoadedState::Contents(json),
            None => LoadedState::Missing,
        };
    }
}
