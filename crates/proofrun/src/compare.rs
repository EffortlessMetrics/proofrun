use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::model::Plan;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanComparison {
    pub obligations_added: Vec<String>,
    pub obligations_removed: Vec<String>,
    pub surfaces_added: Vec<String>,
    pub surfaces_removed: Vec<String>,
    pub cost_delta: f64,
    pub new_fallback_obligations: bool,
}

/// Compare two plans and compute structural differences.
///
/// Uses `BTreeSet` for set operations on obligation ids and surface ids.
/// Cost delta = new total cost − old total cost.
/// `new_fallback_obligations` is true iff the new plan contains fallback-sourced
/// obligations (source `"unknown-fallback"` or `"empty-range-fallback"`) that
/// were not present in the old plan.
pub fn compare_plans(old: &Plan, new: &Plan) -> PlanComparison {
    // Obligation ids
    let old_obligations: BTreeSet<&str> = old.obligations.iter().map(|o| o.id.as_str()).collect();
    let new_obligations: BTreeSet<&str> = new.obligations.iter().map(|o| o.id.as_str()).collect();

    let obligations_added: Vec<String> = new_obligations
        .difference(&old_obligations)
        .map(|s| s.to_string())
        .collect();
    let obligations_removed: Vec<String> = old_obligations
        .difference(&new_obligations)
        .map(|s| s.to_string())
        .collect();

    // Surface ids
    let old_surfaces: BTreeSet<&str> = old
        .selected_surfaces
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    let new_surfaces: BTreeSet<&str> = new
        .selected_surfaces
        .iter()
        .map(|s| s.id.as_str())
        .collect();

    let surfaces_added: Vec<String> = new_surfaces
        .difference(&old_surfaces)
        .map(|s| s.to_string())
        .collect();
    let surfaces_removed: Vec<String> = old_surfaces
        .difference(&new_surfaces)
        .map(|s| s.to_string())
        .collect();

    // Cost delta
    let old_cost: f64 = old.selected_surfaces.iter().map(|s| s.cost).sum();
    let new_cost: f64 = new.selected_surfaces.iter().map(|s| s.cost).sum();
    let cost_delta = new_cost - old_cost;

    // New fallback obligations: obligations in the new plan with a fallback
    // reason source that were not present in the old plan's fallback set.
    let fallback_sources = ["unknown-fallback", "empty-range-fallback"];

    let old_fallback_ids: BTreeSet<&str> = old
        .obligations
        .iter()
        .filter(|o| {
            o.reasons
                .iter()
                .any(|r| fallback_sources.contains(&r.source.as_str()))
        })
        .map(|o| o.id.as_str())
        .collect();

    let new_fallback_ids: BTreeSet<&str> = new
        .obligations
        .iter()
        .filter(|o| {
            o.reasons
                .iter()
                .any(|r| fallback_sources.contains(&r.source.as_str()))
        })
        .map(|o| o.id.as_str())
        .collect();

    let new_fallback_obligations = new_fallback_ids
        .difference(&old_fallback_ids)
        .next()
        .is_some();

    PlanComparison {
        obligations_added,
        obligations_removed,
        surfaces_added,
        surfaces_removed,
        cost_delta,
        new_fallback_obligations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ObligationReason, ObligationRecord, Plan, PlanArtifacts, SelectedSurface, WorkspaceInfo,
    };
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    /// Build a minimal Plan with the given obligations and surfaces.
    fn make_plan(obligations: Vec<ObligationRecord>, surfaces: Vec<SelectedSurface>) -> Plan {
        Plan {
            version: "0.1.0-ref".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            repo_root: "/tmp".to_string(),
            base: "aaa".to_string(),
            head: "bbb".to_string(),
            merge_base: "aaa".to_string(),
            profile: "ci".to_string(),
            config_digest: String::new(),
            plan_digest: String::new(),
            artifacts: PlanArtifacts {
                output_dir: ".proofrun".to_string(),
                diff_patch: ".proofrun/diff.patch".to_string(),
                plan_json: ".proofrun/plan.json".to_string(),
                plan_markdown: ".proofrun/plan.md".to_string(),
                commands_shell: ".proofrun/commands.sh".to_string(),
                github_actions: ".proofrun/github-actions.yml".to_string(),
            },
            workspace: WorkspaceInfo { packages: vec![] },
            changed_paths: vec![],
            obligations,
            selected_surfaces: surfaces,
            omitted_surfaces: vec![],
            diagnostics: vec![],
        }
    }

    /// Build an ObligationRecord with the given id and source.
    fn make_obligation(id: &str, source: &str) -> ObligationRecord {
        ObligationRecord {
            id: id.to_string(),
            reasons: vec![ObligationReason {
                source: source.to_string(),
                path: None,
                rule: None,
                pattern: None,
            }],
        }
    }

    /// Build a SelectedSurface with the given id and cost.
    fn make_surface(id: &str, cost: f64) -> SelectedSurface {
        SelectedSurface {
            id: id.to_string(),
            template: id.to_string(),
            cost,
            covers: vec![],
            run: vec!["echo".to_string()],
        }
    }

    /// Strategy: generate a pair of Plans with overlapping obligation/surface ids
    /// and controllable fallback obligations.
    ///
    /// We generate three disjoint index pools for obligations (shared, old-only, new-only)
    /// and three for surfaces, then build Plans from those pools.
    fn arb_plan_pair() -> impl Strategy<Value = (Plan, Plan)> {
        // Number of shared, old-only, new-only obligations (0..=3 each)
        let obl_counts = (0..=3usize, 0..=3usize, 0..=3usize);
        // Number of shared, old-only, new-only surfaces (0..=3 each)
        let surf_counts = (0..=3usize, 0..=3usize, 0..=3usize);
        // Costs for surfaces: up to 9 surfaces total (3+3+3)
        let costs = prop::collection::vec(0.1f64..100.0f64, 9);
        // For each obligation (up to 9), whether it has a fallback source
        let fallback_flags = prop::collection::vec(prop::bool::ANY, 9);

        (obl_counts, surf_counts, costs, fallback_flags).prop_map(
            |(
                (shared_obl, old_only_obl, new_only_obl),
                (shared_surf, old_only_surf, new_only_surf),
                costs,
                fallback_flags,
            )| {
                let mut idx = 0usize;

                // Build obligation pools
                let mut shared_obligations = Vec::new();
                let mut old_only_obligations = Vec::new();
                let mut new_only_obligations = Vec::new();

                for _ in 0..shared_obl {
                    let source = if fallback_flags[idx % 9] {
                        "unknown-fallback"
                    } else {
                        "rule"
                    };
                    shared_obligations.push(make_obligation(&format!("obl:{idx}"), source));
                    idx += 1;
                }
                for _ in 0..old_only_obl {
                    let source = if fallback_flags[idx % 9] {
                        "empty-range-fallback"
                    } else {
                        "rule"
                    };
                    old_only_obligations.push(make_obligation(&format!("obl:{idx}"), source));
                    idx += 1;
                }
                for _ in 0..new_only_obl {
                    let source = if fallback_flags[idx % 9] {
                        "unknown-fallback"
                    } else {
                        "rule"
                    };
                    new_only_obligations.push(make_obligation(&format!("obl:{idx}"), source));
                    idx += 1;
                }

                // Build surface pools
                let mut shared_surfaces = Vec::new();
                let mut old_only_surfaces = Vec::new();
                let mut new_only_surfaces = Vec::new();
                let mut surf_idx = 0usize;

                for _ in 0..shared_surf {
                    shared_surfaces.push(make_surface(
                        &format!("surf:{surf_idx}"),
                        costs[surf_idx % 9],
                    ));
                    surf_idx += 1;
                }
                for _ in 0..old_only_surf {
                    old_only_surfaces.push(make_surface(
                        &format!("surf:{surf_idx}"),
                        costs[surf_idx % 9],
                    ));
                    surf_idx += 1;
                }
                for _ in 0..new_only_surf {
                    new_only_surfaces.push(make_surface(
                        &format!("surf:{surf_idx}"),
                        costs[surf_idx % 9],
                    ));
                    surf_idx += 1;
                }

                // Assemble old plan: shared + old-only
                let old_obligations: Vec<_> = shared_obligations
                    .iter()
                    .cloned()
                    .chain(old_only_obligations.iter().cloned())
                    .collect();
                let old_surfaces: Vec<_> = shared_surfaces
                    .iter()
                    .cloned()
                    .chain(old_only_surfaces.iter().cloned())
                    .collect();

                // Assemble new plan: shared + new-only
                let new_obligations: Vec<_> = shared_obligations
                    .iter()
                    .cloned()
                    .chain(new_only_obligations.iter().cloned())
                    .collect();
                let new_surfaces: Vec<_> = shared_surfaces
                    .iter()
                    .cloned()
                    .chain(new_only_surfaces.iter().cloned())
                    .collect();

                let old_plan = make_plan(old_obligations, old_surfaces);
                let new_plan = make_plan(new_obligations, new_surfaces);

                (old_plan, new_plan)
            },
        )
    }

    // Feature: proofrun-differentiation, Property 17: Plan comparison correctness
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        /// **Validates: Requirements 20.1, 20.2, 20.3**
        ///
        /// For any two Plans with overlapping/disjoint obligations and surfaces:
        /// (a) obligations_added = ids in new but not old, sorted
        /// (b) obligations_removed = ids in old but not new, sorted
        /// (c) surfaces_added = ids in new but not old, sorted
        /// (d) surfaces_removed = ids in old but not new, sorted
        /// (e) cost_delta = new total cost - old total cost
        /// (f) new_fallback_obligations = true iff new plan has fallback obligations not in old
        /// Also: self-comparison yields empty lists and zero cost delta.
        #[test]
        fn prop_plan_comparison_correctness((old, new) in arb_plan_pair()) {
            let result = compare_plans(&old, &new);

            // Independently compute expected values
            let old_obl_ids: BTreeSet<String> =
                old.obligations.iter().map(|o| o.id.clone()).collect();
            let new_obl_ids: BTreeSet<String> =
                new.obligations.iter().map(|o| o.id.clone()).collect();

            let expected_added_obls: Vec<String> =
                new_obl_ids.difference(&old_obl_ids).cloned().collect();
            let expected_removed_obls: Vec<String> =
                old_obl_ids.difference(&new_obl_ids).cloned().collect();

            let old_surf_ids: BTreeSet<String> =
                old.selected_surfaces.iter().map(|s| s.id.clone()).collect();
            let new_surf_ids: BTreeSet<String> =
                new.selected_surfaces.iter().map(|s| s.id.clone()).collect();

            let expected_added_surfs: Vec<String> =
                new_surf_ids.difference(&old_surf_ids).cloned().collect();
            let expected_removed_surfs: Vec<String> =
                old_surf_ids.difference(&new_surf_ids).cloned().collect();

            let old_cost: f64 = old.selected_surfaces.iter().map(|s| s.cost).sum();
            let new_cost: f64 = new.selected_surfaces.iter().map(|s| s.cost).sum();
            let expected_cost_delta = new_cost - old_cost;

            let fallback_sources = ["unknown-fallback", "empty-range-fallback"];
            let old_fallback_ids: BTreeSet<&str> = old
                .obligations
                .iter()
                .filter(|o| {
                    o.reasons
                        .iter()
                        .any(|r| fallback_sources.contains(&r.source.as_str()))
                })
                .map(|o| o.id.as_str())
                .collect();
            let new_fallback_ids: BTreeSet<&str> = new
                .obligations
                .iter()
                .filter(|o| {
                    o.reasons
                        .iter()
                        .any(|r| fallback_sources.contains(&r.source.as_str()))
                })
                .map(|o| o.id.as_str())
                .collect();
            let expected_new_fallback = new_fallback_ids
                .difference(&old_fallback_ids)
                .next()
                .is_some();

            // (a) obligations_added sorted
            prop_assert_eq!(
                &result.obligations_added, &expected_added_obls,
                "obligations_added mismatch"
            );
            let mut sorted = result.obligations_added.clone();
            sorted.sort();
            prop_assert_eq!(
                &result.obligations_added, &sorted,
                "obligations_added not sorted"
            );

            // (b) obligations_removed sorted
            prop_assert_eq!(
                &result.obligations_removed, &expected_removed_obls,
                "obligations_removed mismatch"
            );
            let mut sorted = result.obligations_removed.clone();
            sorted.sort();
            prop_assert_eq!(
                &result.obligations_removed, &sorted,
                "obligations_removed not sorted"
            );

            // (c) surfaces_added sorted
            prop_assert_eq!(
                &result.surfaces_added, &expected_added_surfs,
                "surfaces_added mismatch"
            );
            let mut sorted = result.surfaces_added.clone();
            sorted.sort();
            prop_assert_eq!(
                &result.surfaces_added, &sorted,
                "surfaces_added not sorted"
            );

            // (d) surfaces_removed sorted
            prop_assert_eq!(
                &result.surfaces_removed, &expected_removed_surfs,
                "surfaces_removed mismatch"
            );
            let mut sorted = result.surfaces_removed.clone();
            sorted.sort();
            prop_assert_eq!(
                &result.surfaces_removed, &sorted,
                "surfaces_removed not sorted"
            );

            // (e) cost_delta
            prop_assert!(
                (result.cost_delta - expected_cost_delta).abs() < 1e-10,
                "cost_delta mismatch: got {}, expected {}",
                result.cost_delta,
                expected_cost_delta
            );

            // (f) new_fallback_obligations
            prop_assert_eq!(
                result.new_fallback_obligations, expected_new_fallback,
                "new_fallback_obligations mismatch"
            );

            // Self-comparison: compare_plans(plan, plan) yields empty diffs
            // and zero cost delta
            let self_old = compare_plans(&old, &old);
            prop_assert!(
                self_old.obligations_added.is_empty(),
                "self-compare old: obligations_added not empty"
            );
            prop_assert!(
                self_old.obligations_removed.is_empty(),
                "self-compare old: obligations_removed not empty"
            );
            prop_assert!(
                self_old.surfaces_added.is_empty(),
                "self-compare old: surfaces_added not empty"
            );
            prop_assert!(
                self_old.surfaces_removed.is_empty(),
                "self-compare old: surfaces_removed not empty"
            );
            prop_assert!(
                self_old.cost_delta.abs() < 1e-10,
                "self-compare old: cost_delta not zero"
            );
            prop_assert!(
                !self_old.new_fallback_obligations,
                "self-compare old: new_fallback_obligations should be false"
            );

            let self_new = compare_plans(&new, &new);
            prop_assert!(
                self_new.obligations_added.is_empty(),
                "self-compare new: obligations_added not empty"
            );
            prop_assert!(
                self_new.obligations_removed.is_empty(),
                "self-compare new: obligations_removed not empty"
            );
            prop_assert!(
                self_new.surfaces_added.is_empty(),
                "self-compare new: surfaces_added not empty"
            );
            prop_assert!(
                self_new.surfaces_removed.is_empty(),
                "self-compare new: surfaces_removed not empty"
            );
            prop_assert!(
                self_new.cost_delta.abs() < 1e-10,
                "self-compare new: cost_delta not zero"
            );
            prop_assert!(
                !self_new.new_fallback_obligations,
                "self-compare new: new_fallback_obligations should be false"
            );
        }
    }
}
