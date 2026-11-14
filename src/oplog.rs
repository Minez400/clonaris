use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct OpEntry {
    pub timestamp: u64,
    pub action: String,
    pub prev_head: String, // "none" if empty
    pub new_head: String,
    pub description: String,
}

pub fn append_op(
    repo_root: &Path,
    action: &str,
    prev_head: Option<&str>,
    new_head: Option<&str>,
    description: &str,
) -> io::Result<()> {
    let oplog_path = repo_root.join(".clonaris").join("oplog");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(oplog_path)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let prev = prev_head.unwrap_or("none");
    let next = new_head.unwrap_or("none");

    writeln!(file, "{}|{}|{}|{}|{}", now, action, prev, next, description)?;
    Ok(())
}

pub fn read_ops(repo_root: &Path) -> io::Result<Vec<OpEntry>> {
    let oplog_path = repo_root.join(".clonaris").join("oplog");
    if !oplog_path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(oplog_path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() == 5 {
            entries.push(OpEntry {
                timestamp: parts[0].parse().unwrap_or(0),
                action: parts[1].to_string(),
                prev_head: parts[2].to_string(),
                new_head: parts[3].to_string(),
                description: parts[4].to_string(),
            });
        }
    }

    Ok(entries)
}

pub fn pop_last_op(repo_root: &Path) -> io::Result<Option<OpEntry>> {
    let oplog_path = repo_root.join(".clonaris").join("oplog");
    if !oplog_path.exists() {
        return Ok(None);
    }

    let mut ops = read_ops(repo_root)?;
    let last = ops.pop();

    let mut out = String::new();
    for op in &ops {
        out.push_str(&format!(
            "{}|{}|{}|{}|{}\n",
            op.timestamp, op.action, op.prev_head, op.new_head, op.description
        ));
    }
    fs::write(oplog_path, out)?;

    Ok(last)
}
