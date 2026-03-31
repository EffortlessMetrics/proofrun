use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRange {
    pub base: String,
    pub head: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitChange {
    pub path: String,
    pub status: String,
}

pub fn collect_git_changes(repo_root: &Utf8Path, range: &GitRange) -> Result<Vec<GitChange>> {
    let output = Command::new("git")
        .args(["-C", repo_root.as_str(), "diff", "--name-status", &range.base, &range.head])
        .output()
        .context("failed to invoke git diff")?;

    if !output.status.success() {
        anyhow::bail!("git diff failed");
    }

    let stdout = String::from_utf8(output.stdout).context("git diff output was not utf-8")?;
    let mut changes = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() >= 2 {
            changes.push(GitChange {
                status: parts[0].to_owned(),
                path: parts[parts.len() - 1].to_owned(),
            });
        }
    }

    Ok(changes)
}
