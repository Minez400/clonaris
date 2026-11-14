use std::fs;
use std::io::{self, Error, ErrorKind};
use std::path::{Path, PathBuf};
use crate::hash::hash_bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub kind: String, // "blob"
    pub hash: String,
    pub path: String, // Relative POSIX-style path
}

#[derive(Debug, Clone)]
pub struct Commit {
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub timestamp: u64,
    pub message: String,
}

pub fn object_path(repo_root: &Path, hash: &str) -> PathBuf {
    let (dir, file) = hash.split_at(2);
    repo_root.join(".clonaris").join("objects").join(dir).join(file)
}

pub fn write_object(repo_root: &Path, data: &[u8]) -> io::Result<String> {
    let hash = hash_bytes(data);
    let path = object_path(repo_root, &hash);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Atomic write: write to temp file then rename, preventing corrupted half-written files
    if !path.exists() {
        let tmp_file = path.with_extension(format!("tmp_{}", std::process::id()));
        fs::write(&tmp_file, data)?;
        if let Err(e) = fs::rename(&tmp_file, &path) {
            let _ = fs::remove_file(&tmp_file);
            return Err(e);
        }
    }

    Ok(hash)
}

pub fn read_object(repo_root: &Path, hash: &str) -> io::Result<Vec<u8>> {
    let path = object_path(repo_root, hash);
    if !path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Objeto {} não encontrado no repositório", hash),
        ));
    }
    fs::read(&path)
}

// Tree serialization & parsing
// Format: "blob <hash> <path>\n"
pub fn serialize_tree(entries: &[TreeEntry]) -> Vec<u8> {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&format!("{} {} {}\n", entry.kind, entry.hash, entry.path));
    }
    out.into_bytes()
}

pub fn parse_tree(data: &[u8]) -> io::Result<Vec<TreeEntry>> {
    let text = std::str::from_utf8(data)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    
    let mut entries = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() == 3 {
            entries.push(TreeEntry {
                kind: parts[0].to_string(),
                hash: parts[1].to_string(),
                path: parts[2].to_string(),
            });
        }
    }
    Ok(entries)
}

// Commit serialization & parsing
pub fn serialize_commit(commit: &Commit) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(&format!("tree {}\n", commit.tree));
    for parent in &commit.parents {
        out.push_str(&format!("parent {}\n", parent));
    }
    out.push_str(&format!("author {}\n", commit.author));
    out.push_str(&format!("timestamp {}\n", commit.timestamp));
    out.push_str("\n");
    out.push_str(&commit.message);
    out.into_bytes()
}

pub fn parse_commit(data: &[u8]) -> io::Result<Commit> {
    let text = std::str::from_utf8(data)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    let mut tree = String::new();
    let mut parents = Vec::new();
    let mut author = String::new();
    let mut timestamp = 0u64;
    let mut message_lines = Vec::new();
    let mut in_message = false;

    for line in text.lines() {
        if in_message {
            message_lines.push(line);
            continue;
        }

        if line.is_empty() {
            in_message = true;
            continue;
        }

        if let Some(val) = line.strip_prefix("tree ") {
            tree = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("parent ") {
            parents.push(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("author ") {
            author = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("timestamp ") {
            timestamp = val.trim().parse().unwrap_or(0);
        }
    }

    Ok(Commit {
        tree,
        parents,
        author,
        timestamp,
        message: message_lines.join("\n"),
    })
}
