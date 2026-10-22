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

/// Explicit feature report for a [`CaptureProvider`](crate::capture::CaptureProvider).
///
/// Callers use this value to present limitations rather than inferring support
/// from the operating system name or waiting for an operation to fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureCapabilities {
    backend: CaptureBackend,
    region_capture: bool,
    active_window_capture: bool,
    display_capture: bool,
    unattended_capture: bool,
    pointer_capture: bool,
}

impl CaptureCapabilities {
    pub const fn new(backend: CaptureBackend) -> Self {
        Self {
            backend,
            region_capture: false,
            active_window_capture: false,
            display_capture: false,
            unattended_capture: false,
            pointer_capture: false,
        }
    }

    pub const fn with_region_capture(mut self, supported: bool) -> Self {
        self.region_capture = supported;
        self
    }

    pub const fn with_active_window_capture(mut self, supported: bool) -> Self {
        self.active_window_capture = supported;
        self
    }

    pub const fn with_display_capture(mut self, supported: bool) -> Self {
        self.display_capture = supported;
        self
    }

    pub const fn with_unattended_capture(mut self, supported: bool) -> Self {
        self.unattended_capture = supported;
        self
    }

    pub const fn with_pointer_capture(mut self, supported: bool) -> Self {
        self.pointer_capture = supported;
        self
    }

    pub const fn backend(&self) -> CaptureBackend {
        self.backend
    }

    pub const fn region_capture(&self) -> bool {
        self.region_capture
    }

    pub const fn active_window_capture(&self) -> bool {
        self.active_window_capture
    }

    pub const fn display_capture(&self) -> bool {
        self.display_capture
    }

    pub const fn unattended_capture(&self) -> bool {
        self.unattended_capture
    }

    pub const fn pointer_capture(&self) -> bool {
        self.pointer_capture
    }
}
