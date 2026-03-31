use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedPath {
    pub path: String,
    pub status: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObligationReason {
    pub source: String,
    pub path: Option<String>,
    pub rule: Option<String>,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObligationRecord {
    pub id: String,
    pub reasons: Vec<ObligationReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedSurface {
    pub id: String,
    pub template: String,
    pub cost: f64,
    pub covers: Vec<String>,
    pub run: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmittedSurface {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanArtifacts {
    pub output_dir: String,
    pub diff_patch: String,
    pub plan_json: String,
    pub plan_markdown: String,
    pub commands_shell: String,
    pub github_actions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub packages: Vec<WorkspacePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePackage {
    pub name: String,
    pub dir: String,
    pub manifest: String,
    pub dependencies: Vec<String>,
    pub reverse_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub version: String,
    pub created_at: String,
    pub repo_root: String,
    pub base: String,
    pub head: String,
    pub merge_base: String,
    pub profile: String,
    pub config_digest: String,
    pub plan_digest: String,
    pub artifacts: PlanArtifacts,
    pub workspace: WorkspaceInfo,
    pub changed_paths: Vec<ChangedPath>,
    pub obligations: Vec<ObligationRecord>,
    pub selected_surfaces: Vec<SelectedSurface>,
    pub omitted_surfaces: Vec<OmittedSurface>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptStep {
    pub id: String,
    pub argv: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout_path: String,
    pub stderr_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub version: String,
    pub executed_at: String,
    pub plan_digest: String,
    pub status: String,
    pub steps: Vec<ReceiptStep>,
}
