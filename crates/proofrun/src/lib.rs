pub mod cargo_workspace;
pub mod config;
pub mod doctor;
pub mod emit;
pub mod explain;
pub mod git;
pub mod model;
pub mod obligations;
pub mod planner;
pub mod run;

pub use cargo_workspace::{PackageInfo, WorkspaceGraph};
pub use config::{load_config, Config, SurfaceTemplate, Rule};
pub use doctor::{doctor_repo, DoctorReport};
pub use emit::{emit_commands_shell, emit_github_actions, emit_plan_markdown};
pub use explain::render_explanation;
pub use git::{collect_git_changes, GitChange, GitRange};
pub use model::{Plan, PlanArtifacts, Receipt, SelectedSurface};
pub use obligations::{compile_obligations, Obligation};
pub use planner::{build_candidates, solve_plan, CandidateSurface};
pub use run::{execute_plan, ExecutionMode};

use anyhow::Result;
use camino::Utf8Path;

pub fn plan_repo(repo_root: &Utf8Path, range: GitRange, profile: &str) -> Result<Plan> {
    let _config = load_config(repo_root)?;
    let _changes = collect_git_changes(repo_root, &range)?;
    let _workspace = WorkspaceGraph::discover(repo_root)?;
    let _ = profile;

    anyhow::bail!(
        "Rust implementation scaffolded but not yet ported; use reference/proofrun_ref.py for the runnable implementation."
    )
}
