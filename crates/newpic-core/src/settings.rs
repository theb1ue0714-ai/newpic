use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

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

pub fn migrate_json(json: &str) -> Result<Settings, SettingsMigrationError> {
    let envelope: SchemaEnvelope = serde_json::from_str(json)?;
    let settings = match envelope.schema_version {
        0 => migrate_schema_zero(serde_json::from_str(json)?),
        1 => serde_json::from_str(json)?,
        version => return Err(SettingsMigrationError::UnsupportedSchema(version)),
    };
    settings.validate()?;
    Ok(settings)
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
    fn backup_corrupted(&self) -> Result<PathBuf, SettingsRepositoryError>;
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

    fn next_corrupted_backup_path(&self) -> PathBuf {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let stem = self
            .path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("settings");
        let extension = self.path.extension().and_then(|value| value.to_str());

        for sequence in 0_u64.. {
            let suffix = if sequence == 0 {
                String::new()
            } else {
                format!("-{sequence}")
            };
            let name = match extension {
                Some(extension) => format!("{stem}.corrupt{suffix}.{extension}"),
                None => format!("{stem}.corrupt{suffix}"),
            };
            let candidate = parent.join(name);
            if !candidate.exists() {
                return candidate;
            }
        }
        unreachable!("the backup sequence is unbounded")
    }
}

impl SettingsRepository for JsonFileSettingsRepository {
    fn load(&self) -> Result<Option<String>, SettingsRepositoryError> {
        match fs::read_to_string(&self.path) {
            Ok(json) => Ok(Some(json)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsRepositoryError> {
        settings.validate()?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings)?;
        fs::write(&self.path, format!("{json}\n"))?;
        Ok(())
    }

    fn backup_corrupted(&self) -> Result<PathBuf, SettingsRepositoryError> {
        let backup_path = self.next_corrupted_backup_path();
        fs::rename(&self.path, &backup_path)?;
        Ok(backup_path)
    }
}

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
}

pub fn load_or_recover<R: SettingsRepository>(
    repository: &R,
) -> Result<LoadSettingsOutcome, LoadSettingsError> {
    let Some(json) = repository.load()? else {
        let settings = Settings::default();
        repository.save(&settings)?;
        return Ok(LoadSettingsOutcome {
            settings,
            notice: None,
        });
    };

    match migrate_json(&json) {
        Ok(settings) => {
            repository.save(&settings)?;
            Ok(LoadSettingsOutcome {
                settings,
                notice: None,
            })
        }
        Err(SettingsMigrationError::MalformedJson(_) | SettingsMigrationError::Invalid(_)) => {
            let backup_path = repository.backup_corrupted()?;
            let settings = Settings::default();
            repository.save(&settings)?;
            Ok(LoadSettingsOutcome {
                settings,
                notice: Some(RecoveryNotice::CorruptedSettings { backup_path }),
            })
        }
        Err(error @ SettingsMigrationError::UnsupportedSchema(_)) => Err(error.into()),
    }
}
