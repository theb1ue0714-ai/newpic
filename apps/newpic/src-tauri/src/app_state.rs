//! Shared application state and the Tauri-independent capture flow.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use chrono::{Datelike, Local, Timelike};
use newpic_core::image_output::{ImageEncoding, JpegQuality};
use newpic_core::naming::{NameAllocator, NamingContext};
use newpic_core::pipeline::{
    CapturePipeline, DeliveryError, DeliveryReport, FileImageStore, ImageStore, ImageStoreError,
    OutputSelection, StoredOutput,
};
use newpic_core::settings::{
    ImageFormat, JsonFileSettingsRepository, SaveDirectory, SaveSettings, Settings, Size,
};
use newpic_core::storage::AtomicImageStore;
use newpic_platform::capture::{
    CaptureOptions, CaptureProvider, CaptureRequest, CaptureSource, CapturedImage,
};
use newpic_platform::clipboard::ImageClipboard;
use serde::Serialize;

use crate::autostart::AutostartStatus;
use crate::diagnostics::{
    probe_destination, CaptureDiagnostics, DiagnosticLog, Diagnostics, EventLevel,
};
use crate::hotkeys::{CaptureAction, HotkeyManager};

pub const DOWNLOADS_SUBFOLDER: &str = "NewPic";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub settings_file: PathBuf,
    pub downloads_dir: PathBuf,
    pub recovery_dir: PathBuf,
}

pub fn resolve_save_directory(save: &SaveSettings, downloads_dir: &Path) -> PathBuf {
    match &save.directory {
        SaveDirectory::DownloadsNewPic => downloads_dir.join(DOWNLOADS_SUBFOLDER),
        SaveDirectory::Custom(path) => path.clone(),
    }
}

pub fn encoding_for(save: &SaveSettings) -> ImageEncoding {
    match save.format {
        ImageFormat::Png => ImageEncoding::Png,
        // Settings validation guarantees 1..=100; fall back to the preset value.
        ImageFormat::Jpeg => ImageEncoding::Jpeg(
            JpegQuality::new(save.jpeg_quality)
                .unwrap_or_else(|_| JpegQuality::new(90).expect("90 is valid")),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    FreeRegion,
    FixedSize,
}

/// Payload sent to the selection overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectionRequest {
    pub action: CaptureAction,
    pub mode: SelectionMode,
    pub fixed_size: Option<Size>,
    /// Set when F8 had no reusable region and fell back to F7 selection.
    pub fallback_from: Option<CaptureAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturePlan {
    /// Capture immediately without user selection.
    Direct(CaptureSource),
    /// Ask the overlay for a selection first.
    Select(SelectionRequest),
}

/// Decides how an action starts. Saved F8 regions are persisted once the
/// selection overlay exists; until one is available F8 opens F7 selection,
/// as the approved specification requires.
pub fn plan_capture(action: CaptureAction, settings: &Settings) -> CapturePlan {
    match action {
        CaptureAction::ActiveWindow => CapturePlan::Direct(CaptureSource::ActiveWindow),
        CaptureAction::Region => CapturePlan::Select(SelectionRequest {
            action,
            mode: SelectionMode::FreeRegion,
            fixed_size: None,
            fallback_from: None,
        }),
        CaptureAction::RepeatRegion => CapturePlan::Select(SelectionRequest {
            action: CaptureAction::Region,
            mode: SelectionMode::FreeRegion,
            fixed_size: None,
            fallback_from: Some(CaptureAction::RepeatRegion),
        }),
        CaptureAction::FixedRegion => CapturePlan::Select(SelectionRequest {
            action,
            mode: SelectionMode::FixedSize,
            fixed_size: Some(settings.fixed_region),
            fallback_from: None,
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub level: EventLevel,
}

/// Builds the notification for a delivery. Successful deliveries respect the
/// success-notification setting; problems are always reported.
pub fn delivery_notice(
    result: &Result<DeliveryReport, DeliveryError>,
    success_notifications: bool,
) -> Option<Notice> {
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            return Some(Notice {
                title: "캡처를 전달하지 못했습니다".to_owned(),
                body: error.to_string(),
                level: EventLevel::Error,
            })
        }
    };
    let mut lines = Vec::new();
    match report.stored() {
        Some(StoredOutput::Stored { path }) => lines.push(format!("저장됨: {}", path.display())),
        Some(StoredOutput::Recovered(recovered)) => lines.push(format!(
            "기본 폴더에 저장할 수 없어 복구 폴더에 저장했습니다: {}",
            recovered.path.display()
        )),
        None => {
            if let Some(error) = report.storage_error() {
                lines.push(format!("파일 저장 실패: {error}"));
            }
        }
    }
    if report.clipboard_attempts().is_some() {
        lines.push("클립보드에 복사됨".to_owned());
    } else if let Some(failure) = report.clipboard_failure() {
        lines.push(format!("클립보드 복사 실패: {failure}"));
    }
    let body = lines.join("\n");
    if !report.is_complete() {
        Some(Notice {
            title: "캡처 일부만 완료되었습니다".to_owned(),
            body,
            level: EventLevel::Warning,
        })
    } else if report.recovered().is_some() {
        Some(Notice {
            title: "캡처를 복구 폴더에 저장했습니다".to_owned(),
            body,
            level: EventLevel::Warning,
        })
    } else if success_notifications {
        Some(Notice {
            title: "캡처 완료".to_owned(),
            body,
            level: EventLevel::Info,
        })
    } else {
        None
    }
}

/// Store used when the naming configuration cannot produce an allocator;
/// keeps the clipboard output working and reports the storage problem.
enum ConfiguredStore {
    Ready(FileImageStore),
    Broken(String),
}

impl ImageStore for ConfiguredStore {
    fn store(&self, bytes: &[u8]) -> Result<StoredOutput, ImageStoreError> {
        match self {
            Self::Ready(store) => store.store(bytes),
            Self::Broken(reason) => Err(ImageStoreError::new(reason.clone())),
        }
    }
}

fn naming_context_now() -> NamingContext {
    let now = Local::now();
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    NamingContext::new(
        u16::try_from(now.year()).unwrap_or(0),
        now.month() as u8,
        now.day() as u8,
        now.hour() as u8,
        now.minute() as u8,
        now.second() as u8,
        hostname,
    )
}

/// Releases the capture-in-progress flag when dropped.
pub struct CaptureGuard<'a>(&'a AtomicBool);

impl Drop for CaptureGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub struct AppState {
    pub paths: AppPaths,
    pub repository: JsonFileSettingsRepository,
    settings: Mutex<Settings>,
    pub settings_notice: Option<String>,
    pub hotkeys: Mutex<HotkeyManager>,
    pub autostart: Mutex<AutostartStatus>,
    pub log: Mutex<DiagnosticLog>,
    provider: Box<dyn CaptureProvider>,
    clipboard: Box<dyn ImageClipboard + Send + Sync>,
    capture_busy: AtomicBool,
}

impl AppState {
    pub fn new(
        paths: AppPaths,
        repository: JsonFileSettingsRepository,
        settings: Settings,
        settings_notice: Option<String>,
        provider: Box<dyn CaptureProvider>,
        clipboard: Box<dyn ImageClipboard + Send + Sync>,
    ) -> Self {
        Self {
            paths,
            repository,
            settings: Mutex::new(settings),
            settings_notice,
            hotkeys: Mutex::new(HotkeyManager::new()),
            autostart: Mutex::new(AutostartStatus::Unknown),
            log: Mutex::new(DiagnosticLog::default()),
            provider,
            clipboard,
            capture_busy: AtomicBool::new(false),
        }
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn replace_settings(&self, settings: Settings) {
        *lock(&self.settings) = settings;
    }

    pub fn record(&self, level: EventLevel, message: impl Into<String>) {
        let message = message.into();
        if cfg!(debug_assertions) {
            eprintln!("[newpic {level:?}] {message}");
        }
        lock(&self.log).push(level, message);
    }

    /// Claims the single capture slot; `None` while another capture runs.
    pub fn try_begin_capture(&self) -> Option<CaptureGuard<'_>> {
        self.capture_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| CaptureGuard(&self.capture_busy))
    }

    pub fn capture(&self, source: CaptureSource) -> Result<CapturedImage, String> {
        let settings = self.settings();
        let options = CaptureOptions::default().with_include_pointer(settings.include_pointer);
        self.provider
            .capture(CaptureRequest::new(source, options))
            .map_err(|error| error.to_string())
    }

    pub fn deliver(&self, image: &CapturedImage) -> Result<DeliveryReport, DeliveryError> {
        let settings = self.settings();
        let encoding = encoding_for(&settings.save);
        let store = match NameAllocator::new(
            settings.naming.pattern.clone(),
            settings.naming.minimum_width,
            encoding.extension(),
            naming_context_now(),
        ) {
            Ok(allocator) => ConfiguredStore::Ready(FileImageStore::new(
                AtomicImageStore::new(
                    resolve_save_directory(&settings.save, &self.paths.downloads_dir),
                    self.paths.recovery_dir.clone(),
                ),
                allocator,
            )),
            Err(error) => ConfiguredStore::Broken(format!("filename pattern is invalid: {error}")),
        };
        CapturePipeline::new(&store, self.clipboard.as_ref()).deliver_selected(
            image,
            encoding,
            OutputSelection::new(settings.save.enabled, settings.clipboard.enabled),
        )
    }

    pub fn diagnostics(&self) -> Diagnostics {
        let settings = self.settings();
        Diagnostics {
            os: std::env::consts::OS,
            session: std::env::var("XDG_SESSION_TYPE").ok(),
            capture: CaptureDiagnostics::from_capabilities(&self.provider.capabilities()),
            hotkeys: lock(&self.hotkeys).report(),
            autostart: lock(&self.autostart).clone(),
            destination: probe_destination(&resolve_save_directory(
                &settings.save,
                &self.paths.downloads_dir,
            )),
            settings_notice: self.settings_notice.clone(),
            events: lock(&self.log).events(),
        }
    }
}

/// Locks a mutex, recovering the data if a previous holder panicked so that
/// one failed capture never disables the tray application.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use newpic_core::storage::{RecoveredImage, StorageError};

    use super::*;

    #[test]
    fn default_destination_is_downloads_newpic() {
        let save = SaveSettings::default();
        assert_eq!(
            resolve_save_directory(&save, Path::new("/home/user/Downloads")),
            PathBuf::from("/home/user/Downloads/NewPic")
        );
        let custom = SaveSettings {
            directory: SaveDirectory::Custom(PathBuf::from("/shots")),
            ..SaveSettings::default()
        };
        assert_eq!(
            resolve_save_directory(&custom, Path::new("/home/user/Downloads")),
            PathBuf::from("/shots")
        );
    }

    #[test]
    fn jpeg_settings_carry_their_quality() {
        let save = SaveSettings {
            format: ImageFormat::Jpeg,
            jpeg_quality: 70,
            ..SaveSettings::default()
        };
        assert_eq!(
            encoding_for(&save),
            ImageEncoding::Jpeg(JpegQuality::new(70).unwrap())
        );
        assert_eq!(encoding_for(&SaveSettings::default()), ImageEncoding::Png);
    }

    #[test]
    fn actions_plan_direct_capture_or_selection() {
        let settings = Settings::default();
        assert_eq!(
            plan_capture(CaptureAction::ActiveWindow, &settings),
            CapturePlan::Direct(CaptureSource::ActiveWindow)
        );
        let CapturePlan::Select(fixed) = plan_capture(CaptureAction::FixedRegion, &settings) else {
            panic!("fixed region needs selection");
        };
        assert_eq!(fixed.mode, SelectionMode::FixedSize);
        assert_eq!(fixed.fixed_size, Some(Size::new(800, 600)));
        let CapturePlan::Select(repeat) = plan_capture(CaptureAction::RepeatRegion, &settings)
        else {
            panic!("repeat falls back to selection");
        };
        assert_eq!(repeat.action, CaptureAction::Region);
        assert_eq!(repeat.fallback_from, Some(CaptureAction::RepeatRegion));
    }

    type ClipResult = Result<(), newpic_platform::clipboard::ClipboardError>;

    fn report_with(
        storage: Option<Result<StoredOutput, String>>,
        clipboard: Option<ClipResult>,
    ) -> Result<DeliveryReport, DeliveryError> {
        struct Store(Mutex<Option<Result<StoredOutput, String>>>);
        impl ImageStore for Store {
            fn store(&self, _: &[u8]) -> Result<StoredOutput, ImageStoreError> {
                lock(&self.0).take().unwrap().map_err(ImageStoreError::new)
            }
        }
        struct Clip(Mutex<Option<ClipResult>>);
        impl ImageClipboard for Clip {
            fn write_image(&self, _: &newpic_platform::clipboard::ClipboardImage) -> ClipResult {
                lock(&self.0).take().unwrap()
            }
        }
        let selection = OutputSelection::new(storage.is_some(), clipboard.is_some());
        let store = Store(Mutex::new(storage));
        let clip = Clip(Mutex::new(clipboard));
        let image = CapturedImage::new(
            1,
            1,
            4,
            vec![0, 0, 0, 255],
            newpic_platform::capture::CaptureScaleMetadata::uniform(
                newpic_platform::geometry::ScaleFactor::new(1.0).unwrap(),
            ),
            newpic_platform::capture::CaptureProvenance::Region,
        )
        .unwrap();
        CapturePipeline::new(&store, &clip).deliver_selected(&image, ImageEncoding::Png, selection)
    }

    fn stored(path: &str) -> Option<Result<StoredOutput, String>> {
        Some(Ok(StoredOutput::Stored {
            path: PathBuf::from(path),
        }))
    }

    #[test]
    fn complete_delivery_notifies_only_when_success_notifications_are_on() {
        let result = report_with(stored("/d/001.png"), Some(Ok(())));
        let notice = delivery_notice(&result, true).unwrap();
        assert_eq!(notice.level, EventLevel::Info);
        assert!(notice.body.contains("/d/001.png"));
        assert!(notice.body.contains("클립보드에 복사됨"));
        assert_eq!(delivery_notice(&result, false), None);
    }

    #[test]
    fn recovery_and_partial_results_are_always_reported() {
        let recovered = report_with(
            Some(Ok(StoredOutput::Recovered(RecoveredImage {
                path: PathBuf::from("/r/001.png"),
                primary_error: StorageError::NameUnavailable {
                    path: PathBuf::from("/d/001.png"),
                },
            }))),
            Some(Ok(())),
        );
        let notice = delivery_notice(&recovered, false).unwrap();
        assert_eq!(notice.level, EventLevel::Warning);
        assert!(notice.body.contains("/r/001.png"));

        let partial = report_with(
            stored("/d/001.png"),
            Some(Err(newpic_platform::clipboard::ClipboardError::permanent(
                newpic_platform::clipboard::ClipboardErrorKind::Unsupported,
                "no clipboard",
            ))),
        );
        let notice = delivery_notice(&partial, false).unwrap();
        assert_eq!(notice.level, EventLevel::Warning);
        assert!(notice.body.contains("클립보드 복사 실패"));
    }
}
