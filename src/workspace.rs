use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Error, ErrorKind};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hash::hash_bytes;
use crate::objects::{
    parse_commit, parse_tree, read_object, serialize_commit, serialize_tree, write_object,
    Commit, TreeEntry,
};
use crate::oplog::append_op;
use crate::repository::{get_author, get_head, set_head};

#[derive(Debug)]
pub struct WorkingDiff {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

impl WorkingDiff {
    pub fn is_clean(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }
}

pub fn load_ignore_patterns(repo_root: &Path) -> Vec<String> {
    let mut patterns = vec![
        ".clonaris".to_string(),
        ".git".to_string(),
        "target".to_string(),
    ];

    let ignore_file = repo_root.join(".clonarisignore");
    if let Ok(content) = fs::read_to_string(ignore_file) {
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                patterns.push(trimmed.trim_end_matches('/').to_string());
            }
        }
    }
    patterns
}

pub fn should_ignore(rel_path: &str, patterns: &[String]) -> bool {
    for pat in patterns {
        if rel_path == pat || rel_path.starts_with(&format!("{}/", pat)) {
            return true;
        }
        // Simple wildcard check (*.log, etc.)
        if pat.starts_with('*') {
            let ext = &pat[1..];
            if rel_path.ends_with(ext) {
                return true;
            }
        }
    }
    false
}

// Recursively walks the repo directory and returns relative paths
pub fn scan_workspace_files(repo_root: &Path) -> io::Result<Vec<String>> {
    let ignore_patterns = load_ignore_patterns(repo_root);
    let mut files = Vec::new();
    walk_dir_inner(repo_root, repo_root, &ignore_patterns, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_dir_inner(
    repo_root: &Path,
    current_dir: &Path,
    ignore_patterns: &[String],
    results: &mut Vec<String>,
) -> io::Result<()> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel_path = path
            .strip_prefix(repo_root)
            .map_err(|e| Error::new(ErrorKind::Other, e))?
            .to_string_lossy()
            .replace('\\', "/");

        if should_ignore(&rel_path, ignore_patterns) {
            continue;
        }

        if path.is_dir() {
            walk_dir_inner(repo_root, &path, ignore_patterns, results)?;
        } else if path.is_file() {
            results.push(rel_path);
        }
    }
    Ok(())
}

pub fn get_head_tree(repo_root: &Path) -> io::Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    if let Some(head_hash) = get_head(repo_root)? {
        let commit_data = read_object(repo_root, &head_hash)?;
        let commit = parse_commit(&commit_data)?;
        let tree_data = read_object(repo_root, &commit.tree)?;
        let tree_entries = parse_tree(&tree_data)?;
        for entry in tree_entries {
            map.insert(entry.path, entry.hash);
        }
    }
    Ok(map)
}

pub fn get_status(repo_root: &Path) -> io::Result<WorkingDiff> {
    let head_tree = get_head_tree(repo_root)?;
    let workspace_files = scan_workspace_files(repo_root)?;
    let mut seen_in_ws = HashSet::new();

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    for rel_path in workspace_files {
        seen_in_ws.insert(rel_path.clone());
        let full_path = repo_root.join(&rel_path);
        let content = fs::read(&full_path)?;
        let current_hash = hash_bytes(&content);

        match head_tree.get(&rel_path) {
            Some(old_hash) => {
                if *old_hash != current_hash {
                    modified.push(rel_path);
                }
            }
            None => {
                added.push(rel_path);
            }
        }
    }

    for (old_path, _) in &head_tree {
        if !seen_in_ws.contains(old_path) {
            deleted.push(old_path.clone());
        }
    }

    added.sort();
    modified.sort();
    deleted.sort();

    Ok(WorkingDiff {
        added,
        modified,
        deleted,
    })
}

pub fn save(repo_root: &Path, message: &str) -> io::Result<Option<String>> {
    let diff = get_status(repo_root)?;
    if diff.is_clean() {
        return Ok(None);
    }

    let files = scan_workspace_files(repo_root)?;
    let mut tree_entries = Vec::new();

    for rel_path in files {
        let full_path = repo_root.join(&rel_path);
        let data = fs::read(&full_path)?;
        let blob_hash = write_object(repo_root, &data)?;

        tree_entries.push(TreeEntry {
            kind: "blob".to_string(),
            hash: blob_hash,
            path: rel_path,
        });
    }

    let tree_bytes = serialize_tree(&tree_entries);
    let tree_hash = write_object(repo_root, &tree_bytes)?;

    let prev_head = get_head(repo_root)?;
    let mut parents = Vec::new();
    if let Some(ref h) = prev_head {
        parents.push(h.clone());
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let commit = Commit {
        tree: tree_hash,
        parents,
        author: get_author(repo_root),
        timestamp: now,
        message: message.to_string(),
    };

    let commit_bytes = serialize_commit(&commit);
    let commit_hash = write_object(repo_root, &commit_bytes)?;

    // Update branch/HEAD
    set_head(repo_root, &commit_hash)?;

    // Log operation
    append_op(
        repo_root,
        "save",
        prev_head.as_deref(),
        Some(&commit_hash),
        message,
    )?;

    Ok(Some(commit_hash))
}

pub fn restore_tree(repo_root: &Path, target_tree_hash: &str) -> io::Result<()> {
    let target_tree_data = read_object(repo_root, target_tree_hash)?;
    let target_tree_entries = parse_tree(&target_tree_data)?;
    let target_paths: HashSet<String> = target_tree_entries.iter().map(|e| e.path.clone()).collect();

    // BUG FIX: Only delete files that were tracked in the previous HEAD commit and are NOT in the target tree!
    // Untracked local files are safely preserved.
    let head_tree = get_head_tree(repo_root)?;
    for (tracked_file, _) in head_tree {
        if !target_paths.contains(&tracked_file) {
            let full_path = repo_root.join(&tracked_file);
            let _ = fs::remove_file(full_path);
        }
    }

    // Write all files from target tree
    for entry in target_tree_entries {
        let blob_data = read_object(repo_root, &entry.hash)?;
        let file_path = repo_root.join(&entry.path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file_path, blob_data)?;
    }

    Ok(())
}

pub fn discard_file(repo_root: &Path, rel_path: &str) -> io::Result<bool> {
    let head_tree = get_head_tree(repo_root)?;
    let full_path = repo_root.join(rel_path);

    match head_tree.get(rel_path) {
        Some(blob_hash) => {
            // File existed in HEAD: restore it to exact snapshot state
            let blob_data = read_object(repo_root, blob_hash)?;
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(full_path, blob_data)?;
            Ok(true)
        }
        None => {
            // File is newly added and not in HEAD: delete it if it exists
            if full_path.exists() {
                fs::remove_file(full_path)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}
