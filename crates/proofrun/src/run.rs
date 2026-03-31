use crate::model::{Plan, Receipt, ReceiptStep};
use anyhow::{Context, Result};
use camino::Utf8Path;

const TOOL_VERSION: &str = "0.1.0-ref";

#[derive(Debug, Clone, Copy)]
pub enum ExecutionMode {
    Execute,
    DryRun,
}

pub fn execute_plan(repo_root: &Utf8Path, plan: &Plan, mode: ExecutionMode) -> Result<Receipt> {
    let output_dir = repo_root.join(&plan.artifacts.output_dir);
    let logs_dir = output_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)
        .with_context(|| format!("failed to create logs directory {logs_dir}"))?;

    let mut steps: Vec<ReceiptStep> = Vec::new();
    let mut overall_status = match mode {
        ExecutionMode::DryRun => "dry-run",
        ExecutionMode::Execute => "passed",
    };

    for (index_0, surface) in plan.selected_surfaces.iter().enumerate() {
        let index = index_0 + 1; // 1-based
        let surface_id = &surface.id;
        let stdout_rel = format!(
            "{}/logs/{:02}-{}.stdout.log",
            plan.artifacts.output_dir, index, surface_id
        );
        let stderr_rel = format!(
            "{}/logs/{:02}-{}.stderr.log",
            plan.artifacts.output_dir, index, surface_id
        );
        let stdout_path = repo_root.join(&stdout_rel);
        let stderr_path = repo_root.join(&stderr_rel);
        let command: Vec<String> = surface.run.clone();

        match mode {
            ExecutionMode::DryRun => {
                std::fs::write(&stdout_path, "")
                    .with_context(|| format!("failed to write {stdout_path}"))?;
                std::fs::write(&stderr_path, "")
                    .with_context(|| format!("failed to write {stderr_path}"))?;
                steps.push(ReceiptStep {
                    id: surface_id.clone(),
                    argv: command,
                    exit_code: 0,
                    duration_ms: 0,
                    stdout_path: stdout_rel,
                    stderr_path: stderr_rel,
                });
            }
            ExecutionMode::Execute => {
                let program = command.first().context("surface run command is empty")?;
                let args = &command[1..];

                let started = std::time::Instant::now();
                let output = std::process::Command::new(program)
                    .args(args)
                    .current_dir(repo_root.as_std_path())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output()
                    .with_context(|| format!("failed to spawn command: {}", command.join(" ")))?;
                let elapsed_ms = started.elapsed().as_millis() as u64;

                std::fs::write(&stdout_path, &output.stdout)
                    .with_context(|| format!("failed to write {stdout_path}"))?;
                std::fs::write(&stderr_path, &output.stderr)
                    .with_context(|| format!("failed to write {stderr_path}"))?;

                let exit_code = output.status.code().unwrap_or(1);
                steps.push(ReceiptStep {
                    id: surface_id.clone(),
                    argv: command,
                    exit_code,
                    duration_ms: elapsed_ms,
                    stdout_path: stdout_rel,
                    stderr_path: stderr_rel,
                });

                if exit_code != 0 {
                    overall_status = "failed";
                    break;
                }
            }
        }
    }

    let receipt = Receipt {
        version: TOOL_VERSION.to_string(),
        executed_at: crate::utc_now(),
        plan_digest: plan.plan_digest.clone(),
        status: overall_status.to_string(),
        steps,
    };

    // Write receipt.json with sorted keys, 2-space indent, trailing newline
    let receipt_value =
        serde_json::to_value(&receipt).context("failed to serialize receipt to JSON value")?;
    let receipt_json = serde_json::to_string_pretty(&receipt_value)
        .context("failed to serialize receipt to pretty JSON")?
        + "\n";
    let receipt_path = output_dir.join("receipt.json");
    std::fs::write(&receipt_path, &receipt_json)
        .with_context(|| format!("failed to write {receipt_path}"))?;

    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlanArtifacts, SelectedSurface, WorkspaceInfo};
    use proptest::prelude::*;

    /// Strategy for generating a random surface id (lowercase letters with optional dots).
    fn arb_surface_id() -> impl Strategy<Value = String> {
        "[a-z]{1,6}(\\.[a-z]{1,4}){0,2}"
    }

    /// Strategy for generating a random SelectedSurface with safe ids and run commands.
    fn arb_selected_surface() -> impl Strategy<Value = SelectedSurface> {
        (
            arb_surface_id(),
            prop::collection::vec("[a-z0-9]{1,8}", 1..=3),
        )
            .prop_map(|(id, run)| SelectedSurface {
                id: id.clone(),
                template: id,
                cost: 1.0,
                covers: vec![],
                run,
            })
    }

    /// Build a Plan with the given surfaces and an output_dir relative path.
    fn make_plan(surfaces: Vec<SelectedSurface>, output_dir: &str, plan_digest: &str) -> Plan {
        Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            repo_root: "/unused".to_string(),
            base: "aaa".to_string(),
            head: "bbb".to_string(),
            merge_base: "aaa".to_string(),
            profile: "ci".to_string(),
            config_digest: String::new(),
            plan_digest: plan_digest.to_string(),
            artifacts: PlanArtifacts {
                output_dir: output_dir.to_string(),
                diff_patch: format!("{output_dir}/diff.patch"),
                plan_json: format!("{output_dir}/plan.json"),
                plan_markdown: format!("{output_dir}/plan.md"),
                commands_shell: format!("{output_dir}/commands.sh"),
                github_actions: format!("{output_dir}/github-actions.yml"),
            },
            workspace: WorkspaceInfo { packages: vec![] },
            changed_paths: vec![],
            obligations: vec![],
            selected_surfaces: surfaces,
            omitted_surfaces: vec![],
            diagnostics: vec![],
        }
    }

    // Feature: rust-native-planner, Property 13: Dry-run receipt invariants
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(50))]

        /// **Validates: Requirements 12.1, 12.6, 12.7, 12.8**
        ///
        /// For any Plan executed in dry-run mode, the resulting Receipt has
        /// status "dry-run", one step per selected surface with exit_code 0
        /// and duration_ms 0, correct log file paths, and plan_digest matching
        /// the input plan.
        #[test]
        fn prop_dry_run_receipt_invariants(
            surfaces in prop::collection::vec(arb_selected_surface(), 1..=5),
            plan_digest in "[a-f0-9]{64}",
        ) {
            let tmp = tempfile::tempdir().expect("failed to create temp dir");
            let repo_root = camino::Utf8Path::from_path(tmp.path())
                .expect("temp dir is not valid UTF-8");
            let output_dir = ".proofrun";

            let plan = make_plan(surfaces.clone(), output_dir, &plan_digest);
            let receipt = execute_plan(repo_root, &plan, ExecutionMode::DryRun)
                .expect("dry-run should not fail");

            // (a) Receipt status is "dry-run"
            prop_assert_eq!(&receipt.status, "dry-run");

            // (b) One step per selected surface
            prop_assert_eq!(
                receipt.steps.len(),
                surfaces.len(),
                "expected one step per surface"
            );

            for (i, (step, surface)) in receipt.steps.iter().zip(surfaces.iter()).enumerate() {
                let index = i + 1; // 1-based

                // Step id matches surface id
                prop_assert_eq!(&step.id, &surface.id);

                // (c) exit_code 0 and duration_ms 0
                prop_assert_eq!(step.exit_code, 0, "dry-run step exit_code must be 0");
                prop_assert_eq!(step.duration_ms, 0, "dry-run step duration_ms must be 0");

                // (d) Log file paths match expected format
                let expected_stdout = format!(
                    "{}/logs/{:02}-{}.stdout.log",
                    output_dir, index, surface.id
                );
                let expected_stderr = format!(
                    "{}/logs/{:02}-{}.stderr.log",
                    output_dir, index, surface.id
                );
                prop_assert_eq!(
                    &step.stdout_path, &expected_stdout,
                    "stdout path mismatch for step {}", index
                );
                prop_assert_eq!(
                    &step.stderr_path, &expected_stderr,
                    "stderr path mismatch for step {}", index
                );

                // (f) Log files exist and are empty
                let stdout_abs = repo_root.join(&step.stdout_path);
                let stderr_abs = repo_root.join(&step.stderr_path);
                let stdout_content = std::fs::read_to_string(&stdout_abs)
                    .expect("stdout log file should exist");
                let stderr_content = std::fs::read_to_string(&stderr_abs)
                    .expect("stderr log file should exist");
                prop_assert!(
                    stdout_content.is_empty(),
                    "stdout log should be empty in dry-run"
                );
                prop_assert!(
                    stderr_content.is_empty(),
                    "stderr log should be empty in dry-run"
                );
            }

            // (e) Receipt plan_digest matches input plan's plan_digest
            prop_assert_eq!(
                &receipt.plan_digest, &plan_digest,
                "receipt plan_digest must match input plan"
            );
        }
    }
}
