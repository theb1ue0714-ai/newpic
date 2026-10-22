//! Shared test fixtures and platform test helpers for NewPic.

use std::sync::Mutex;

use newpic_platform::capabilities::{
    CapabilityStatus, CaptureBackend, CaptureCapabilities, CaptureOperation,
};
use newpic_platform::capture::{
    CaptureError, CaptureFallback, CaptureProvenance, CaptureProvider, CaptureRequest,
    CaptureSource, CapturedImage, PixelScale,
};
use newpic_platform::geometry::{
    Display, DisplayId, DisplayOrientation, PhysicalRect, ScaleFactor,
};

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
                .with_region_capture(CapabilityStatus::supported())
                .with_active_window_capture(CapabilityStatus::supported())
                .with_display_capture(CapabilityStatus::supported())
                .with_unattended_capture(CapabilityStatus::supported())
                .with_pointer_capture(CapabilityStatus::supported()),
            vec![Display::new(
                DisplayId::new("fake-primary"),
                PhysicalRect::new(0, 0, 1920, 1080).expect("valid fake display bounds"),
                ScaleFactor::new(1.0).expect("valid fake scale"),
                true,
                DisplayOrientation::Landscape,
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
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request.clone());

        let (operation, operation_name) = match request.source() {
            CaptureSource::Region(_) => (CaptureOperation::Region, "region"),
            CaptureSource::ActiveWindow => (CaptureOperation::ActiveWindow, "active_window"),
            CaptureSource::Display(_) => (CaptureOperation::Display, "display"),
        };
        require_capability(&self.capabilities, operation, operation_name)?;
        if request.options().include_pointer() {
            require_capability(
                &self.capabilities,
                CaptureOperation::Pointer,
                "pointer_capture",
            )?;
        }
        if let CaptureSource::Display(id) = request.source() {
            if !self.displays.iter().any(|display| display.id() == id) {
                return Err(CaptureError::DisplayNotFound(id.clone()));
            }
        }

        let provenance = match request.source() {
            CaptureSource::Region(_) => CaptureProvenance::Region,
            CaptureSource::ActiveWindow => CaptureProvenance::ActiveWindow {
                fallback: Some(CaptureFallback::WholeWindow),
            },
            CaptureSource::Display(id) => CaptureProvenance::Display(id.clone()),
        };

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

fn require_capability(
    capabilities: &CaptureCapabilities,
    operation: CaptureOperation,
    operation_name: &'static str,
) -> Result<(), CaptureError> {
    let status = capabilities.status(operation);
    if status.is_available() {
        return Ok(());
    }
    Err(CaptureError::Unsupported {
        operation: operation_name,
        reason: status.reason().unwrap_or("unsupported").to_owned(),
    })
}
