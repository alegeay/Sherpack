//! Dependency resolution with diamond detection
//!
//! Key features:
//! - Strict mode: Error on ANY conflict (safest default)
//! - Diamond detection with clear error messages
//! - No automatic resolution - humans must choose

use semver::{Version, VersionReq};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{RepoError, Result};
use crate::index::PackEntry;
use crate::lock::{LockFile, LockedDependency};

/// Dependency from Pack.yaml
#[derive(Debug, Clone)]
pub struct DependencySpec {
    pub name: String,
    pub version: String,
    pub repository: String,
    pub condition: Option<String>,
    pub tags: Vec<String>,
    pub alias: Option<String>,
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

/// Dependency resolver
pub struct DependencyResolver<'a> {
    /// Function to fetch pack entry from repository
    fetch_pack: Box<dyn Fn(&str, &str, &str) -> Result<PackEntry> + 'a>,
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

            let version = Version::parse(&entry.version).map_err(|e| RepoError::ResolutionFailed {
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
        self.dependencies.insert(dep.effective_name().to_string(), dep);
    }

    /// Add a requirer to existing dependency
    pub fn add_requirer(&mut self, name: &str, requirer: String) {
        if let Some(dep) = self.dependencies.get_mut(name) {
            if !dep.required_by.contains(&requirer) {
                dep.required_by.push(requirer);
            }
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
        let packs: HashMap<(&str, &str), PackEntry> = [
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
}
