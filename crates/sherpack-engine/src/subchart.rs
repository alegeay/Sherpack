//! Subchart discovery and configuration
//!
//! This module provides types for discovering and managing subcharts
//! within a Sherpack pack.

use sherpack_core::{Dependency, LoadedPack};
use std::path::PathBuf;

/// Configuration for subchart rendering
#[derive(Debug, Clone)]
pub struct SubchartConfig {
    /// Maximum depth for nested subcharts (default: 10)
    pub max_depth: usize,

    /// Directory name for subcharts (default: "charts")
    pub subcharts_dir: String,

    /// Whether to fail on missing subcharts referenced in dependencies
    pub strict: bool,
}

impl Default for SubchartConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            subcharts_dir: "charts".to_string(),
            strict: false,
        }
    }
}

impl SubchartConfig {
    /// Create a new config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum depth for nested subcharts
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set the subcharts directory name
    pub fn with_subcharts_dir(mut self, dir: impl Into<String>) -> Self {
        self.subcharts_dir = dir.into();
        self
    }

    /// Enable strict mode (fail on missing subcharts)
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }
}

/// Information about a discovered subchart
#[derive(Debug)]
pub struct SubchartInfo {
    /// Effective name (alias if set, otherwise directory name)
    pub name: String,

    /// Path to the subchart directory
    pub path: PathBuf,

    /// Loaded pack
    pub pack: LoadedPack,

    /// Whether enabled based on condition evaluation
    pub enabled: bool,

    /// The dependency definition from parent Pack.yaml (if any)
    pub dependency: Option<Dependency>,

    /// Reason if disabled
    pub disabled_reason: Option<String>,
}

impl SubchartInfo {
    /// Check if this subchart should be rendered
    pub fn should_render(&self) -> bool {
        self.enabled
    }

    /// Get the effective name for value scoping
    pub fn scope_name(&self) -> &str {
        &self.name
    }
}

/// Result of subchart discovery
#[derive(Debug, Default)]
pub struct DiscoveryResult {
    /// Successfully discovered subcharts
    pub subcharts: Vec<SubchartInfo>,

    /// Warnings during discovery (e.g., invalid Pack.yaml)
    pub warnings: Vec<String>,

    /// Missing subcharts referenced in dependencies
    pub missing: Vec<String>,
}

impl DiscoveryResult {
    /// Create a new empty discovery result
    pub fn new() -> Self {
        Self::default()
    }

    /// Get enabled subcharts only
    pub fn enabled_subcharts(&self) -> impl Iterator<Item = &SubchartInfo> {
        self.subcharts.iter().filter(|s| s.enabled)
    }

    /// Get disabled subcharts only
    pub fn disabled_subcharts(&self) -> impl Iterator<Item = &SubchartInfo> {
        self.subcharts.iter().filter(|s| !s.enabled)
    }

    /// Check if there are any warnings or issues
    pub fn has_issues(&self) -> bool {
        !self.warnings.is_empty() || !self.missing.is_empty()
    }

    /// Total count of discovered subcharts
    pub fn total_count(&self) -> usize {
        self.subcharts.len()
    }

    /// Count of enabled subcharts
    pub fn enabled_count(&self) -> usize {
        self.subcharts.iter().filter(|s| s.enabled).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subchart_config_default() {
        let config = SubchartConfig::default();
        assert_eq!(config.max_depth, 10);
        assert_eq!(config.subcharts_dir, "charts");
        assert!(!config.strict);
    }

    #[test]
    fn test_subchart_config_builder() {
        let config = SubchartConfig::new()
            .with_max_depth(5)
            .with_subcharts_dir("packs")
            .strict();

        assert_eq!(config.max_depth, 5);
        assert_eq!(config.subcharts_dir, "packs");
        assert!(config.strict);
    }

    #[test]
    fn test_discovery_result_counts() {
        let mut result = DiscoveryResult::new();

        // Simulate adding subcharts (we can't create real SubchartInfo without LoadedPack)
        assert_eq!(result.total_count(), 0);
        assert_eq!(result.enabled_count(), 0);
        assert!(!result.has_issues());

        result.warnings.push("test warning".to_string());
        assert!(result.has_issues());

        result.missing.push("missing-chart".to_string());
        assert!(result.has_issues());
    }
}
