//! # CLI Commands Implementation
//!
//! Handlers for all user-facing Clonaris CLI subcommands:
//! `init`, `status`, `diff`, `save`, `amend`, `discard`, `resolve`, `log`, `ops`,
//! `undo`, `redo`, `branch`, `switch`, `merge`, `tag`, and `completion`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cherry_pick;
use crate::diff::{compute_diff, format_colored_diff, is_binary};
use crate::doctor;
use crate::git_bridge;
use crate::merge::execute_merge;
use crate::objects::{
    parse_commit, parse_tree, read_object, serialize_commit, serialize_tree, write_object,
    Commit, TreeEntry,
};
use crate::oplog::{
    append_op, pop_last_op, pop_last_redo, push_redo, read_ops, restore_op_to_log,
};
use crate::remote;
use crate::repository::{
    create_tag, find_repo_root, get_author, get_current_branch, get_head, init, list_tags,
    set_head,
};
use crate::stash;
use crate::workspace::{
    discard_file, get_head_tree, get_status, restore_tree, save, scan_workspace_files,
};

/// Command: `clonaris init`
/// Initializes a new `.clonaris/` repository in the current directory.
pub fn cmd_init() -> io::Result<()> {
    let current = env::current_dir()?;
    init(&current)?;
    println!("✓ Initialized empty Clonaris repository in {}", current.display());
    println!("  Default branch: main");
    println!("  Tip: Create files and run 'clonaris save -m \"your message\"'");
    Ok(())
}

/// Command: `clonaris status`
/// Displays the current active branch, latest snapshot, and modified/added/deleted files.
pub fn cmd_status() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    let branch = get_current_branch(&root);
    let head = get_head(&root)?;
    let diff = get_status(&root)?;

    println!("On branch: \x1b[1;32m{}\x1b[0m", branch);
    match head {
        Some(h) => println!("Latest snapshot: \x1b[33m{:.8}\x1b[0m", h),
        None => println!("No snapshots recorded yet."),
    }
    println!();

    if diff.is_clean() {
        println!("✓ Working tree clean. Nothing to commit.");
        return Ok(());
    }

    if !diff.added.is_empty() {
        println!("\x1b[1;32mNew files:\x1b[0m");
        for file in &diff.added {
            println!("  + {}", file);
        }
    }

    if !diff.modified.is_empty() {
        println!("\x1b[1;34mModified files:\x1b[0m");
        for file in &diff.modified {
            println!("  ~ {}", file);
        }
    }

    if !diff.deleted.is_empty() {
        println!("\x1b[1;31mDeleted files:\x1b[0m");
        for file in &diff.deleted {
            println!("  - {}", file);
        }
    }

    println!();
    println!("Run 'clonaris diff' to inspect line-by-line changes.");
    println!("Run 'clonaris save -m \"message\"' to commit changes directly.");
    Ok(())
}

/// Command: `clonaris diff [file]`
/// Shows line-by-line color diffs against the latest snapshot.
pub fn cmd_diff(target_file: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let diff = get_status(&root)?;
    let head_tree = get_head_tree(&root)?;

    if diff.is_clean() {
        println!("Working tree clean. No changes to display.");
        return Ok(());
    }

    let mut printed_anything = false;

    // Modified files
    for file in &diff.modified {
        if let Some(target) = target_file {
            if file != target {
                continue;
            }
        }

        printed_anything = true;
        let old_hash = head_tree.get(file).unwrap();
        let old_bytes = read_object(&root, old_hash)?;
        let new_bytes = fs::read(root.join(file))?;

        if is_binary(&old_bytes) || is_binary(&new_bytes) {
            println!("\x1b[1;33mBinary file {} differs from previous snapshot\x1b[0m", file);
            continue;
        }

        let old_text = String::from_utf8_lossy(&old_bytes);
        let new_text = String::from_utf8_lossy(&new_bytes);

        let lines = compute_diff(&old_text, &new_text);
        print!("{}", format_colored_diff(file, &lines));
    }

    // Added files
    for file in &diff.added {
        if let Some(target) = target_file {
            if file != target {
                continue;
            }
        }

        printed_anything = true;
        let new_bytes = fs::read(root.join(file))?;
        if is_binary(&new_bytes) {
            println!("\x1b[1;32mNew binary file added: {}\x1b[0m", file);
            continue;
        }

        let new_text = String::from_utf8_lossy(&new_bytes);
        let lines = compute_diff("", &new_text);
        print!("{}", format_colored_diff(file, &lines));
    }

    // Deleted files
    for file in &diff.deleted {
        if let Some(target) = target_file {
            if file != target {
                continue;
            }
        }

        printed_anything = true;
        let old_hash = head_tree.get(file).unwrap();
        let old_bytes = read_object(&root, old_hash)?;
        if is_binary(&old_bytes) {
            println!("\x1b[1;31mBinary file deleted: {}\x1b[0m", file);
            continue;
        }

        let old_text = String::from_utf8_lossy(&old_bytes);
        let lines = compute_diff(&old_text, "");
        print!("{}", format_colored_diff(file, &lines));
    }

    if !printed_anything && target_file.is_some() {
        println!("File '{}' has no uncommitted changes.", target_file.unwrap());
    }

    Ok(())
}

/// Command: `clonaris discard <file | .>`
/// Discards uncommitted modifications in a specific file or the entire working tree.
pub fn cmd_discard(target: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    if target == "." {
        let diff = get_status(&root)?;
        if diff.is_clean() {
            println!("Nothing to discard (working tree clean).");
            return Ok(());
        }
        for file in diff.added.iter().chain(diff.modified.iter()).chain(diff.deleted.iter()) {
            discard_file(&root, file)?;
        }
        println!("✓ All local modifications have been discarded.");
    } else {
        if discard_file(&root, target)? {
            println!("✓ Changes in '{}' discarded successfully.", target);
        } else {
            println!("No modifications found to discard in '{}'.", target);
        }
    }
    Ok(())
}

/// Command: `clonaris resolve <file> [--current | --target]`
/// Interactively or automatically resolves merge conflict markers.
pub fn cmd_resolve(file: &str, strategy: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };
    crate::resolve::prompt_resolve_file(&root.join(file), strategy)
}

/// Command: `clonaris completion [bash | zsh]`
/// Outputs shell completion script to stdout.
pub fn cmd_completion(shell: Option<&str>) -> io::Result<()> {
    crate::completion::generate_completion(shell.unwrap_or("bash"));
    Ok(())
}

/// Command: `clonaris save -m "message"`
/// Commits the entire working copy state directly into a new immutable snapshot.
pub fn cmd_save(message: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let diff = get_status(&root)?;
    if diff.is_clean() {
        println!("Nothing to save (working tree clean).");
        return Ok(());
    }

    let total_changes = diff.added.len() + diff.modified.len() + diff.deleted.len();
    match save(&root, message)? {
        Some(commit_hash) => {
            let branch = get_current_branch(&root);
            println!(
                "✓ [\x1b[1;32m{}\x1b[0m \x1b[33m{:.8}\x1b[0m] {}",
                branch, commit_hash, message
            );
            println!(
                "  {} change(s): +{} added, ~{} modified, -{} deleted",
                total_changes,
                diff.added.len(),
                diff.modified.len(),
                diff.deleted.len()
            );
        }
        None => {
            println!("Nothing to save.");
        }
    }
    Ok(())
}

/// Command: `clonaris amend -m "message"`
/// Updates the latest commit with the new message and current files, preserving undo capability.
pub fn cmd_amend(message: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let head_hash = match get_head(&root)? {
        Some(h) => h,
        None => {
            eprintln!("Error: No previous commit found to amend.");
            std::process::exit(1);
        }
    };

    let old_commit_data = read_object(&root, &head_hash)?;
    let old_commit = parse_commit(&old_commit_data)?;

    let files = scan_workspace_files(&root)?;
    let mut tree_entries = Vec::new();
    for rel_path in files {
        let full_path = root.join(&rel_path);
        let data = fs::read(&full_path)?;
        let blob_hash = write_object(&root, &data)?;
        tree_entries.push(TreeEntry {
            kind: "blob".to_string(),
            hash: blob_hash,
            path: rel_path,
        });
    }

    let tree_bytes = serialize_tree(&tree_entries);
    let tree_hash = write_object(&root, &tree_bytes)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let new_commit = Commit {
        tree: tree_hash,
        parents: old_commit.parents,
        author: get_author(&root),
        timestamp: now,
        message: message.to_string(),
    };

    let commit_bytes = serialize_commit(&new_commit);
    let new_commit_hash = write_object(&root, &commit_bytes)?;

    set_head(&root, &new_commit_hash)?;

    append_op(
        &root,
        "amend",
        Some(&head_hash),
        Some(&new_commit_hash),
        &format!("Amend: {}", message),
    )?;

    let branch = get_current_branch(&root);
    println!(
        "✓ [\x1b[1;32m{}\x1b[0m \x1b[33m{:.8}\x1b[0m] Snapshot updated successfully (amend)!",
        branch, new_commit_hash
    );
    println!("Tip: You can revert this amend at any time using 'clonaris undo'.");
    Ok(())
}

/// Command: `clonaris log`
/// Displays commit ancestry from HEAD formatted with ASCII tree graph lines.
pub fn cmd_log() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let mut current_hash = match get_head(&root)? {
        Some(h) => h,
        None => {
            println!("No snapshots found in this repository.");
            return Ok(());
        }
    };

    let mut count = 0;
    while !current_hash.is_empty() {
        let commit_data = match read_object(&root, &current_hash) {
            Ok(d) => d,
            Err(_) => break,
        };

        let commit = match parse_commit(&commit_data) {
            Ok(c) => c,
            Err(_) => break,
        };

        let date_str = format_timestamp(commit.timestamp);

        println!("\x1b[33m*   commit {}\x1b[0m", current_hash);
        println!("|   Author:    {}", commit.author);
        println!("|   Date:      {}", date_str);
        println!("|   Tree:      {:.8}", commit.tree);
        if commit.parents.len() > 1 {
            println!("|   Merge:     {}", commit.parents.iter().map(|p| format!("{:.8}", p)).collect::<Vec<_>>().join(" + "));
        }
        println!("|");
        for line in commit.message.lines() {
            println!("|       {}", line);
        }
        println!("|");

        count += 1;
        if let Some(parent) = commit.parents.first() {
            current_hash = parent.clone();
        } else {
            break;
        }
    }

    println!("Total snapshots: {}", count);
    Ok(())
}

/// Command: `clonaris undo`
/// Reverts the latest operation, restoring disk files and branch pointers safely.
pub fn cmd_undo() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let last_op = match pop_last_op(&root)? {
        Some(op) => op,
        None => {
            println!("No operations in history to undo.");
            return Ok(());
        }
    };

    println!(
        "Undoing operation: \x1b[1;36m{}\x1b[0m ({})",
        last_op.action, last_op.description
    );

    // Push into redo stack before reverting
    push_redo(&root, &last_op)?;

    if last_op.prev_head == "none" {
        // Revert to pristine state (before first commit)
        let current_branch = get_current_branch(&root);
        let head_ref = root.join(".clonaris").join("refs").join("heads").join(current_branch);
        let _ = fs::remove_file(head_ref);
        println!("✓ Reverted to initial state (repository with no commits).");
    } else {
        // Restore prev_head commit
        let commit_data = read_object(&root, &last_op.prev_head)?;
        let commit = parse_commit(&commit_data)?;

        restore_tree(&root, &commit.tree)?;
        set_head(&root, &last_op.prev_head)?;

        println!(
            "✓ Snapshot restored successfully to \x1b[33m{:.8}\x1b[0m",
            last_op.prev_head
        );
    }

    println!("Tip: Run 'clonaris redo' if you wish to reapply this operation.");
    Ok(())
}

/// Command: `clonaris redo`
/// Reapplies the most recently undone operation.
pub fn cmd_redo() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let redo_op = match pop_last_redo(&root)? {
        Some(op) => op,
        None => {
            println!("No undone operations to redo.");
            return Ok(());
        }
    };

    println!(
        "Redoing operation: \x1b[1;36m{}\x1b[0m ({})",
        redo_op.action, redo_op.description
    );

    if redo_op.new_head != "none" {
        let commit_data = read_object(&root, &redo_op.new_head)?;
        let commit = parse_commit(&commit_data)?;

        restore_tree(&root, &commit.tree)?;
        set_head(&root, &redo_op.new_head)?;

        // Put op back into oplog
        restore_op_to_log(&root, &redo_op)?;

        println!(
            "✓ Snapshot restored to \x1b[33m{:.8}\x1b[0m",
            redo_op.new_head
        );
    }

    Ok(())
}

/// Command: `clonaris ops`
/// Displays a human-readable chronological timeline table of all operations.
pub fn cmd_ops() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let ops = read_ops(&root, "oplog")?;
    if ops.is_empty() {
        println!("No operations recorded in transaction log yet.");
        return Ok(());
    }

    println!("\x1b[1mOperation Timeline (OpLog):\x1b[0m");
    println!(
        "{:<4}  {:<19}  {:<8}  {:<10}  {}",
        "ID", "DATE & TIME (UTC)", "ACTION", "SNAPSHOT", "DESCRIPTION"
    );
    println!("{}", "-".repeat(75));

    for (i, op) in ops.iter().enumerate() {
        let date_str = format_timestamp(op.timestamp);
        let snapshot_short = if op.new_head == "none" {
            "-".to_string()
        } else {
            format!("{:.8}", op.new_head)
        };

        let desc_first_line = op.description.lines().next().unwrap_or("");

        println!(
            "#{:<3}  {:<19}  \x1b[1;36m{:<8}\x1b[0m  \x1b[33m{:<10}\x1b[0m  {}",
            i + 1,
            date_str,
            op.action,
            snapshot_short,
            desc_first_line
        );
    }

    println!();
    println!("Tip: You can revert operations above using 'clonaris undo'.");
    Ok(())
}

/// Command: `clonaris merge <branch>`
/// Merges `target_branch` into active branch with non-blocking conflict support.
pub fn cmd_merge(target_branch: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };
    execute_merge(&root, target_branch)
}

/// Command: `clonaris branch [name]`
/// Lists all existing branches or creates a new branch pointer at current HEAD.
pub fn cmd_branch(name: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let heads_dir = root.join(".clonaris").join("refs").join("heads");

    match name {
        Some(new_branch) => {
            let trimmed = new_branch.trim();
            if trimmed.is_empty() || trimmed.contains("..") || trimmed.starts_with('/') {
                eprintln!("Error: Invalid branch name: '{}'", new_branch);
                return Ok(());
            }

            let branch_file = heads_dir.join(trimmed);
            if branch_file.exists() {
                eprintln!("Error: Branch '{}' already exists.", trimmed);
                return Ok(());
            }

            let current_head = match get_head(&root)? {
                Some(h) => h,
                None => {
                    eprintln!("Error: Cannot create a branch without at least one snapshot.");
                    return Ok(());
                }
            };

            if let Some(parent) = branch_file.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::write(&branch_file, format!("{}\n", current_head))?;
            println!("✓ Branch '\x1b[1;32m{}\x1b[0m' created at snapshot {:.8}", trimmed, current_head);
        }
        None => {
            let current = get_current_branch(&root);
            println!("Available branches:");
            let mut branches = Vec::new();
            list_branches_recursive(&heads_dir, &heads_dir, &mut branches);
            branches.sort();
            for b_name in branches {
                if b_name == current {
                    println!("* \x1b[1;32m{}\x1b[0m (active)", b_name);
                } else {
                    println!("  {}", b_name);
                }
            }
        }
    }

    Ok(())
}

fn list_branches_recursive(base_dir: &Path, current_dir: &Path, results: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(current_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                list_branches_recursive(base_dir, &path, results);
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(base_dir) {
                    results.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
}

/// Command: `clonaris switch <branch>`
/// Switches to `branch_name` with smart data-loss protection for uncommitted changes.
pub fn cmd_switch(branch_name: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    let branch_file = root.join(".clonaris").join("refs").join("heads").join(branch_name);
    if !branch_file.exists() {
        eprintln!("Error: Branch '{}' was not found.", branch_name);
        return Ok(());
    }

    let diff = get_status(&root)?;
    if !diff.is_clean() {
        // SMART CHECKOUT: Prevent overwriting conflicting uncommitted modifications
        let target_commit_hash = fs::read_to_string(&branch_file)?.trim().to_string();
        let commit_data = read_object(&root, &target_commit_hash)?;
        let commit = parse_commit(&commit_data)?;
        let target_tree_data = read_object(&root, &commit.tree)?;
        let target_entries = parse_tree(&target_tree_data)?;
        let target_map: HashMap<String, String> = target_entries.into_iter().map(|e| (e.path, e.hash)).collect();

        let head_tree = get_head_tree(&root)?;
        let mut conflicting_files = Vec::new();

        for modified in &diff.modified {
            if let Some(target_hash) = target_map.get(modified) {
                let current_head_hash = head_tree.get(modified);
                if current_head_hash != Some(target_hash) {
                    conflicting_files.push(modified.clone());
                }
            }
        }

        for added in &diff.added {
            if target_map.contains_key(added) {
                conflicting_files.push(added.clone());
            }
        }

        if !conflicting_files.is_empty() {
            eprintln!("\x1b[1;31mError:\x1b[0m Cannot switch branch.");
            eprintln!("Your uncommitted local changes to the following files would be overwritten:");
            for f in conflicting_files {
                eprintln!("  ! {}", f);
            }
            eprintln!();
            eprintln!("Tip: Commit your changes first ('clonaris save -m \"checkpoint\"')");
            eprintln!("     or discard local modifications using 'clonaris discard <file>'.");
            return Ok(());
        }
    }

    let target_commit_hash = fs::read_to_string(&branch_file)?.trim().to_string();
    let commit_data = read_object(&root, &target_commit_hash)?;
    let commit = parse_commit(&commit_data)?;

    // Restore files
    restore_tree(&root, &commit.tree)?;

    // Update HEAD symbolic ref
    let head_file = root.join(".clonaris").join("HEAD");
    fs::write(head_file, format!("ref: refs/heads/{}\n", branch_name))?;

    // Record op
    append_op(
        &root,
        "switch",
        None,
        Some(&target_commit_hash),
        &format!("Switched to branch {}", branch_name),
    )?;

    println!("✓ Switched to branch '\x1b[1;32m{}\x1b[0m' ({:.8})", branch_name, target_commit_hash);
    Ok(())
}

/// Command: `clonaris tag [name]`
/// Lists all tags or creates a new static tag pointing to current HEAD.
pub fn cmd_tag(name: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found.");
            std::process::exit(1);
        }
    };

    match name {
        Some(tag_name) => {
            let current_head = match get_head(&root)? {
                Some(h) => h,
                None => {
                    eprintln!("Error: Cannot create a tag without commits.");
                    return Ok(());
                }
            };
            create_tag(&root, tag_name, &current_head)?;
            println!("✓ Tag '\x1b[1;35m{}\x1b[0m' created at snapshot {:.8}", tag_name, current_head);
        }
        None => {
            let tags = list_tags(&root)?;
            if tags.is_empty() {
                println!("No tags created yet. Use 'clonaris tag <name>' to create one.");
                return Ok(());
            }
            println!("Tags:");
            for (tag, hash) in tags {
                println!("  \x1b[1;35m{:<15}\x1b[0m -> \x1b[33m{:.8}\x1b[0m", tag, hash);
            }
        }
    }
    Ok(())
}

/// Command: `clonaris remote [add|remove|list] ...`
/// Manages remote repository endpoints.
pub fn cmd_remote(subcmd: Option<&str>, arg1: Option<&str>, arg2: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    match subcmd {
        None | Some("list") => {
            let remotes = remote::list_remotes(&root)?;
            if remotes.is_empty() {
                println!("No remotes configured.");
                println!("Tip: Add one with 'clonaris remote add origin <url>'");
            } else {
                println!("\x1b[1mConfigured remotes:\x1b[0m");
                for r in remotes {
                    println!("  \x1b[1;36m{:<12}\x1b[0m {}", r.name, r.url);
                }
            }
        }
        Some("add") => {
            let name = match arg1 {
                Some(n) => n,
                None => {
                    eprintln!("Error: Missing remote name. Example: clonaris remote add origin git@github.com:user/repo.git");
                    std::process::exit(1);
                }
            };
            let url = match arg2 {
                Some(u) => u,
                None => {
                    eprintln!("Error: Missing remote URL. Example: clonaris remote add origin git@github.com:user/repo.git");
                    std::process::exit(1);
                }
            };
            remote::add_remote(&root, name, url)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            println!("✓ Added remote '{}' -> {}", name, url);
        }
        Some("remove") | Some("rm") => {
            let name = match arg1 {
                Some(n) => n,
                None => {
                    eprintln!("Error: Missing remote name. Example: clonaris remote remove origin");
                    std::process::exit(1);
                }
            };
            remote::remove_remote(&root, name)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            println!("✓ Removed remote '{}'", name);
        }
        Some(unknown) => {
            eprintln!("Error: Unknown remote action '{}'. Available: list, add, remove", unknown);
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Command: `clonaris push [remote] [branch]`
/// Synchronizes local commits to the remote GitHub/Git server.
pub fn cmd_push(remote: Option<&str>, branch: Option<&str>, set_upstream: bool) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    let remote_name = remote.unwrap_or("origin");
    let current_branch = get_current_branch(&root);
    let branch_name = branch.unwrap_or(&current_branch);

    git_bridge::push(&root, remote_name, branch_name, set_upstream)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}

/// Command: `clonaris pull [remote] [branch]`
/// Fetches updates from the remote server and merges or fast-forwards into current branch.
pub fn cmd_pull(remote: Option<&str>, branch: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    let remote_name = remote.unwrap_or("origin");
    let current_branch = get_current_branch(&root);
    let branch_name = branch.unwrap_or(&current_branch);

    git_bridge::pull(&root, remote_name, branch_name)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}

/// Command: `clonaris clone <url> [destination]`
/// Clones a remote repository and initializes Clonaris environment.
pub fn cmd_clone(url: &str, destination: Option<&str>) -> io::Result<()> {
    let dest_path = destination.map(Path::new);
    git_bridge::clone_repo(url, dest_path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}

/// Command: `clonaris stash [push|pop|list|apply|drop]`
/// Manages lightweight local stashes.
pub fn cmd_stash(subcmd: Option<&str>, arg: Option<&str>) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    match subcmd {
        None | Some("push") => {
            let res = stash::push_stash(&root, arg)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            println!("✓ {}", res);
        }
        Some("list") => {
            let stashes = stash::list_stashes(&root)?;
            if stashes.is_empty() {
                println!("No stashes saved.");
            } else {
                println!("\x1b[1mSaved stashes:\x1b[0m");
                for s in stashes {
                    println!(
                        "  \x1b[1;33mstash@{{{}}}\x1b[0m [{}] (\x1b[32m{}\x1b[0m) {}",
                        s.index,
                        format_timestamp(s.timestamp),
                        s.branch,
                        s.message
                    );
                }
            }
        }
        Some("pop") => {
            let idx = arg.and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let res = stash::pop_stash(&root, idx)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            println!("✓ {}", res);
        }
        Some("apply") => {
            let idx = arg.and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let res = stash::apply_stash(&root, idx)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            println!("✓ {}", res);
        }
        Some("drop") => {
            let idx = arg.and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let res = stash::drop_stash(&root, idx)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            println!("✓ {}", res);
        }
        Some(other) => {
            // If user typed: clonaris stash "my message", treat as push message
            let res = stash::push_stash(&root, Some(other))
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            println!("✓ {}", res);
        }
    }
    Ok(())
}

/// Command: `clonaris cherry-pick <commit>`
/// Applies the delta from a commit onto the current branch.
pub fn cmd_cherry_pick(commit_hash: &str) -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    let res = cherry_pick::cherry_pick(&root, commit_hash)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    println!("{}", res);
    Ok(())
}

/// Command: `clonaris doctor`
/// Performs a full repository integrity and health audit.
pub fn cmd_doctor() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    println!("🩺 Running Clonaris repository health audit...");
    let report = doctor::run_doctor(&root)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    println!("\x1b[1mAudit Results:\x1b[0m");
    println!("  Total CAS Objects:   {}", report.total_objects);
    println!("  Verified SHA-256:    \x1b[32m{}\x1b[0m", report.verified_objects);
    println!("  Valid Branches:      {}", report.valid_branches);
    println!("  Valid Tags:          {}", report.valid_tags);
    println!("  Repository Size:     {:.2} KB", (report.total_bytes as f64) / 1024.0);

    if report.corrupted_objects.is_empty() {
        println!("\n\x1b[1;32m✓ Repository is in perfect health. Zero corrupted objects found.\x1b[0m");
    } else {
        println!("\n\x1b[1;31m⚠️ Detected {} corruption issues:\x1b[0m", report.corrupted_objects.len());
        for err in report.corrupted_objects {
            println!("  ! {}", err);
        }
    }
    Ok(())
}

/// Command: `clonaris gc`
/// Prunes unreachable objects to reclaim disk space.
pub fn cmd_gc() -> io::Result<()> {
    let root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("Error: No Clonaris repository found in this directory or any parent.");
            std::process::exit(1);
        }
    };

    println!("🧹 Running generational garbage collection...");
    let report = doctor::run_gc(&root)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    println!("\x1b[1mGarbage Collection Results:\x1b[0m");
    println!("  Reachable Objects:   {}", report.reachable_objects);
    println!("  Pruned Objects:      \x1b[32m{}\x1b[0m", report.pruned_objects);
    println!("  Reclaimed Disk Space: {:.2} KB", (report.reclaimed_bytes as f64) / 1024.0);
    println!("\n\x1b[1;32m✓ Repository successfully optimized and clean.\x1b[0m");
    Ok(())
}

/// Prints standard help and command usage information.
pub fn print_help() {
    println!("\x1b[1mClonaris VCS\x1b[0m - Simple, frictionless version control");
    println!("100% Pure Rust (Zero external dependencies)");
    println!();
    println!("\x1b[1mUSAGE:\x1b[0m");
    println!("    clonaris <COMMAND> [OPTIONS]");
    println!();
    println!("\x1b[1mCORE COMMANDS:\x1b[0m");
    println!("    init                 Initialize a new repository (.clonaris/)");
    println!("    status               Show working tree status (new, modified, deleted)");
    println!("    diff [file]          Show line-by-line colored diffs");
    println!("    save -m \"<msg>\"      Create an atomic snapshot directly (zero staging)");
    println!("    amend -m \"<msg>\"     Safely update the latest snapshot");
    println!("    discard <file|.>     Discard uncommitted modifications in files");
    println!("    resolve <file>       Resolve merge conflict markers interactively");
    println!("    branch [name]        List branches or create a new branch pointer");
    println!("    switch <branch>      Switch active branch with data-loss protection");
    println!("    merge <branch>       Merge another branch (with non-blocking conflicts)");
    println!("    tag [name]           List tags or create a tag at current snapshot");
    println!("    log                  Display snapshot history with ASCII tree graph");
    println!("    ops                  Display chronological transaction log (OpLog)");
    println!("    undo                 Safely revert the latest operation");
    println!("    redo                 Reapply the latest undone operation");
    println!();
    println!("\x1b[1mREMOTE & GITHUB SYNC:\x1b[0m");
    println!("    remote [add|rm|list] Manage remote repository endpoints");
    println!("    push [remote] [br]   Push snapshots to GitHub / remote Git server");
    println!("    pull [remote] [br]   Pull and merge updates from GitHub / remote");
    println!("    clone <url> [dir]    Clone remote repository and initialize Clonaris");
    println!();
    println!("\x1b[1mDEVELOPER SUPERPOWERS:\x1b[0m");
    println!("    stash [push|pop|list] Save/restore uncommitted changes instantly");
    println!("    cherry-pick <commit> Apply specific commit delta onto current branch");
    println!("    doctor               Deep repository health and integrity audit");
    println!("    gc                   Generational garbage collection (reclaim space)");
    println!("    completion [shell]   Generate shell autocompletion script (bash or zsh)");
    println!("    help                 Display this help information");
    println!();
    println!("\x1b[1mEXAMPLE:\x1b[0m");
    println!("    clonaris init");
    println!("    echo 'hello' > index.txt");
    println!("    clonaris save -m 'Initial commit'");
    println!("    clonaris remote add origin git@github.com:user/repo.git");
    println!("    clonaris push -u origin main");
}

fn format_timestamp(timestamp: u64) -> String {
    let seconds_in_day = 86400;
    let days = timestamp / seconds_in_day;
    let rem_seconds = timestamp % seconds_in_day;
    let hours = rem_seconds / 3600;
    let minutes = (rem_seconds % 3600) / 60;
    let seconds = rem_seconds % 60;

    let mut year = 1970;
    let mut d = days;

    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        year += 1;
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30,
        31, 31, 30, 31, 30, 31,
    ];

    let mut month = 1;
    for &dim in &days_in_months {
        if d < dim {
            break;
        }
        d -= dim;
        month += 1;
    }
    let day = d + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, hours, minutes, seconds
    )
}
