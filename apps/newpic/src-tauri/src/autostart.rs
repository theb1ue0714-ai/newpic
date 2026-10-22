//! Reconciles the login-startup setting with the operating system.

use serde::Serialize;

/// Port to the per-user login startup registration.
pub trait AutostartControl {
    fn is_enabled(&self) -> Result<bool, String>;
    fn enable(&self) -> Result<(), String>;
    fn disable(&self) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AutostartStatus {
    /// Startup has not been reconciled yet.
    Unknown,
    InSync {
        enabled: bool,
    },
    Failed {
        desired: bool,
        reason: String,
    },
}

impl AutostartStatus {
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// Makes the OS registration match `desired`. Never panics or aborts: any
/// failure is returned as a status for diagnostics and notification.
pub fn reconcile(desired: bool, control: &dyn AutostartControl) -> AutostartStatus {
    let result = control.is_enabled().and_then(|current| {
        if current == desired {
            Ok(())
        } else if desired {
            control.enable()
        } else {
            control.disable()
        }
    });
    match result {
        Ok(()) => AutostartStatus::InSync { enabled: desired },
        Err(reason) => AutostartStatus::Failed { desired, reason },
    }
}

impl AutostartControl for tauri_plugin_autostart::AutoLaunchManager {
    fn is_enabled(&self) -> Result<bool, String> {
        tauri_plugin_autostart::AutoLaunchManager::is_enabled(self).map_err(|e| e.to_string())
    }

    fn enable(&self) -> Result<(), String> {
        tauri_plugin_autostart::AutoLaunchManager::enable(self).map_err(|e| e.to_string())
    }

    fn disable(&self) -> Result<(), String> {
        tauri_plugin_autostart::AutoLaunchManager::disable(self).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    struct FakeControl {
        enabled: Cell<bool>,
        fail_query: bool,
        fail_change: bool,
        changes: RefCell<Vec<bool>>,
    }

    impl FakeControl {
        fn new(enabled: bool) -> Self {
            Self {
                enabled: Cell::new(enabled),
                fail_query: false,
                fail_change: false,
                changes: RefCell::new(Vec::new()),
            }
        }
    }

    impl AutostartControl for FakeControl {
        fn is_enabled(&self) -> Result<bool, String> {
            if self.fail_query {
                return Err("query failed".into());
            }
            Ok(self.enabled.get())
        }

        fn enable(&self) -> Result<(), String> {
            self.change(true)
        }

        fn disable(&self) -> Result<(), String> {
            self.change(false)
        }
    }

    impl FakeControl {
        fn change(&self, value: bool) -> Result<(), String> {
            if self.fail_change {
                return Err("permission denied".into());
            }
            self.changes.borrow_mut().push(value);
            self.enabled.set(value);
            Ok(())
        }
    }

    #[test]
    fn matching_state_makes_no_changes() {
        let control = FakeControl::new(true);
        assert_eq!(
            reconcile(true, &control),
            AutostartStatus::InSync { enabled: true }
        );
        assert!(control.changes.borrow().is_empty());
    }

    #[test]
    fn differing_state_is_enabled_or_disabled() {
        let control = FakeControl::new(false);
        assert_eq!(
            reconcile(true, &control),
            AutostartStatus::InSync { enabled: true }
        );
        assert_eq!(
            reconcile(false, &control),
            AutostartStatus::InSync { enabled: false }
        );
        assert_eq!(*control.changes.borrow(), vec![true, false]);
    }

    #[test]
    fn failures_become_a_status_instead_of_an_abort() {
        let mut control = FakeControl::new(false);
        control.fail_change = true;
        let status = reconcile(true, &control);
        assert!(status.is_failed());
        assert_eq!(
            status,
            AutostartStatus::Failed {
                desired: true,
                reason: "permission denied".into()
            }
        );

        let mut control = FakeControl::new(false);
        control.fail_query = true;
        assert!(reconcile(false, &control).is_failed());
    }
}
