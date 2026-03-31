use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::cargo_workspace::WorkspaceGraph;
use crate::config::load_config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub repo_root: String,
    pub config_path: String,
    pub cargo_manifest_path: String,
    pub package_count: usize,
    pub packages: Vec<String>,
    pub issues: Vec<String>,
}

/// Check repo readiness, matching the reference `doctor()` function.
///
/// 1. Set repo_root, config_path, cargo_manifest_path
/// 2. Check if Cargo.toml exists — if not, add issue
/// 3. Check if proofrun.toml exists — if not, add issue with specific message
/// 4. Load config (using default fallback)
/// 5. Try to discover workspace — if successful, get package names; if failed, set packages to empty
/// 6. Check: no packages, no profiles, no surfaces, no rules
/// 7. Return DoctorReport
pub fn doctor_repo(repo_root: &Utf8Path) -> DoctorReport {
    let config_path = repo_root.join("proofrun.toml");
    let cargo_path = repo_root.join("Cargo.toml");

    // Load config (falls back to built-in default if proofrun.toml missing)
    let config = load_config(repo_root).unwrap_or_else(|_| {
        // If config loading fails entirely, parse the default
        toml::from_str(crate::config::DEFAULT_CONFIG_TOML)
            .expect("built-in default config must parse")
    });

    // Try to discover workspace packages
    let packages: Vec<String> = match WorkspaceGraph::discover(repo_root) {
        Ok(graph) => graph.packages.iter().map(|p| p.name.clone()).collect(),
        Err(_) => Vec::new(),
    };

    let mut issues = Vec::new();

    if !cargo_path.exists() {
        issues.push("missing Cargo.toml".to_owned());
    }
    if !config_path.exists() {
        issues.push("proofrun.toml missing; using built-in default config".to_owned());
    }
    if packages.is_empty() {
        issues.push("no Cargo packages discovered".to_owned());
    }
    if config.profiles.is_empty() {
        issues.push("no profiles configured".to_owned());
    }
    if config.surfaces.is_empty() {
        issues.push("no surfaces configured".to_owned());
    }
    if config.rules.is_empty() {
        issues.push("no rules configured".to_owned());
    }

    DoctorReport {
        repo_root: repo_root.to_string(),
        config_path: config_path.to_string(),
        cargo_manifest_path: cargo_path.to_string(),
        package_count: packages.len(),
        packages,
        issues,
    }
}
