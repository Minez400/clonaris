//! # Repository & Reference Management
//!
//! Handles repository discovery, initialization (`.clonaris/` layout), symbolic and
//! direct HEAD resolution, branch pointer manipulation, and static tags.

use std::env;
use std::fs;
use std::io::{self, Error, ErrorKind};
use std::path::{Path, PathBuf};

/// Climbs up directory ancestors from the current working directory to locate the `.clonaris/` root.
pub fn find_repo_root() -> Option<PathBuf> {
    let mut current = env::current_dir().ok()?;
    loop {
        if current.join(".clonaris").is_dir() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Initializes a new Clonaris repository in `target_dir`.
///
/// Sets up:
/// - `.clonaris/objects/` (immutable content-addressable store)
/// - `.clonaris/refs/heads/` (branch references)
/// - `.clonaris/refs/tags/` (tag references)
/// - `.clonaris/HEAD` pointing symbolically to `refs/heads/main`
/// - `.clonaris/config` with default user identity
/// - `.clonarisignore` with standard default ignore rules
pub fn init(target_dir: &Path) -> io::Result<()> {
    let clonaris_dir = target_dir.join(".clonaris");
    if clonaris_dir.exists() {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            "A Clonaris repository already exists in this directory.",
        ));
    }

    fs::create_dir_all(clonaris_dir.join("objects"))?;
    fs::create_dir_all(clonaris_dir.join("refs").join("heads"))?;
    fs::create_dir_all(clonaris_dir.join("refs").join("tags"))?;

    // Create HEAD pointing to default 'main' branch
    fs::write(clonaris_dir.join("HEAD"), "ref: refs/heads/main\n")?;

    // Default user configuration
    let username = env::var("USER").unwrap_or_else(|_| "developer".to_string());
    let default_config = format!(
        "[user]\nname = {}\nemail = {}@localhost\n",
        username, username
    );
    fs::write(clonaris_dir.join("config"), default_config)?;

    // Create .clonarisignore if not present
    let ignore_file = target_dir.join(".clonarisignore");
    if !ignore_file.exists() {
        let default_ignore = "target/\n.git/\n.clonaris/\n*.log\n";
        fs::write(ignore_file, default_ignore)?;
    }

    // Touch transaction oplog
    fs::write(clonaris_dir.join("oplog"), "")?;

    Ok(())
}

/// Resolves the current HEAD commit hash.
///
/// Supports:
/// - Symbolic reference: `"ref: refs/heads/<branch>"` -> reads target ref file.
/// - Detached HEAD: direct 64-character commit hash string.
/// Returns `None` if repository has no commits yet.
pub fn get_head(repo_root: &Path) -> io::Result<Option<String>> {
    let head_file = repo_root.join(".clonaris").join("HEAD");
    if !head_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(head_file)?;
    let trimmed = content.trim();

    if let Some(ref_path) = trimmed.strip_prefix("ref: ") {
        let target = repo_root.join(".clonaris").join(ref_path);
        if target.exists() {
            let hash = fs::read_to_string(target)?;
            let h = hash.trim().to_string();
            if h.is_empty() {
                Ok(None)
            } else {
                Ok(Some(h))
            }
        } else {
            Ok(None)
        }
    } else if !trimmed.is_empty() {
        Ok(Some(trimmed.to_string()))
    } else {
        Ok(None)
    }
}

/// Updates the current HEAD reference (or the branch it points to) to a new commit hash.
pub fn set_head(repo_root: &Path, commit_hash: &str) -> io::Result<()> {
    let head_file = repo_root.join(".clonaris").join("HEAD");
    let content = fs::read_to_string(&head_file).unwrap_or_default();
    let trimmed = content.trim();

    if let Some(ref_path) = trimmed.strip_prefix("ref: ") {
        let target = repo_root.join(".clonaris").join(ref_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, format!("{}\n", commit_hash))?;
    } else {
        fs::write(head_file, format!("{}\n", commit_hash))?;
    }
    Ok(())
}

/// Returns the name of the currently active branch (e.g., `"main"`), or `"(detached: abc...)"`.
pub fn get_current_branch(repo_root: &Path) -> String {
    let head_file = repo_root.join(".clonaris").join("HEAD");
    if let Ok(content) = fs::read_to_string(head_file) {
        let trimmed = content.trim();
        if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
            return branch.to_string();
        } else if !trimmed.is_empty() {
            return format!("(detached: {:.7})", trimmed);
        }
    }
    "main".to_string()
}

/// Reads the author identity string from `.clonaris/config`, falling back to `$USER`.
pub fn get_author(repo_root: &Path) -> String {
    let config_path = repo_root.join(".clonaris").join("config");
    if let Ok(content) = fs::read_to_string(config_path) {
        let mut name = String::new();
        let mut email = String::new();
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("name = ") {
                name = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("email = ") {
                email = val.trim().to_string();
            }
        }
        if !name.is_empty() {
            if !email.is_empty() {
                return format!("{} <{}>", name, email);
            }
            return name;
        }
    }
    let username = env::var("USER").unwrap_or_else(|_| "developer".to_string());
    format!("{} <{}@localhost>", username, username)
}

/// Creates a static tag reference pointing to a specific commit hash.
pub fn create_tag(repo_root: &Path, name: &str, target_hash: &str) -> io::Result<()> {
    let tag_dir = repo_root.join(".clonaris").join("refs").join("tags");
    fs::create_dir_all(&tag_dir)?;
    let tag_file = tag_dir.join(name);
    if tag_file.exists() {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!("Tag '{}' already exists.", name),
        ));
    }
    fs::write(tag_file, format!("{}\n", target_hash))?;
    Ok(())
}

/// Lists all static tags in the repository sorted alphabetically.
pub fn list_tags(repo_root: &Path) -> io::Result<Vec<(String, String)>> {
    let tag_dir = repo_root.join(".clonaris").join("refs").join("tags");
    if !tag_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tags = Vec::new();
    for entry in fs::read_dir(tag_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let hash = fs::read_to_string(entry.path())?.trim().to_string();
        tags.push((name, hash));
    }
    tags.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(tags)
}
