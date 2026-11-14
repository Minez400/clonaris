//! # Cherry-Pick Engine
//!
//! Applies the delta from an arbitrary commit onto the current active branch using
//! Clonaris's 3-way merge engine. In case of conflicts, conflict markers are inserted
//! safely without locking the repository.

use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::diff::is_binary;
use crate::merge::{get_tree_map, three_way_merge_text};
use crate::objects::{
    parse_commit, read_object, serialize_commit, serialize_tree, write_object,
    Commit, TreeEntry,
};
use crate::oplog::append_op;
use crate::repository::{get_author, get_current_branch, get_head, set_head};
use crate::workspace::{get_status, restore_tree};

/// Cherry-picks a specified commit onto the current HEAD.
pub fn cherry_pick(repo_root: &Path, commit_hash: &str) -> Result<String, String> {
    // 1. Check workspace hygiene
    let status = get_status(repo_root).map_err(|e| e.to_string())?;
    if !status.is_clean() {
        return Err("Cannot cherry-pick with uncommitted local changes. Please save or stash them first.".to_string());
    }

    // 2. Resolve current HEAD
    let head_hash = match get_head(repo_root).map_err(|e| e.to_string())? {
        Some(h) => h,
        None => return Err("Cannot cherry-pick onto an empty repository.".to_string()),
    };

    if head_hash == commit_hash {
        return Err("Target commit is already the current HEAD.".to_string());
    }

    // 3. Read target commit
    let target_data = read_object(repo_root, commit_hash)
        .map_err(|_| format!("Commit '{}' not found in repository.", commit_hash))?;
    let target_commit = parse_commit(&target_data)
        .map_err(|e| format!("Failed to parse target commit: {}", e))?;

    // 4. Base is target's parent (or empty if root commit)
    let base_tree_map = if let Some(parent_hash) = target_commit.parents.first() {
        let parent_data = read_object(repo_root, parent_hash)
            .map_err(|e| format!("Failed to read parent commit {}: {}", parent_hash, e))?;
        let parent_commit = parse_commit(&parent_data)
            .map_err(|e| format!("Failed to parse parent commit: {}", e))?;
        get_tree_map(repo_root, &parent_commit.tree).map_err(|e| e.to_string())?
    } else {
        std::collections::HashMap::new()
    };

    // 5. Read current HEAD tree
    let head_data = read_object(repo_root, &head_hash).map_err(|e| e.to_string())?;
    let head_commit = parse_commit(&head_data).map_err(|e| e.to_string())?;
    let head_tree_map = get_tree_map(repo_root, &head_commit.tree).map_err(|e| e.to_string())?;

    // 6. Read target commit tree
    let target_tree_map = get_tree_map(repo_root, &target_commit.tree).map_err(|e| e.to_string())?;

    // 7. Collect all unique file paths across base, local, and target
    let mut all_paths = HashSet::new();
    for p in base_tree_map.keys() {
        all_paths.insert(p.clone());
    }
    for p in head_tree_map.keys() {
        all_paths.insert(p.clone());
    }
    for p in target_tree_map.keys() {
        all_paths.insert(p.clone());
    }

    let mut merged_entries = Vec::new();
    let mut conflicted_files = Vec::new();

    for path in all_paths {
        let in_base = base_tree_map.get(&path);
        let in_head = head_tree_map.get(&path);
        let in_target = target_tree_map.get(&path);

        // Case A: File deleted in target commit
        if in_base.is_some() && in_target.is_none() {
            if in_head == in_base {
                // Safely delete in merged result
                continue;
            } else if in_head.is_some() {
                // Conflict: modified locally but deleted in cherry-picked commit
                conflicted_files.push(path.clone());
                // Keep local version
                merged_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: in_head.unwrap().clone(),
                    path: path.clone(),
                });
                continue;
            }
        }

        // Case B: File added in target commit
        if in_base.is_none() && in_target.is_some() {
            if in_head.is_none() {
                // Simply take the file from target
                merged_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: in_target.unwrap().clone(),
                    path: path.clone(),
                });
                continue;
            }
        }

        // Case C: File unchanged between base and target (delta didn't touch it)
        if in_base == in_target {
            if let Some(h) = in_head {
                merged_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: h.clone(),
                    path: path.clone(),
                });
            }
            continue;
        }

        // Case D: File modified in target commit - perform 3-way merge
        let base_bytes = match in_base {
            Some(h) => read_object(repo_root, h).unwrap_or_default(),
            None => Vec::new(),
        };
        let head_bytes = match in_head {
            Some(h) => read_object(repo_root, h).unwrap_or_default(),
            None => Vec::new(),
        };
        let target_bytes = match in_target {
            Some(h) => read_object(repo_root, h).unwrap_or_default(),
            None => Vec::new(),
        };

        if is_binary(&base_bytes) || is_binary(&head_bytes) || is_binary(&target_bytes) {
            // Binary conflict
            if in_head != in_target {
                conflicted_files.push(path.clone());
            }
            if let Some(h) = in_head.or(in_target) {
                merged_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: h.clone(),
                    path: path.clone(),
                });
            }
            continue;
        }

        let base_str = String::from_utf8_lossy(&base_bytes);
        let head_str = String::from_utf8_lossy(&head_bytes);
        let target_str = String::from_utf8_lossy(&target_bytes);

        let branch = get_current_branch(repo_root);
        let merge_res = three_way_merge_text(
            &base_str,
            &head_str,
            &target_str,
            &branch,
            &format!("cherry-pick {:.7}", commit_hash),
        );

        if merge_res.has_conflict {
            conflicted_files.push(path.clone());
        }

        let new_blob_hash = write_object(repo_root, &merge_res.content).map_err(|e| e.to_string())?;
        merged_entries.push(TreeEntry {
            kind: "blob".to_string(),
            hash: new_blob_hash,
            path: path.clone(),
        });
    }

    // 8. Write merged tree
    merged_entries.sort_by(|a, b| a.path.cmp(&b.path));
    let tree_bytes = serialize_tree(&merged_entries);
    let tree_hash = write_object(repo_root, &tree_bytes).map_err(|e| e.to_string())?;

    // 9. Create cherry-pick commit
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let cherry_msg = format!(
        "{}\n\n(cherry picked from commit {})",
        target_commit.message.trim(),
        commit_hash
    );

    let new_commit = Commit {
        tree: tree_hash.clone(),
        parents: vec![head_hash.clone()],
        author: get_author(repo_root),
        timestamp: now,
        message: cherry_msg,
    };

    let new_commit_bytes = serialize_commit(&new_commit);
    let new_commit_hash = write_object(repo_root, &new_commit_bytes).map_err(|e| e.to_string())?;

    // 10. Update working directory and HEAD
    restore_tree(repo_root, &tree_hash).map_err(|e| e.to_string())?;
    set_head(repo_root, &new_commit_hash).map_err(|e| e.to_string())?;

    append_op(
        repo_root,
        "cherry-pick",
        Some(&head_hash),
        Some(&new_commit_hash),
        &format!("Cherry-picked commit {:.7}", commit_hash),
    )
    .map_err(|e| e.to_string())?;

    if !conflicted_files.is_empty() {
        let list = conflicted_files.join("\n  * ");
        Ok(format!(
            "⚠️  Cherry-pick completed with CONFLICTS in {} file(s):\n  * {}\n\nConflict markers (<<<<<<< CURRENT / >>>>>>> TARGET) were written to disk.\nResolve interactively with 'clonaris resolve <file>'.",
            conflicted_files.len(),
            list
        ))
    } else {
        Ok(format!(
            "✅ Cherry-picked commit {:.7} successfully into [{:.7}].",
            commit_hash, new_commit_hash
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{get_head, init, set_head};
    use crate::workspace::save;
    use std::fs;

    #[test]
    fn test_cherry_pick_clean() {
        let tmp = std::env::temp_dir().join(format!("clonaris_test_cherry_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        init(&tmp).unwrap();

        // 1. Initial commit on main
        fs::write(tmp.join("base.txt"), "Base\n").unwrap();
        let base_commit = save(&tmp, "Base commit").unwrap().unwrap();

        // 2. Commit on feature branch
        // Point HEAD to a feature branch
        fs::write(tmp.join(".clonaris").join("HEAD"), "ref: refs/heads/feature\n").unwrap();
        set_head(&tmp, &base_commit).unwrap();

        fs::write(tmp.join("feature.txt"), "Feature work\n").unwrap();
        let feature_commit = save(&tmp, "Feature commit").unwrap().unwrap();

        // 3. Switch back to main (restore tree first while still on feature, then switch HEAD)
        let base_commit_bytes = read_object(&tmp, &base_commit).unwrap();
        let base_c = parse_commit(&base_commit_bytes).unwrap();
        restore_tree(&tmp, &base_c.tree).unwrap();
        fs::write(tmp.join(".clonaris").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert!(!tmp.join("feature.txt").exists());

        // 4. Cherry-pick feature commit onto main
        let res = cherry_pick(&tmp, &feature_commit).unwrap();
        assert!(res.contains("Cherry-picked commit"));

        // Verify feature.txt now exists on main
        assert!(tmp.join("feature.txt").exists());
        let content = fs::read_to_string(tmp.join("feature.txt")).unwrap();
        assert_eq!(content, "Feature work\n");

        // Verify main has new commit
        let main_head = get_head(&tmp).unwrap().unwrap();
        assert_ne!(main_head, base_commit);

        let _ = fs::remove_dir_all(&tmp);
    }
}
