//! # Remote Repository Management
//!
//! Handles configuration of remote repositories (e.g. GitHub, GitLab) in `.clonaris/config`.
//! Supports adding, removing, listing, and querying remote URLs.

use std::fs;
use std::io::{self, Error, ErrorKind};
use std::path::Path;

/// Represents a configured remote repository endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// Remote name identifier (e.g., `"origin"`).
    pub name: String,
    /// Remote URL (e.g., `"git@github.com:user/repo.git"` or `"https://github.com/user/repo.git"`).
    pub url: String,
}

/// Lists all configured remote repositories from `.clonaris/config`.
pub fn list_remotes(repo_root: &Path) -> io::Result<Vec<Remote>> {
    let config_path = repo_root.join(".clonaris").join("config");
    if !config_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&config_path)?;
    let mut remotes = Vec::new();
    let mut current_remote_name: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[remote \"") && trimmed.ends_with("\"]") {
            let name = &trimmed[9..trimmed.len() - 2];
            current_remote_name = Some(name.to_string());
        } else if trimmed.starts_with('[') {
            current_remote_name = None;
        } else if let Some(ref name) = current_remote_name {
            if let Some(val) = trimmed.strip_prefix("url = ") {
                remotes.push(Remote {
                    name: name.clone(),
                    url: val.trim().to_string(),
                });
                current_remote_name = None;
            } else if let Some(val) = trimmed.strip_prefix("url=") {
                remotes.push(Remote {
                    name: name.clone(),
                    url: val.trim().to_string(),
                });
                current_remote_name = None;
            }
        }
    }

    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(remotes)
}

/// Retrieves a specific remote by name from `.clonaris/config`.
pub fn get_remote(repo_root: &Path, name: &str) -> io::Result<Option<Remote>> {
    let remotes = list_remotes(repo_root)?;
    Ok(remotes.into_iter().find(|r| r.name == name))
}

/// Adds a new remote configuration to `.clonaris/config`.
/// Fails if a remote with the same name already exists.
pub fn add_remote(repo_root: &Path, name: &str, url: &str) -> io::Result<()> {
    if name.is_empty() || url.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Remote name and URL cannot be empty.",
        ));
    }

    if let Some(_) = get_remote(repo_root, name)? {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!("Remote '{}' already exists.", name),
        ));
    }

    let config_path = repo_root.join(".clonaris").join("config");
    let mut content = if config_path.exists() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }

    content.push_str(&format!("\n[remote \"{}\"]\n\turl = {}\n", name, url));
    fs::write(&config_path, content)?;

    Ok(())
}

/// Removes an existing remote configuration from `.clonaris/config`.
/// Fails if the remote does not exist.
pub fn remove_remote(repo_root: &Path, name: &str) -> io::Result<()> {
    if get_remote(repo_root, name)?.is_none() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Remote '{}' not found.", name),
        ));
    }

    let config_path = repo_root.join(".clonaris").join("config");
    let content = fs::read_to_string(&config_path)?;

    let target_header = format!("[remote \"{}\"]", name);
    let mut new_lines = Vec::new();
    let mut skipping = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == target_header {
            skipping = true;
            continue;
        }

        if skipping {
            if trimmed.starts_with('[') {
                skipping = false;
                new_lines.push(line);
            } else {
                // skip remote body lines
                continue;
            }
        } else {
            new_lines.push(line);
        }
    }

    let mut new_content = new_lines.join("\n");
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    fs::write(&config_path, new_content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_remote_crud() {
        let tmp = std::env::temp_dir().join(format!("clonaris_test_remote_{}", std::process::id()));
        let clonaris_dir = tmp.join(".clonaris");
        fs::create_dir_all(&clonaris_dir).unwrap();

        let config_file = clonaris_dir.join("config");
        fs::write(&config_file, "[user]\nname = Tester\nemail = tester@local\n").unwrap();

        // 1. Initially empty
        let remotes = list_remotes(&tmp).unwrap();
        assert!(remotes.is_empty());

        // 2. Add remote
        add_remote(&tmp, "origin", "git@github.com:user/repo.git").unwrap();
        let remotes = list_remotes(&tmp).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].url, "git@github.com:user/repo.git");

        // 3. Add duplicate should fail
        assert!(add_remote(&tmp, "origin", "another_url").is_err());

        // 4. Add second remote
        add_remote(&tmp, "upstream", "https://github.com/upstream/repo.git").unwrap();
        let remotes = list_remotes(&tmp).unwrap();
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[1].name, "upstream");

        // 5. Query single remote
        let r = get_remote(&tmp, "upstream").unwrap().unwrap();
        assert_eq!(r.url, "https://github.com/upstream/repo.git");

        // 6. Remove remote
        remove_remote(&tmp, "origin").unwrap();
        let remotes = list_remotes(&tmp).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].name, "upstream");

        // Cleanup
        let _ = fs::remove_dir_all(&tmp);
    }
}
