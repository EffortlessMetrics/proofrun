use crate::model::{Plan, Receipt, ReceiptStep};
use anyhow::{Context, Result};
use camino::Utf8Path;
use std::collections::BTreeMap;

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

/// Execute a plan, resuming from a previous receipt.
/// Skips steps with exit_code 0 in the previous receipt; re-executes the rest.
pub fn execute_with_resume(
    repo_root: &Utf8Path,
    plan: &Plan,
    previous_receipt: &Receipt,
    mode: ExecutionMode,
) -> Result<Receipt> {
    // Verify plan digest match
    if previous_receipt.plan_digest != plan.plan_digest {
        anyhow::bail!(
            "plan digest mismatch: receipt has '{}', plan has '{}'",
            previous_receipt.plan_digest,
            plan.plan_digest
        );
    }

    let output_dir = repo_root.join(&plan.artifacts.output_dir);
    let logs_dir = output_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)
        .with_context(|| format!("failed to create logs directory {logs_dir}"))?;

    // Build a lookup of previous steps by surface id
    let prev_steps: BTreeMap<&str, &ReceiptStep> = previous_receipt
        .steps
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();

    let mut steps: Vec<ReceiptStep> = Vec::new();
    let mut overall_status = match mode {
        ExecutionMode::DryRun => "dry-run",
        ExecutionMode::Execute => "passed",
    };

    for (index_0, surface) in plan.selected_surfaces.iter().enumerate() {
        let index = index_0 + 1; // 1-based
        let surface_id = &surface.id;

        // Check if previous receipt has a passing step for this surface
        if let Some(prev_step) = prev_steps.get(surface_id.as_str()) {
            if prev_step.exit_code == 0 {
                // Carry forward the previous step data
                steps.push((*prev_step).clone());
                continue;
            }
        }

        // Execute fresh (same logic as execute_plan)
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

/// Execute only the failed steps from a previous receipt.
/// Carries forward passed steps, re-executes failed steps, skips steps not in the previous receipt.
pub fn execute_failed_only(
    repo_root: &Utf8Path,
    plan: &Plan,
    previous_receipt: &Receipt,
    mode: ExecutionMode,
) -> Result<Receipt> {
    // Verify plan digest match
    if previous_receipt.plan_digest != plan.plan_digest {
        anyhow::bail!(
            "plan digest mismatch: receipt has '{}', plan has '{}'",
            previous_receipt.plan_digest,
            plan.plan_digest
        );
    }

    let output_dir = repo_root.join(&plan.artifacts.output_dir);
    let logs_dir = output_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)
        .with_context(|| format!("failed to create logs directory {logs_dir}"))?;

    // Build a lookup of previous steps by surface id
    let prev_steps: BTreeMap<&str, &ReceiptStep> = previous_receipt
        .steps
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();

    let mut steps: Vec<ReceiptStep> = Vec::new();
    let mut overall_status = match mode {
        ExecutionMode::DryRun => "dry-run",
        ExecutionMode::Execute => "passed",
    };

    for (index_0, surface) in plan.selected_surfaces.iter().enumerate() {
        let index = index_0 + 1; // 1-based
        let surface_id = &surface.id;

        // Look up in previous receipt
        let prev_step = match prev_steps.get(surface_id.as_str()) {
            Some(step) => step,
            None => {
                // Not found in previous receipt — skip entirely
                continue;
            }
        };

        // If previously passed, carry forward
        if prev_step.exit_code == 0 {
            steps.push((*prev_step).clone());
            continue;
        }

        // Previously failed — execute fresh
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

    /// Strategy for generating a Vec of SelectedSurfaces with unique ids.
    /// Uses a fixed prefix + index to guarantee uniqueness.
    fn arb_unique_surfaces(
        count: std::ops::RangeInclusive<usize>,
    ) -> impl Strategy<Value = Vec<SelectedSurface>> {
        prop::collection::vec(prop::collection::vec("[a-z0-9]{1,8}", 1..=3), count).prop_map(
            |runs| {
                runs.into_iter()
                    .enumerate()
                    .map(|(i, run)| {
                        let id = format!("surface.{i}");
                        SelectedSurface {
                            id: id.clone(),
                            template: id,
                            cost: 1.0,
                            covers: vec![],
                            run,
                        }
                    })
                    .collect()
            },
        )
    }

    /// Build a Receipt from a list of steps with the given plan_digest.
    fn make_receipt(steps: Vec<ReceiptStep>, plan_digest: &str) -> Receipt {
        let all_passed = steps.iter().all(|s| s.exit_code == 0);
        Receipt {
            version: "0.1.0-ref".to_string(),
            executed_at: "2026-01-01T00:00:00Z".to_string(),
            plan_digest: plan_digest.to_string(),
            status: if all_passed { "passed" } else { "failed" }.to_string(),
            steps,
        }
    }

    /// Build ReceiptSteps from surfaces with the given exit codes.
    fn make_receipt_steps(surfaces: &[SelectedSurface], exit_codes: &[i32]) -> Vec<ReceiptStep> {
        surfaces
            .iter()
            .zip(exit_codes.iter())
            .enumerate()
            .map(|(i, (surface, &exit_code))| {
                let index = i + 1;
                ReceiptStep {
                    id: surface.id.clone(),
                    argv: surface.run.clone(),
                    exit_code,
                    duration_ms: if exit_code == 0 { 42 } else { 99 },
                    stdout_path: format!(".proofrun/logs/{:02}-{}.stdout.log", index, surface.id),
                    stderr_path: format!(".proofrun/logs/{:02}-{}.stderr.log", index, surface.id),
                }
            })
            .collect()
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

    // Property 14: Resume and failed-only digest verification
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 18.3, 18.4, 19.2, 19.3**
        ///
        /// For any Plan and Receipt where plan_digest differs, both
        /// execute_with_resume and execute_failed_only return an error.
        /// When digests match, execution proceeds successfully.
        #[test]
        fn prop_resume_and_failed_only_digest_verification(
            surfaces in arb_unique_surfaces(1..=5),
            digest_a in "[a-f0-9]{64}",
            digest_b in "[a-f0-9]{64}",
        ) {
            let tmp = tempfile::tempdir().expect("failed to create temp dir");
            let repo_root = camino::Utf8Path::from_path(tmp.path())
                .expect("temp dir is not valid UTF-8");
            let output_dir = ".proofrun";

            let plan = make_plan(surfaces.clone(), output_dir, &digest_a);

            // Build a receipt with all steps passing
            let exit_codes: Vec<i32> = vec![0; surfaces.len()];
            let steps = make_receipt_steps(&surfaces, &exit_codes);

            // --- Matching digest: should succeed ---
            let matching_receipt = make_receipt(steps.clone(), &digest_a);
            let resume_result = execute_with_resume(
                repo_root, &plan, &matching_receipt, ExecutionMode::DryRun,
            );
            prop_assert!(
                resume_result.is_ok(),
                "execute_with_resume should succeed when digests match"
            );

            let failed_only_result = execute_failed_only(
                repo_root, &plan, &matching_receipt, ExecutionMode::DryRun,
            );
            prop_assert!(
                failed_only_result.is_ok(),
                "execute_failed_only should succeed when digests match"
            );

            // --- Mismatching digest: should error ---
            if digest_a != digest_b {
                let mismatched_receipt = make_receipt(steps, &digest_b);
                let resume_err = execute_with_resume(
                    repo_root, &plan, &mismatched_receipt, ExecutionMode::DryRun,
                );
                prop_assert!(
                    resume_err.is_err(),
                    "execute_with_resume should fail when digests mismatch"
                );

                let failed_only_err = execute_failed_only(
                    repo_root, &plan, &mismatched_receipt, ExecutionMode::DryRun,
                );
                prop_assert!(
                    failed_only_err.is_err(),
                    "execute_failed_only should fail when digests mismatch"
                );
            }
        }
    }

    // Property 15: Resume step merging
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 18.1, 18.2, 18.5, 18.6**
        ///
        /// For any Plan with 1–5 surfaces and a previous Receipt with mixed
        /// pass/fail steps, execute_with_resume in dry-run mode produces a
        /// Receipt where: passed steps carry forward (same data), failed steps
        /// are re-executed (exit_code 0 in dry-run), and total step count
        /// equals the plan surface count.
        #[test]
        fn prop_resume_step_merging(
            surfaces in arb_unique_surfaces(1..=5),
            plan_digest in "[a-f0-9]{64}",
        ) {
            let n = surfaces.len();
            // Generate a mix of pass/fail: odd-indexed surfaces fail
            let exit_codes: Vec<i32> = (0..n).map(|i| if i % 2 == 0 { 0 } else { 1 }).collect();

            let tmp = tempfile::tempdir().expect("failed to create temp dir");
            let repo_root = camino::Utf8Path::from_path(tmp.path())
                .expect("temp dir is not valid UTF-8");
            let output_dir = ".proofrun";

            let plan = make_plan(surfaces.clone(), output_dir, &plan_digest);
            let prev_steps = make_receipt_steps(&surfaces, &exit_codes);
            let prev_receipt = make_receipt(prev_steps.clone(), &plan_digest);

            let result = execute_with_resume(
                repo_root, &plan, &prev_receipt, ExecutionMode::DryRun,
            )
            .expect("resume dry-run should succeed");

            // (c) Total step count equals plan surface count
            prop_assert_eq!(
                result.steps.len(),
                n,
                "resume receipt should have one step per plan surface"
            );

            for (i, step) in result.steps.iter().enumerate() {
                let prev_exit = exit_codes[i];
                let prev_step = &prev_steps[i];

                // Step id matches surface id
                prop_assert_eq!(&step.id, &surfaces[i].id);

                if prev_exit == 0 {
                    // (a) Passed steps carry forward original data
                    prop_assert_eq!(
                        step.exit_code, 0,
                        "carried-forward step should have exit_code 0"
                    );
                    prop_assert_eq!(
                        step.duration_ms, prev_step.duration_ms,
                        "carried-forward step should preserve duration_ms"
                    );
                    prop_assert_eq!(
                        &step.argv, &prev_step.argv,
                        "carried-forward step should preserve argv"
                    );
                    prop_assert_eq!(
                        &step.stdout_path, &prev_step.stdout_path,
                        "carried-forward step should preserve stdout_path"
                    );
                    prop_assert_eq!(
                        &step.stderr_path, &prev_step.stderr_path,
                        "carried-forward step should preserve stderr_path"
                    );
                } else {
                    // (b) Failed steps are re-executed (dry-run → exit_code 0, duration 0)
                    prop_assert_eq!(
                        step.exit_code, 0,
                        "re-executed step in dry-run should have exit_code 0"
                    );
                    prop_assert_eq!(
                        step.duration_ms, 0,
                        "re-executed step in dry-run should have duration_ms 0"
                    );
                }
            }

            // (d) In dry-run mode, status should be "dry-run"
            prop_assert_eq!(&result.status, "dry-run");
        }
    }

    // Property 16: Failed-only step selection
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 19.1, 19.4, 19.5**
        ///
        /// For any Plan and Receipt in dry-run mode, execute_failed_only
        /// produces a Receipt where: only failed steps are re-executed,
        /// passed steps carry forward unchanged, and surfaces not in the
        /// previous receipt are skipped.
        #[test]
        fn prop_failed_only_step_selection(
            surfaces in arb_unique_surfaces(1..=5),
            plan_digest in "[a-f0-9]{64}",
        ) {
            let n = surfaces.len();
            // Generate a mix: even-indexed pass, odd-indexed fail
            let exit_codes: Vec<i32> = (0..n).map(|i| if i % 2 == 0 { 0 } else { 1 }).collect();

            let tmp = tempfile::tempdir().expect("failed to create temp dir");
            let repo_root = camino::Utf8Path::from_path(tmp.path())
                .expect("temp dir is not valid UTF-8");
            let output_dir = ".proofrun";

            let plan = make_plan(surfaces.clone(), output_dir, &plan_digest);
            let prev_steps = make_receipt_steps(&surfaces, &exit_codes);
            let prev_receipt = make_receipt(prev_steps.clone(), &plan_digest);

            let result = execute_failed_only(
                repo_root, &plan, &prev_receipt, ExecutionMode::DryRun,
            )
            .expect("failed-only dry-run should succeed");

            // All surfaces were in the previous receipt, so total step count
            // should equal the plan surface count.
            prop_assert_eq!(
                result.steps.len(),
                n,
                "failed-only receipt should have one step per surface present in previous receipt"
            );

            for (i, step) in result.steps.iter().enumerate() {
                let prev_exit = exit_codes[i];
                let prev_step = &prev_steps[i];

                prop_assert_eq!(&step.id, &surfaces[i].id);

                if prev_exit == 0 {
                    // (b) Previously passed steps carry forward unchanged
                    prop_assert_eq!(
                        step.exit_code, 0,
                        "carried-forward passed step should have exit_code 0"
                    );
                    prop_assert_eq!(
                        step.duration_ms, prev_step.duration_ms,
                        "carried-forward passed step should preserve duration_ms"
                    );
                    prop_assert_eq!(
                        &step.argv, &prev_step.argv,
                        "carried-forward passed step should preserve argv"
                    );
                    prop_assert_eq!(
                        &step.stdout_path, &prev_step.stdout_path,
                        "carried-forward passed step should preserve stdout_path"
                    );
                    prop_assert_eq!(
                        &step.stderr_path, &prev_step.stderr_path,
                        "carried-forward passed step should preserve stderr_path"
                    );
                } else {
                    // (a) Failed steps are re-executed (dry-run → exit_code 0)
                    prop_assert_eq!(
                        step.exit_code, 0,
                        "re-executed failed step in dry-run should have exit_code 0"
                    );
                    prop_assert_eq!(
                        step.duration_ms, 0,
                        "re-executed failed step in dry-run should have duration_ms 0"
                    );
                }
            }

            // Status should be "dry-run" in dry-run mode
            prop_assert_eq!(&result.status, "dry-run");
        }

        /// **Validates: Requirements 19.1, 19.4, 19.5**
        ///
        /// When the plan has surfaces not present in the previous receipt,
        /// execute_failed_only skips those surfaces entirely.
        #[test]
        fn prop_failed_only_skips_surfaces_not_in_receipt(
            surfaces in arb_unique_surfaces(2..=5),
            plan_digest in "[a-f0-9]{64}",
        ) {
            let n = surfaces.len();
            // Build a receipt with only the first half of surfaces (all failed)
            let receipt_count = n / 2;
            let receipt_surfaces = &surfaces[..receipt_count];
            let exit_codes: Vec<i32> = vec![1; receipt_count];

            let tmp = tempfile::tempdir().expect("failed to create temp dir");
            let repo_root = camino::Utf8Path::from_path(tmp.path())
                .expect("temp dir is not valid UTF-8");
            let output_dir = ".proofrun";

            let plan = make_plan(surfaces.clone(), output_dir, &plan_digest);
            let prev_steps = make_receipt_steps(receipt_surfaces, &exit_codes);
            let prev_receipt = make_receipt(prev_steps, &plan_digest);

            let result = execute_failed_only(
                repo_root, &plan, &prev_receipt, ExecutionMode::DryRun,
            )
            .expect("failed-only dry-run should succeed");

            // Only surfaces that were in the previous receipt should appear
            prop_assert_eq!(
                result.steps.len(),
                receipt_count,
                "failed-only should only include surfaces from previous receipt"
            );

            // All re-executed steps should have exit_code 0 (dry-run)
            for step in &result.steps {
                prop_assert_eq!(
                    step.exit_code, 0,
                    "re-executed step in dry-run should have exit_code 0"
                );
            }

            // Verify the step ids match only the receipt surfaces
            let step_ids: Vec<&str> = result.steps.iter().map(|s| s.id.as_str()).collect();
            let expected_ids: Vec<&str> = receipt_surfaces.iter().map(|s| s.id.as_str()).collect();
            prop_assert_eq!(step_ids, expected_ids);
        }
    }
}
