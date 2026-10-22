//! Shared test fixtures and platform test helpers for NewPic.

use std::sync::Mutex;

use newpic_platform::capabilities::{CaptureBackend, CaptureCapabilities};
use newpic_platform::capture::{
    CaptureError, CaptureFallback, CaptureProvenance, CaptureProvider, CaptureRequest,
    CapturedImage, PixelScale,
};
use newpic_platform::geometry::{Display, DisplayId, PhysicalRect, ScaleFactor};

/// Deterministic public-contract fake for capture consumers.
///
/// It records complete requests as observable boundary values and does not
/// expose or imitate a native provider's internal calls.
#[derive(Debug)]
pub struct FakeCaptureProvider {
    capabilities: CaptureCapabilities,
    displays: Vec<Display>,
    requests: Mutex<Vec<CaptureRequest>>,
}

impl FakeCaptureProvider {
    pub fn new(capabilities: CaptureCapabilities, displays: Vec<Display>) -> Self {
        Self {
            capabilities,
            displays,
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn recorded_requests(&self) -> Vec<CaptureRequest> {
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Default for FakeCaptureProvider {
    fn default() -> Self {
        Self::new(
            CaptureCapabilities::new(CaptureBackend::Test)
                .with_region_capture(true)
                .with_active_window_capture(true)
                .with_display_capture(true)
                .with_unattended_capture(true)
                .with_pointer_capture(false),
            vec![Display::new(
                DisplayId::new("fake-primary"),
                PhysicalRect::new(0, 0, 1920, 1080).expect("valid fake display bounds"),
                ScaleFactor::new(1.0).expect("valid fake scale"),
                true,
            )],
        )
    }
}

impl CaptureProvider for FakeCaptureProvider {
    fn capabilities(&self) -> CaptureCapabilities {
        self.capabilities.clone()
    }

    fn displays(&self) -> Result<Vec<Display>, CaptureError> {
        Ok(self.displays.clone())
    }

    fn capture(&self, request: CaptureRequest) -> Result<CapturedImage, CaptureError> {
        let provenance = match &request {
            CaptureRequest::Region(_) => CaptureProvenance::Region,
            CaptureRequest::ActiveWindow => CaptureProvenance::ActiveWindow {
                fallback: Some(CaptureFallback::WholeWindow),
            },
            CaptureRequest::Display(id) => CaptureProvenance::Display(id.clone()),
        };
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request);

        CapturedImage::new(
            2,
            2,
            8,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
            PixelScale::ONE,
            provenance,
        )
    }
}
