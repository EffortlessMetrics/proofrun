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
    /// Compile a proof plan from a change source.
    Plan {
        #[arg(long)]
        base: Option<String>,
        #[arg(long)]
        head: Option<String>,
        #[arg(long, default_value = "ci")]
        profile: String,
        #[arg(long, default_value = ".")]
        repo: Utf8PathBuf,
        #[arg(long)]
        staged: bool,
        #[arg(long)]
        working_tree: bool,
        #[arg(long)]
        paths_from_stdin: bool,
        // Budget gates
        #[arg(long)]
        max_cost: Option<f64>,
        #[arg(long)]
        max_surfaces: Option<usize>,
        #[arg(long)]
        fail_on_fallback: bool,
        #[arg(long)]
        warn_on_workspace_smoke_only: bool,
    },
    /// Render a plan summary as Markdown, or query path/obligation/surface traceability.
    Explain {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        obligation: Option<String>,
        #[arg(long)]
        surface: Option<String>,
    },
    /// Output full traceability chain as JSON.
    Trace {
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
        #[arg(long)]
        resume: Option<Utf8PathBuf>,
        #[arg(long)]
        failed_only: bool,
        #[arg(long)]
        receipt: Option<Utf8PathBuf>,
    },
    /// Compare two plans and show structural differences.
    Compare {
        old_plan: Utf8PathBuf,
        new_plan: Utf8PathBuf,
    },
    /// Check repo readiness.
    Doctor {
        #[arg(long, default_value = ".")]
        repo: Utf8PathBuf,
        #[arg(long)]
        strict: bool,
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
    /// Emit CI matrix JSON.
    Matrix {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
    },
    /// Emit structured JSON.
    Json {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
    },
    /// Emit nextest filterset expressions.
    NextestFiltersets {
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

fn load_receipt(receipt_path: &Utf8PathBuf) -> Result<proofrun::Receipt> {
    let raw = std::fs::read_to_string(receipt_path)
        .with_context(|| format!("failed to read receipt file {receipt_path}"))?;
    let receipt: proofrun::Receipt = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse receipt file {receipt_path}"))?;
    Ok(receipt)
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
            staged,
            working_tree,
            paths_from_stdin,
            max_cost,
            max_surfaces,
            fail_on_fallback,
            warn_on_workspace_smoke_only,
        } => {
            let has_git_range = base.is_some() || head.is_some();

            // Validate exactly one change source
            if let Err(msg) = proofrun::validate_change_source_flags(
                has_git_range,
                staged,
                working_tree,
                paths_from_stdin,
            ) {
                eprintln!("{msg}");
                std::process::exit(1);
            }

            // Map flags to ChangeSource
            let source = if has_git_range {
                let base = base.expect("--base is required when using git range");
                let head = head.expect("--head is required when using git range");
                proofrun::ChangeSource::GitRange(proofrun::GitRange {
                    base: base.clone(),
                    head: head.clone(),
                })
            } else if staged {
                proofrun::ChangeSource::Staged
            } else if working_tree {
                proofrun::ChangeSource::WorkingTree
            } else {
                // paths_from_stdin
                let lines: Vec<String> = std::io::BufRead::lines(std::io::stdin().lock())
                    .collect::<std::io::Result<Vec<String>>>()
                    .context("failed to read paths from stdin")?;
                proofrun::ChangeSource::PathsFromStdin(lines)
            };

            // For git range, also collect changes for write_plan_artifacts
            let patch = if let proofrun::ChangeSource::GitRange(ref range) = source {
                let git_changes = proofrun::collect_git_changes(&repo, range)?;
                Some(git_changes.patch)
            } else {
                None
            };

            let plan = proofrun::plan_from_source(&repo, source, &profile)?;

            if let Some(patch) = patch {
                proofrun::write_plan_artifacts(&repo, &plan, &patch)?;
            }

            print_json_sorted(&plan)?;

            // Budget gates — always output plan first, then check gates
            let gate_result = proofrun::check_budget_gates(
                &plan,
                max_cost,
                max_surfaces,
                fail_on_fallback,
                warn_on_workspace_smoke_only,
            );
            for msg in &gate_result.messages {
                eprintln!("{msg}");
            }
            if gate_result.failed {
                std::process::exit(1);
            }
        }
        Command::Explain {
            plan: plan_path,
            path,
            obligation,
            surface,
        } => {
            let plan = load_plan(&plan_path)?;
            if let Some(p) = path {
                let result = proofrun::query_path(&plan, &p);
                print_json_sorted(&result)?;
            } else if let Some(id) = obligation {
                let result = proofrun::query_obligation(&plan, &id)?;
                print_json_sorted(&result)?;
            } else if let Some(id) = surface {
                let result = proofrun::query_surface(&plan, &id)?;
                print_json_sorted(&result)?;
            } else {
                print!("{}", proofrun::emit_plan_markdown(&plan));
            }
        }
        Command::Trace { plan: plan_path } => {
            let plan = load_plan(&plan_path)?;
            let result = proofrun::trace_plan(&plan);
            print_json_sorted(&result)?;
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
            EmitKind::Matrix { plan: plan_path } => {
                let plan = load_plan(&plan_path)?;
                print!("{}", proofrun::emit_matrix_json(&plan));
            }
            EmitKind::Json { plan: plan_path } => {
                let plan = load_plan(&plan_path)?;
                print!("{}", proofrun::emit_structured_json(&plan));
            }
            EmitKind::NextestFiltersets { plan: plan_path } => {
                let plan = load_plan(&plan_path)?;
                print!("{}", proofrun::emit_nextest_filtersets(&plan));
            }
        },
        Command::Run {
            plan: plan_path,
            dry_run,
            resume,
            failed_only,
            receipt,
        } => {
            let plan = load_plan(&plan_path)?;
            let mode = if dry_run {
                proofrun::ExecutionMode::DryRun
            } else {
                proofrun::ExecutionMode::Execute
            };
            let repo_root = camino::Utf8PathBuf::from(&plan.repo_root);

            let result_receipt = if let Some(resume_path) = resume {
                let previous_receipt = load_receipt(&resume_path)?;
                proofrun::execute_with_resume(&repo_root, &plan, &previous_receipt, mode)?
            } else if failed_only {
                let receipt_path = receipt.ok_or_else(|| {
                    anyhow::anyhow!("--receipt <path> is required when using --failed-only")
                })?;
                let previous_receipt = load_receipt(&receipt_path)?;
                proofrun::execute_failed_only(&repo_root, &plan, &previous_receipt, mode)?
            } else {
                proofrun::execute_plan(&repo_root, &plan, mode)?
            };
            print_json_sorted(&result_receipt)?;
        }
        Command::Compare { old_plan, new_plan } => {
            let old = load_plan(&old_plan)?;
            let new = load_plan(&new_plan)?;
            let comparison = proofrun::compare_plans(&old, &new);
            print_json_sorted(&comparison)?;
        }
        Command::Doctor { repo, strict } => {
            let report = proofrun::doctor_repo(&repo);
            print_json_sorted(&report)?;
            if strict && proofrun::should_fail_strict(&report) {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
