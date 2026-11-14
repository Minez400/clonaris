//! # Git Interoperability Bridge Engine
//!
//! Provides bidirectional synchronization between Clonaris repositories and Git/GitHub remotes.
//! Uses native system Git transport commands via `std::process::Command` without adding any
//! external crate dependencies, enabling complete compatibility with GitHub, GitLab, and Bitbucket.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::objects::{parse_commit, parse_tree, read_object, serialize_commit, serialize_tree, Commit, TreeEntry, write_object};
use crate::oplog::append_op;
use crate::remote::get_remote;
use crate::repository::{get_head, init, set_head};
use crate::workspace::{restore_tree, scan_workspace_files};

/// Checks if standard `git` CLI is available in `$PATH`.
pub fn is_git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Ensures that a Git repository (.git/) exists in `repo_root` and that `.clonaris/` is in `.gitignore`.
pub fn ensure_git_repo(repo_root: &Path, branch: &str) -> Result<(), String> {
    if !is_git_available() {
        return Err("Git is required for remote synchronization but was not found in PATH.".to_string());
    }

    let git_dir = repo_root.join(".git");
    if !git_dir.exists() {
        let status = Command::new("git")
            .arg("init")
            .arg("-b")
            .arg(branch)
            .current_dir(repo_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("Failed to initialize Git bridge mirror: {}", e))?;

        if !status.success() {
            return Err("Failed to initialize Git repository in project directory.".to_string());
        }
    }

    // Ensure .gitignore contains .clonaris/ so Git never tracks Clonaris internal database
    let gitignore = repo_root.join(".gitignore");
    let content = if gitignore.exists() {
        fs::read_to_string(&gitignore).unwrap_or_default()
    } else {
        String::new()
    };

    if !content.lines().any(|l| l.trim() == ".clonaris/" || l.trim() == ".clonaris") {
        let mut new_content = content;
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(".clonaris/\n");
        let _ = fs::write(gitignore, new_content);
    }

    Ok(())
}

/// Formats an author string into a valid Git committer header line (e.g. `Name <email> timestamp +0000`).
fn format_git_committer(author: &str, timestamp: u64) -> String {
    let clean_author = if author.contains('<') && author.contains('>') {
        author.to_string()
    } else {
        format!("{} <{}@localhost>", author, author)
    };
    format!("committer {} {} +0000", clean_author, timestamp)
}

/// Exports the latest Clonaris commit on `branch` into the local Git mirror using `git fast-import`.
/// Returns the resulting Git commit SHA.
pub fn export_to_git(repo_root: &Path, branch: &str) -> Result<String, String> {
    ensure_git_repo(repo_root, branch)?;

    let head_hash = match get_head(repo_root).map_err(|e| e.to_string())? {
        Some(h) => h,
        None => return Err("No Clonaris snapshots found to export. Save a snapshot first with 'clonaris save'.".to_string()),
    };

    let commit_bytes = read_object(repo_root, &head_hash)
        .map_err(|e| format!("Failed to read commit object {}: {}", head_hash, e))?;
    let commit = parse_commit(&commit_bytes)
        .map_err(|e| format!("Failed to parse commit object: {}", e))?;

    let tree_bytes = read_object(repo_root, &commit.tree)
        .map_err(|e| format!("Failed to read tree object {}: {}", commit.tree, e))?;
    let tree_entries = parse_tree(&tree_bytes)
        .map_err(|e| format!("Failed to parse tree object: {}", e))?;

    let mut import_stream: Vec<u8> = Vec::new();
    let ref_target = format!("refs/heads/{}", branch);

    // Fast-import header
    let committer_line = format_git_committer(&commit.author, commit.timestamp);
    let msg_bytes = commit.message.as_bytes();

    writeln!(&mut import_stream, "commit {}", ref_target).unwrap();
    writeln!(&mut import_stream, "{}", committer_line).unwrap();
    writeln!(&mut import_stream, "data {}", msg_bytes.len()).unwrap();
    import_stream.extend_from_slice(msg_bytes);
    writeln!(&mut import_stream).unwrap();
    writeln!(&mut import_stream, "deleteall").unwrap();

    // Add all files from Clonaris Tree
    for entry in tree_entries {
        let blob_data = match read_object(repo_root, &entry.hash) {
            Ok(d) => d,
            Err(_) => continue,
        };
        writeln!(&mut import_stream, "M 100644 inline {}", entry.path).unwrap();
        writeln!(&mut import_stream, "data {}", blob_data.len()).unwrap();
        import_stream.extend_from_slice(&blob_data);
        writeln!(&mut import_stream).unwrap();
    }

    // Execute git fast-import
    let mut child = Command::new("git")
        .arg("fast-import")
        .arg("--quiet")
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git fast-import: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&import_stream)
            .map_err(|e| format!("Failed to stream objects to git fast-import: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("git fast-import failed: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git fast-import error: {}", err));
    }

    // Query resulting Git commit hash
    let rev_output = Command::new("git")
        .arg("rev-parse")
        .arg(&ref_target)
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("Failed to query git rev-parse: {}", e))?;

    if !rev_output.status.success() {
        return Err("Failed to resolve exported Git commit hash.".to_string());
    }

    let git_hash = String::from_utf8_lossy(&rev_output.stdout).trim().to_string();

    // Point Git's HEAD to this branch if detached or on another branch
    let _ = Command::new("git")
        .arg("symbolic-ref")
        .arg("HEAD")
        .arg(&ref_target)
        .current_dir(repo_root)
        .output();

    Ok(git_hash)
}

/// Pushes the active Clonaris branch to the specified remote repository (e.g. GitHub).
pub fn push(
    repo_root: &Path,
    remote_name: &str,
    branch: &str,
    set_upstream: bool,
) -> Result<(), String> {
    if !is_git_available() {
        return Err("Git is required to push to remote servers.".to_string());
    }

    let remote = match get_remote(repo_root, remote_name).map_err(|e| e.to_string())? {
        Some(r) => r,
        None => {
            return Err(format!(
                "Remote '{}' is not configured.\nUse 'clonaris remote add {} <url>' to configure it.",
                remote_name, remote_name
            ))
        }
    };

    ensure_git_repo(repo_root, branch)?;

    // Ensure Git has the remote URL configured
    let remote_check = Command::new("git")
        .arg("remote")
        .arg("get-url")
        .arg(remote_name)
        .current_dir(repo_root)
        .output();

    let needs_add = match remote_check {
        Ok(out) => !out.status.success(),
        Err(_) => true,
    };

    if needs_add {
        let _ = Command::new("git")
            .arg("remote")
            .arg("add")
            .arg(remote_name)
            .arg(&remote.url)
            .current_dir(repo_root)
            .status();
    } else {
        // Update URL if changed
        let _ = Command::new("git")
            .arg("remote")
            .arg("set-url")
            .arg(remote_name)
            .arg(&remote.url)
            .current_dir(repo_root)
            .status();
    }

    println!("🔄 Synchronizing Clonaris snapshot to Git mirror...");
    let git_hash = export_to_git(repo_root, branch)?;
    println!("📦 Exported commit {} to Git bridge.", &git_hash[..7.min(git_hash.len())]);

    println!("🚀 Pushing to {} ({}) on branch '{}'...", remote_name, remote.url, branch);

    let mut push_cmd = Command::new("git");
    push_cmd.arg("push").current_dir(repo_root);

    if set_upstream {
        push_cmd.arg("-u");
    }

    push_cmd.arg(remote_name).arg(branch);

    let status = push_cmd
        .status()
        .map_err(|e| format!("Failed to execute git push: {}", e))?;

    if !status.success() {
        return Err("Push failed. Check your network connection, remote permissions, or pull remote changes first.".to_string());
    }

    // Record remote reference in Clonaris
    let remotes_ref_dir = repo_root
        .join(".clonaris")
        .join("refs")
        .join("remotes")
        .join(remote_name);
    let _ = fs::create_dir_all(&remotes_ref_dir);
    if let Ok(Some(clonaris_head)) = get_head(repo_root) {
        let _ = fs::write(remotes_ref_dir.join(branch), format!("{}\n", clonaris_head));
    }

    println!("✅ Successfully pushed to {}/{}", remote_name, branch);
    Ok(())
}

/// Pulls updates from the remote repository, converts Git commits to Clonaris CAS objects,
/// and merges or fast-forwards the current branch.
pub fn pull(repo_root: &Path, remote_name: &str, branch: &str) -> Result<(), String> {
    if !is_git_available() {
        return Err("Git is required to pull from remote servers.".to_string());
    }

    let remote = match get_remote(repo_root, remote_name).map_err(|e| e.to_string())? {
        Some(r) => r,
        None => {
            return Err(format!(
                "Remote '{}' is not configured.\nUse 'clonaris remote add {} <url>' to configure it.",
                remote_name, remote_name
            ))
        }
    };

    ensure_git_repo(repo_root, branch)?;

    println!("⬇️  Fetching updates from {} ({}) branch '{}'...", remote_name, remote.url, branch);

    let fetch_status = Command::new("git")
        .arg("fetch")
        .arg(remote_name)
        .arg(branch)
        .current_dir(repo_root)
        .status()
        .map_err(|e| format!("Failed to execute git fetch: {}", e))?;

    if !fetch_status.success() {
        return Err("Fetch failed. Please check your network and remote URL.".to_string());
    }

    // Inspect FETCH_HEAD
    let log_output = Command::new("git")
        .arg("log")
        .arg("-1")
        .arg("--format=%an <%ae>|%at|%B")
        .arg("FETCH_HEAD")
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("Failed to read fetched commit details: {}", e))?;

    if !log_output.status.success() {
        return Err("No updates found on remote branch.".to_string());
    }

    let log_str = String::from_utf8_lossy(&log_output.stdout);
    let parts: Vec<&str> = log_str.splitn(3, '|').collect();
    if parts.len() < 3 {
        return Err("Failed to parse remote commit metadata.".to_string());
    }

    let author = parts[0].trim().to_string();
    let timestamp: u64 = parts[1].trim().parse().unwrap_or(0);
    let message = parts[2].trim().to_string();

    // Read list of all files in FETCH_HEAD tree via git ls-tree -r -z
    let ls_output = Command::new("git")
        .arg("ls-tree")
        .arg("-r")
        .arg("-z")
        .arg("FETCH_HEAD")
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("Failed to read remote file tree: {}", e))?;

    let raw_ls = ls_output.stdout;
    let mut tree_entries = Vec::new();

    // Split on \0
    for record in raw_ls.split(|&b| b == 0) {
        if record.is_empty() {
            continue;
        }
        let record_str = String::from_utf8_lossy(record);
        // Format: <mode> <type> <hash>\t<filepath>
        if let Some(tab_idx) = record_str.find('\t') {
            let meta = &record_str[..tab_idx];
            let filepath = &record_str[tab_idx + 1..];
            let meta_parts: Vec<&str> = meta.split_whitespace().collect();
            if meta_parts.len() >= 3 && meta_parts[1] == "blob" {
                let git_blob_hash = meta_parts[2];

                // Fetch raw blob data from Git
                let cat_output = Command::new("git")
                    .arg("cat-file")
                    .arg("-p")
                    .arg(git_blob_hash)
                    .current_dir(repo_root)
                    .output()
                    .map_err(|e| format!("Failed to extract git blob {}: {}", git_blob_hash, e))?;

                // Store in Clonaris CAS (SHA-256)
                let clonaris_blob_hash = write_object(repo_root, &cat_output.stdout)
                    .map_err(|e| format!("Failed to write Clonaris blob: {}", e))?;

                tree_entries.push(TreeEntry {
                    kind: "blob".to_string(),
                    hash: clonaris_blob_hash,
                    path: filepath.to_string(),
                });
            }
        }
    }

    // Write Clonaris Tree object
    let serialized_tree = serialize_tree(&tree_entries);
    let tree_hash = write_object(repo_root, &serialized_tree)
        .map_err(|e| format!("Failed to write tree object: {}", e))?;

    // Determine parent for this commit
    let current_head = get_head(repo_root).map_err(|e| e.to_string())?;
    let parents = match current_head {
        Some(ref h) => vec![h.clone()],
        None => Vec::new(),
    };

    let remote_commit = Commit {
        tree: tree_hash.clone(),
        parents,
        author,
        timestamp,
        message,
    };

    let remote_commit_bytes = serialize_commit(&remote_commit);
    let new_commit_hash = write_object(repo_root, &remote_commit_bytes)
        .map_err(|e| format!("Failed to write pulled commit: {}", e))?;

    // Update working tree files
    restore_tree(repo_root, &tree_hash)
        .map_err(|e| format!("Failed to update working directory files: {}", e))?;

    set_head(repo_root, &new_commit_hash).map_err(|e| e.to_string())?;

    append_op(
        repo_root,
        "pull",
        current_head.as_deref(),
        Some(&new_commit_hash),
        &format!("Pulled updates from {}/{}", remote_name, branch),
    )
    .map_err(|e| e.to_string())?;

    println!("✅ Successfully pulled from {}/{}! (Snapshot {})", remote_name, branch, &new_commit_hash[..7]);
    Ok(())
}

/// Clones a remote repository into `target_dir` and initializes a full Clonaris environment.
pub fn clone_repo(url: &str, target_path: Option<&Path>) -> Result<PathBuf, String> {
    if !is_git_available() {
        return Err("Git is required to clone remote repositories.".to_string());
    }

    let default_dest = match url.split('/').last().and_then(|s| s.strip_suffix(".git").or(Some(s))) {
        Some(name) if !name.is_empty() => name,
        _ => "clonaris_repo",
    };

    let dest_buf = match target_path {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(default_dest),
    };

    println!("📥 Cloning '{}' into '{}'...", url, dest_buf.display());

    let status = Command::new("git")
        .arg("clone")
        .arg(url)
        .arg(&dest_buf)
        .status()
        .map_err(|e| format!("Failed to execute git clone: {}", e))?;

    if !status.success() {
        return Err("Git clone failed.".to_string());
    }

    // Initialize Clonaris in cloned directory
    init(&dest_buf).map_err(|e| format!("Failed to initialize Clonaris in cloned project: {}", e))?;

    // Add origin remote
    let _ = crate::remote::add_remote(&dest_buf, "origin", url);

    // Initial snapshot of cloned files
    let workspace_files = scan_workspace_files(&dest_buf).map_err(|e| e.to_string())?;
    let mut tree_entries = Vec::new();
    for rel_path in workspace_files {
        let full_path = dest_buf.join(&rel_path);
        let content = fs::read(&full_path).map_err(|e| e.to_string())?;
        let hash = write_object(&dest_buf, &content).map_err(|e| e.to_string())?;
        tree_entries.push(TreeEntry {
            kind: "blob".to_string(),
            hash,
            path: rel_path,
        });
    }

    tree_entries.sort_by(|a, b| a.path.cmp(&b.path));
    let tree_bytes = serialize_tree(&tree_entries);
    let tree_hash = write_object(&dest_buf, &tree_bytes).map_err(|e| e.to_string())?;

    let author = crate::repository::get_author(&dest_buf);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let initial_commit = Commit {
        tree: tree_hash,
        parents: Vec::new(),
        author,
        timestamp,
        message: format!("Cloned from {}", url),
    };

    let commit_bytes = serialize_commit(&initial_commit);
    let commit_hash = write_object(&dest_buf, &commit_bytes).map_err(|e| e.to_string())?;
    set_head(&dest_buf, &commit_hash).map_err(|e| e.to_string())?;

    append_op(
        &dest_buf,
        "clone",
        None,
        Some(&commit_hash),
        &format!("Cloned repository from {}", url),
    )
    .map_err(|e| e.to_string())?;

    println!("✅ Cloned and initialized Clonaris repository in {}", dest_buf.display());
    Ok(dest_buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::init;
    use crate::workspace::save;

    #[test]
    fn test_export_to_git_bridge() {
        if !is_git_available() {
            return;
        }

        let tmp = std::env::temp_dir().join(format!("clonaris_test_git_bridge_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        init(&tmp).unwrap();

        fs::write(tmp.join("bridged.txt"), "Hello from Clonaris to Git!\n").unwrap();
        save(&tmp, "Bridge test commit").unwrap();

        let git_hash = export_to_git(&tmp, "main").unwrap();
        assert_eq!(git_hash.len(), 40);

        // Verify git log contains our commit
        let log_out = Command::new("git")
            .arg("log")
            .arg("-1")
            .arg("--format=%s")
            .current_dir(&tmp)
            .output()
            .unwrap();

        let msg = String::from_utf8_lossy(&log_out.stdout).trim().to_string();
        assert_eq!(msg, "Bridge test commit");

        let _ = fs::remove_dir_all(&tmp);
    }
}
