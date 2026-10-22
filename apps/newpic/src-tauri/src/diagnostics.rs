//! Privacy-conscious diagnostics: operational metadata only, never pixels,
//! clipboard contents, or settings values beyond what the user configured.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use newpic_platform::capabilities::{CapabilityStatus, CaptureCapabilities, CaptureOperation};
use serde::Serialize;

use crate::autostart::AutostartStatus;
use crate::hotkeys::RegistrationEntry;

pub const MAX_EVENTS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticEvent {
    pub sequence: u64,
    pub level: EventLevel,
    pub message: String,
}

/// A bounded ring of recent operational events.
#[derive(Debug, Default)]
pub struct DiagnosticLog {
    events: VecDeque<DiagnosticEvent>,
    next_sequence: u64,
}

impl DiagnosticLog {
    pub fn push(&mut self, level: EventLevel, message: impl Into<String>) {
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.next_sequence += 1;
        self.events.push_back(DiagnosticEvent {
            sequence: self.next_sequence,
            level,
            message: message.into(),
        });
    }

    pub fn events(&self) -> Vec<DiagnosticEvent> {
        self.events.iter().cloned().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityEntry {
    pub operation: &'static str,
    pub state: &'static str,
    pub reason: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureDiagnostics {
    pub backend: String,
    pub capabilities: Vec<CapabilityEntry>,
}

impl CaptureDiagnostics {
    pub fn from_capabilities(capabilities: &CaptureCapabilities) -> Self {
        let entries = [
            (CaptureOperation::Region, "region"),
            (CaptureOperation::ActiveWindow, "active_window"),
            (CaptureOperation::Display, "display"),
            (CaptureOperation::Unattended, "unattended"),
            (CaptureOperation::Pointer, "pointer"),
        ]
        .into_iter()
        .map(|(operation, name)| {
            let status = capabilities.status(operation);
            CapabilityEntry {
                operation: name,
                state: match status {
                    CapabilityStatus::Supported => "supported",
                    CapabilityStatus::Limited { .. } => "limited",
                    CapabilityStatus::Unsupported { .. } => "unsupported",
                },
                reason: status.reason().map(str::to_owned),
                hint: status.hint().map(str::to_owned),
            }
        })
        .collect();
        Self {
            backend: format!("{:?}", capabilities.backend()),
            capabilities: entries,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DestinationState {
    Writable,
    /// The folder does not exist yet; it will be created on the next save.
    WillBeCreated,
    NotWritable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DestinationStatus {
    pub path: PathBuf,
    #[serde(flatten)]
    pub state: DestinationState,
}

/// Checks writability without creating the destination: probes the folder
/// itself, or its nearest existing ancestor when it is missing.
pub fn probe_destination(path: &Path) -> DestinationStatus {
    let mut candidate = path;
    let missing = !path.is_dir();
    if missing {
        match path.ancestors().skip(1).find(|ancestor| ancestor.is_dir()) {
            Some(ancestor) => candidate = ancestor,
            None => {
                return DestinationStatus {
                    path: path.to_owned(),
                    state: DestinationState::NotWritable {
                        reason: "no existing parent folder".to_owned(),
                    },
                }
            }
        }
    }
    let state = match tempfile::Builder::new()
        .prefix(".newpic-probe-")
        .tempfile_in(candidate)
    {
        Ok(_) if missing => DestinationState::WillBeCreated,
        Ok(_) => DestinationState::Writable,
        Err(error) => DestinationState::NotWritable {
            reason: error.to_string(),
        },
    };
    DestinationStatus {
        path: path.to_owned(),
        state,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub os: &'static str,
    pub session: Option<String>,
    pub capture: CaptureDiagnostics,
    pub hotkeys: Vec<RegistrationEntry>,
    pub autostart: AutostartStatus,
    pub destination: DestinationStatus,
    pub settings_notice: Option<String>,
    pub events: Vec<DiagnosticEvent>,
}

#[cfg(test)]
mod tests {
    use newpic_platform::capabilities::CaptureBackend;

    use super::*;

    #[test]
    fn log_keeps_only_the_most_recent_events() {
        let mut log = DiagnosticLog::default();
        for index in 0..(MAX_EVENTS + 5) {
            log.push(EventLevel::Info, format!("event {index}"));
        }
        let events = log.events();
        assert_eq!(events.len(), MAX_EVENTS);
        assert_eq!(events[0].message, "event 5");
        assert_eq!(events.last().unwrap().sequence, (MAX_EVENTS + 5) as u64);
    }

    #[test]
    fn destination_probe_never_creates_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("Downloads").join("NewPic");

        let status = probe_destination(&missing);

        assert_eq!(status.state, DestinationState::WillBeCreated);
        assert!(!missing.exists());
        assert_eq!(
            probe_destination(root.path()).state,
            DestinationState::Writable
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn capabilities_are_reported_with_reasons() {
        let capabilities = CaptureCapabilities::new(CaptureBackend::XdgDesktopPortal)
            .with_active_window_capture(CapabilityStatus::unsupported(
                "portal cannot target a window",
                Some("use region capture"),
            ));

        let report = CaptureDiagnostics::from_capabilities(&capabilities);

        assert_eq!(report.backend, "XdgDesktopPortal");
        let window = &report.capabilities[1];
        assert_eq!(window.operation, "active_window");
        assert_eq!(window.state, "unsupported");
        assert_eq!(window.hint.as_deref(), Some("use region capture"));
    }
}
