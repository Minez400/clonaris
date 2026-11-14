//! # Lightweight Stash & Shelving Engine
//!
//! Provides fast context switching without committing changes or polluting branch history.
//! Stashes are stored in `.clonaris/stashes/` and safely preserve both modified and untracked files.

use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::objects::{parse_commit, read_object, serialize_tree, TreeEntry, write_object};
use crate::oplog::append_op;
use crate::repository::{get_current_branch, get_head};
use crate::workspace::{get_status, restore_tree, scan_workspace_files};

/// Metadata representation of a saved stash.
#[derive(Debug, Clone)]
pub struct StashEntry {
    /// Zero-based index (0 is newest: `stash@{0}`).
    pub index: usize,
    /// Storage file name on disk.
    pub file_name: String,
    /// Branch name when the stash was recorded.
    pub branch: String,
    /// Unix timestamp in seconds.
    pub timestamp: u64,
    /// Head commit hash when stashed.
    pub head_commit: String,
    /// CAS hash of the stashed workspace Tree.
    pub tree_hash: String,
    /// User description or automatic WIP summary.
    pub message: String,
}

/// Directory path where stashes are stored: `.clonaris/stashes/`
fn stashes_dir(repo_root: &Path) -> std::path::PathBuf {
    repo_root.join(".clonaris").join("stashes")
}

/// Lists all saved stashes sorted newest to oldest.
pub fn list_stashes(repo_root: &Path) -> io::Result<Vec<StashEntry>> {
    let dir = stashes_dir(repo_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("stash") {
            let content = fs::read_to_string(&path)?;
            let mut branch = String::new();
            let mut head_commit = String::new();
            let mut timestamp = 0u64;
            let mut tree_hash = String::new();
            let mut message = String::new();

            for line in content.lines() {
                if let Some(val) = line.strip_prefix("branch: ") {
                    branch = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("head: ") {
                    head_commit = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("timestamp: ") {
                    timestamp = val.trim().parse().unwrap_or(0);
                } else if let Some(val) = line.strip_prefix("tree: ") {
                    tree_hash = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("message: ") {
                    message = val.trim().to_string();
                }
            }

            entries.push(StashEntry {
                index: 0, // assigned after sorting
                file_name: entry.file_name().to_string_lossy().to_string(),
                branch,
                timestamp,
                head_commit,
                tree_hash,
                message,
            });
        }
    }

    // Sort newest first
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.index = i;
    }

    Ok(entries)
}

/// Saves uncommitted changes to a new stash and resets workspace to clean HEAD.
pub fn push_stash(repo_root: &Path, custom_message: Option<&str>) -> Result<String, String> {
    let status = get_status(repo_root).map_err(|e| e.to_string())?;
    if status.is_clean() {
        return Err("No local changes to save to stash.".to_string());
    }

    let head_hash = match get_head(repo_root).map_err(|e| e.to_string())? {
        Some(h) => h,
        None => return Err("Cannot stash changes in a repository with no initial snapshot.".to_string()),
    };

    let branch = get_current_branch(repo_root);
    let head_commit_bytes = read_object(repo_root, &head_hash).map_err(|e| e.to_string())?;
    let head_commit = parse_commit(&head_commit_bytes).map_err(|e| e.to_string())?;

    // Capture entire current workspace state into a Tree object
    let workspace_files = scan_workspace_files(repo_root).map_err(|e| e.to_string())?;
    let mut tree_entries = Vec::new();
    for rel_path in workspace_files {
        let full_path = repo_root.join(&rel_path);
        let content = fs::read(&full_path).map_err(|e| e.to_string())?;
        let blob_hash = write_object(repo_root, &content).map_err(|e| e.to_string())?;
        tree_entries.push(TreeEntry {
            kind: "blob".to_string(),
            hash: blob_hash,
            path: rel_path,
        });
    }

    tree_entries.sort_by(|a, b| a.path.cmp(&b.path));
    let tree_bytes = serialize_tree(&tree_entries);
    let tree_hash = write_object(repo_root, &tree_bytes).map_err(|e| e.to_string())?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let message = match custom_message {
        Some(m) if !m.trim().is_empty() => m.trim().to_string(),
        _ => format!("WIP on {}: {:.7} {}", branch, head_hash, head_commit.message.lines().next().unwrap_or("")),
    };

    let dir = stashes_dir(repo_root);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let stash_filename = format!("stash_{}_{}.stash", now, &tree_hash[..8]);
    let stash_file = dir.join(&stash_filename);

    let content = format!(
        "branch: {}\nhead: {}\ntimestamp: {}\ntree: {}\nmessage: {}\n",
        branch, head_hash, now, tree_hash, message
    );

    fs::write(&stash_file, content).map_err(|e| e.to_string())?;

    // Restore clean HEAD
    restore_tree(repo_root, &head_commit.tree).map_err(|e| e.to_string())?;

    let summary = format!("Saved working directory to stash@{{0}}: {}", message);
    append_op(
        repo_root,
        "stash",
        Some(&head_hash),
        Some(&head_hash),
        &summary,
    )
    .map_err(|e| e.to_string())?;

    Ok(summary)
}

/// Applies a stash entry onto the current working directory without deleting it.
pub fn apply_stash(repo_root: &Path, target_index: usize) -> Result<String, String> {
    let stashes = list_stashes(repo_root).map_err(|e| e.to_string())?;
    let target = stashes
        .into_iter()
        .find(|s| s.index == target_index)
        .ok_or_else(|| format!("Stash stash@{{{}}} does not exist.", target_index))?;

    restore_tree(repo_root, &target.tree_hash).map_err(|e| e.to_string())?;

    let head_hash = get_head(repo_root).ok().flatten();
    append_op(
        repo_root,
        "stash-apply",
        head_hash.as_deref(),
        head_hash.as_deref(),
        &format!("Applied stash@{{{}}}: {}", target_index, target.message),
    )
    .map_err(|e| e.to_string())?;

    Ok(format!("Applied stash@{{{}}}: {}", target_index, target.message))
}

/// Pops a stash entry: applies changes and deletes the stash record.
pub fn pop_stash(repo_root: &Path, target_index: usize) -> Result<String, String> {
    let stashes = list_stashes(repo_root).map_err(|e| e.to_string())?;
    let target = stashes
        .into_iter()
        .find(|s| s.index == target_index)
        .ok_or_else(|| format!("Stash stash@{{{}}} does not exist.", target_index))?;

    // Apply first
    restore_tree(repo_root, &target.tree_hash).map_err(|e| e.to_string())?;

    // Delete stash file
    let file_path = stashes_dir(repo_root).join(&target.file_name);
    let _ = fs::remove_file(file_path);

    let head_hash = get_head(repo_root).ok().flatten();
    append_op(
        repo_root,
        "stash-pop",
        head_hash.as_deref(),
        head_hash.as_deref(),
        &format!("Popped stash@{{{}}}: {}", target_index, target.message),
    )
    .map_err(|e| e.to_string())?;

    Ok(format!("Dropped and applied stash@{{{}}}: {}", target_index, target.message))
}

/// Deletes a specific stash without applying it.
pub fn drop_stash(repo_root: &Path, target_index: usize) -> Result<String, String> {
    let stashes = list_stashes(repo_root).map_err(|e| e.to_string())?;
    let target = stashes
        .into_iter()
        .find(|s| s.index == target_index)
        .ok_or_else(|| format!("Stash stash@{{{}}} does not exist.", target_index))?;

    let file_path = stashes_dir(repo_root).join(&target.file_name);
    fs::remove_file(file_path).map_err(|e| e.to_string())?;

    Ok(format!("Dropped stash@{{{}}}: {}", target_index, target.message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::init;
    use crate::workspace::save;
    use std::fs;

    #[test]
    fn test_stash_flow() {
        let tmp = std::env::temp_dir().join(format!("clonaris_test_stash_{}", std::process::id()));
        init(&tmp).unwrap();

        // 1. Initial snapshot
        fs::write(tmp.join("hello.txt"), "Original content\n").unwrap();
        save(&tmp, "Initial snapshot").unwrap();

        // 2. Modify file
        fs::write(tmp.join("hello.txt"), "Modified content\n").unwrap();
        fs::write(tmp.join("untracked.txt"), "New feature\n").unwrap();

        // 3. Push stash
        let msg = push_stash(&tmp, Some("Test stash")).unwrap();
        assert!(msg.contains("stash@{0}"));

        // Verify workspace was restored to original content
        let content = fs::read_to_string(tmp.join("hello.txt")).unwrap();
        assert_eq!(content, "Original content\n");

        // 4. List stashes
        let stashes = list_stashes(&tmp).unwrap();
        assert_eq!(stashes.len(), 1);
        assert_eq!(stashes[0].index, 0);
        assert_eq!(stashes[0].message, "Test stash");

        // 5. Pop stash
        let pop_res = pop_stash(&tmp, 0).unwrap();
        assert!(pop_res.contains("Dropped and applied"));

        // Verify changes are back
        let restored_content = fs::read_to_string(tmp.join("hello.txt")).unwrap();
        assert_eq!(restored_content, "Modified content\n");
        let untracked_content = fs::read_to_string(tmp.join("untracked.txt")).unwrap();
        assert_eq!(untracked_content, "New feature\n");

        // Stashes should now be empty
        let empty_stashes = list_stashes(&tmp).unwrap();
        assert!(empty_stashes.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }
}
