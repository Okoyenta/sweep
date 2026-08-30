//! Rendering for `sweep undo`.
//!
//! Reports per-item restoration results. Items whose Recycle Bin entry has been
//! purged are called out explicitly as `unrecoverable` rather than failing
//! silently (FR-008).

use std::io::Write;

use crate::services::undo_service::UndoOutcome;

/// Print an undo outcome to stdout.
pub fn print_outcome(outcome: &UndoOutcome) {
    let mut out = std::io::stdout();
    let _ = write_outcome(outcome, &mut out);
}

/// Write an undo outcome to `w`.
pub fn write_outcome(outcome: &UndoOutcome, w: &mut impl Write) -> std::io::Result<()> {
    match outcome {
        UndoOutcome::NoSession => writeln!(w, "no session to undo"),
        UndoOutcome::Error(e) => writeln!(w, "undo failed: {e}"),
        UndoOutcome::Restored {
            session_id,
            restored,
            unrecoverable,
        } => {
            for path in unrecoverable {
                writeln!(
                    w,
                    "  unrecoverable (recycle bin purged): {}",
                    path.display()
                )?;
            }
            writeln!(
                w,
                "restored {} item(s) from session {}{}",
                restored,
                session_id,
                if unrecoverable.is_empty() {
                    String::new()
                } else {
                    format!(", {} unrecoverable", unrecoverable.len())
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn no_session_reports_cleanly() {
        let mut buf = Vec::new();
        write_outcome(&UndoOutcome::NoSession, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap().trim(), "no session to undo");
    }

    #[test]
    fn purged_items_are_reported_unrecoverable() {
        let mut buf = Vec::new();
        write_outcome(
            &UndoOutcome::Restored {
                session_id: "s1".into(),
                restored: 1,
                unrecoverable: vec![PathBuf::from("/tmp/gone")],
            },
            &mut buf,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("unrecoverable (recycle bin purged)"));
        assert!(out.contains("restored 1 item(s) from session s1"));
        assert!(out.contains("1 unrecoverable"));
    }
}
