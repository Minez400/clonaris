//! # Operation Log & Universal Undo Engine
//!
//! Maintains an append-only transaction log (`.clonaris/oplog`) recording every
//! modifying operation (`save`, `switch`, `merge`, `amend`).
//!
//! Enables instantaneous, risk-free `undo` and `redo` without complex reflog parsing.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// A single recorded operation in the transaction history.
#[derive(Debug, Clone)]
pub struct OpEntry {
    /// Unix timestamp when the operation occurred.
    pub timestamp: u64,
    /// Action verb: `"save"`, `"switch"`, `"merge"`, `"amend"`.
    pub action: String,
    /// Snapshot hash before the operation (`"none"` if initialized).
    pub prev_head: String,
    /// Snapshot hash after the operation.
    pub new_head: String,
    /// Human-readable summary or commit message.
    pub description: String,
}

/// Encodes multiline descriptions to preserve the single-line-per-entry format.
fn encode_desc(desc: &str) -> String {
    desc.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "")
}

/// Decodes escaped descriptions back to their original formatting.
fn decode_desc(desc: &str) -> String {
    desc.replace("\\n", "\n").replace("\\\\", "\\")
}

/// Appends a new operation record to `.clonaris/oplog` and clears the redo stack.
pub fn append_op(
    repo_root: &Path,
    action: &str,
    prev_head: Option<&str>,
    new_head: Option<&str>,
    description: &str,
) -> io::Result<()> {
    // Standard undo/redo semantics: new actions clear the redo log
    let redo_path = repo_root.join(".clonaris").join("redolog");
    if redo_path.exists() {
        let _ = fs::write(&redo_path, "");
    }

    let oplog_path = repo_root.join(".clonaris").join("oplog");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(oplog_path)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let prev = prev_head.unwrap_or("none");
    let next = new_head.unwrap_or("none");
    let safe_desc = encode_desc(description);

    writeln!(file, "{}|{}|{}|{}|{}", now, action, prev, next, safe_desc)?;
    Ok(())
}

/// Reads all operation entries from an oplog file (`oplog` or `redolog`).
pub fn read_ops(repo_root: &Path, filename: &str) -> io::Result<Vec<OpEntry>> {
    let path = repo_root.join(".clonaris").join(filename);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() == 5 {
            entries.push(OpEntry {
                timestamp: parts[0].parse().unwrap_or(0),
                action: parts[1].to_string(),
                prev_head: parts[2].to_string(),
                new_head: parts[3].to_string(),
                description: decode_desc(parts[4]),
            });
        }
    }

    Ok(entries)
}

/// Pops and returns the last operation from an oplog file, rewriting the remaining entries.
pub fn pop_op(repo_root: &Path, filename: &str) -> io::Result<Option<OpEntry>> {
    let path = repo_root.join(".clonaris").join(filename);
    if !path.exists() {
        return Ok(None);
    }

    let mut ops = read_ops(repo_root, filename)?;
    let last = ops.pop();

    let mut out = String::new();
    for op in &ops {
        out.push_str(&format!(
            "{}|{}|{}|{}|{}\n",
            op.timestamp,
            op.action,
            op.prev_head,
            op.new_head,
            encode_desc(&op.description)
        ));
    }
    fs::write(path, out)?;

    Ok(last)
}

/// Pushes an operation entry into the redo stack (`.clonaris/redolog`).
pub fn push_redo(repo_root: &Path, op: &OpEntry) -> io::Result<()> {
    let redo_path = repo_root.join(".clonaris").join("redolog");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(redo_path)?;

    writeln!(
        file,
        "{}|{}|{}|{}|{}",
        op.timestamp,
        op.action,
        op.prev_head,
        op.new_head,
        encode_desc(&op.description)
    )?;
    Ok(())
}

/// Convenience function to pop the latest operation from `oplog`.
pub fn pop_last_op(repo_root: &Path) -> io::Result<Option<OpEntry>> {
    pop_op(repo_root, "oplog")
}

/// Convenience function to pop the latest undone operation from `redolog`.
pub fn pop_last_redo(repo_root: &Path) -> io::Result<Option<OpEntry>> {
    pop_op(repo_root, "redolog")
}

/// Restores an operation back to `oplog` during a redo action.
pub fn restore_op_to_log(repo_root: &Path, op: &OpEntry) -> io::Result<()> {
    let oplog_path = repo_root.join(".clonaris").join("oplog");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(oplog_path)?;

    writeln!(
        file,
        "{}|{}|{}|{}|{}",
        op.timestamp,
        op.action,
        op.prev_head,
        op.new_head,
        encode_desc(&op.description)
    )?;
    Ok(())
}
