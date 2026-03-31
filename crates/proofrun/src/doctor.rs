use camino::Utf8Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub repo_root: String,
    pub config_present: bool,
    pub cargo_manifest_present: bool,
    pub issues: Vec<String>,
}

pub fn doctor_repo(repo_root: &Utf8Path) -> DoctorReport {
    let config_present = repo_root.join("proofrun.toml").exists();
    let cargo_manifest_present = repo_root.join("Cargo.toml").exists();

    let mut issues = Vec::new();
    if !config_present {
        issues.push("missing proofrun.toml".to_owned());
    }
    if !cargo_manifest_present {
        issues.push("missing Cargo.toml".to_owned());
    }

    DoctorReport {
        repo_root: repo_root.to_string(),
        config_present,
        cargo_manifest_present,
        issues,
    }
}
