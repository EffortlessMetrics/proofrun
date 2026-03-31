use crate::config::{Config, SurfaceTemplate};
use crate::obligations::expand_template;
use camino::Utf8Path;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct CandidateSurface {
    pub id: String,
    pub template: String,
    pub cost: f64,
    pub covers: Vec<String>,
    pub run: Vec<String>,
}

/// Simple fnmatch-style glob matching where `*` matches any substring.
///
/// This matches Python's `fnmatch.fnmatch` behavior on non-path strings
/// (obligation ids like `pkg:core:tests`). Unlike path glob matching,
/// `*` here crosses `:` separators.
fn fnmatch_match(pattern: &str, text: &str) -> bool {
    // Convert pattern to regex: `*` → `.*`, escape everything else
    let mut regex_str = String::from("^");
    for ch in pattern.chars() {
        if ch == '*' {
            regex_str.push_str(".*");
        } else if ch == '?' {
            regex_str.push('.');
        } else {
            regex_str.push_str(&regex::escape(&ch.to_string()));
        }
    }
    regex_str.push('$');
    regex::Regex::new(&regex_str)
        .map(|re| re.is_match(text))
        .unwrap_or(false)
}

/// Extract distinct package names from obligations matching `pkg:*:*` pattern.
/// Returns sorted list of unique package names (second element when split by `:`).
fn candidate_bindings(obligations: &[String]) -> Vec<BTreeMap<String, String>> {
    let mut bindings: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    let pkgs: BTreeSet<String> = obligations
        .iter()
        .filter(|ob| ob.starts_with("pkg:"))
        .filter_map(|ob| {
            let parts: Vec<&str> = ob.split(':').collect();
            if parts.len() >= 3 {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .collect();
    for pkg in pkgs {
        let mut binding = BTreeMap::new();
        binding.insert("pkg".to_string(), pkg);
        bindings.push(binding);
    }
    bindings
}

/// Check if any string in the given list contains `{pkg}`.
fn uses_pkg(surface: &SurfaceTemplate) -> bool {
    let check = |s: &str| s.contains("{pkg}");
    check(&surface.id)
        || surface.covers.iter().any(|s| check(s))
        || surface.run.iter().any(|s| check(s))
}

/// Port of Python `build_candidates`.
///
/// Expands surface templates into concrete candidate surfaces by:
/// 1. Computing bindings from pkg obligations
/// 2. Expanding templates with bindings
/// 3. Matching cover patterns against obligations via fnmatch
/// 4. Deduplicating and sorting
pub fn build_candidates(
    config: &Config,
    obligations: &[String],
    profile: &str,
    output_dir: &Utf8Path,
) -> Vec<CandidateSurface> {
    let bindings = candidate_bindings(obligations);
    let mut candidates: Vec<CandidateSurface> = Vec::new();

    for surface in &config.surfaces {
        let has_pkg = uses_pkg(surface);
        let active_bindings: Vec<&BTreeMap<String, String>> = if has_pkg {
            bindings.iter().filter(|b| b.contains_key("pkg")).collect()
        } else {
            vec![&bindings[0]] // just the empty binding
        };

        if has_pkg && active_bindings.is_empty() {
            continue;
        }

        for binding in &active_bindings {
            let mut values = BTreeMap::new();
            values.insert("profile".to_string(), profile.to_string());
            values.insert(
                "artifacts.diff_patch".to_string(),
                format!("{}/diff.patch", output_dir),
            );
            for (k, v) in *binding {
                values.insert(k.clone(), v.clone());
            }

            // Expand cover patterns
            let cover_patterns: Vec<String> = surface
                .covers
                .iter()
                .filter_map(|pat| expand_template(pat, &values).ok())
                .collect();

            // Match obligations against cover patterns
            let covered: BTreeSet<String> = obligations
                .iter()
                .filter(|ob| cover_patterns.iter().any(|pat| fnmatch_match(pat, ob)))
                .cloned()
                .collect();

            let covered_sorted: Vec<String> = covered.into_iter().collect();

            if covered_sorted.is_empty() {
                continue;
            }

            // Expand run arguments
            let run: Vec<String> = surface
                .run
                .iter()
                .filter_map(|arg| expand_template(arg, &values).ok())
                .collect();

            // Build surface id
            let base_id = &surface.id;
            let surface_id = if binding.is_empty() {
                base_id.clone()
            } else {
                let binding_str: String = binding
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{base_id}[{binding_str}]")
            };

            candidates.push(CandidateSurface {
                id: surface_id,
                template: base_id.clone(),
                cost: surface.cost,
                covers: covered_sorted,
                run,
            });
        }
    }

    // Deduplicate by (id, covers), keeping last occurrence.
    // Walk backwards to find the last occurrence of each key, then collect in order.
    let mut seen: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
    let mut deduped: Vec<CandidateSurface> = Vec::new();
    for candidate in candidates.into_iter().rev() {
        let key = (candidate.id.clone(), candidate.covers.clone());
        if seen.insert(key) {
            deduped.push(candidate);
        }
    }
    deduped.reverse();

    let mut result = deduped;

    // Sort by (cost asc, -covers.len() i.e. covers count desc, id asc)
    result.sort_by(|a, b| {
        a.cost
            .partial_cmp(&b.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.covers.len().cmp(&a.covers.len()))
            .then_with(|| a.id.cmp(&b.id))
    });

    result
}

/// Port of Python `solve_exact_cover`.
///
/// Branch-and-bound solver that finds the minimum-cost set of candidates
/// covering all obligations. Tie-breaks by (cost, count, sorted_ids).
/// Returns selected surfaces sorted by id ascending.
pub fn solve_exact_cover(
    obligations: &[String],
    candidates: &[CandidateSurface],
) -> anyhow::Result<Vec<CandidateSurface>> {
    // Build obligation → candidate indices
    let mut by_obligation: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for ob in obligations {
        by_obligation.insert(ob.clone(), Vec::new());
    }
    for (idx, candidate) in candidates.iter().enumerate() {
        for ob in &candidate.covers {
            if let Some(list) = by_obligation.get_mut(ob) {
                list.push(idx);
            }
        }
    }

    // Check all obligations are coverable
    for (ob, covering) in &by_obligation {
        if covering.is_empty() {
            anyhow::bail!("no candidate surface covers obligation {ob}");
        }
    }

    // Best solution tracking: (cost, count, signature)
    let mut best_choice: Option<Vec<usize>> = None;
    let mut best_score: Option<(f64, usize, Vec<String>)> = None;

    fn recurse(
        remaining: &BTreeSet<String>,
        chosen: &mut Vec<usize>,
        chosen_ids: &mut BTreeSet<String>,
        cost: f64,
        candidates: &[CandidateSurface],
        by_obligation: &BTreeMap<String, Vec<usize>>,
        best_choice: &mut Option<Vec<usize>>,
        best_score: &mut Option<(f64, usize, Vec<String>)>,
    ) {
        if remaining.is_empty() {
            let mut signature: Vec<String> =
                chosen.iter().map(|&i| candidates[i].id.clone()).collect();
            signature.sort();
            let candidate_score = (cost, chosen.len(), signature);
            if best_score.is_none() || candidate_score < *best_score.as_ref().unwrap() {
                *best_score = Some(candidate_score);
                *best_choice = Some(chosen.clone());
            }
            return;
        }

        // Prune: if (cost, count) >= (best_cost, best_count)
        if let Some(ref best) = best_score {
            if (cost, chosen.len()) >= (best.0, best.1) {
                return;
            }
        }

        // Pick most-constrained obligation (fewest candidates, then alphabetical)
        let target = remaining
            .iter()
            .min_by_key(|ob| {
                let count = by_obligation.get(*ob).map(|v| v.len()).unwrap_or(0);
                (count, (*ob).clone())
            })
            .unwrap()
            .clone();

        // Get candidates for this obligation, sorted by (cost, -covers.len, id)
        let mut options: Vec<usize> = by_obligation.get(&target).cloned().unwrap_or_default();
        options.sort_by(|&a, &b| {
            candidates[a]
                .cost
                .partial_cmp(&candidates[b].cost)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| candidates[b].covers.len().cmp(&candidates[a].covers.len()))
                .then_with(|| candidates[a].id.cmp(&candidates[b].id))
        });

        for &idx in &options {
            let candidate = &candidates[idx];
            if chosen_ids.contains(&candidate.id) {
                continue;
            }

            let new_remaining: BTreeSet<String> = remaining
                .difference(
                    &candidate
                        .covers
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<String>>(),
                )
                .cloned()
                .collect();

            chosen.push(idx);
            chosen_ids.insert(candidate.id.clone());

            recurse(
                &new_remaining,
                chosen,
                chosen_ids,
                cost + candidate.cost,
                candidates,
                by_obligation,
                best_choice,
                best_score,
            );

            chosen.pop();
            chosen_ids.remove(&candidate.id);
        }
    }

    let remaining: BTreeSet<String> = obligations.iter().cloned().collect();
    recurse(
        &remaining,
        &mut Vec::new(),
        &mut BTreeSet::new(),
        0.0,
        candidates,
        &by_obligation,
        &mut best_choice,
        &mut best_score,
    );

    match best_choice {
        Some(indices) => {
            let mut result: Vec<CandidateSurface> =
                indices.into_iter().map(|i| candidates[i].clone()).collect();
            result.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(result)
        }
        None => anyhow::bail!("failed to solve proof plan"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Defaults, SurfaceTemplate, UnknownConfig};
    use proptest::prelude::*;

    /// Helper: build a minimal Config for testing build_candidates.
    fn test_config(surfaces: Vec<SurfaceTemplate>) -> Config {
        Config {
            version: 1,
            defaults: Defaults {
                output_dir: ".proofrun".to_string(),
            },
            profiles: BTreeMap::new(),
            surfaces,
            rules: vec![],
            unknown: UnknownConfig::default(),
        }
    }

    #[test]
    fn test_fnmatch_match_star_matches_any() {
        assert!(fnmatch_match("pkg:*:tests", "pkg:core:tests"));
        assert!(fnmatch_match("pkg:*:tests", "pkg:app:tests"));
        assert!(!fnmatch_match("pkg:*:tests", "workspace:smoke"));
    }

    #[test]
    fn test_fnmatch_match_exact() {
        assert!(fnmatch_match("workspace:smoke", "workspace:smoke"));
        assert!(!fnmatch_match("workspace:smoke", "workspace:docs"));
    }

    #[test]
    fn test_fnmatch_match_question_mark() {
        assert!(fnmatch_match("pkg:?:tests", "pkg:a:tests"));
        assert!(!fnmatch_match("pkg:?:tests", "pkg:core:tests"));
    }

    #[test]
    fn test_candidate_bindings_basic() {
        let obligations = vec![
            "pkg:core:tests".to_string(),
            "pkg:core:mutation-diff".to_string(),
            "pkg:app:tests".to_string(),
            "workspace:smoke".to_string(),
        ];
        let bindings = candidate_bindings(&obligations);
        // First binding is empty
        assert!(bindings[0].is_empty());
        // Then one per distinct package, sorted
        assert_eq!(bindings.len(), 3); // {} + {pkg:app} + {pkg:core}
        assert_eq!(bindings[1].get("pkg").unwrap(), "app");
        assert_eq!(bindings[2].get("pkg").unwrap(), "core");
    }

    #[test]
    fn test_candidate_bindings_no_pkg_obligations() {
        let obligations = vec!["workspace:smoke".to_string(), "workspace:docs".to_string()];
        let bindings = candidate_bindings(&obligations);
        assert_eq!(bindings.len(), 1);
        assert!(bindings[0].is_empty());
    }

    #[test]
    fn test_build_candidates_basic_pkg_template() {
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_string(),
            covers: vec!["pkg:{pkg}:tests".to_string()],
            cost: 3.0,
            run: vec![
                "cargo".to_string(),
                "nextest".to_string(),
                "run".to_string(),
                "-E".to_string(),
                "package({pkg})".to_string(),
            ],
        }];
        let config = test_config(surfaces);
        let obligations = vec!["pkg:core:tests".to_string(), "pkg:app:tests".to_string()];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        assert_eq!(candidates.len(), 2);
        // Sorted by (cost, -covers.len, id) — both cost=3, covers.len=1, so by id
        assert_eq!(candidates[0].id, "tests.pkg[pkg=app]");
        assert_eq!(candidates[0].covers, vec!["pkg:app:tests"]);
        assert_eq!(candidates[0].template, "tests.pkg");
        assert!(candidates[0].run.contains(&"package(app)".to_string()));

        assert_eq!(candidates[1].id, "tests.pkg[pkg=core]");
        assert_eq!(candidates[1].covers, vec!["pkg:core:tests"]);
    }

    #[test]
    fn test_build_candidates_non_pkg_template() {
        let surfaces = vec![SurfaceTemplate {
            id: "workspace.smoke".to_string(),
            covers: vec!["workspace:smoke".to_string()],
            cost: 2.0,
            run: vec!["cargo".to_string(), "test".to_string()],
        }];
        let config = test_config(surfaces);
        let obligations = vec!["workspace:smoke".to_string()];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "workspace.smoke");
        assert_eq!(candidates[0].covers, vec!["workspace:smoke"]);
    }

    #[test]
    fn test_build_candidates_wildcard_cover() {
        // workspace.all-tests covers pkg:*:tests — should match all pkg:X:tests obligations
        let surfaces = vec![SurfaceTemplate {
            id: "workspace.all-tests".to_string(),
            covers: vec!["pkg:*:tests".to_string()],
            cost: 10.0,
            run: vec![
                "cargo".to_string(),
                "nextest".to_string(),
                "run".to_string(),
            ],
        }];
        let config = test_config(surfaces);
        let obligations = vec![
            "pkg:app:tests".to_string(),
            "pkg:core:tests".to_string(),
            "workspace:smoke".to_string(),
        ];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "workspace.all-tests");
        assert_eq!(
            candidates[0].covers,
            vec!["pkg:app:tests", "pkg:core:tests"]
        );
    }

    #[test]
    fn test_build_candidates_empty_covers_filtered() {
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_string(),
            covers: vec!["pkg:{pkg}:tests".to_string()],
            cost: 3.0,
            run: vec!["cargo".to_string(), "test".to_string()],
        }];
        let config = test_config(surfaces);
        // No pkg obligations, so no bindings with pkg → no candidates
        let obligations = vec!["workspace:smoke".to_string()];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        assert!(candidates.is_empty());
    }

    #[test]
    fn test_build_candidates_sort_order() {
        let surfaces = vec![
            SurfaceTemplate {
                id: "expensive".to_string(),
                covers: vec!["workspace:smoke".to_string()],
                cost: 10.0,
                run: vec!["echo".to_string()],
            },
            SurfaceTemplate {
                id: "cheap".to_string(),
                covers: vec!["workspace:smoke".to_string()],
                cost: 2.0,
                run: vec!["echo".to_string()],
            },
        ];
        let config = test_config(surfaces);
        let obligations = vec!["workspace:smoke".to_string()];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].id, "cheap"); // cost 2
        assert_eq!(candidates[1].id, "expensive"); // cost 10
    }

    #[test]
    fn test_build_candidates_profile_substitution() {
        let surfaces = vec![SurfaceTemplate {
            id: "tests.pkg".to_string(),
            covers: vec!["pkg:{pkg}:tests".to_string()],
            cost: 3.0,
            run: vec![
                "cargo".to_string(),
                "nextest".to_string(),
                "run".to_string(),
                "--profile".to_string(),
                "{profile}".to_string(),
            ],
        }];
        let config = test_config(surfaces);
        let obligations = vec!["pkg:core:tests".to_string()];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].run.contains(&"ci".to_string()));
    }

    #[test]
    fn test_build_candidates_artifacts_diff_patch_substitution() {
        let surfaces = vec![SurfaceTemplate {
            id: "mutation.diff".to_string(),
            covers: vec!["pkg:{pkg}:mutation-diff".to_string()],
            cost: 13.0,
            run: vec![
                "cargo".to_string(),
                "mutants".to_string(),
                "--in-diff".to_string(),
                "{artifacts.diff_patch}".to_string(),
            ],
        }];
        let config = test_config(surfaces);
        let obligations = vec!["pkg:core:mutation-diff".to_string()];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0]
            .run
            .contains(&".proofrun/diff.patch".to_string()));
    }

    #[test]
    fn test_build_candidates_dedup_keeps_last() {
        // Two surfaces with same id pattern but different costs — after dedup, last wins
        let surfaces = vec![
            SurfaceTemplate {
                id: "workspace.smoke".to_string(),
                covers: vec!["workspace:smoke".to_string()],
                cost: 2.0,
                run: vec!["first".to_string()],
            },
            SurfaceTemplate {
                id: "workspace.smoke".to_string(),
                covers: vec!["workspace:smoke".to_string()],
                cost: 5.0,
                run: vec!["second".to_string()],
            },
        ];
        let config = test_config(surfaces);
        let obligations = vec!["workspace:smoke".to_string()];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        // Should have only 1 after dedup (same id, same covers)
        assert_eq!(candidates.len(), 1);
        // Last occurrence wins
        assert_eq!(candidates[0].run, vec!["second"]);
        assert_eq!(candidates[0].cost, 5.0);
    }

    #[test]
    fn test_build_candidates_full_default_config_core_change() {
        // Simulate the core-change scenario with default config
        let config: Config =
            toml::from_str(crate::config::DEFAULT_CONFIG_TOML).expect("default config parses");
        let obligations = vec![
            "pkg:core:mutation-diff".to_string(),
            "pkg:core:tests".to_string(),
            "workspace:smoke".to_string(),
        ];
        let output_dir = Utf8Path::new(".proofrun");

        let candidates = build_candidates(&config, &obligations, "ci", output_dir);

        // Should have candidates for:
        // - tests.pkg[pkg=core] (covers pkg:core:tests)
        // - workspace.all-tests (covers pkg:core:tests via pkg:*:tests)
        // - mutation.diff[pkg=core] (covers pkg:core:mutation-diff)
        // - workspace.smoke (covers workspace:smoke)
        let ids: Vec<&str> = candidates.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"workspace.smoke"), "missing workspace.smoke");
        assert!(
            ids.contains(&"tests.pkg[pkg=core]"),
            "missing tests.pkg[pkg=core]"
        );
        assert!(
            ids.contains(&"mutation.diff[pkg=core]"),
            "missing mutation.diff[pkg=core]"
        );
        assert!(
            ids.contains(&"workspace.all-tests"),
            "missing workspace.all-tests"
        );

        // Verify sort order: cost ascending
        for i in 1..candidates.len() {
            assert!(
                candidates[i - 1].cost <= candidates[i].cost
                    || (candidates[i - 1].cost == candidates[i].cost
                        && candidates[i - 1].covers.len() >= candidates[i].covers.len()),
                "candidates not sorted correctly at index {}",
                i
            );
        }

        // All candidates should have non-empty covers
        for c in &candidates {
            assert!(!c.covers.is_empty(), "candidate {} has empty covers", c.id);
        }
    }

    // ── solve_exact_cover tests ──────────────────────────────────────────

    /// Helper: build a CandidateSurface for solver tests.
    fn candidate(id: &str, cost: f64, covers: &[&str]) -> CandidateSurface {
        CandidateSurface {
            id: id.to_string(),
            template: id.to_string(),
            cost,
            covers: covers.iter().map(|s| s.to_string()).collect(),
            run: vec!["echo".to_string(), id.to_string()],
        }
    }

    #[test]
    fn test_solve_exact_cover_single_obligation_single_candidate() {
        let obligations = vec!["ob:a".to_string()];
        let candidates = vec![candidate("s1", 5.0, &["ob:a"])];
        let result = solve_exact_cover(&obligations, &candidates).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "s1");
    }

    #[test]
    fn test_solve_exact_cover_minimum_cost() {
        // Two candidates both cover the same obligation; solver picks cheaper one
        let obligations = vec!["ob:a".to_string()];
        let candidates = vec![
            candidate("expensive", 10.0, &["ob:a"]),
            candidate("cheap", 2.0, &["ob:a"]),
        ];
        let result = solve_exact_cover(&obligations, &candidates).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "cheap");
    }

    #[test]
    fn test_solve_exact_cover_multiple_obligations() {
        // Three obligations: one candidate covers two, another covers the third
        let obligations = vec!["ob:a".to_string(), "ob:b".to_string(), "ob:c".to_string()];
        let candidates = vec![
            candidate("s1", 3.0, &["ob:a", "ob:b"]),
            candidate("s2", 4.0, &["ob:c"]),
            candidate("s3", 5.0, &["ob:a"]),
            candidate("s4", 5.0, &["ob:b"]),
        ];
        // Optimal: s1 (cost 3, covers a+b) + s2 (cost 4, covers c) = cost 7
        // Alternative: s3+s4+s2 = cost 14
        let result = solve_exact_cover(&obligations, &candidates).unwrap();
        let ids: Vec<&str> = result.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["s1", "s2"]);
        let total_cost: f64 = result.iter().map(|c| c.cost).sum();
        assert!((total_cost - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_solve_exact_cover_uncoverable_obligation() {
        let obligations = vec!["ob:a".to_string(), "ob:b".to_string()];
        let candidates = vec![candidate("s1", 1.0, &["ob:a"])];
        // ob:b has no covering candidate
        let result = solve_exact_cover(&obligations, &candidates);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("ob:b"),
            "error should mention uncoverable obligation, got: {err_msg}"
        );
    }

    #[test]
    fn test_solve_exact_cover_tiebreak_fewer_surfaces() {
        // Two solutions with equal cost: one uses 1 surface, other uses 2
        let obligations = vec!["ob:a".to_string(), "ob:b".to_string()];
        let candidates = vec![
            candidate("combo", 6.0, &["ob:a", "ob:b"]),
            candidate("s1", 3.0, &["ob:a"]),
            candidate("s2", 3.0, &["ob:b"]),
        ];
        // combo: cost 6, count 1
        // s1+s2: cost 6, count 2
        // Tie-break: fewer surfaces wins → combo
        let result = solve_exact_cover(&obligations, &candidates).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "combo");
    }

    #[test]
    fn test_solve_exact_cover_tiebreak_lexicographic_ids() {
        // Two solutions: same cost, same count, different ids
        let obligations = vec!["ob:a".to_string()];
        let candidates = vec![
            candidate("beta", 5.0, &["ob:a"]),
            candidate("alpha", 5.0, &["ob:a"]),
        ];
        // Same cost, same count (1), tie-break by sorted ids: ["alpha"] < ["beta"]
        let result = solve_exact_cover(&obligations, &candidates).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "alpha");
    }

    #[test]
    fn test_solve_exact_cover_result_sorted_by_id() {
        let obligations = vec!["ob:a".to_string(), "ob:b".to_string(), "ob:c".to_string()];
        let candidates = vec![
            candidate("z-surface", 1.0, &["ob:c"]),
            candidate("a-surface", 1.0, &["ob:a"]),
            candidate("m-surface", 1.0, &["ob:b"]),
        ];
        let result = solve_exact_cover(&obligations, &candidates).unwrap();
        let ids: Vec<&str> = result.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["a-surface", "m-surface", "z-surface"]);
    }

    // ── Property-based tests ─────────────────────────────────────────────

    /// Pool of package names for property test generation.
    const PKG_POOL: &[&str] = &["alpha", "beta", "gamma"];

    /// Strategy: pick 1-3 distinct package names from the pool.
    fn arb_pkg_names() -> impl Strategy<Value = Vec<String>> {
        proptest::sample::subsequence(PKG_POOL, 1..=3)
            .prop_map(|names| names.into_iter().map(|s| s.to_string()).collect())
    }

    /// Strategy: generate obligations from package names plus optional workspace obligations.
    fn arb_obligations(pkg_names: Vec<String>) -> impl Strategy<Value = Vec<String>> {
        let mut pkg_obs: Vec<String> = Vec::new();
        for name in &pkg_names {
            // Each package gets at least one pkg obligation
            pkg_obs.push(format!("pkg:{name}:tests"));
        }
        let base = pkg_obs;
        proptest::bool::ANY.prop_map(move |add_workspace| {
            let mut obs = base.clone();
            if add_workspace {
                obs.push("workspace:smoke".to_string());
            }
            obs.sort();
            obs.dedup();
            obs
        })
    }

    /// Strategy: generate a mix of pkg-bearing and non-pkg surface templates.
    fn arb_surface_templates() -> impl Strategy<Value = Vec<SurfaceTemplate>> {
        let pkg_template = (1u32..=20).prop_map(|cost| SurfaceTemplate {
            id: "tests.pkg".to_string(),
            covers: vec!["pkg:{pkg}:tests".to_string()],
            cost: cost as f64,
            run: vec![
                "cargo".to_string(),
                "nextest".to_string(),
                "run".to_string(),
                "--profile".to_string(),
                "{profile}".to_string(),
                "-E".to_string(),
                "package({pkg})".to_string(),
            ],
        });

        let mutation_template = (1u32..=20).prop_map(|cost| SurfaceTemplate {
            id: "mutation.diff".to_string(),
            covers: vec!["pkg:{pkg}:mutation-diff".to_string()],
            cost: cost as f64,
            run: vec![
                "cargo".to_string(),
                "mutants".to_string(),
                "--in-diff".to_string(),
                "{artifacts.diff_patch}".to_string(),
                "--package".to_string(),
                "{pkg}".to_string(),
            ],
        });

        let workspace_smoke = (1u32..=20).prop_map(|cost| SurfaceTemplate {
            id: "workspace.smoke".to_string(),
            covers: vec!["workspace:smoke".to_string()],
            cost: cost as f64,
            run: vec![
                "cargo".to_string(),
                "test".to_string(),
                "--workspace".to_string(),
            ],
        });

        let all_tests = (1u32..=20).prop_map(|cost| SurfaceTemplate {
            id: "workspace.all-tests".to_string(),
            covers: vec!["pkg:*:tests".to_string()],
            cost: cost as f64,
            run: vec![
                "cargo".to_string(),
                "nextest".to_string(),
                "run".to_string(),
                "--profile".to_string(),
                "{profile}".to_string(),
                "--workspace".to_string(),
            ],
        });

        // Pick 1-3 templates from the pool of template strategies
        proptest::sample::subsequence(vec![0u8, 1, 2, 3], 1..=3).prop_flat_map(move |indices| {
            let strats: Vec<BoxedStrategy<SurfaceTemplate>> = indices
                .iter()
                .map(|&i| match i {
                    0 => pkg_template.clone().boxed(),
                    1 => mutation_template.clone().boxed(),
                    2 => workspace_smoke.clone().boxed(),
                    _ => all_tests.clone().boxed(),
                })
                .collect();
            strats.into_iter().collect::<Vec<_>>().into_iter().fold(
                Just(Vec::new()).boxed(),
                |acc, strat| {
                    (acc, strat)
                        .prop_map(|(mut v, s)| {
                            v.push(s);
                            v
                        })
                        .boxed()
                },
            )
        })
    }

    /// Strategy: generate a full scenario for candidate expansion testing.
    fn arb_candidate_scenario() -> impl Strategy<Value = (Config, Vec<String>, String)> {
        arb_pkg_names()
            .prop_flat_map(|pkg_names| {
                (
                    arb_surface_templates(),
                    arb_obligations(pkg_names),
                    proptest::sample::select(vec!["ci".to_string(), "local".to_string()]),
                )
            })
            .prop_map(|(surfaces, obligations, profile)| {
                let config = test_config(surfaces);
                (config, obligations, profile)
            })
    }

    // Feature: rust-native-planner, Property 5: Candidate expansion produces correct bindings and covers
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        #[test]
        fn prop_candidate_expansion_correct_bindings_and_covers(
            (config, obligations, profile) in arb_candidate_scenario()
        ) {
            let output_dir = Utf8Path::new(".proofrun");
            let candidates = build_candidates(&config, &obligations, &profile, output_dir);

            // Collect distinct packages from pkg:* obligations
            let distinct_pkgs: BTreeSet<String> = obligations
                .iter()
                .filter(|ob| ob.starts_with("pkg:"))
                .filter_map(|ob| {
                    let parts: Vec<&str> = ob.split(':').collect();
                    if parts.len() >= 3 {
                        Some(parts[1].to_string())
                    } else {
                        None
                    }
                })
                .collect();

            for surface_tpl in &config.surfaces {
                let has_pkg = uses_pkg(surface_tpl);

                // Collect candidates that came from this template
                let from_template: Vec<&CandidateSurface> = candidates
                    .iter()
                    .filter(|c| c.template == surface_tpl.id)
                    .collect();

                if has_pkg {
                    // (a) Pkg-bearing templates expand once per distinct package
                    // (only those packages whose obligations actually match the cover patterns)
                    let expanded_pkgs: BTreeSet<String> = from_template
                        .iter()
                        .filter_map(|c| {
                            // Extract pkg from id like "template[pkg=name]"
                            let bracket_start = c.id.find('[');
                            let bracket_end = c.id.find(']');
                            if let (Some(s), Some(e)) = (bracket_start, bracket_end) {
                                let binding_str = &c.id[s + 1..e];
                                binding_str.strip_prefix("pkg=").map(|p| p.to_string())
                            } else {
                                None
                            }
                        })
                        .collect();

                    // Each expanded pkg must be from the distinct_pkgs set
                    for pkg in &expanded_pkgs {
                        prop_assert!(
                            distinct_pkgs.contains(pkg),
                            "Candidate expanded for pkg '{}' which is not in obligations. \
                             distinct_pkgs={:?}",
                            pkg,
                            distinct_pkgs
                        );
                    }

                    // (c) Verify id format: template[pkg=name]
                    for c in &from_template {
                        let expected_prefix = format!("{}[pkg=", surface_tpl.id);
                        prop_assert!(
                            c.id.starts_with(&expected_prefix) && c.id.ends_with(']'),
                            "Pkg-bearing candidate id '{}' doesn't match format '{}[pkg=name]'",
                            c.id,
                            surface_tpl.id
                        );
                    }
                } else {
                    // (b) Non-pkg templates expand exactly once (if they cover anything)
                    prop_assert!(
                        from_template.len() <= 1,
                        "Non-pkg template '{}' expanded {} times, expected at most 1",
                        surface_tpl.id,
                        from_template.len()
                    );

                    // (c) Verify id format: just template_id (no brackets)
                    for c in &from_template {
                        prop_assert!(
                            c.id == surface_tpl.id,
                            "Non-pkg candidate id '{}' should be '{}'",
                            c.id,
                            surface_tpl.id
                        );
                    }
                }
            }

            // (d) No {profile}, {pkg}, or {artifacts.diff_patch} placeholders remain in run args
            for c in &candidates {
                for arg in &c.run {
                    prop_assert!(
                        !arg.contains("{profile}"),
                        "Candidate '{}' has unsubstituted {{profile}} in run arg: '{}'",
                        c.id,
                        arg
                    );
                    prop_assert!(
                        !arg.contains("{pkg}"),
                        "Candidate '{}' has unsubstituted {{pkg}} in run arg: '{}'",
                        c.id,
                        arg
                    );
                    prop_assert!(
                        !arg.contains("{artifacts.diff_patch}"),
                        "Candidate '{}' has unsubstituted {{artifacts.diff_patch}} in run arg: '{}'",
                        c.id,
                        arg
                    );
                }
            }

            // (e) Each candidate's covers are a subset of the obligations
            let obligation_set: BTreeSet<&str> = obligations.iter().map(|s| s.as_str()).collect();
            for c in &candidates {
                for cover in &c.covers {
                    prop_assert!(
                        obligation_set.contains(cover.as_str()),
                        "Candidate '{}' covers '{}' which is not in obligations {:?}",
                        c.id,
                        cover,
                        obligations
                    );
                }

                // Additionally verify covers match fnmatch against the template's cover patterns
                // by re-expanding the template's cover patterns with the candidate's binding
                // and checking each covered obligation matches at least one pattern
                let binding: BTreeMap<String, String> = if let Some(s) = c.id.find('[') {
                    let binding_str = &c.id[s + 1..c.id.len() - 1];
                    binding_str
                        .split(',')
                        .filter_map(|kv| {
                            let mut parts = kv.splitn(2, '=');
                            let k = parts.next()?;
                            let v = parts.next()?;
                            Some((k.to_string(), v.to_string()))
                        })
                        .collect()
                } else {
                    BTreeMap::new()
                };

                let mut values = BTreeMap::new();
                values.insert("profile".to_string(), profile.clone());
                values.insert(
                    "artifacts.diff_patch".to_string(),
                    format!("{}/diff.patch", output_dir),
                );
                for (k, v) in &binding {
                    values.insert(k.clone(), v.clone());
                }

                let expanded_patterns: Vec<String> = config
                    .surfaces
                    .iter()
                    .find(|s| s.id == c.template)
                    .map(|s| {
                        s.covers
                            .iter()
                            .filter_map(|pat| expand_template(pat, &values).ok())
                            .collect()
                    })
                    .unwrap_or_default();

                for cover in &c.covers {
                    let matches_pattern = expanded_patterns
                        .iter()
                        .any(|pat| fnmatch_match(pat, cover));
                    prop_assert!(
                        matches_pattern,
                        "Candidate '{}' cover '{}' doesn't match any expanded pattern {:?}",
                        c.id,
                        cover,
                        expanded_patterns
                    );
                }
            }
        }
    }

    // Feature: rust-native-planner, Property 6: Candidate list invariants
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(100))]

        #[test]
        fn prop_candidate_list_invariants(
            (config, obligations, profile) in arb_candidate_scenario()
        ) {
            // **Validates: Requirements 6.5, 6.7, 6.8**
            let output_dir = Utf8Path::new(".proofrun");
            let candidates = build_candidates(&config, &obligations, &profile, output_dir);

            // (a) Every candidate has non-empty covers
            for c in &candidates {
                prop_assert!(
                    !c.covers.is_empty(),
                    "Candidate '{}' has empty covers",
                    c.id
                );
            }

            // (b) No two candidates share the same (id, covers) tuple
            let mut seen: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
            for c in &candidates {
                let key = (c.id.clone(), c.covers.clone());
                prop_assert!(
                    seen.insert(key.clone()),
                    "Duplicate (id, covers) tuple found: ({}, {:?})",
                    c.id,
                    c.covers
                );
            }

            // (c) List is sorted by (cost ascending, covers count descending, id ascending)
            for i in 1..candidates.len() {
                let prev = &candidates[i - 1];
                let curr = &candidates[i];
                let order = prev
                    .cost
                    .partial_cmp(&curr.cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| curr.covers.len().cmp(&prev.covers.len()))
                    .then_with(|| prev.id.cmp(&curr.id));
                prop_assert!(
                    order != std::cmp::Ordering::Greater,
                    "Candidates not sorted at index {}: ({}, cost={}, covers={}) should come before ({}, cost={}, covers={})",
                    i,
                    prev.id, prev.cost, prev.covers.len(),
                    curr.id, curr.cost, curr.covers.len()
                );
            }
        }
    }

    // Feature: rust-native-planner, Property 7: Solver finds minimum-cost complete cover
    // **Validates: Requirements 7.1, 7.6, 7.7**

    /// Strategy: generate a random solver instance with 3-6 obligations and 4-8 candidates,
    /// ensuring every obligation is covered by at least one candidate.
    fn arb_solver_instance() -> impl Strategy<Value = (Vec<String>, Vec<CandidateSurface>)> {
        // Pick obligation count 3..=6, candidate count 4..=8
        (3usize..=6, 4usize..=8).prop_flat_map(|(num_obs, num_cands)| {
            let obligations: Vec<String> = (0..num_obs).map(|i| format!("ob_{i}")).collect();
            let ob_count = obligations.len();

            // For each candidate, generate a random non-empty subset of obligations and a cost
            let cand_strats: Vec<_> = (0..num_cands)
                .map(move |cand_idx| {
                    // Generate a bitmask for which obligations this candidate covers (at least 1 bit set)
                    (1u64..((1u64 << ob_count) as u64), 1u32..=20).prop_map(
                        move |(mask, cost_int)| {
                            let mut covers: Vec<String> = Vec::new();
                            for ob_i in 0..ob_count {
                                if mask & (1u64 << ob_i) != 0 {
                                    covers.push(format!("ob_{ob_i}"));
                                }
                            }
                            covers.sort();
                            CandidateSurface {
                                id: format!("c_{cand_idx}"),
                                template: format!("c_{cand_idx}"),
                                cost: cost_int as f64,
                                covers,
                                run: vec!["echo".to_string(), format!("c_{cand_idx}")],
                            }
                        },
                    )
                })
                .collect();

            // Collect all candidate strategies into a single Vec<CandidateSurface>
            let obligations_clone = obligations.clone();
            cand_strats
                .into_iter()
                .fold(
                    Just(Vec::new()).boxed(),
                    |acc: proptest::strategy::BoxedStrategy<Vec<CandidateSurface>>, strat| {
                        (acc, strat)
                            .prop_map(|(mut v, c)| {
                                v.push(c);
                                v
                            })
                            .boxed()
                    },
                )
                .prop_map(move |mut candidates| {
                    // Ensure every obligation is covered by at least one candidate.
                    // Add a catch-all candidate if needed.
                    let covered: BTreeSet<String> = candidates
                        .iter()
                        .flat_map(|c| c.covers.iter().cloned())
                        .collect();
                    let uncovered: Vec<String> = obligations_clone
                        .iter()
                        .filter(|ob| !covered.contains(*ob))
                        .cloned()
                        .collect();
                    if !uncovered.is_empty() {
                        let mut catch_all_covers = uncovered;
                        catch_all_covers.sort();
                        candidates.push(CandidateSurface {
                            id: format!("c_{}", candidates.len()),
                            template: format!("c_{}", candidates.len()),
                            cost: 20.0,
                            covers: catch_all_covers,
                            run: vec!["echo".to_string(), format!("c_{}", candidates.len())],
                        });
                    }
                    (obligations_clone.clone(), candidates)
                })
        })
    }

    /// Brute-force: enumerate all subsets of candidates, find the minimum-cost
    /// complete cover. Returns the best score tuple (cost, count, sorted_ids).
    fn brute_force_best_cover(
        obligations: &[String],
        candidates: &[CandidateSurface],
    ) -> Option<(f64, usize, Vec<String>)> {
        let ob_set: BTreeSet<String> = obligations.iter().cloned().collect();
        let n = candidates.len();
        let mut best: Option<(f64, usize, Vec<String>)> = None;

        for mask in 1u64..(1u64 << n) {
            // Collect covered obligations and check for duplicate ids
            let mut covered = BTreeSet::new();
            let mut cost = 0.0f64;
            let mut ids = Vec::new();
            let mut id_set = BTreeSet::new();
            let mut has_dup_id = false;

            for i in 0..n {
                if mask & (1u64 << i) != 0 {
                    let c = &candidates[i];
                    if !id_set.insert(c.id.clone()) {
                        has_dup_id = true;
                        break;
                    }
                    for ob in &c.covers {
                        covered.insert(ob.clone());
                    }
                    cost += c.cost;
                    ids.push(c.id.clone());
                }
            }

            if has_dup_id {
                continue;
            }

            // Check if this subset covers all obligations
            if covered.is_superset(&ob_set) {
                ids.sort();
                let score = (cost, ids.len(), ids);
                if best.is_none() || score < *best.as_ref().unwrap() {
                    best = Some(score);
                }
            }
        }

        best
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(50))]

        #[test]
        fn prop_solver_finds_minimum_cost_complete_cover(
            (obligations, candidates) in arb_solver_instance()
        ) {
            let result = solve_exact_cover(&obligations, &candidates).unwrap();

            // (a) Solution covers all obligations
            let covered: BTreeSet<String> = result
                .iter()
                .flat_map(|c| c.covers.iter().cloned())
                .collect();
            let ob_set: BTreeSet<String> = obligations.iter().cloned().collect();
            prop_assert!(
                covered.is_superset(&ob_set),
                "Solution does not cover all obligations. Missing: {:?}",
                ob_set.difference(&covered).collect::<Vec<_>>()
            );

            // (b) Solution is sorted by id ascending
            for i in 1..result.len() {
                prop_assert!(
                    result[i - 1].id < result[i].id,
                    "Result not sorted by id: '{}' should come before '{}'",
                    result[i - 1].id,
                    result[i].id
                );
            }

            // (c) Solution cost is minimal — verify by brute-force
            let solver_cost: f64 = result.iter().map(|c| c.cost).sum();
            let mut solver_ids: Vec<String> = result.iter().map(|c| c.id.clone()).collect();
            solver_ids.sort();
            let solver_score = (solver_cost, result.len(), solver_ids);

            let brute_best = brute_force_best_cover(&obligations, &candidates)
                .expect("brute force should find at least one cover");

            prop_assert!(
                solver_score == brute_best,
                "Solver score {:?} != brute-force best {:?}",
                solver_score,
                brute_best
            );
        }
    }
}
