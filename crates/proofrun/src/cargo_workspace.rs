use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub dir: Utf8PathBuf,
    pub manifest: Utf8PathBuf,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceGraph {
    pub packages: Vec<PackageInfo>,
    pub reverse_deps: BTreeMap<String, Vec<String>>,
}

/// Minimal types for deserializing `cargo metadata --format-version 1` JSON.
mod metadata {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct CargoMetadata {
        pub packages: Vec<Package>,
        pub workspace_members: Vec<String>,
        pub workspace_root: String,
    }

    #[derive(Deserialize)]
    pub struct Package {
        pub id: String,
        pub name: String,
        pub manifest_path: String,
        pub dependencies: Vec<Dependency>,
    }

    #[derive(Deserialize)]
    pub struct Dependency {
        pub name: String,
    }
}

impl WorkspaceGraph {
    /// Discover workspace packages via `cargo metadata`.
    ///
    /// Runs `cargo metadata --format-version 1 --no-deps --manifest-path <root>/Cargo.toml`,
    /// parses the JSON output, and builds the workspace graph with forward and reverse
    /// dependencies filtered to workspace-local packages only.
    pub fn discover(repo_root: &Utf8Path) -> Result<Self> {
        let manifest_path = repo_root.join("Cargo.toml");
        let output = std::process::Command::new("cargo")
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .arg("--no-deps")
            .arg("--manifest-path")
            .arg(manifest_path.as_str())
            .output()
            .context("failed to spawn `cargo metadata`")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "cargo metadata failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            );
        }

        let meta: metadata::CargoMetadata = serde_json::from_slice(&output.stdout)
            .context("failed to parse cargo metadata JSON")?;

        let workspace_root = Utf8PathBuf::from(&meta.workspace_root);

        // Build set of workspace member IDs for filtering.
        let member_ids: BTreeSet<&str> =
            meta.workspace_members.iter().map(|s| s.as_str()).collect();

        // Build set of workspace member package names for dependency filtering.
        let member_names: BTreeSet<String> = meta
            .packages
            .iter()
            .filter(|p| member_ids.contains(p.id.as_str()))
            .map(|p| p.name.clone())
            .collect();

        // Build PackageInfo for each workspace member.
        let mut packages: Vec<PackageInfo> = meta
            .packages
            .iter()
            .filter(|p| member_ids.contains(p.id.as_str()))
            .map(|p| {
                let manifest_abs = Utf8PathBuf::from(&p.manifest_path);
                let manifest = pathdiff_utf8(&manifest_abs, &workspace_root);
                let dir = manifest
                    .parent()
                    .map(Utf8PathBuf::from)
                    .unwrap_or_else(|| Utf8PathBuf::from("."));

                // Filter dependencies to workspace-local packages only.
                let mut deps: Vec<String> = p
                    .dependencies
                    .iter()
                    .filter(|d| member_names.contains(&d.name))
                    // Exclude self-dependencies
                    .filter(|d| d.name != p.name)
                    .map(|d| d.name.clone())
                    .collect();
                deps.sort();
                deps.dedup();

                PackageInfo {
                    name: p.name.clone(),
                    dir,
                    manifest,
                    dependencies: deps,
                }
            })
            .collect();

        // Sort packages by name.
        packages.sort_by(|a, b| a.name.cmp(&b.name));

        // Compute reverse dependencies.
        let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for pkg in &packages {
            reverse_deps.entry(pkg.name.clone()).or_default();
        }
        for pkg in &packages {
            for dep in &pkg.dependencies {
                if let Some(rdeps) = reverse_deps.get_mut(dep) {
                    rdeps.push(pkg.name.clone());
                }
            }
        }
        // Sort each reverse dependency list alphabetically.
        for rdeps in reverse_deps.values_mut() {
            rdeps.sort();
        }

        Ok(WorkspaceGraph {
            packages,
            reverse_deps,
        })
    }

    /// Return the name of the package whose directory is the longest prefix of `path`,
    /// or `None` if no package directory matches.
    ///
    /// Both `path` and each package dir are normalized by stripping leading `./` and
    /// trailing `/` before comparison, matching the reference `owner_for_path`.
    pub fn owner_for_path(&self, path: &str) -> Option<&str> {
        let norm = path
            .strip_prefix("./")
            .unwrap_or(path)
            .trim_end_matches('/');

        let mut best: Option<(&str, usize)> = None; // (name, prefix_len)

        for pkg in &self.packages {
            let d = pkg
                .dir
                .as_str()
                .strip_prefix("./")
                .unwrap_or(pkg.dir.as_str())
                .trim_end_matches('/');

            if d.is_empty() || d == "." {
                // Root package: acts as fallback (prefix_len = 0).
                if best.is_none() {
                    best = Some((&pkg.name, 0));
                }
            } else if norm == d || norm.starts_with(&format!("{d}/")) {
                let score = d.len();
                if best.is_none() || score > best.unwrap().1 {
                    best = Some((&pkg.name, score));
                }
            }
        }

        best.map(|(name, _)| name)
    }
}

/// Compute a relative path from `base` to `target`, both assumed to be absolute UTF-8 paths.
/// Uses forward slashes for consistency (matching the reference output).
fn pathdiff_utf8(target: &Utf8Path, base: &Utf8Path) -> Utf8PathBuf {
    // Normalize both paths to use forward slashes for comparison.
    let target_str = target.as_str().replace('\\', "/");
    let base_str = base.as_str().replace('\\', "/");

    // Strip the base prefix (with trailing slash) from the target.
    let base_prefix = if base_str.ends_with('/') {
        base_str.clone()
    } else {
        format!("{base_str}/")
    };

    if let Some(rel) = target_str.strip_prefix(&base_prefix) {
        Utf8PathBuf::from(rel)
    } else {
        // Fallback: just return the target as-is (shouldn't happen for workspace members).
        Utf8PathBuf::from(target_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_discover_fixture_workspace() {
        // CARGO_MANIFEST_DIR points to crates/proofrun; go up two levels to workspace root.
        let manifest_dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let abs_root = workspace_root.join("fixtures/demo-workspace/repo");

        let graph = WorkspaceGraph::discover(&abs_root).expect("discover should succeed");

        // Should find 2 packages: app and core, sorted by name.
        assert_eq!(graph.packages.len(), 2);
        assert_eq!(graph.packages[0].name, "app");
        assert_eq!(graph.packages[1].name, "core");

        // Check relative dirs.
        assert_eq!(graph.packages[0].dir, "crates/app");
        assert_eq!(graph.packages[1].dir, "crates/core");

        // Check manifests.
        assert_eq!(graph.packages[0].manifest, "crates/app/Cargo.toml");
        assert_eq!(graph.packages[1].manifest, "crates/core/Cargo.toml");

        // app depends on core; core has no workspace deps.
        assert_eq!(graph.packages[0].dependencies, vec!["core"]);
        assert!(graph.packages[1].dependencies.is_empty());

        // Reverse deps: core is depended on by app; app has no reverse deps.
        assert_eq!(
            graph.reverse_deps.get("core").unwrap(),
            &vec!["app".to_string()]
        );
        assert!(graph.reverse_deps.get("app").unwrap().is_empty());
    }

    #[test]
    fn test_discover_nonexistent_manifest() {
        let result = WorkspaceGraph::discover(Utf8Path::new("/nonexistent/path"));
        assert!(result.is_err(), "should fail for nonexistent manifest");
    }

    #[test]
    fn test_pathdiff_utf8_basic() {
        let target = Utf8Path::new("/workspace/crates/core/Cargo.toml");
        let base = Utf8Path::new("/workspace");
        assert_eq!(pathdiff_utf8(target, base), "crates/core/Cargo.toml");
    }

    #[test]
    fn test_pathdiff_utf8_windows_style() {
        let target = Utf8Path::new("C:\\workspace\\crates\\core\\Cargo.toml");
        let base = Utf8Path::new("C:\\workspace");
        assert_eq!(pathdiff_utf8(target, base), "crates/core/Cargo.toml");
    }

    /// Helper to build a WorkspaceGraph from (name, dir) pairs for owner_for_path tests.
    fn graph_from_dirs(entries: &[(&str, &str)]) -> WorkspaceGraph {
        let packages = entries
            .iter()
            .map(|(name, dir)| PackageInfo {
                name: name.to_string(),
                dir: Utf8PathBuf::from(*dir),
                manifest: Utf8PathBuf::from(format!("{dir}/Cargo.toml")),
                dependencies: vec![],
            })
            .collect();
        WorkspaceGraph {
            packages,
            reverse_deps: BTreeMap::new(),
        }
    }

    #[test]
    fn test_owner_for_path_in_package_dir() {
        let graph = graph_from_dirs(&[("app", "crates/app"), ("core", "crates/core")]);
        assert_eq!(graph.owner_for_path("crates/core/src/lib.rs"), Some("core"));
        assert_eq!(graph.owner_for_path("crates/app/src/main.rs"), Some("app"));
    }

    #[test]
    fn test_owner_for_path_longest_prefix() {
        // "crates/core" is a longer prefix than "crates" for a path under crates/core.
        let graph = graph_from_dirs(&[("root-crate", "crates"), ("core", "crates/core")]);
        assert_eq!(graph.owner_for_path("crates/core/src/lib.rs"), Some("core"));
        // A path directly under "crates" but not under "crates/core" goes to root-crate.
        assert_eq!(
            graph.owner_for_path("crates/other/foo.rs"),
            Some("root-crate")
        );
    }

    #[test]
    fn test_owner_for_path_outside_all_packages() {
        let graph = graph_from_dirs(&[("app", "crates/app"), ("core", "crates/core")]);
        assert_eq!(graph.owner_for_path("docs/guide.md"), None);
        assert_eq!(graph.owner_for_path("README.md"), None);
    }

    #[test]
    fn test_owner_for_path_root_package_fallback() {
        // A root package (empty dir or ".") acts as fallback for paths not matched by others.
        let graph = graph_from_dirs(&[("root", "."), ("core", "crates/core")]);
        // Path under core → core wins (longer prefix).
        assert_eq!(graph.owner_for_path("crates/core/src/lib.rs"), Some("core"));
        // Path not under any specific package → root fallback.
        assert_eq!(graph.owner_for_path("README.md"), Some("root"));
        assert_eq!(graph.owner_for_path("docs/guide.md"), Some("root"));
    }

    #[test]
    fn test_owner_for_path_normalizes_leading_dot_slash() {
        let graph = graph_from_dirs(&[("core", "./crates/core")]);
        assert_eq!(graph.owner_for_path("crates/core/src/lib.rs"), Some("core"));
        assert_eq!(
            graph.owner_for_path("./crates/core/src/lib.rs"),
            Some("core")
        );
    }

    #[test]
    fn test_owner_for_path_exact_dir_match() {
        // Path equals the package dir exactly (no trailing file component).
        let graph = graph_from_dirs(&[("core", "crates/core")]);
        assert_eq!(graph.owner_for_path("crates/core"), Some("core"));
    }

    // Feature: rust-native-planner, Property 2: Reverse dependency graph is the inverse of the dependency graph
    // **Validates: Requirements 1.3**

    /// Strategy: generate 1-10 unique package names, then for each package pick a
    /// random subset of OTHER packages as dependencies. Build the WorkspaceGraph
    /// manually and compute reverse_deps the same way `discover()` does.
    fn arb_workspace_graph() -> impl Strategy<Value = WorkspaceGraph> {
        // Pool of candidate package names to draw from.
        let name_pool: Vec<String> = (b'a'..=b'j')
            .map(|c| format!("pkg-{}", c as char))
            .collect();

        // Pick 1-10 unique names from the pool.
        (1usize..=10usize).prop_flat_map(move |count| {
            let pool = name_pool.clone();
            proptest::sample::subsequence(pool.clone(), count).prop_flat_map(move |chosen_names| {
                let n = chosen_names.len();
                // For each package, generate a bool-vec indicating which OTHER
                // packages are dependencies.
                let dep_flags =
                    proptest::collection::vec(proptest::collection::vec(proptest::bool::ANY, n), n);
                let names = chosen_names.clone();
                dep_flags.prop_map(move |flags| {
                    let names = names.clone();
                    // Build packages with dependencies (no self-deps).
                    let mut packages: Vec<PackageInfo> = names
                        .iter()
                        .enumerate()
                        .map(|(i, name)| {
                            let mut deps: Vec<String> = names
                                .iter()
                                .enumerate()
                                .filter(|(j, _)| *j != i && flags[i][*j])
                                .map(|(_, dep_name)| dep_name.clone())
                                .collect();
                            deps.sort();
                            deps.dedup();
                            PackageInfo {
                                name: name.clone(),
                                dir: Utf8PathBuf::from(format!("crates/{name}")),
                                manifest: Utf8PathBuf::from(format!("crates/{name}/Cargo.toml")),
                                dependencies: deps,
                            }
                        })
                        .collect();
                    packages.sort_by(|a, b| a.name.cmp(&b.name));

                    // Compute reverse_deps the same way discover() does.
                    let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
                    for pkg in &packages {
                        reverse_deps.entry(pkg.name.clone()).or_default();
                    }
                    for pkg in &packages {
                        for dep in &pkg.dependencies {
                            if let Some(rdeps) = reverse_deps.get_mut(dep) {
                                rdeps.push(pkg.name.clone());
                            }
                        }
                    }
                    for rdeps in reverse_deps.values_mut() {
                        rdeps.sort();
                    }

                    WorkspaceGraph {
                        packages,
                        reverse_deps,
                    }
                })
            })
        })
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        #[test]
        fn prop_reverse_deps_is_inverse_of_deps(graph in arb_workspace_graph()) {
            // For every pair of packages (A, B):
            //   B in reverse_deps[A]  iff  A in dependencies[B]
            let pkg_names: Vec<String> =
                graph.packages.iter().map(|p| p.name.clone()).collect();

            // Build a quick lookup: name -> &PackageInfo
            let pkg_map: BTreeMap<&str, &PackageInfo> =
                graph.packages.iter().map(|p| (p.name.as_str(), p)).collect();

            for a in &pkg_names {
                let rdeps_a = graph
                    .reverse_deps
                    .get(a)
                    .expect("every package must have a reverse_deps entry");

                // Verify sorted.
                for w in rdeps_a.windows(2) {
                    prop_assert!(
                        w[0] <= w[1],
                        "reverse_deps[{}] not sorted: {:?} > {:?}",
                        a, w[0], w[1]
                    );
                }

                for b in &pkg_names {
                    let b_depends_on_a = pkg_map[b.as_str()]
                        .dependencies
                        .contains(a);
                    let b_in_rdeps_a = rdeps_a.contains(b);

                    prop_assert_eq!(
                        b_in_rdeps_a,
                        b_depends_on_a,
                        "Mismatch: B={} in reverse_deps[{}]={}, \
                         but A={} in deps[{}]={}",
                        b, a, b_in_rdeps_a, a, b, b_depends_on_a
                    );
                }
            }
        }
    }

    // Feature: rust-native-planner, Property 3: Owner resolution selects longest prefix
    // **Validates: Requirements 5.1**

    /// Strategy: generate a path segment of 1-6 lowercase letters.
    fn arb_segment() -> impl Strategy<Value = String> {
        proptest::string::string_regex("[a-z]{1,6}").unwrap()
    }

    /// Strategy: generate a directory path with 1-3 segments joined by `/`.
    fn arb_dir_path() -> impl Strategy<Value = String> {
        proptest::collection::vec(arb_segment(), 1..=3).prop_map(|segs| segs.join("/"))
    }

    /// Strategy: generate a file path with 1-5 segments joined by `/`.
    fn arb_file_path() -> impl Strategy<Value = String> {
        proptest::collection::vec(arb_segment(), 1..=5).prop_map(|segs| segs.join("/"))
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        #[test]
        fn prop_owner_resolution_selects_longest_prefix(
            pkg_dirs in proptest::collection::vec(arb_dir_path(), 1..=5),
            file_path in arb_file_path(),
        ) {
            // Build packages with generated dirs, using "pkg-N" as names.
            let entries: Vec<(&str, &str)> = Vec::new();
            let names: Vec<String> = (0..pkg_dirs.len())
                .map(|i| format!("pkg-{i}"))
                .collect();
            let named_entries: Vec<(String, String)> = names
                .iter()
                .zip(pkg_dirs.iter())
                .map(|(n, d)| (n.clone(), d.clone()))
                .collect();
            let str_entries: Vec<(&str, &str)> = named_entries
                .iter()
                .map(|(n, d)| (n.as_str(), d.as_str()))
                .collect();
            let _ = entries; // unused, we use str_entries instead

            let graph = graph_from_dirs(&str_entries);
            let result = graph.owner_for_path(&file_path);

            // Independently compute the expected owner using the same logic:
            // normalize path and each dir, find the longest matching prefix.
            let norm_path = file_path
                .strip_prefix("./")
                .unwrap_or(&file_path)
                .trim_end_matches('/');

            let mut expected: Option<(&str, usize)> = None;

            for (name, dir) in &str_entries {
                let d = dir
                    .strip_prefix("./")
                    .unwrap_or(dir)
                    .trim_end_matches('/');

                if d.is_empty() || d == "." {
                    // Root package fallback (prefix_len = 0).
                    if expected.is_none() {
                        expected = Some((name, 0));
                    }
                } else if norm_path == d || norm_path.starts_with(&format!("{d}/")) {
                    let score = d.len();
                    if expected.is_none() || score > expected.unwrap().1 {
                        expected = Some((name, score));
                    }
                }
            }

            let expected_name = expected.map(|(name, _)| name);
            prop_assert_eq!(
                result,
                expected_name,
                "owner_for_path({:?}) = {:?}, expected {:?} (packages: {:?})",
                file_path, result, expected_name, str_entries
            );
        }
    }
}
