use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "cargo-proofrun")]
#[command(about = "Deterministic proof-plan compiler for Cargo workspaces")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile a proof plan from a Git change range.
    Plan {
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, default_value = "ci")]
        profile: String,
        #[arg(long, default_value = ".")]
        repo: Utf8PathBuf,
    },
    /// Render a plan summary as Markdown.
    Explain {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
    },
    /// Emit derived artifacts from an existing plan.
    Emit {
        #[command(subcommand)]
        emit_kind: EmitKind,
    },
    /// Execute or dry-run a plan.
    Run {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Check repo readiness.
    Doctor {
        #[arg(long, default_value = ".")]
        repo: Utf8PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum EmitKind {
    /// Emit a shell script.
    Shell {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
    },
    /// Emit a GitHub Actions step.
    GithubActions {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
    },
}

fn load_plan(plan_path: &Utf8PathBuf) -> Result<proofrun::Plan> {
    let raw = std::fs::read_to_string(plan_path)
        .with_context(|| format!("failed to read plan file {plan_path}"))?;
    let plan: proofrun::Plan = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse plan file {plan_path}"))?;
    Ok(plan)
}

fn print_json_sorted(value: &impl serde::Serialize) -> Result<()> {
    let json_value = serde_json::to_value(value).context("failed to serialize to JSON value")?;
    let pretty =
        serde_json::to_string_pretty(&json_value).context("failed to serialize to pretty JSON")?;
    println!("{pretty}");
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Plan {
            base,
            head,
            profile,
            repo,
        } => {
            let range = proofrun::GitRange {
                base: base.clone(),
                head: head.clone(),
            };
            // Collect git changes to get the patch for write_plan_artifacts
            let git_changes = proofrun::collect_git_changes(&repo, &range)?;
            let plan = proofrun::plan_repo(&repo, range, &profile)?;
            proofrun::write_plan_artifacts(&repo, &plan, &git_changes.patch)?;
            print_json_sorted(&plan)?;
        }
        Command::Explain { plan: plan_path } => {
            let plan = load_plan(&plan_path)?;
            print!("{}", proofrun::emit_plan_markdown(&plan));
        }
        Command::Emit { emit_kind } => match emit_kind {
            EmitKind::Shell { plan: plan_path } => {
                let plan = load_plan(&plan_path)?;
                print!("{}", proofrun::emit_commands_shell(&plan));
            }
            EmitKind::GithubActions { plan: plan_path } => {
                let plan = load_plan(&plan_path)?;
                print!("{}", proofrun::emit_github_actions(&plan));
            }
        },
        Command::Run {
            plan: plan_path,
            dry_run,
        } => {
            let plan = load_plan(&plan_path)?;
            let mode = if dry_run {
                proofrun::ExecutionMode::DryRun
            } else {
                proofrun::ExecutionMode::Execute
            };
            // Derive repo_root from the plan's repo_root field, or use current dir
            let repo_root = camino::Utf8PathBuf::from(&plan.repo_root);
            let receipt = proofrun::execute_plan(&repo_root, &plan, mode)?;
            print_json_sorted(&receipt)?;
        }
        Command::Doctor { repo } => {
            let report = proofrun::doctor_repo(&repo);
            print_json_sorted(&report)?;
        }
    }

    Ok(())
}
