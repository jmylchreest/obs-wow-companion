//! Recording ownership and conservative mutation guards.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    #[default]
    Idle,
    Starting,
    Managed,
    Observed,
    Uncertain,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Guard {
    pub ownership: Ownership,
    pub paused: bool,
    pub split_pending: bool,
    pub stop_pending: bool,
}
impl Guard {
    pub fn arm(&mut self, already_recording: bool) -> bool {
        if already_recording {
            self.ownership = Ownership::Observed;
            false
        } else if self.ownership == Ownership::Idle {
            self.ownership = Ownership::Starting;
            true
        } else {
            false
        }
    }
    pub fn started(&mut self) {
        self.ownership = if self.ownership == Ownership::Starting {
            Ownership::Managed
        } else {
            Ownership::Observed
        };
    }
    pub fn can_mutate(&self) -> bool {
        self.ownership == Ownership::Managed && !self.paused && !self.stop_pending
    }
    pub fn request_split(&mut self) -> bool {
        if !self.can_mutate() || self.split_pending {
            false
        } else {
            self.split_pending = true;
            true
        }
    }
    pub fn file_changed(&mut self) {
        if !self.split_pending && self.ownership == Ownership::Managed {
            self.ownership = Ownership::Uncertain;
        }
        self.split_pending = false;
    }
    pub fn request_stop(&mut self) -> bool {
        if self.can_mutate() {
            self.stop_pending = true;
            true
        } else {
            false
        }
    }
    pub const fn stopped(&mut self) {
        self.ownership = Ownership::Idle;
        self.split_pending = false;
        self.stop_pending = false;
        self.paused = false;
    }
    pub const fn lose_ownership(&mut self) {
        self.ownership = Ownership::Uncertain;
        self.split_pending = false;
        self.stop_pending = false;
    }
    pub const fn reconcile(&mut self, recording: bool) {
        self.ownership = if recording {
            Ownership::Observed
        } else {
            Ownership::Idle
        };
        self.paused = false;
        self.split_pending = false;
        self.stop_pending = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_recording_cannot_be_mutated() {
        let mut g = Guard::default();
        assert!(!g.arm(true));
        assert!(!g.request_stop());
        assert!(!g.request_split());
    }
    #[test]
    fn ownership_lost_on_disconnect_and_unexpected_file() {
        let mut g = Guard::default();
        assert!(g.arm(false));
        g.started();
        assert!(g.request_split());
        g.file_changed();
        assert!(g.can_mutate());
        g.file_changed();
        assert!(!g.can_mutate());
        g.reconcile(true);
        assert!(!g.request_stop());
    }
    #[test]
    fn start_is_not_ownership_until_confirmed() {
        let mut g = Guard::default();
        g.arm(false);
        assert!(!g.can_mutate());
        g.lose_ownership();
        g.started();
        assert_eq!(g.ownership, Ownership::Observed);
    }
}
