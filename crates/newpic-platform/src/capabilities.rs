/// The capture implementation currently selected by a platform adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptureBackend {
    WindowsGraphicsCapture,
    WindowsGdi,
    MacOsScreenCaptureKit,
    X11,
    XdgDesktopPortal,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureOperation {
    Region,
    ActiveWindow,
    Display,
    Unattended,
    Pointer,
}

/// Diagnostic-ready support state for one provider feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,
    Limited {
        reason: String,
        hint: Option<String>,
    },
    Unsupported {
        reason: String,
        hint: Option<String>,
    },
}

impl CapabilityStatus {
    pub const fn supported() -> Self {
        Self::Supported
    }

    pub fn limited(reason: impl Into<String>, hint: Option<impl Into<String>>) -> Self {
        Self::Limited {
            reason: reason.into(),
            hint: hint.map(Into::into),
        }
    }

    pub fn unsupported(reason: impl Into<String>, hint: Option<impl Into<String>>) -> Self {
        Self::Unsupported {
            reason: reason.into(),
            hint: hint.map(Into::into),
        }
    }

    pub const fn is_available(&self) -> bool {
        !matches!(self, Self::Unsupported { .. })
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Supported => None,
            Self::Limited { reason, .. } | Self::Unsupported { reason, .. } => Some(reason),
        }
    }

    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Supported => None,
            Self::Limited { hint, .. } | Self::Unsupported { hint, .. } => hint.as_deref(),
        }
    }
}

/// Explicit feature report for a [`CaptureProvider`](crate::capture::CaptureProvider).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureCapabilities {
    backend: CaptureBackend,
    region_capture: CapabilityStatus,
    active_window_capture: CapabilityStatus,
    display_capture: CapabilityStatus,
    unattended_capture: CapabilityStatus,
    pointer_capture: CapabilityStatus,
}

impl CaptureCapabilities {
    pub fn new(backend: CaptureBackend) -> Self {
        let not_reported =
            || CapabilityStatus::unsupported("provider did not report support", None::<String>);
        Self {
            backend,
            region_capture: not_reported(),
            active_window_capture: not_reported(),
            display_capture: not_reported(),
            unattended_capture: not_reported(),
            pointer_capture: not_reported(),
        }
    }

    pub fn with_region_capture(mut self, status: CapabilityStatus) -> Self {
        self.region_capture = status;
        self
    }

    pub fn with_active_window_capture(mut self, status: CapabilityStatus) -> Self {
        self.active_window_capture = status;
        self
    }

    pub fn with_display_capture(mut self, status: CapabilityStatus) -> Self {
        self.display_capture = status;
        self
    }

    pub fn with_unattended_capture(mut self, status: CapabilityStatus) -> Self {
        self.unattended_capture = status;
        self
    }

    pub fn with_pointer_capture(mut self, status: CapabilityStatus) -> Self {
        self.pointer_capture = status;
        self
    }

    pub const fn backend(&self) -> CaptureBackend {
        self.backend
    }

    pub const fn region_capture(&self) -> &CapabilityStatus {
        &self.region_capture
    }

    pub const fn active_window_capture(&self) -> &CapabilityStatus {
        &self.active_window_capture
    }

    pub const fn display_capture(&self) -> &CapabilityStatus {
        &self.display_capture
    }

    pub const fn unattended_capture(&self) -> &CapabilityStatus {
        &self.unattended_capture
    }

    pub const fn pointer_capture(&self) -> &CapabilityStatus {
        &self.pointer_capture
    }

    pub const fn status(&self, operation: CaptureOperation) -> &CapabilityStatus {
        match operation {
            CaptureOperation::Region => self.region_capture(),
            CaptureOperation::ActiveWindow => self.active_window_capture(),
            CaptureOperation::Display => self.display_capture(),
            CaptureOperation::Unattended => self.unattended_capture(),
            CaptureOperation::Pointer => self.pointer_capture(),
        }
    }
}
