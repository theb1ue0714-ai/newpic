use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use image::ImageReader;
use newpic_core::image_output::{encode_image, ImageEncoding, JpegQuality};
use newpic_core::pipeline::{
    CapturePipeline, DeliveryError, ImageStore, ImageStoreError, StoredOutput,
};
use newpic_core::storage::{RecoveredImage, StorageError};
use newpic_platform::clipboard::{
    ClipboardError, ClipboardErrorKind, ClipboardImage, ImageClipboard,
};
use newpic_platform::capture::{
    CaptureProvenance, CaptureScaleMetadata, CaptureScaleSegment, CaptureScaleSource,
    CapturedImage, DisplayScaleSource,
};
use newpic_platform::geometry::{DisplayId, PhysicalRect, ScaleFactor};

fn test_image() -> CapturedImage {
    CapturedImage::new(
        2,
        2,
        8,
        vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 255, 255, 255, 0,
        ],
        CaptureScaleMetadata::uniform(ScaleFactor::new(1.0).unwrap()),
        CaptureProvenance::Region,
    )
    .unwrap()
}

#[derive(Debug, Clone, Copy)]
enum StoreBehavior {
    Stored,
    Recovered,
    Failed,
}

#[derive(Debug)]
struct FakeStore {
    behavior: StoreBehavior,
    calls: AtomicUsize,
    encoded: Mutex<Vec<Vec<u8>>>,
}

impl FakeStore {
    fn new(behavior: StoreBehavior) -> Self {
        Self {
            behavior,
            calls: AtomicUsize::new(0),
            encoded: Mutex::new(Vec::new()),
        }
    }
}

impl ImageStore for FakeStore {
    fn store(&self, bytes: &[u8]) -> Result<StoredOutput, ImageStoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.encoded.lock().unwrap().push(bytes.to_vec());
        match self.behavior {
            StoreBehavior::Stored => Ok(StoredOutput::Stored {
                path: PathBuf::from("/captures/001.png"),
            }),
            StoreBehavior::Recovered => Ok(StoredOutput::Recovered(RecoveredImage {
                path: PathBuf::from("/recovery/001.png"),
                primary_error: StorageError::NameUnavailable {
                    path: PathBuf::from("/captures/001.png"),
                },
            })),
            StoreBehavior::Failed => Err(ImageStoreError::new("disk unavailable")),
        }
    }
}

#[derive(Debug)]
struct FakeClipboard {
    results: Mutex<Vec<Result<(), ClipboardError>>>,
    calls: AtomicUsize,
    images: Mutex<Vec<ClipboardImage>>,
}

impl FakeClipboard {
    fn new(results: Vec<Result<(), ClipboardError>>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().rev().collect()),
            calls: AtomicUsize::new(0),
            images: Mutex::new(Vec::new()),
        }
    }
}

impl ImageClipboard for FakeClipboard {
    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.images.lock().unwrap().push(image.clone());
        self.results.lock().unwrap().pop().unwrap_or(Ok(()))
    }
}

fn pipeline<'a>(
    store: &'a dyn ImageStore,
    clipboard: &'a dyn ImageClipboard,
) -> CapturePipeline<'a> {
    CapturePipeline::new(store, clipboard)
        .with_clipboard_retry(3, Duration::ZERO)
        .expect("valid retry policy")
}

#[test]
fn storage_and_clipboard_success_produce_complete_report() {
    let store = FakeStore::new(StoreBehavior::Stored);
    let clipboard = FakeClipboard::new(vec![Ok(())]);

    let report = pipeline(&store, &clipboard)
        .deliver(&test_image(), ImageEncoding::Png)
        .expect("complete delivery");

    assert!(report.is_complete());
    assert_eq!(report.stored_path(), Some(PathBuf::from("/captures/001.png").as_path()));
    assert_eq!(report.clipboard_attempts(), Some(1));
}

#[test]
fn clipboard_failure_preserves_successful_storage_path() {
    let store = FakeStore::new(StoreBehavior::Stored);
    let clipboard = FakeClipboard::new(vec![Err(ClipboardError::permanent(
        ClipboardErrorKind::Unsupported,
        "clipboard unavailable",
    ))]);

    let report = pipeline(&store, &clipboard)
        .deliver(&test_image(), ImageEncoding::Png)
        .expect("partial delivery");

    assert!(!report.is_complete());
    assert_eq!(report.stored_path(), Some(PathBuf::from("/captures/001.png").as_path()));
    assert_eq!(report.clipboard_failure().unwrap().attempts(), 1);
}

#[test]
fn storage_failure_preserves_successful_clipboard_result() {
    let store = FakeStore::new(StoreBehavior::Failed);
    let clipboard = FakeClipboard::new(vec![Ok(())]);

    let report = pipeline(&store, &clipboard)
        .deliver(&test_image(), ImageEncoding::Png)
        .expect("partial delivery");

    assert!(report.storage_error().is_some());
    assert_eq!(report.clipboard_attempts(), Some(1));
    assert_eq!(clipboard.images.lock().unwrap().len(), 1);
}

#[test]
fn two_output_failures_are_aggregated_after_both_are_attempted() {
    let store = FakeStore::new(StoreBehavior::Failed);
    let clipboard = FakeClipboard::new(vec![Err(ClipboardError::permanent(
        ClipboardErrorKind::PermissionDenied,
        "denied",
    ))]);

    let failure = pipeline(&store, &clipboard)
        .deliver(&test_image(), ImageEncoding::Png)
        .expect_err("both outputs fail");

    let DeliveryError::Outputs { storage, clipboard } = failure else {
        panic!("expected aggregate output failure");
    };
    assert!(storage.to_string().contains("disk unavailable"));
    assert_eq!(clipboard.attempts(), 1);
    assert_eq!(store.calls.load(Ordering::SeqCst), 1);
    assert_eq!(clipboard.error().kind(), ClipboardErrorKind::PermissionDenied);
}

#[test]
fn transient_clipboard_busy_retries_are_bounded_without_duplicate_storage() {
    let store = FakeStore::new(StoreBehavior::Stored);
    let clipboard = FakeClipboard::new(vec![
        Err(ClipboardError::busy("busy one")),
        Err(ClipboardError::busy("busy two")),
        Err(ClipboardError::busy("busy three")),
        Ok(()),
    ]);

    let report = pipeline(&store, &clipboard)
        .deliver(&test_image(), ImageEncoding::Png)
        .expect("storage makes this a partial delivery");

    assert_eq!(report.clipboard_failure().unwrap().attempts(), 3);
    assert_eq!(clipboard.calls.load(Ordering::SeqCst), 3);
    assert_eq!(store.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn permanent_clipboard_errors_are_not_retried() {
    let store = FakeStore::new(StoreBehavior::Stored);
    let clipboard = FakeClipboard::new(vec![
        Err(ClipboardError::permanent(
            ClipboardErrorKind::PermissionDenied,
            "denied",
        )),
        Ok(()),
    ]);

    let report = pipeline(&store, &clipboard)
        .deliver(&test_image(), ImageEncoding::Png)
        .expect("storage makes this a partial delivery");

    assert_eq!(report.clipboard_failure().unwrap().attempts(), 1);
    assert_eq!(clipboard.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn png_decode_matches_source_dimensions_and_pixels() {
    let image = test_image();
    let encoded = encode_image(&image, ImageEncoding::Png).expect("encode PNG");
    let decoded = ImageReader::new(std::io::Cursor::new(encoded.bytes()))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
        .into_rgba8();

    assert_eq!(decoded.dimensions(), (image.width(), image.height()));
    assert_eq!(decoded.as_raw(), image.rgba());
}

#[test]
fn padded_rows_and_mixed_scale_metadata_encode_only_real_pixels_without_mutation() {
    let metadata = CaptureScaleMetadata::mixed(vec![
        CaptureScaleSegment::new(
            PhysicalRect::new(0, 0, 1, 2).unwrap(),
            CaptureScaleSource::Display(DisplayScaleSource::new(
                DisplayId::new("left").unwrap(),
                ScaleFactor::new(1.0).unwrap(),
                0,
            )),
        ),
        CaptureScaleSegment::new(
            PhysicalRect::new(1, 0, 1, 2).unwrap(),
            CaptureScaleSource::Display(DisplayScaleSource::new(
                DisplayId::new("right").unwrap(),
                ScaleFactor::new(2.0).unwrap(),
                1,
            )),
        ),
    ])
    .unwrap();
    let source = vec![
        1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, // row 1 + padding
        9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97, // row 2 + padding
    ];
    let image = CapturedImage::new(
        2,
        2,
        12,
        source.clone(),
        metadata,
        CaptureProvenance::Region,
    )
    .unwrap();

    let encoded = encode_image(&image, ImageEncoding::Png).unwrap();
    let decoded = image::load_from_memory(encoded.bytes()).unwrap().into_rgba8();

    assert_eq!(
        decoded.as_raw(),
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    );
    assert_eq!(image.rgba(), source);
    assert_eq!(image.row_stride(), 12);
}

#[test]
fn clipboard_receives_tightly_packed_pixels_without_mutating_capture() {
    let source = vec![
        1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, // row 1 + padding
        9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97, // row 2 + padding
    ];
    let image = CapturedImage::new(
        2,
        2,
        12,
        source.clone(),
        CaptureScaleMetadata::uniform(ScaleFactor::new(1.0).unwrap()),
        CaptureProvenance::Region,
    )
    .unwrap();
    let store = FakeStore::new(StoreBehavior::Stored);
    let clipboard = FakeClipboard::new(vec![Ok(())]);

    pipeline(&store, &clipboard)
        .deliver(&image, ImageEncoding::Png)
        .unwrap();

    let images = clipboard.images.lock().unwrap();
    assert_eq!(images[0].dimensions(), (2, 2));
    assert_eq!(
        images[0].rgba(),
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    );
    assert_eq!(image.rgba(), source);
}

#[test]
fn jpeg_quality_rejects_values_outside_one_through_one_hundred() {
    assert!(JpegQuality::new(0).is_err());
    assert_eq!(JpegQuality::new(1).unwrap().get(), 1);
    assert_eq!(JpegQuality::new(100).unwrap().get(), 100);
    assert!(JpegQuality::new(101).is_err());
}

#[test]
fn jpeg_encoding_uses_validated_quality_and_source_dimensions() {
    let quality = JpegQuality::new(85).unwrap();
    let encoded = encode_image(&test_image(), ImageEncoding::Jpeg(quality)).unwrap();
    let decoded = image::load_from_memory(encoded.bytes()).unwrap();

    assert_eq!(decoded.width(), 2);
    assert_eq!(decoded.height(), 2);
    assert_eq!(encoded.extension(), "jpg");
}

#[test]
fn recovered_storage_result_is_propagated_with_primary_error() {
    let store = FakeStore::new(StoreBehavior::Recovered);
    let clipboard = FakeClipboard::new(vec![Ok(())]);

    let report = pipeline(&store, &clipboard)
        .deliver(&test_image(), ImageEncoding::Png)
        .unwrap();

    let recovered = report.recovered().expect("recovery result");
    assert_eq!(recovered.path, PathBuf::from("/recovery/001.png"));
    assert!(recovered.primary_error.to_string().contains("already in use"));
    assert!(report.is_complete());
}
