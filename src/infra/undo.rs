//! Undo journal: records each trashed session so `sweep undo` can restore the
//! most recent run from the Recycle Bin.
//!
//! The journal is a TOML file in the sweep data dir. Every `sweep clean` (and
//! every guard/idle auto-clean that actually trashes) appends an [`UndoSession`]
//! capturing what was trashed, enabling one-command, trash-backed recovery.
//!
//! The journal shapes live in `domain::models` so services can build them
//! without depending on this module (Constitution Principle III).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub use crate::domain::models::{UndoItem, UndoJournal, UndoSession};

const JOURNAL_FILE: &str = "undo_journal.toml";
const MAX_SESSIONS: usize = 20;

/// Absolute path of the on-disk undo journal.
pub fn journal_path() -> PathBuf {
    crate::infra::paths::data_dir().join(JOURNAL_FILE)
}

/// Load the journal, returning an empty one if absent or unreadable.
///
/// A corrupt journal is treated as empty rather than fatal: losing undo history
/// must never block a clean run.
pub fn read_journal() -> UndoJournal {
    let p = journal_path();
    match std::fs::read_to_string(&p) {
        Ok(s) => parse_journal(&s),
        Err(_) => UndoJournal::default(),
    }
}

/// Parse journal TOML, falling back to an empty journal on malformed input.
fn parse_journal(s: &str) -> UndoJournal {
    toml::from_str(s).unwrap_or_default()
}

/// Persist the journal to disk (creating the data dir if needed).
pub fn write_journal(j: &UndoJournal) -> std::io::Result<()> {
    let p = journal_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let s = toml::to_string(j).unwrap_or_default();
    std::fs::write(&p, s)
}

/// Append a new session (with `items`) to the journal and keep at most the most
/// recent [`MAX_SESSIONS`] runs.
///
/// An empty `items` list is ignored so a no-op clean does not shadow the last
/// real session that `sweep undo` could still restore.
pub fn append_session(items: Vec<UndoItem>) -> std::io::Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let mut j = read_journal();
    let session_id = format!("{}-{}", now_unix(), j.sessions.len());
    j.sessions.push(UndoSession {
        session_id,
        timestamp: now_unix(),
        items,
    });
    if j.sessions.len() > MAX_SESSIONS {
        let excess = j.sessions.len() - MAX_SESSIONS;
        j.sessions.drain(0..excess);
    }
    write_journal(&j)
}

/// Restore the newest journaled session's items from the Recycle Bin.
///
/// Items whose trash entry no longer exists (the Bin was purged) are reported
/// as unrecoverable rather than failing the whole operation.
pub fn restore_latest() -> UndoResult {
    let journal = read_journal();
    let latest = match journal.sessions.last() {
        Some(s) => s.clone(),
        None => return UndoResult::NoSession,
    };

    let trash_items = match trash::os_limited::list() {
        Ok(items) => items,
        Err(_) => return UndoResult::Error("could not read Recycle Bin index".into()),
    };

    let mut restored = 0u64;
    let mut unrecoverable = Vec::new();
    let mut pairs: Vec<(PathBuf, trash::TrashItem)> = Vec::new();
    for item in &latest.items {
        match find_trash_item(&trash_items, item) {
            Some(t) => pairs.push((item.original_path.clone(), t)),
            None => unrecoverable.push(item.original_path.clone()),
        }
    }
    for (orig, t) in pairs {
        if trash::os_limited::restore_all(std::iter::once(t)).is_ok() {
            restored += 1;
        } else {
            unrecoverable.push(orig);
        }
    }

    UndoResult::Restored {
        session_id: latest.session_id,
        restored,
        unrecoverable,
    }
}

fn find_trash_item(
    trash_items: &[trash::TrashItem],
    journal_item: &UndoItem,
) -> Option<trash::TrashItem> {
    let target = match_key(&journal_item.trash_path);
    trash_items
        .iter()
        .find(|t| match_key(&t.original_path()) == target)
        .cloned()
}

/// Normalize a path for comparison between the journal and the trash index.
///
/// A category root can reach the journal with forward slashes (it came from
/// `sweep.toml`) while the Recycle Bin reports the canonical backslash form, so
/// separators and case are folded before matching. Without this every restore
/// would be misreported as "recycle bin purged".
fn match_key(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Outcome of an `undo` operation for user-facing reporting.
pub enum UndoResult {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str) -> UndoItem {
        UndoItem {
            original_path: PathBuf::from(path),
            trash_path: PathBuf::from(path),
            size_bytes: 10,
        }
    }

    #[test]
    fn journal_round_trips_through_toml() {
        let j = UndoJournal {
            sessions: vec![UndoSession {
                session_id: "s1".into(),
                timestamp: 42,
                items: vec![item("/tmp/a"), item("/tmp/b")],
            }],
        };
        let encoded = toml::to_string(&j).unwrap();
        let decoded = parse_journal(&encoded);
        assert_eq!(decoded.sessions.len(), 1);
        assert_eq!(decoded.sessions[0].session_id, "s1");
        assert_eq!(decoded.sessions[0].items.len(), 2);
        assert_eq!(decoded.sessions[0].items[0].size_bytes, 10);
    }

    #[test]
    fn corrupt_journal_parses_as_empty() {
        let decoded = parse_journal("this is not { valid toml ][");
        assert!(decoded.sessions.is_empty());
    }

    #[test]
    fn newest_session_is_last() {
        let j = UndoJournal {
            sessions: vec![
                UndoSession {
                    session_id: "old".into(),
                    timestamp: 1,
                    items: vec![item("/tmp/old")],
                },
                UndoSession {
                    session_id: "new".into(),
                    timestamp: 2,
                    items: vec![item("/tmp/new")],
                },
            ],
        };
        assert_eq!(j.sessions.last().unwrap().session_id, "new");
    }

    #[test]
    fn match_key_folds_separators_and_case() {
        // The journal may hold a forward-slash path from sweep.toml while the
        // Recycle Bin reports the canonical backslash form.
        assert_eq!(
            match_key(std::path::Path::new(r"C:/Users/Me/Cache\a.bin")),
            match_key(std::path::Path::new(r"c:\users\me\cache\a.bin"))
        );
    }
}
