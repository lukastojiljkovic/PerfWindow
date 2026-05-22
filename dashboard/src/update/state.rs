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
