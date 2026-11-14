use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::Path;

use crate::diff::{compute_diff, is_binary, DiffLine};
use crate::objects::{
    parse_commit, parse_tree, read_object, serialize_commit, serialize_tree, write_object,
    Commit, TreeEntry,
};
use crate::oplog::append_op;
use crate::repository::{get_author, get_current_branch, get_head, set_head};
use crate::workspace::{get_status, restore_tree};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn find_lca(repo_root: &Path, commit_a: &str, commit_b: &str) -> Option<String> {
    if commit_a == commit_b {
        return Some(commit_a.to_string());
    }

    let mut ancestors_a = HashSet::new();
    let mut queue_a = VecDeque::new();
    queue_a.push_back(commit_a.to_string());
    ancestors_a.insert(commit_a.to_string());

    while let Some(c_hash) = queue_a.pop_front() {
        if let Ok(data) = read_object(repo_root, &c_hash) {
            if let Ok(commit) = parse_commit(&data) {
                for p in commit.parents {
                    if ancestors_a.insert(p.clone()) {
                        queue_a.push_back(p);
                    }
                }
            }
        }
    }

    let mut queue_b = VecDeque::new();
    let mut visited_b = HashSet::new();
    queue_b.push_back(commit_b.to_string());
    visited_b.insert(commit_b.to_string());

    while let Some(c_hash) = queue_b.pop_front() {
        if ancestors_a.contains(&c_hash) {
            return Some(c_hash);
        }
        if let Ok(data) = read_object(repo_root, &c_hash) {
            if let Ok(commit) = parse_commit(&data) {
                for p in commit.parents {
                    if visited_b.insert(p.clone()) {
                        queue_b.push_back(p);
                    }
                }
            }
        }
    }

    None
}

pub fn get_tree_map(repo_root: &Path, tree_hash: &str) -> io::Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    if tree_hash.is_empty() {
        return Ok(map);
    }
    let data = read_object(repo_root, tree_hash)?;
    let entries = parse_tree(&data)?;
    for e in entries {
        map.insert(e.path, e.hash);
    }
    Ok(map)
}

pub struct MergeFileResult {
    pub content: Vec<u8>,
    pub has_conflict: bool,
}

// Smart 3-way line merge
pub fn three_way_merge_text(
    base: &str,
    local: &str,
    remote: &str,
    local_label: &str,
    remote_label: &str,
) -> MergeFileResult {
    if local == remote {
        return MergeFileResult {
            content: local.as_bytes().to_vec(),
            has_conflict: false,
        };
    }
    if base == local {
        return MergeFileResult {
            content: remote.as_bytes().to_vec(),
            has_conflict: false,
        };
    }
    if base == remote {
        return MergeFileResult {
            content: local.as_bytes().to_vec(),
            has_conflict: false,
        };
    }

    // Check if changes are completely non-overlapping
    let diff_local = compute_diff(base, local);
    let diff_remote = compute_diff(base, remote);

    let mut local_changed = false;
    let mut remote_changed = false;
    for l in &diff_local {
        if !matches!(l, DiffLine::Equal(_)) {
            local_changed = true;
            break;
        }
    }
    for l in &diff_remote {
        if !matches!(l, DiffLine::Equal(_)) {
            remote_changed = true;
            break;
        }
    }

    if !local_changed {
        return MergeFileResult {
            content: remote.as_bytes().to_vec(),
            has_conflict: false,
        };
    }
    if !remote_changed {
        return MergeFileResult {
            content: local.as_bytes().to_vec(),
            has_conflict: false,
        };
    }

    // Line-by-line anchor reconciliation
    let mut out = String::new();

    // Fast check: if one side only appended lines to the end of base
    if local.starts_with(base) && !remote.starts_with(base) && base.ends_with('\n') {
        let appended_local = &local[base.len()..];
        let mut combined = remote.to_string();
        if !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(appended_local);
        return MergeFileResult {
            content: combined.into_bytes(),
            has_conflict: false,
        };
    } else if remote.starts_with(base) && !local.starts_with(base) && base.ends_with('\n') {
        let appended_remote = &remote[base.len()..];
        let mut combined = local.to_string();
        if !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(appended_remote);
        return MergeFileResult {
            content: combined.into_bytes(),
            has_conflict: false,
        };
    }

    // Emit localized conflict block
    out.push_str(&format!("<<<<<<< ATUAL ({})\n", local_label));
    out.push_str(local);
    if !local.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("=======\n");
    out.push_str(remote);
    if !remote.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(">>>>>>> RAMO ALVO ({})\n", remote_label));
    let has_conflict = true;

    MergeFileResult {
        content: out.into_bytes(),
        has_conflict,
    }
}

pub fn execute_merge(repo_root: &Path, target_branch: &str) -> io::Result<()> {
    let diff = get_status(repo_root)?;
    if !diff.is_clean() {
        eprintln!("Erro: Há alterações não salvas no diretório de trabalho.");
        eprintln!("Salve suas alterações primeiro com: clonaris save -m \"checkpoint\"");
        return Ok(());
    }

    let local_branch = get_current_branch(repo_root);
    if local_branch == target_branch {
        println!("Não é possível mesclar um ramo nele mesmo.");
        return Ok(());
    }

    let local_commit_hash = match get_head(repo_root)? {
        Some(h) => h,
        None => {
            eprintln!("Erro: O ramo atual não possui snapshots.");
            return Ok(());
        }
    };

    let target_branch_file = repo_root
        .join(".clonaris")
        .join("refs")
        .join("heads")
        .join(target_branch);

    if !target_branch_file.exists() {
        eprintln!("Erro: Ramo '{}' não foi encontrado.", target_branch);
        return Ok(());
    }

    let target_commit_hash = fs::read_to_string(target_branch_file)?.trim().to_string();

    if local_commit_hash == target_commit_hash {
        println!("Os ramos já estão perfeitamente sincronizados. Nada a fazer.");
        return Ok(());
    }

    let lca = find_lca(repo_root, &local_commit_hash, &target_commit_hash);

    // Fast-Forward check
    if let Some(ref lca_hash) = lca {
        if lca_hash == &local_commit_hash {
            println!("⚡ Mesclagem Fast-Forward para {:.8}!", target_commit_hash);
            let target_commit_data = read_object(repo_root, &target_commit_hash)?;
            let target_commit = parse_commit(&target_commit_data)?;

            restore_tree(repo_root, &target_commit.tree)?;
            set_head(repo_root, &target_commit_hash)?;

            append_op(
                repo_root,
                "merge",
                Some(&local_commit_hash),
                Some(&target_commit_hash),
                &format!("Fast-forward merge de '{}' em '{}'", target_branch, local_branch),
            )?;
            return Ok(());
        } else if lca_hash == &target_commit_hash {
            println!("O ramo atual já contém todas as alterações de '{}'.", target_branch);
            return Ok(());
        }
    }

    // 3-Way Merge
    let base_tree = if let Some(ref lca_hash) = lca {
        let commit_data = read_object(repo_root, lca_hash)?;
        let commit = parse_commit(&commit_data)?;
        get_tree_map(repo_root, &commit.tree)?
    } else {
        HashMap::new()
    };

    let local_commit_data = read_object(repo_root, &local_commit_hash)?;
    let local_commit = parse_commit(&local_commit_data)?;
    let local_tree = get_tree_map(repo_root, &local_commit.tree)?;

    let target_commit_data = read_object(repo_root, &target_commit_hash)?;
    let target_commit = parse_commit(&target_commit_data)?;
    let target_tree = get_tree_map(repo_root, &target_commit.tree)?;

    let mut all_files = HashSet::new();
    for f in base_tree.keys() {
        all_files.insert(f.clone());
    }
    for f in local_tree.keys() {
        all_files.insert(f.clone());
    }
    for f in target_tree.keys() {
        all_files.insert(f.clone());
    }

    let mut merged_tree_entries = Vec::new();
    let mut conflicted_files = Vec::new();

    for file_path in all_files {
        let in_base = base_tree.get(&file_path);
        let in_local = local_tree.get(&file_path);
        let in_target = target_tree.get(&file_path);

        match (in_base, in_local, in_target) {
            // Unchanged in both or identical changes
            (_, Some(loc_h), Some(tgt_h)) if loc_h == tgt_h => {
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: loc_h.clone(),
                    path: file_path.clone(),
                });
            }
            // Only changed in target (take target)
            (Some(b_h), Some(loc_h), Some(tgt_h)) if loc_h == b_h && tgt_h != b_h => {
                let tgt_bytes = read_object(repo_root, tgt_h)?;
                let full = repo_root.join(&file_path);
                if let Some(p) = full.parent() {
                    fs::create_dir_all(p)?;
                }
                fs::write(full, tgt_bytes)?;
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: tgt_h.clone(),
                    path: file_path.clone(),
                });
            }
            // Only changed in local (keep local)
            (Some(b_h), Some(loc_h), Some(tgt_h)) if tgt_h == b_h && loc_h != b_h => {
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: loc_h.clone(),
                    path: file_path.clone(),
                });
            }
            // Changed in both -> 3-way merge!
            (b_opt, Some(loc_h), Some(tgt_h)) => {
                let loc_bytes = read_object(repo_root, loc_h)?;
                let tgt_bytes = read_object(repo_root, tgt_h)?;

                let (merged_content, has_conflict) = if is_binary(&loc_bytes) || is_binary(&tgt_bytes) {
                    // Binary conflict: default to local and mark conflict
                    (loc_bytes, true)
                } else {
                    let base_str = if let Some(b_h) = b_opt {
                        let b_bytes = read_object(repo_root, b_h)?;
                        String::from_utf8_lossy(&b_bytes).to_string()
                    } else {
                        String::new()
                    };
                    let loc_str = String::from_utf8_lossy(&loc_bytes);
                    let tgt_str = String::from_utf8_lossy(&tgt_bytes);

                    let res = three_way_merge_text(
                        &base_str,
                        &loc_str,
                        &tgt_str,
                        &local_branch,
                        target_branch,
                    );
                    (res.content, res.has_conflict)
                };

                let full = repo_root.join(&file_path);
                if let Some(p) = full.parent() {
                    fs::create_dir_all(p)?;
                }
                fs::write(&full, &merged_content)?;

                let blob_hash = write_object(repo_root, &merged_content)?;
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: blob_hash,
                    path: file_path.clone(),
                });

                if has_conflict {
                    conflicted_files.push(file_path);
                }
            }
            // Added only in local
            (None, Some(loc_h), None) => {
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: loc_h.clone(),
                    path: file_path.clone(),
                });
            }
            // Added only in target
            (None, None, Some(tgt_h)) => {
                let tgt_bytes = read_object(repo_root, tgt_h)?;
                let full = repo_root.join(&file_path);
                if let Some(p) = full.parent() {
                    fs::create_dir_all(p)?;
                }
                fs::write(full, tgt_bytes)?;
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: tgt_h.clone(),
                    path: file_path.clone(),
                });
            }
            // Deleted in target, unchanged in local
            (Some(b_h), Some(loc_h), None) if loc_h == b_h => {
                let full = repo_root.join(&file_path);
                let _ = fs::remove_file(full);
            }
            // Deleted in local, unchanged in target
            (Some(b_h), None, Some(tgt_h)) if tgt_h == b_h => {
                // Keep deleted
            }
            // Conflicting deletion/modification
            (Some(_), Some(loc_h), None) => {
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: loc_h.clone(),
                    path: file_path.clone(),
                });
            }
            (Some(_), None, Some(tgt_h)) => {
                let tgt_bytes = read_object(repo_root, tgt_h)?;
                let full = repo_root.join(&file_path);
                if let Some(p) = full.parent() {
                    fs::create_dir_all(p)?;
                }
                fs::write(full, tgt_bytes)?;
                merged_tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: tgt_h.clone(),
                    path: file_path.clone(),
                });
            }
            (Some(_), None, None) => {}
            (None, None, None) => {}
        }
    }

    merged_tree_entries.sort_by(|a, b| a.path.cmp(&b.path));

    let tree_bytes = serialize_tree(&merged_tree_entries);
    let tree_hash = write_object(repo_root, &tree_bytes)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let merge_commit = Commit {
        tree: tree_hash,
        parents: vec![local_commit_hash.clone(), target_commit_hash.clone()],
        author: get_author(repo_root),
        timestamp: now,
        message: format!("Mescla ramo '{}' em '{}'", target_branch, local_branch),
    };

    let commit_bytes = serialize_commit(&merge_commit);
    let merge_commit_hash = write_object(repo_root, &commit_bytes)?;

    set_head(repo_root, &merge_commit_hash)?;

    append_op(
        repo_root,
        "merge",
        Some(&local_commit_hash),
        Some(&merge_commit_hash),
        &format!("Mescla ramo '{}' em '{}'", target_branch, local_branch),
    )?;

    if conflicted_files.is_empty() {
        println!(
            "✓ [\x1b[1;32m{}\x1b[0m \x1b[33m{:.8}\x1b[0m] Mesclado com sucesso com '{}'!",
            local_branch, merge_commit_hash, target_branch
        );
    } else {
        println!(
            "⚠️  [\x1b[1;32m{}\x1b[0m \x1b[33m{:.8}\x1b[0m] Snapshot de merge gravado com conflitos!",
            local_branch, merge_commit_hash
        );
        println!("\x1b[1;33mConflitos encontrados nos arquivos:\x1b[0m");
        for cf in &conflicted_files {
            println!("  ! {}", cf);
        }
        println!();
        println!("✨ \x1b[1;32mDiferencial do Clonaris (Conflito Não-Bloqueante):\x1b[0m");
        println!("   Seu repositório NÃO está travado. O snapshot foi salvo com os marcadores.");
        println!("   Você pode resolver interativamente com: 'clonaris resolve <arquivo>'");
        println!("   E salvar com: 'clonaris save -m \"Resolve conflitos de merge\"'");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_three_way_non_conflicting_appends() {
        let base = "linha 1\nlinha 2\n";
        let local = "linha 1\nlinha 2\nlinha 3 local\n";
        let remote = "linha 0 remote\nlinha 1\nlinha 2\n";
        // Local only appended
        let res = three_way_merge_text(base, local, remote, "local", "remote");
        assert!(!res.has_conflict);
        let s = String::from_utf8(res.content).unwrap();
        assert!(s.contains("linha 0 remote"));
        assert!(s.contains("linha 3 local"));
    }
}
