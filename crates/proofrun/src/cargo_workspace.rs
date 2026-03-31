use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub dir: Utf8PathBuf,
    pub manifest: Utf8PathBuf,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceGraph {
    pub packages: Vec<PackageInfo>,
}

impl WorkspaceGraph {
    pub fn discover(repo_root: &Utf8Path) -> Result<Self> {
        let _ = repo_root;
        anyhow::bail!(
            "Rust workspace discovery scaffolded but not yet implemented; planned backend is cargo metadata --format-version 1."
        )
    }

    pub fn owner_for_path(&self, _path: &Utf8Path) -> Option<&PackageInfo> {
        None
    }
}
