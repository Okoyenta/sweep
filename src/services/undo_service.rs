//! Restores the most recent trashed session from the undo journal.
//!
//! Wraps the low-level `infra::undo` journal logic and surfaces a typed outcome
//! for the CLI layer to report to the user.

use std::path::PathBuf;

use crate::infra::undo::{restore_latest, UndoResult};

/// Service that performs `sweep undo`.
pub struct UndoService;

impl UndoService {
    /// Create an undo service. Holds no state; the journal is read per call.
    pub fn new() -> Self {
        Self
    }

    /// Restore the newest journaled session's items from the Recycle Bin.
    pub fn undo(&self) -> UndoOutcome {
        match restore_latest() {
            UndoResult::NoSession => UndoOutcome::NoSession,
            UndoResult::Error(e) => UndoOutcome::Error(e),
            UndoResult::Restored {
                session_id,
                restored,
                unrecoverable,
            } => UndoOutcome::Restored {
                session_id,
                restored,
                unrecoverable,
            },
        }
    }
}

impl Default for UndoService {
    fn default() -> Self {
        Self::new()
    }
}

/// User-facing result of an undo operation.
pub enum UndoOutcome {
    /// No journal existed; nothing to undo.
    NoSession,
    /// `restored` items were moved back; `unrecoverable` were not in the Bin.
    Restored {
        session_id: String,
        restored: u64,
        unrecoverable: Vec<PathBuf>,
    },
    /// A low-level error prevented reading the trash index.
    Error(String),
}
