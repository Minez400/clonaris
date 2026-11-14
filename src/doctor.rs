//! # Repository Doctor & Garbage Collection (GC) Engine
//!
//! Provides deep integrity verification for CAS objects and disk space reclamation
//! through generational reachability garbage collection.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::hash::hash_bytes;
use crate::objects::{parse_commit, parse_tree, read_object};
use crate::repository::{get_head, list_tags};
use crate::stash::list_stashes;

/// Summary statistics reported by `clonaris doctor`.
pub struct DoctorReport {
    pub total_objects: usize,
    pub verified_objects: usize,
    pub corrupted_objects: Vec<String>,
    pub valid_branches: usize,
    pub valid_tags: usize,
    pub total_bytes: u64,
}

/// Runs a complete health and integrity audit on the repository.
pub fn run_doctor(repo_root: &Path) -> Result<DoctorReport, String> {
    let objects_dir = repo_root.join(".clonaris").join("objects");
    let mut total_objects = 0;
    let mut verified_objects = 0;
    let mut corrupted_objects = Vec::new();
    let mut total_bytes = 0u64;

    if objects_dir.exists() {
        for entry in fs::read_dir(&objects_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                for sub_entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
                    let sub_entry = sub_entry.map_err(|e| e.to_string())?;
                    let sub_path = sub_entry.path();
                    if sub_path.is_file() {
                        total_objects += 1;
                        let file_name = sub_entry.file_name().to_string_lossy().to_string();
                        let expected_hash = format!("{}{}", dir_name, file_name);

                        match fs::read(&sub_path) {
                            Ok(data) => {
                                total_bytes += data.len() as u64;
                                let actual_hash = hash_bytes(&data);
                                if actual_hash == expected_hash {
                                    verified_objects += 1;
                                } else {
                                    corrupted_objects.push(expected_hash);
                                }
                            }
                            Err(_) => {
                                corrupted_objects.push(expected_hash);
                            }
                        }
                    }
                }
            }
        }
    }

    // Verify HEAD
    if let Ok(Some(head_hash)) = get_head(repo_root) {
        if read_object(repo_root, &head_hash).is_err() {
            corrupted_objects.push(format!("HEAD pointer ({}) missing object", head_hash));
        }
    }

    // Verify branches
    let heads_dir = repo_root.join(".clonaris").join("refs").join("heads");
    let mut valid_branches = 0;
    if heads_dir.exists() {
        let branch_files = collect_files_recursive(&heads_dir);
        for f in branch_files {
            if let Ok(content) = fs::read_to_string(&f) {
                let target = content.trim();
                if !target.is_empty() {
                    if read_object(repo_root, target).is_ok() {
                        valid_branches += 1;
                    } else {
                        corrupted_objects.push(format!("Branch ref {} points to missing object {}", f.display(), target));
                    }
                }
            }
        }
    }

    // Verify tags
    let tags = list_tags(repo_root).map_err(|e| e.to_string())?;
    let mut valid_tags = 0;
    for (name, hash) in tags {
        if read_object(repo_root, &hash).is_ok() {
            valid_tags += 1;
        } else {
            corrupted_objects.push(format!("Tag '{}' points to missing object {}", name, hash));
        }
    }

    Ok(DoctorReport {
        total_objects,
        verified_objects,
        corrupted_objects,
        valid_branches,
        valid_tags,
        total_bytes,
    })
}

/// Statistics reported by `clonaris gc`.
pub struct GcReport {
    pub reachable_objects: usize,
    pub pruned_objects: usize,
    pub reclaimed_bytes: u64,
}

/// Prunes orphaned/unreachable CAS objects that are no longer referenced by any branch,
/// tag, HEAD, or stash.
pub fn run_gc(repo_root: &Path) -> Result<GcReport, String> {
    let mut reachable = HashSet::new();

    // 1. Traverse from HEAD
    if let Ok(Some(head_hash)) = get_head(repo_root) {
        mark_commit_reachable(repo_root, &head_hash, &mut reachable);
    }

    // 2. Traverse from all branches
    let heads_dir = repo_root.join(".clonaris").join("refs").join("heads");
    if heads_dir.exists() {
        for f in collect_files_recursive(&heads_dir) {
            if let Ok(content) = fs::read_to_string(&f) {
                let hash = content.trim();
                if !hash.is_empty() {
                    mark_commit_reachable(repo_root, hash, &mut reachable);
                }
            }
        }
    }

    // 3. Traverse from all tags
    if let Ok(tags) = list_tags(repo_root) {
        for (_, hash) in tags {
            mark_commit_reachable(repo_root, &hash, &mut reachable);
        }
    }

    // 4. Traverse from all stashes
    if let Ok(stashes) = list_stashes(repo_root) {
        for s in stashes {
            mark_tree_reachable(repo_root, &s.tree_hash, &mut reachable);
            mark_commit_reachable(repo_root, &s.head_commit, &mut reachable);
        }
    }

    // 5. Scan all disk objects and delete unreachable ones
    let objects_dir = repo_root.join(".clonaris").join("objects");
    let mut pruned_objects = 0;
    let mut reclaimed_bytes = 0u64;

    if objects_dir.exists() {
        for entry in fs::read_dir(&objects_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                for sub in fs::read_dir(&path).map_err(|e| e.to_string())? {
                    let sub = sub.map_err(|e| e.to_string())?;
                    let sub_path = sub.path();
                    if sub_path.is_file() {
                        let file_name = sub.file_name().to_string_lossy().to_string();
                        let full_hash = format!("{}{}", dir_name, file_name);

                        if !reachable.contains(&full_hash) {
                            if let Ok(meta) = fs::metadata(&sub_path) {
                                reclaimed_bytes += meta.len();
                            }
                            let _ = fs::remove_file(&sub_path);
                            pruned_objects += 1;
                        }
                    }
                }

                // Remove directory if empty
                let _ = fs::remove_dir(&path);
            }
        }
    }

    Ok(GcReport {
        reachable_objects: reachable.len(),
        pruned_objects,
        reclaimed_bytes,
    })
}

/// Recursively marks a commit and all its referenced trees, blobs, and parents as reachable.
fn mark_commit_reachable(repo_root: &Path, commit_hash: &str, reachable: &mut HashSet<String>) {
    if !reachable.insert(commit_hash.to_string()) {
        return;
    }

    if let Ok(data) = read_object(repo_root, commit_hash) {
        if let Ok(commit) = parse_commit(&data) {
            mark_tree_reachable(repo_root, &commit.tree, reachable);
            for parent in commit.parents {
                mark_commit_reachable(repo_root, &parent, reachable);
            }
        }
    }
}

/// Recursively marks a tree and all its blobs as reachable.
fn mark_tree_reachable(repo_root: &Path, tree_hash: &str, reachable: &mut HashSet<String>) {
    if !reachable.insert(tree_hash.to_string()) {
        return;
    }

    if let Ok(data) = read_object(repo_root, tree_hash) {
        if let Ok(entries) = parse_tree(&data) {
            for entry in entries {
                reachable.insert(entry.hash);
            }
        }
    }
}

/// Helper to recursively gather file paths inside a directory.
fn collect_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_files_recursive(&path));
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::init;
    use crate::workspace::save;

    #[test]
    fn test_doctor_and_gc() {
        let tmp = std::env::temp_dir().join(format!("clonaris_test_doctor_{}", std::process::id()));
        init(&tmp).unwrap();

        fs::write(tmp.join("file1.txt"), "Version 1\n").unwrap();
        save(&tmp, "Commit 1").unwrap();

        fs::write(tmp.join("file1.txt"), "Version 2\n").unwrap();
        save(&tmp, "Commit 2").unwrap();

        // Doctor audit
        let doc = run_doctor(&tmp).unwrap();
        assert!(doc.corrupted_objects.is_empty());
        assert!(doc.verified_objects > 0);
        assert_eq!(doc.valid_branches, 1);

        // GC audit
        let gc = run_gc(&tmp).unwrap();
        assert!(gc.reachable_objects > 0);

        let _ = fs::remove_dir_all(&tmp);
    }
}
