//! State the background check writes and the UI reads.

use crate::update::release::Release;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// One of five terminal states reported by the background checker. The
/// initial state at process start is [`UpdateState::Idle`]; the checker
/// transitions through [`UpdateState::Checking`] and ends in one of the
/// three remaining variants.
#[derive(Debug, Clone, Default)]
pub enum UpdateState {
    /// No check has been requested yet.
    #[default]
    Idle,
    /// A check is in flight.
    Checking,
    /// The latest release matches or precedes the running version.
    NoUpdate { checked_at: SystemTime },
    /// A newer release was found.
    Available {
        release: Release,
        checked_at: SystemTime,
    },
    /// The check could not complete.
    Failed {
        reason: String,
        checked_at: SystemTime,
    },
}

impl UpdateState {
    /// The timestamp of the last completed check, if any.
    pub fn checked_at(&self) -> Option<SystemTime> {
        match self {
            UpdateState::Idle | UpdateState::Checking => None,
            UpdateState::NoUpdate { checked_at }
            | UpdateState::Available { checked_at, .. }
            | UpdateState::Failed { checked_at, .. } => Some(*checked_at),
        }
    }
}

/// The handle shared between background threads and the UI.
pub type SharedUpdateState = Arc<Mutex<UpdateState>>;

/// Build a fresh shared state in the [`UpdateState::Idle`] variant.
pub fn new_shared() -> SharedUpdateState {
    Arc::new(Mutex::new(UpdateState::Idle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn dummy_release() -> crate::update::Release {
        crate::update::Release {
            tag_name: "v0.2.6".to_string(),
            name: "PerfWindow 0.2.6".to_string(),
            html_url: "https://example/v0.2.6".to_string(),
            body: "notes".to_string(),
            assets: vec![],
        }
    }

    #[test]
    fn default_state_is_idle() {
        let s = UpdateState::default();
        assert!(matches!(s, UpdateState::Idle));
    }

    #[test]
    fn idle_advances_to_checking_on_check_start() {
        let s = UpdateState::Checking;
        assert!(matches!(s, UpdateState::Checking));
    }

    #[test]
    fn no_update_carries_a_checked_at_timestamp() {
        let t = SystemTime::now();
        let s = UpdateState::NoUpdate { checked_at: t };
        match s {
            UpdateState::NoUpdate { checked_at } => assert_eq!(checked_at, t),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn available_carries_release_and_checked_at() {
        let t = SystemTime::now();
        let r = dummy_release();
        let s = UpdateState::Available {
            release: r.clone(),
            checked_at: t,
        };
        match s {
            UpdateState::Available {
                release,
                checked_at,
            } => {
                assert_eq!(release.tag_name, r.tag_name);
                assert_eq!(checked_at, t);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn failed_carries_reason_and_checked_at() {
        let t = SystemTime::now();
        let s = UpdateState::Failed {
            checked_at: t,
            reason: "network".to_string(),
        };
        match s {
            UpdateState::Failed { checked_at, reason } => {
                assert_eq!(checked_at, t);
                assert_eq!(reason, "network");
            }
            _ => panic!("wrong variant"),
        }
    }
}
