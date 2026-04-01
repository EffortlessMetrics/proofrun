use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::model::ChangedPath;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRange {
    pub base: String,
    pub head: String,
}

#[derive(Debug, Clone)]
pub struct GitChanges {
    pub merge_base: String,
    pub changes: Vec<ChangedPath>,
    pub patch: String,
}

/// Run a git command in `repo_root`, returning stdout on success or a
/// descriptive error (including stderr) on failure.
fn git_capture(repo_root: &Utf8Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(["-C", repo_root.as_str()])
        .args(args)
        .output()
        .with_context(|| format!("failed to invoke git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    String::from_utf8(output.stdout)
        .with_context(|| format!("git {} output was not utf-8", args.join(" ")))
}

/// Parse a single line from `git diff --name-status` output into a `ChangedPath`.
///
/// Handles M, A, D statuses (single path) and R/C statuses (two paths,
/// destination is used, status normalized to single letter).
/// Returns `None` for blank or unparseable lines.
pub fn parse_name_status_line(line: &str) -> Option<ChangedPath> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split('\t').collect();
    let status = parts.first()?;

    if (status.starts_with('R') || status.starts_with('C')) && parts.len() >= 3 {
        // Rename or copy: take destination path, normalize status to single letter
        Some(ChangedPath {
            path: parts[2].to_owned(),
            status: status[..1].to_owned(),
            owner: None,
        })
    } else if parts.len() >= 2 {
        Some(ChangedPath {
            path: parts[1].to_owned(),
            status: (*status).to_owned(),
            owner: None,
        })
    } else {
        None
    }
}

/// Collect git changes between two revisions.
///
/// 1. Resolves the merge-base between `range.base` and `range.head`.
/// 2. Parses `git diff --name-status` into `Vec<ChangedPath>`.
/// 3. Captures the full binary diff as the patch string.
pub fn collect_git_changes(repo_root: &Utf8Path, range: &GitRange) -> Result<GitChanges> {
    // 1. Resolve merge-base
    let merge_base = git_capture(repo_root, &["merge-base", &range.base, &range.head])?
        .trim()
        .to_owned();

    // 2. Parse name-status diff
    let name_status = git_capture(
        repo_root,
        &["diff", "--name-status", &merge_base, &range.head],
    )?;

    let changes: Vec<ChangedPath> = name_status
        .lines()
        .filter_map(parse_name_status_line)
        .collect();

    // 3. Capture binary diff as patch
    let patch = git_capture(repo_root, &["diff", "--binary", &merge_base, &range.head])?;

    Ok(GitChanges {
        merge_base,
        changes,
        patch,
    })
}

/// Get the current HEAD commit SHA.
pub fn head_sha(repo_root: &Utf8Path) -> Result<String> {
    let sha = git_capture(repo_root, &["rev-parse", "HEAD"])?;
    Ok(sha.trim().to_owned())
}

/// Collect staged (indexed) changes.
///
/// Paths from `git diff --name-status --cached`, patch from
/// `git diff --cached --binary`.  Sets `base = HEAD_SHA`,
/// `head = "STAGED"`, `merge_base = HEAD_SHA`.
pub fn collect_staged_changes(repo_root: &Utf8Path) -> Result<GitChanges> {
    let sha = head_sha(repo_root)?;

    let name_status = git_capture(repo_root, &["diff", "--name-status", "--cached"])?;
    let changes: Vec<ChangedPath> = name_status
        .lines()
        .filter_map(parse_name_status_line)
        .collect();

    let patch = git_capture(repo_root, &["diff", "--cached", "--binary"])?;

    Ok(GitChanges {
        merge_base: sha,
        changes,
        patch,
    })
}

/// Collect working tree changes (staged + unstaged vs HEAD).
///
/// Paths from `git diff --name-status HEAD`, patch from
/// `git diff HEAD --binary`.  Sets `base = HEAD_SHA`,
/// `head = "WORKING_TREE"`, `merge_base = HEAD_SHA`.
pub fn collect_working_tree_changes(repo_root: &Utf8Path) -> Result<GitChanges> {
    let sha = head_sha(repo_root)?;

    let name_status = git_capture(repo_root, &["diff", "--name-status", "HEAD"])?;
    let changes: Vec<ChangedPath> = name_status
        .lines()
        .filter_map(parse_name_status_line)
        .collect();

    let patch = git_capture(repo_root, &["diff", "HEAD", "--binary"])?;

    Ok(GitChanges {
        merge_base: sha,
        changes,
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: rust-native-planner, Property 16: Git name-status parsing
    // **Validates: Requirements 2.2**

    /// Strategy: generate a single path segment of 1-8 chars from [a-z0-9_.]
    fn path_segment() -> impl Strategy<Value = String> {
        proptest::string::string_regex("[a-z0-9_.]{1,8}").unwrap()
    }

    /// Strategy: generate a file path of 1-4 segments joined by '/'
    fn file_path() -> impl Strategy<Value = String> {
        proptest::collection::vec(path_segment(), 1..=4).prop_map(|segs| segs.join("/"))
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        #[test]
        fn prop_name_status_mad(
            status in prop_oneof![Just("M"), Just("A"), Just("D")],
            path in file_path(),
        ) {
            let line = format!("{}\t{}", status, path);
            let cp = parse_name_status_line(&line)
                .expect("should parse a well-formed M/A/D line");
            prop_assert_eq!(&cp.status, &status);
            prop_assert_eq!(&cp.path, &path);
            prop_assert!(cp.owner.is_none());
        }

        #[test]
        fn prop_name_status_rename_copy(
            status_char in prop_oneof![Just('R'), Just('C')],
            score in 0u32..=100,
            old_path in file_path(),
            new_path in file_path(),
        ) {
            let line = format!("{}{:03}\t{}\t{}", status_char, score, old_path, new_path);
            let cp = parse_name_status_line(&line)
                .expect("should parse a well-formed R/C line");
            prop_assert_eq!(&cp.status, &status_char.to_string());
            prop_assert_eq!(&cp.path, &new_path, "R/C should use destination path");
            prop_assert!(cp.owner.is_none());
        }
    }

    #[test]
    fn parse_modified() {
        let line = "M\tsrc/main.rs";
        let cp = parse_name_status_line(line).unwrap();
        assert_eq!(cp.status, "M");
        assert_eq!(cp.path, "src/main.rs");
        assert!(cp.owner.is_none());
    }

    #[test]
    fn parse_added() {
        let line = "A\tnew_file.txt";
        let cp = parse_name_status_line(line).unwrap();
        assert_eq!(cp.status, "A");
        assert_eq!(cp.path, "new_file.txt");
    }

    #[test]
    fn parse_deleted() {
        let line = "D\told_file.txt";
        let cp = parse_name_status_line(line).unwrap();
        assert_eq!(cp.status, "D");
        assert_eq!(cp.path, "old_file.txt");
    }

    #[test]
    fn parse_rename_takes_destination() {
        let line = "R100\told/path.rs\tnew/path.rs";
        let cp = parse_name_status_line(line).unwrap();
        assert_eq!(cp.status, "R");
        assert_eq!(cp.path, "new/path.rs");
    }

    #[test]
    fn parse_copy_takes_destination() {
        let line = "C095\tsrc/a.rs\tsrc/b.rs";
        let cp = parse_name_status_line(line).unwrap();
        assert_eq!(cp.status, "C");
        assert_eq!(cp.path, "src/b.rs");
    }

    #[test]
    fn parse_empty_line_returns_none() {
        assert!(parse_name_status_line("").is_none());
        assert!(parse_name_status_line("   ").is_none());
    }

    #[test]
    fn parse_malformed_line_returns_none() {
        // No tab separator — single token
        assert!(parse_name_status_line("M").is_none());
    }
}
