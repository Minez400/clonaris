//! # Interactive Conflict Resolution Engine
//!
//! Parses and resolves Clonaris merge conflict blocks (`<<<<<<< CURRENT`, `=======`, `>>>>>>> TARGET`).
//! Provides both interactive prompts for terminal users and CLI flags (`--current`, `--target`) for automation.

use std::fs;
use std::io::{self, stdin, stdout, Write};
use std::path::Path;

/// Chosen strategy to reconcile a conflict block.
#[derive(Debug, PartialEq, Eq)]
pub enum ResolveChoice {
    /// Keep the current branch's lines.
    Current,
    /// Accept the target branch's lines.
    Target,
}

/// Checks whether file content contains unresolved Clonaris conflict markers.
pub fn has_conflicts(content: &str) -> bool {
    content.contains("<<<<<<< CURRENT") && content.contains("=======") && content.contains(">>>>>>> TARGET")
}

/// Replaces all conflict marker sections in `content` with the chosen side's lines.
pub fn resolve_conflict_markers(content: &str, choice: ResolveChoice) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].starts_with("<<<<<<< CURRENT") {
            let mut current_block = Vec::new();
            let mut target_block = Vec::new();
            let mut in_target = false;
            i += 1;

            while i < lines.len() && !lines[i].starts_with(">>>>>>> TARGET") {
                if lines[i] == "=======" {
                    in_target = true;
                } else if in_target {
                    target_block.push(lines[i]);
                } else {
                    current_block.push(lines[i]);
                }
                i += 1;
            }

            if i < lines.len() && lines[i].starts_with(">>>>>>> TARGET") {
                i += 1; // Skip closing marker
            }

            match choice {
                ResolveChoice::Current => {
                    for line in current_block {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                ResolveChoice::Target => {
                    for line in target_block {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        } else {
            out.push_str(lines[i]);
            out.push('\n');
            i += 1;
        }
    }

    out
}

/// Resolves conflicts in `file_path`, prompting the user interactively or applying `strategy_arg`.
pub fn prompt_resolve_file(file_path: &Path, strategy_arg: Option<&str>) -> io::Result<()> {
    if !file_path.exists() {
        eprintln!("Error: File '{}' was not found.", file_path.display());
        return Ok(());
    }

    let content = fs::read_to_string(file_path)?;
    if !has_conflicts(&content) {
        println!("✓ File '{}' has no pending conflict markers.", file_path.display());
        return Ok(());
    }

    let choice = match strategy_arg {
        Some("--current") | Some("-c") => ResolveChoice::Current,
        Some("--target") | Some("-t") => ResolveChoice::Target,
        _ => {
            println!("\x1b[1mConflict detected in:\x1b[0m \x1b[33m{}\x1b[0m", file_path.display());
            println!("How would you like to resolve this conflict?");
            println!("  [1] Keep current version (mine)");
            println!("  [2] Accept target branch version (theirs)");
            println!("  [3] Cancel");
            print!("Choose option [1/2/3]: ");
            stdout().flush()?;

            let mut input = String::new();
            stdin().read_line(&mut input)?;
            match input.trim() {
                "1" => ResolveChoice::Current,
                "2" => ResolveChoice::Target,
                _ => {
                    println!("Conflict resolution canceled.");
                    return Ok(());
                }
            }
        }
    };

    let resolved = resolve_conflict_markers(&content, choice);
    fs::write(file_path, resolved)?;

    println!("✓ Conflicts resolved successfully in '{}'!", file_path.display());
    println!("  Tip: Commit the resolution with: clonaris save -m \"Resolve conflict in {}\"", file_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_current() {
        let text = "before\n<<<<<<< CURRENT (main)\nmy line\n=======\ntheir line\n>>>>>>> TARGET (feature)\nafter\n";
        let res = resolve_conflict_markers(text, ResolveChoice::Current);
        assert_eq!(res, "before\nmy line\nafter\n");
    }

    #[test]
    fn test_resolve_target() {
        let text = "before\n<<<<<<< CURRENT (main)\nmy line\n=======\ntheir line\n>>>>>>> TARGET (feature)\nafter\n";
        let res = resolve_conflict_markers(text, ResolveChoice::Target);
        assert_eq!(res, "before\ntheir line\nafter\n");
    }
}
