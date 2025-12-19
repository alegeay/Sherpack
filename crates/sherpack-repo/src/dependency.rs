//! Dependency resolution with diamond detection and conditional filtering
//!
//! Key features:
//! - **Static disable**: Skip dependencies with `enabled: false`
//! - **Condition evaluation**: Skip dependencies based on values.yaml conditions
//! - **Diamond detection**: Error on conflicting versions
//! - **No automatic resolution**: Humans must resolve conflicts

use semver::{Version, VersionReq};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{RepoError, Result};
use crate::index::PackEntry;
use crate::lock::{LockFile, LockedDependency};

// Re-export core types for convenience
pub use sherpack_core::{Dependency, ResolvePolicy};

/// Reason why a dependency was skipped during resolution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// `enabled: false` in Pack.yaml
    StaticDisabled,
    /// `resolve: never` in Pack.yaml
    PolicyNever,
    /// Condition evaluated to false
    ConditionFalse { condition: String },
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaticDisabled => write!(f, "enabled: false"),
            Self::PolicyNever => write!(f, "resolve: never"),
            Self::ConditionFalse { condition } => write!(f, "condition '{}' is false", condition),
        }
    }
}

/// A dependency that was skipped during resolution
#[derive(Debug, Clone)]
pub struct SkippedDependency {
    /// The original dependency
    pub dependency: Dependency,
    /// Why it was skipped
    pub reason: SkipReason,
}

/// Result of filtering dependencies before resolution
#[derive(Debug, Default)]
pub struct FilterResult {
    /// Dependencies that should be resolved
    pub to_resolve: Vec<DependencySpec>,
    /// Dependencies that were skipped
    pub skipped: Vec<SkippedDependency>,
}

impl FilterResult {
    /// Check if any dependencies were skipped
    pub fn has_skipped(&self) -> bool {
        !self.skipped.is_empty()
    }

    /// Get a summary of skipped dependencies for display
    pub fn skipped_summary(&self) -> String {
        if self.skipped.is_empty() {
            return String::new();
        }

        self.skipped
            .iter()
            .map(|s| format!("  {} ({})", s.dependency.name, s.reason))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Filter dependencies based on enabled flag, resolve policy, and conditions
///
/// This should be called BEFORE resolution to skip dependencies that don't
/// need to be downloaded (for air-gapped environments).
///
/// # Arguments
/// * `deps` - Dependencies from Pack.yaml
/// * `values` - Values for condition evaluation (from values.yaml)
///
/// # Returns
/// A `FilterResult` containing dependencies to resolve and those that were skipped
pub fn filter_dependencies(deps: &[Dependency], values: &serde_json::Value) -> FilterResult {
    let mut result = FilterResult::default();

    for dep in deps {
        // Check static enabled flag
        if !dep.enabled {
            result.skipped.push(SkippedDependency {
                dependency: dep.clone(),
                reason: SkipReason::StaticDisabled,
            });
            continue;
        }

        // Check resolve policy
        match dep.resolve {
            ResolvePolicy::Never => {
                result.skipped.push(SkippedDependency {
                    dependency: dep.clone(),
                    reason: SkipReason::PolicyNever,
                });
                continue;
            }
            ResolvePolicy::Always => {
                // Always resolve, ignore condition
                result.to_resolve.push(DependencySpec::from(dep));
            }
            ResolvePolicy::WhenEnabled => {
                // Check condition if present
                if dep.should_resolve(values) {
                    result.to_resolve.push(DependencySpec::from(dep));
                } else if let Some(condition) = &dep.condition {
                    result.skipped.push(SkippedDependency {
                        dependency: dep.clone(),
                        reason: SkipReason::ConditionFalse {
                            condition: condition.clone(),
                        },
                    });
                }
            }
        }
    }

    result
}

/// Dependency specification for resolution (internal use)
///
/// This is a simplified view of `Dependency` used during resolution.
#[derive(Debug, Clone)]
pub struct DependencySpec {
    pub name: String,
    pub version: String,
    pub repository: String,
    pub condition: Option<String>,
    pub tags: Vec<String>,
    pub alias: Option<String>,
}

impl From<&Dependency> for DependencySpec {
    fn from(dep: &Dependency) -> Self {
        Self {
            name: dep.name.clone(),
            version: dep.version.clone(),
            repository: dep.repository.clone(),
            condition: dep.condition.clone(),
            tags: dep.tags.clone(),
            alias: dep.alias.clone(),
        }
    }
}

impl DependencySpec {
    /// Get effective name (alias or original name)
    pub fn effective_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    /// Parse version constraint
    pub fn version_req(&self) -> Result<VersionReq> {
        VersionReq::parse(&self.version).map_err(|e| RepoError::ResolutionFailed {
            message: format!("Invalid version constraint '{}': {}", self.version, e),
        })
    }
}

/// Resolved dependency with concrete version
#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: Version,
    pub repository: String,
    pub constraint: String,
    pub alias: Option<String>,
    pub download_url: String,
    pub digest: Option<String>,
    pub transitive_deps: Vec<String>,
    /// Who required this dependency
    pub required_by: Vec<String>,
}

impl ResolvedDependency {
    pub fn effective_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

/// Type alias for the pack fetcher function
type PackFetcher<'a> = Box<dyn Fn(&str, &str, &str) -> Result<PackEntry> + 'a>;

/// Dependency resolver
pub struct DependencyResolver<'a> {
    /// Function to fetch pack entry from repository
    fetch_pack: PackFetcher<'a>,
}

impl<'a> DependencyResolver<'a> {
    /// Create a new resolver with a fetch function
    pub fn new<F>(fetch_pack: F) -> Self
    where
        F: Fn(&str, &str, &str) -> Result<PackEntry> + 'a,
    {
        Self {
            fetch_pack: Box::new(fetch_pack),
        }
    }

    /// Resolve dependencies from specs
    ///
    /// This will error on any diamond dependency (same package required at different versions).
    /// Users must explicitly resolve conflicts using aliases or pinning versions.
    pub fn resolve(&self, deps: &[DependencySpec]) -> Result<DependencyGraph> {
        let mut graph = DependencyGraph::new();
        let mut queue: VecDeque<(DependencySpec, String)> = deps
            .iter()
            .map(|d| (d.clone(), "root".to_string()))
            .collect();

        let mut seen: HashSet<String> = HashSet::new();

        while let Some((dep, required_by)) = queue.pop_front() {
            let effective_name = dep.effective_name().to_string();

            // Check for conflicts
            if let Some(existing) = graph.get(&effective_name) {
                // Same package, potentially different version
                let existing_version = &existing.version;
                let dep_req = dep.version_req()?;

                // Check if existing version satisfies new constraint
                if !dep_req.matches(existing_version) {
                    // CONFLICT! Don't auto-resolve, error out
                    return Err(RepoError::DiamondConflict {
                        conflicts: format_diamond_conflict(
                            &dep.name,
                            &dep.version,
                            &required_by,
                            &existing.version.to_string(),
                            &existing.required_by,
                        ),
                    });
                }

                // Compatible - just add requirer
                graph.add_requirer(&effective_name, required_by);
                continue;
            }

            if seen.contains(&effective_name) {
                continue;
            }
            seen.insert(effective_name.clone());

            // Fetch pack from repository
            let entry = (self.fetch_pack)(&dep.repository, &dep.name, &dep.version)?;

            let version =
                Version::parse(&entry.version).map_err(|e| RepoError::ResolutionFailed {
                    message: format!("Invalid version '{}': {}", entry.version, e),
                })?;

            // Collect transitive dependencies
            let transitive: Vec<String> = entry
                .dependencies
                .iter()
                .map(|d| d.alias.clone().unwrap_or_else(|| d.name.clone()))
                .collect();

            // Add to graph
            let resolved = ResolvedDependency {
                name: dep.name.clone(),
                version,
                repository: dep.repository.clone(),
                constraint: dep.version.clone(),
                alias: dep.alias.clone(),
                download_url: entry.download_url().unwrap_or_default().to_string(),
                digest: entry.digest.clone(),
                transitive_deps: transitive.clone(),
                required_by: vec![required_by.clone()],
            };

            graph.add(resolved);

            // Queue transitive dependencies
            for trans_dep in &entry.dependencies {
                let spec = DependencySpec {
                    name: trans_dep.name.clone(),
                    version: trans_dep.version.clone(),
                    repository: trans_dep
                        .repository
                        .clone()
                        .unwrap_or_else(|| dep.repository.clone()),
                    condition: trans_dep.condition.clone(),
                    tags: trans_dep.tags.clone(),
                    alias: trans_dep.alias.clone(),
                };
                queue.push_back((spec, effective_name.clone()));
            }
        }

        // Final check for any diamond dependencies we might have missed
        graph.check_diamonds()?;

        Ok(graph)
    }

    /// Resolve from existing lock file (for verification)
    pub fn resolve_from_lock(&self, lock: &LockFile) -> Result<DependencyGraph> {
        let mut graph = DependencyGraph::new();

        for locked in &lock.dependencies {
            let resolved = ResolvedDependency {
                name: locked.name.clone(),
                version: locked.version.clone(),
                repository: locked.repository.clone(),
                constraint: locked.constraint.clone(),
                alias: locked.alias.clone(),
                download_url: String::new(), // Will be fetched when downloading
                digest: Some(locked.digest.clone()),
                transitive_deps: locked.dependencies.clone(),
                required_by: vec!["lock file".to_string()],
            };
            graph.add(resolved);
        }

        Ok(graph)
    }
}

/// Dependency graph
#[derive(Debug, Default)]
pub struct DependencyGraph {
    dependencies: HashMap<String, ResolvedDependency>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a resolved dependency
    pub fn add(&mut self, dep: ResolvedDependency) {
        self.dependencies
            .insert(dep.effective_name().to_string(), dep);
    }

    /// Add a requirer to existing dependency
    pub fn add_requirer(&mut self, name: &str, requirer: String) {
        if let Some(dep) = self.dependencies.get_mut(name)
            && !dep.required_by.contains(&requirer)
        {
            dep.required_by.push(requirer);
        }
    }

    /// Get a dependency by name
    pub fn get(&self, name: &str) -> Option<&ResolvedDependency> {
        self.dependencies.get(name)
    }

    /// Iterate over all dependencies
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedDependency> {
        self.dependencies.values()
    }

    /// Get dependency count
    pub fn len(&self) -> usize {
        self.dependencies.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
    }

    /// Check for diamond dependencies
    pub fn check_diamonds(&self) -> Result<()> {
        // Group by original name (not alias)
        let mut by_name: HashMap<&str, Vec<&ResolvedDependency>> = HashMap::new();

        for dep in self.dependencies.values() {
            by_name.entry(&dep.name).or_default().push(dep);
        }

        // Check for multiple versions of same package
        let conflicts: Vec<_> = by_name
            .iter()
            .filter(|(_, deps)| {
                let versions: HashSet<_> = deps.iter().map(|d| &d.version).collect();
                versions.len() > 1
            })
            .map(|(name, deps)| {
                let versions: Vec<_> = deps
                    .iter()
                    .map(|d| format!("{} (required by: {})", d.version, d.required_by.join(", ")))
                    .collect();
                format!("  {}: {}", name, versions.join(" vs "))
            })
            .collect();

        if !conflicts.is_empty() {
            return Err(RepoError::DiamondConflict {
                conflicts: format!(
                    "The following packages are required at multiple versions:\n{}",
                    conflicts.join("\n")
                ),
            });
        }

        Ok(())
    }

    /// Convert to lock file
    pub fn to_lock_file(&self, pack_yaml_content: &str) -> LockFile {
        let mut lock = LockFile::new(pack_yaml_content);

        for dep in self.dependencies.values() {
            lock.add(LockedDependency {
                name: dep.name.clone(),
                version: dep.version.clone(),
                repository: dep.repository.clone(),
                digest: dep.digest.clone().unwrap_or_default(),
                constraint: dep.constraint.clone(),
                alias: dep.alias.clone(),
                dependencies: dep.transitive_deps.clone(),
            });
        }

        lock
    }

    /// Get dependencies in install order (topological sort)
    pub fn install_order(&self) -> Vec<&ResolvedDependency> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut in_progress = HashSet::new();

        fn visit<'a>(
            name: &str,
            graph: &'a DependencyGraph,
            visited: &mut HashSet<String>,
            in_progress: &mut HashSet<String>,
            result: &mut Vec<&'a ResolvedDependency>,
        ) {
            if visited.contains(name) {
                return;
            }
            if in_progress.contains(name) {
                // Circular dependency - skip for now
                return;
            }

            in_progress.insert(name.to_string());

            if let Some(dep) = graph.get(name) {
                // Visit transitive deps first
                for trans in &dep.transitive_deps {
                    visit(trans, graph, visited, in_progress, result);
                }
                result.push(dep);
            }

            in_progress.remove(name);
            visited.insert(name.to_string());
        }

        for name in self.dependencies.keys() {
            visit(name, self, &mut visited, &mut in_progress, &mut result);
        }

        result
    }

    /// Render as tree for display
    pub fn render_tree(&self) -> String {
        let mut lines = Vec::new();

        for dep in self.dependencies.values() {
            if dep.required_by.iter().any(|r| r == "root") {
                render_tree_node(dep, self, &mut lines, "", true);
            }
        }

        lines.join("\n")
    }
}

fn render_tree_node(
    dep: &ResolvedDependency,
    graph: &DependencyGraph,
    lines: &mut Vec<String>,
    prefix: &str,
    is_last: bool,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let name_display = if dep.alias.is_some() {
        format!("{} (alias: {})", dep.name, dep.effective_name())
    } else {
        dep.name.clone()
    };

    lines.push(format!(
        "{}{}{}@{}",
        prefix, connector, name_display, dep.version
    ));

    let new_prefix = format!("{}{}   ", prefix, if is_last { " " } else { "│" });

    let trans_count = dep.transitive_deps.len();
    for (i, trans_name) in dep.transitive_deps.iter().enumerate() {
        if let Some(trans_dep) = graph.get(trans_name) {
            render_tree_node(trans_dep, graph, lines, &new_prefix, i == trans_count - 1);
        }
    }
}

/// Format a diamond conflict error message
fn format_diamond_conflict(
    name: &str,
    new_constraint: &str,
    new_requirer: &str,
    existing_version: &str,
    existing_requirers: &[String],
) -> String {
    format!(
        r#"Diamond dependency conflict for '{name}':

  Version {existing_version} required by: {existing}
  Version {new_constraint} required by: {new_requirer}

Solutions:
  1. Pin a specific version in your Pack.yaml:
     dependencies:
       - name: {name}
         version: "{existing_version}"  # or another compatible version

  2. Use aliases to install both versions (creates TWO deployments!):
     dependencies:
       - name: {name}
         version: "{existing_version}"
         alias: {name}-v1
       - name: {name}
         version: "{new_constraint}"
         alias: {name}-v2

  3. Update the conflicting dependency to use a compatible version

For more information: https://sherpack.io/docs/dependencies#conflicts"#,
        name = name,
        existing_version = existing_version,
        new_constraint = new_constraint,
        new_requirer = new_requirer,
        existing = existing_requirers.join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexDependency;

    fn mock_pack(name: &str, version: &str, deps: Vec<(&str, &str)>) -> PackEntry {
        PackEntry {
            name: name.to_string(),
            version: version.to_string(),
            app_version: None,
            description: None,
            home: None,
            icon: None,
            sources: vec![],
            keywords: vec![],
            maintainers: vec![],
            urls: vec![format!("https://example.com/{}-{}.tgz", name, version)],
            digest: Some(format!("sha256:{}_{}", name, version)),
            created: None,
            deprecated: false,
            dependencies: deps
                .into_iter()
                .map(|(n, v)| IndexDependency {
                    name: n.to_string(),
                    version: v.to_string(),
                    repository: None,
                    condition: None,
                    tags: vec![],
                    alias: None,
                })
                .collect(),
            annotations: std::collections::HashMap::new(),
            api_version: None,
            r#type: None,
        }
    }

    #[test]
    fn test_simple_resolution() {
        let packs: HashMap<(&str, &str), PackEntry> = [
            (("repo", "nginx"), mock_pack("nginx", "15.0.0", vec![])),
            (("repo", "redis"), mock_pack("redis", "17.0.0", vec![])),
        ]
        .into_iter()
        .collect();

        let resolver = DependencyResolver::new(|repo, name, _version| {
            packs
                .get(&(repo, name))
                .cloned()
                .ok_or_else(|| RepoError::PackNotFound {
                    name: name.to_string(),
                    repo: repo.to_string(),
                })
        });

        let deps = vec![
            DependencySpec {
                name: "nginx".to_string(),
                version: "^15.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: None,
            },
            DependencySpec {
                name: "redis".to_string(),
                version: "^17.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: None,
            },
        ];

        let graph = resolver.resolve(&deps).unwrap();
        assert_eq!(graph.len(), 2);
        assert!(graph.get("nginx").is_some());
        assert!(graph.get("redis").is_some());
    }

    #[test]
    fn test_transitive_resolution() {
        let packs: HashMap<(&str, &str), PackEntry> = [
            (
                ("repo", "app"),
                mock_pack("app", "1.0.0", vec![("redis", "^17.0.0")]),
            ),
            (("repo", "redis"), mock_pack("redis", "17.0.0", vec![])),
        ]
        .into_iter()
        .collect();

        let resolver = DependencyResolver::new(|repo, name, _version| {
            packs
                .get(&(repo, name))
                .cloned()
                .ok_or_else(|| RepoError::PackNotFound {
                    name: name.to_string(),
                    repo: repo.to_string(),
                })
        });

        let deps = vec![DependencySpec {
            name: "app".to_string(),
            version: "^1.0.0".to_string(),
            repository: "repo".to_string(),
            condition: None,
            tags: vec![],
            alias: None,
        }];

        let graph = resolver.resolve(&deps).unwrap();
        assert_eq!(graph.len(), 2);
        assert!(graph.get("app").is_some());
        assert!(graph.get("redis").is_some());
    }

    #[test]
    fn test_diamond_detection() {
        // app1 -> redis@17.0.0
        // app2 -> redis@16.0.0
        // This should FAIL with diamond conflict
        let packs: HashMap<(&str, &str), PackEntry> = [
            (
                ("repo", "app1"),
                mock_pack("app1", "1.0.0", vec![("redis", "=17.0.0")]),
            ),
            (
                ("repo", "app2"),
                mock_pack("app2", "1.0.0", vec![("redis", "=16.0.0")]),
            ),
            (("repo", "redis"), mock_pack("redis", "17.0.0", vec![])),
        ]
        .into_iter()
        .collect();

        let resolver = DependencyResolver::new(|repo, name, _version| {
            packs
                .get(&(repo, name))
                .cloned()
                .ok_or_else(|| RepoError::PackNotFound {
                    name: name.to_string(),
                    repo: repo.to_string(),
                })
        });

        let deps = vec![
            DependencySpec {
                name: "app1".to_string(),
                version: "^1.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: None,
            },
            DependencySpec {
                name: "app2".to_string(),
                version: "^1.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: None,
            },
        ];

        let result = resolver.resolve(&deps);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, RepoError::DiamondConflict { .. }));
    }

    #[test]
    fn test_compatible_versions() {
        // app1 -> redis@^17.0.0
        // app2 -> redis@^17.0.0
        // Both can use 17.0.0, no conflict
        let packs: HashMap<(&str, &str), PackEntry> = [
            (
                ("repo", "app1"),
                mock_pack("app1", "1.0.0", vec![("redis", "^17.0.0")]),
            ),
            (
                ("repo", "app2"),
                mock_pack("app2", "1.0.0", vec![("redis", "^17.0.0")]),
            ),
            (("repo", "redis"), mock_pack("redis", "17.0.0", vec![])),
        ]
        .into_iter()
        .collect();

        let resolver = DependencyResolver::new(|repo, name, _version| {
            packs
                .get(&(repo, name))
                .cloned()
                .ok_or_else(|| RepoError::PackNotFound {
                    name: name.to_string(),
                    repo: repo.to_string(),
                })
        });

        let deps = vec![
            DependencySpec {
                name: "app1".to_string(),
                version: "^1.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: None,
            },
            DependencySpec {
                name: "app2".to_string(),
                version: "^1.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: None,
            },
        ];

        let graph = resolver.resolve(&deps).unwrap();
        assert_eq!(graph.len(), 3); // app1, app2, redis

        let redis = graph.get("redis").unwrap();
        assert!(redis.required_by.contains(&"app1".to_string()));
        assert!(redis.required_by.contains(&"app2".to_string()));
    }

    #[test]
    fn test_alias_allows_multiple_versions() {
        let packs: HashMap<(&str, &str), PackEntry> =
            [(("repo", "redis"), mock_pack("redis", "17.0.0", vec![]))]
                .into_iter()
                .collect();

        let resolver = DependencyResolver::new(|repo, name, _version| {
            packs
                .get(&(repo, name))
                .cloned()
                .ok_or_else(|| RepoError::PackNotFound {
                    name: name.to_string(),
                    repo: repo.to_string(),
                })
        });

        // Using aliases, we can have "two" redis instances
        let deps = vec![
            DependencySpec {
                name: "redis".to_string(),
                version: "^17.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: Some("cache-redis".to_string()),
            },
            DependencySpec {
                name: "redis".to_string(),
                version: "^17.0.0".to_string(),
                repository: "repo".to_string(),
                condition: None,
                tags: vec![],
                alias: Some("session-redis".to_string()),
            },
        ];

        let graph = resolver.resolve(&deps).unwrap();
        assert_eq!(graph.len(), 2);
        assert!(graph.get("cache-redis").is_some());
        assert!(graph.get("session-redis").is_some());
    }

    #[test]
    fn test_install_order() {
        let packs: HashMap<(&str, &str), PackEntry> = [
            (
                ("repo", "app"),
                mock_pack("app", "1.0.0", vec![("db", "^1.0.0")]),
            ),
            (
                ("repo", "db"),
                mock_pack("db", "1.0.0", vec![("common", "^1.0.0")]),
            ),
            (("repo", "common"), mock_pack("common", "1.0.0", vec![])),
        ]
        .into_iter()
        .collect();

        let resolver = DependencyResolver::new(|repo, name, _version| {
            packs
                .get(&(repo, name))
                .cloned()
                .ok_or_else(|| RepoError::PackNotFound {
                    name: name.to_string(),
                    repo: repo.to_string(),
                })
        });

        let deps = vec![DependencySpec {
            name: "app".to_string(),
            version: "^1.0.0".to_string(),
            repository: "repo".to_string(),
            condition: None,
            tags: vec![],
            alias: None,
        }];

        let graph = resolver.resolve(&deps).unwrap();
        let order = graph.install_order();

        // common should come before db, db before app
        let names: Vec<_> = order.iter().map(|d| d.name.as_str()).collect();
        let common_idx = names.iter().position(|&n| n == "common").unwrap();
        let db_idx = names.iter().position(|&n| n == "db").unwrap();
        let app_idx = names.iter().position(|&n| n == "app").unwrap();

        assert!(common_idx < db_idx);
        assert!(db_idx < app_idx);
    }

    #[test]
    fn test_render_tree() {
        let mut graph = DependencyGraph::new();

        graph.add(ResolvedDependency {
            name: "app".to_string(),
            version: Version::new(1, 0, 0),
            repository: "repo".to_string(),
            constraint: "^1.0.0".to_string(),
            alias: None,
            download_url: String::new(),
            digest: None,
            transitive_deps: vec!["redis".to_string()],
            required_by: vec!["root".to_string()],
        });

        graph.add(ResolvedDependency {
            name: "redis".to_string(),
            version: Version::new(17, 0, 0),
            repository: "repo".to_string(),
            constraint: "^17.0.0".to_string(),
            alias: None,
            download_url: String::new(),
            digest: None,
            transitive_deps: vec![],
            required_by: vec!["app".to_string()],
        });

        let tree = graph.render_tree();
        assert!(tree.contains("app@1.0.0"));
        assert!(tree.contains("redis@17.0.0"));
    }

    // =========================================================================
    // Filtering tests
    // =========================================================================

    fn make_dep(
        name: &str,
        enabled: bool,
        resolve: ResolvePolicy,
        condition: Option<&str>,
    ) -> Dependency {
        Dependency {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            repository: "https://example.com".to_string(),
            enabled,
            condition: condition.map(String::from),
            resolve,
            tags: vec![],
            alias: None,
        }
    }

    #[test]
    fn test_filter_all_enabled() {
        let deps = vec![
            make_dep("nginx", true, ResolvePolicy::WhenEnabled, None),
            make_dep("redis", true, ResolvePolicy::WhenEnabled, None),
        ];
        let values = serde_json::json!({});

        let result = filter_dependencies(&deps, &values);

        assert_eq!(result.to_resolve.len(), 2);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn test_filter_static_disabled() {
        let deps = vec![
            make_dep("nginx", true, ResolvePolicy::WhenEnabled, None),
            make_dep("redis", false, ResolvePolicy::WhenEnabled, None),
        ];
        let values = serde_json::json!({});

        let result = filter_dependencies(&deps, &values);

        assert_eq!(result.to_resolve.len(), 1);
        assert_eq!(result.to_resolve[0].name, "nginx");

        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].dependency.name, "redis");
        assert_eq!(result.skipped[0].reason, SkipReason::StaticDisabled);
    }

    #[test]
    fn test_filter_policy_never() {
        let deps = vec![
            make_dep("nginx", true, ResolvePolicy::WhenEnabled, None),
            make_dep("redis", true, ResolvePolicy::Never, None),
        ];
        let values = serde_json::json!({});

        let result = filter_dependencies(&deps, &values);

        assert_eq!(result.to_resolve.len(), 1);
        assert_eq!(result.to_resolve[0].name, "nginx");

        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].dependency.name, "redis");
        assert_eq!(result.skipped[0].reason, SkipReason::PolicyNever);
    }

    #[test]
    fn test_filter_policy_always_ignores_condition() {
        let deps = vec![make_dep(
            "redis",
            true,
            ResolvePolicy::Always,
            Some("redis.enabled"),
        )];
        // redis.enabled is false, but resolve: always ignores condition
        let values = serde_json::json!({
            "redis": { "enabled": false }
        });

        let result = filter_dependencies(&deps, &values);

        assert_eq!(result.to_resolve.len(), 1);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn test_filter_condition_true() {
        let deps = vec![make_dep(
            "redis",
            true,
            ResolvePolicy::WhenEnabled,
            Some("redis.enabled"),
        )];
        let values = serde_json::json!({
            "redis": { "enabled": true }
        });

        let result = filter_dependencies(&deps, &values);

        assert_eq!(result.to_resolve.len(), 1);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn test_filter_condition_false() {
        let deps = vec![make_dep(
            "redis",
            true,
            ResolvePolicy::WhenEnabled,
            Some("redis.enabled"),
        )];
        let values = serde_json::json!({
            "redis": { "enabled": false }
        });

        let result = filter_dependencies(&deps, &values);

        assert!(result.to_resolve.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].dependency.name, "redis");
        assert!(matches!(
            &result.skipped[0].reason,
            SkipReason::ConditionFalse { condition } if condition == "redis.enabled"
        ));
    }

    #[test]
    fn test_filter_condition_missing_is_falsy() {
        let deps = vec![make_dep(
            "redis",
            true,
            ResolvePolicy::WhenEnabled,
            Some("redis.enabled"),
        )];
        // redis.enabled not set at all → path doesn't exist → falsy → skip
        let values = serde_json::json!({});

        let result = filter_dependencies(&deps, &values);

        // Missing condition path = falsy = skip the dependency
        assert!(result.to_resolve.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert!(matches!(
            &result.skipped[0].reason,
            SkipReason::ConditionFalse { condition } if condition == "redis.enabled"
        ));
    }

    #[test]
    fn test_filter_complex_scenario() {
        let deps = vec![
            make_dep("nginx", true, ResolvePolicy::WhenEnabled, None), // enabled, no condition
            make_dep(
                "redis",
                true,
                ResolvePolicy::WhenEnabled,
                Some("redis.enabled"),
            ), // condition true
            make_dep(
                "postgres",
                true,
                ResolvePolicy::WhenEnabled,
                Some("db.enabled"),
            ), // condition false
            make_dep("mongodb", false, ResolvePolicy::WhenEnabled, None), // static disabled
            make_dep("vault", true, ResolvePolicy::Never, None),       // never resolve
            make_dep(
                "consul",
                true,
                ResolvePolicy::Always,
                Some("consul.enabled"),
            ), // always (ignore condition)
        ];
        let values = serde_json::json!({
            "redis": { "enabled": true },
            "db": { "enabled": false },
            "consul": { "enabled": false }  // ignored due to resolve: always
        });

        let result = filter_dependencies(&deps, &values);

        // Should resolve: nginx, redis, consul
        assert_eq!(result.to_resolve.len(), 3);
        let resolved_names: Vec<_> = result.to_resolve.iter().map(|d| d.name.as_str()).collect();
        assert!(resolved_names.contains(&"nginx"));
        assert!(resolved_names.contains(&"redis"));
        assert!(resolved_names.contains(&"consul"));

        // Should skip: postgres (condition false), mongodb (static disabled), vault (never)
        assert_eq!(result.skipped.len(), 3);
    }

    #[test]
    fn test_skip_reason_display() {
        assert_eq!(SkipReason::StaticDisabled.to_string(), "enabled: false");
        assert_eq!(SkipReason::PolicyNever.to_string(), "resolve: never");
        assert_eq!(
            SkipReason::ConditionFalse {
                condition: "redis.enabled".to_string()
            }
            .to_string(),
            "condition 'redis.enabled' is false"
        );
    }

    #[test]
    fn test_filter_result_summary() {
        let deps = vec![
            make_dep("redis", false, ResolvePolicy::WhenEnabled, None),
            make_dep("postgres", true, ResolvePolicy::Never, None),
        ];
        let values = serde_json::json!({});

        let result = filter_dependencies(&deps, &values);

        assert!(result.has_skipped());
        let summary = result.skipped_summary();
        assert!(summary.contains("redis"));
        assert!(summary.contains("enabled: false"));
        assert!(summary.contains("postgres"));
        assert!(summary.contains("resolve: never"));
    }

    #[test]
    fn test_dependency_spec_from_dependency() {
        let dep = Dependency {
            name: "redis".to_string(),
            version: "^17.0.0".to_string(),
            repository: "https://example.com".to_string(),
            enabled: true,
            condition: Some("redis.enabled".to_string()),
            resolve: ResolvePolicy::WhenEnabled,
            tags: vec!["cache".to_string()],
            alias: Some("my-redis".to_string()),
        };

        let spec = DependencySpec::from(&dep);

        assert_eq!(spec.name, "redis");
        assert_eq!(spec.version, "^17.0.0");
        assert_eq!(spec.repository, "https://example.com");
        assert_eq!(spec.condition, Some("redis.enabled".to_string()));
        assert_eq!(spec.tags, vec!["cache".to_string()]);
        assert_eq!(spec.alias, Some("my-redis".to_string()));
        assert_eq!(spec.effective_name(), "my-redis");
    }
}
