// Line-by-line Diff engine with LCS and common prefix/suffix optimization
// Pure Rust, zero external dependencies.

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DiffLine {
    Equal(String),
    Insert(String),
    Delete(String),
}

pub fn is_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8000)];
    sample.contains(&0)
}

pub fn compute_diff(old_content: &str, new_content: &str) -> Vec<DiffLine> {
    if old_content == new_content {
        return old_content
            .lines()
            .map(|s| DiffLine::Equal(s.to_string()))
            .collect();
    }

    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let n = old_lines.len();
    let m = new_lines.len();

    // Optimization: Trim common prefix
    let mut start = 0;
    while start < n && start < m && old_lines[start] == new_lines[start] {
        start += 1;
    }

    // Optimization: Trim common suffix
    let mut old_end = n;
    let mut new_end = m;
    while old_end > start && new_end > start && old_lines[old_end - 1] == new_lines[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }

    let mut result = Vec::with_capacity(n + m);

    // 1. Add common prefix
    for line in &old_lines[..start] {
        result.push(DiffLine::Equal((*line).to_string()));
    }

    let mid_old = &old_lines[start..old_end];
    let mid_new = &new_lines[start..new_end];
    let mid_n = mid_old.len();
    let mid_m = mid_new.len();

    // If remaining middle is too large (> 3000 x 3000) fallback to direct delete + insert to avoid OOM
    if mid_n * mid_m > 9_000_000 {
        for line in mid_old {
            result.push(DiffLine::Delete((*line).to_string()));
        }
        for line in mid_new {
            result.push(DiffLine::Insert((*line).to_string()));
        }
    } else if mid_n > 0 || mid_m > 0 {
        // LCS DP Table on middle slice
        let mut dp = vec![vec![0u32; mid_m + 1]; mid_n + 1];

        for i in 0..mid_n {
            for j in 0..mid_m {
                if mid_old[i] == mid_new[j] {
                    dp[i + 1][j + 1] = dp[i][j] + 1;
                } else {
                    dp[i + 1][j + 1] = dp[i][j + 1].max(dp[i + 1][j]);
                }
            }
        }

        let mut mid_diff = Vec::new();
        let mut i = mid_n;
        let mut j = mid_m;

        while i > 0 || j > 0 {
            if i > 0 && j > 0 && mid_old[i - 1] == mid_new[j - 1] {
                mid_diff.push(DiffLine::Equal(mid_old[i - 1].to_string()));
                i -= 1;
                j -= 1;
            } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
                mid_diff.push(DiffLine::Insert(mid_new[j - 1].to_string()));
                j -= 1;
            } else if i > 0 && (j == 0 || dp[i][j - 1] < dp[i - 1][j]) {
                mid_diff.push(DiffLine::Delete(mid_old[i - 1].to_string()));
                i -= 1;
            }
        }

        mid_diff.reverse();
        result.extend(mid_diff);
    }

    // 3. Add common suffix
    for line in &old_lines[old_end..n] {
        result.push(DiffLine::Equal((*line).to_string()));
    }

    result
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

    #[test]
    fn test_diff_prefix_suffix_optimization() {
        let old = "header\na\nb\nc\nfooter";
        let new = "header\na\nMODIFICADO\nc\nfooter";
        let diff = compute_diff(old, new);

        assert_eq!(diff[0], DiffLine::Equal("header".to_string()));
        assert_eq!(diff[1], DiffLine::Equal("a".to_string()));
        assert_eq!(diff[2], DiffLine::Delete("b".to_string()));
        assert_eq!(diff[3], DiffLine::Insert("MODIFICADO".to_string()));
        assert_eq!(diff[4], DiffLine::Equal("c".to_string()));
        assert_eq!(diff[5], DiffLine::Equal("footer".to_string()));
    }
}
