// Line-by-line Diff engine with LCS (Longest Common Subsequence)
// Pure Rust, zero external dependencies.

#[derive(Debug, PartialEq, Eq)]
pub enum DiffLine {
    Equal(String),
    Insert(String),
    Delete(String),
}

pub fn is_binary(bytes: &[u8]) -> bool {
    // Check first 8000 bytes for null character
    let sample = &bytes[..bytes.len().min(8000)];
    sample.contains(&0)
}

pub fn compute_diff(old_content: &str, new_content: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let n = old_lines.len();
    let m = new_lines.len();

    // If both files are identical
    if old_content == new_content {
        return old_lines.into_iter().map(|s| DiffLine::Equal(s.to_string())).collect();
    }

    // DP table for LCS
    let mut dp = vec![vec![0u32; m + 1]; n + 1];

    for i in 0..n {
        for j in 0..m {
            if old_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    // Backtrack to find diff
    let mut diff = Vec::new();
    let mut i = n;
    let mut j = m;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            diff.push(DiffLine::Equal(old_lines[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            diff.push(DiffLine::Insert(new_lines[j - 1].to_string()));
            j -= 1;
        } else if i > 0 && (j == 0 || dp[i][j - 1] < dp[i - 1][j]) {
            diff.push(DiffLine::Delete(old_lines[i - 1].to_string()));
            i -= 1;
        }
    }

    diff.reverse();
    diff
}

pub fn format_colored_diff(file_path: &str, diff_lines: &[DiffLine]) -> String {
    let mut out = String::new();
    out.push_str(&format!("\x1b[1;37m--- a/{}\x1b[0m\n", file_path));
    out.push_str(&format!("\x1b[1;37m+++ b/{}\x1b[0m\n", file_path));

    let mut has_changes = false;
    for line in diff_lines {
        match line {
            DiffLine::Insert(text) => {
                has_changes = true;
                out.push_str(&format!("\x1b[32m+ {}\x1b[0m\n", text));
            }
            DiffLine::Delete(text) => {
                has_changes = true;
                out.push_str(&format!("\x1b[31m- {}\x1b[0m\n", text));
            }
            DiffLine::Equal(text) => {
                out.push_str(&format!("  {}\n", text));
            }
        }
    }

    if !has_changes {
        return String::new();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_insertion_deletion() {
        let old = "linha 1\nlinha 2\nlinha 3";
        let new = "linha 1\nlinha modificada\nlinha 3\nlinha 4";
        let diff = compute_diff(old, new);

        assert!(diff.contains(&DiffLine::Delete("linha 2".to_string())));
        assert!(diff.contains(&DiffLine::Insert("linha modificada".to_string())));
        assert!(diff.contains(&DiffLine::Insert("linha 4".to_string())));
    }
}
