//! CRD upgrade strategy implementations
//!
//! Provides different strategies for deciding whether to apply CRD changes
//! based on their safety analysis.

use super::analyzer::{ChangeSeverity, CrdAnalysis};

/// Decision on whether to apply CRD changes
#[derive(Debug, Clone)]
pub enum UpgradeDecision {
    /// Apply all changes
    Apply,
    /// Apply changes but skip some (with reasons)
    ApplyPartial {
        /// Changes that will be skipped
        skipped: Vec<SkippedChange>,
    },
    /// Reject the upgrade entirely
    Reject {
        /// Reason for rejection
        reason: String,
        /// List of problematic changes
        blocking_changes: Vec<String>,
    },
}

impl UpgradeDecision {
    /// Check if this decision allows any changes to be applied
    pub fn allows_apply(&self) -> bool {
        matches!(self, Self::Apply | Self::ApplyPartial { .. })
    }

    /// Check if this decision completely rejects the upgrade
    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Reject { .. })
    }

    /// Get blocking change messages if rejected
    pub fn blocking_messages(&self) -> Option<&[String]> {
        match self {
            Self::Reject {
                blocking_changes, ..
            } => Some(blocking_changes),
            _ => None,
        }
    }
}

/// A change that was skipped during partial apply
#[derive(Debug, Clone)]
pub struct SkippedChange {
    /// Path to the skipped change
    pub path: String,
    /// Human-readable reason for skipping
    pub reason: String,
    /// Severity of the skipped change
    pub severity: ChangeSeverity,
}

/// Strategy for deciding whether to apply CRD changes
///
/// This trait allows different policies for handling CRD updates,
/// from strict (reject any dangerous changes) to permissive (apply everything).
pub trait UpgradeStrategy: Send + Sync {
    /// Decide whether to apply the CRD changes
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision;

    /// Get a human-readable name for this strategy
    fn name(&self) -> &'static str;
}

/// Safe strategy: reject any dangerous changes
///
/// This is the default strategy. It allows safe changes and warnings,
/// but rejects the upgrade if any dangerous changes are detected.
/// Use `--force-crd-update` to override.
#[derive(Debug, Default, Clone, Copy)]
pub struct SafeStrategy;

impl UpgradeStrategy for SafeStrategy {
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision {
        if analysis.is_new {
            return UpgradeDecision::Apply;
        }

        if analysis.has_dangerous_changes() {
            let blocking: Vec<String> = analysis
                .dangerous_changes()
                .map(|c| c.message.clone())
                .collect();

            UpgradeDecision::Reject {
                reason: format!(
                    "{} dangerous change(s) detected. Use --force-crd-update to override.",
                    blocking.len()
                ),
                blocking_changes: blocking,
            }
        } else {
            UpgradeDecision::Apply
        }
    }

    fn name(&self) -> &'static str {
        "safe"
    }
}

/// Force strategy: apply all changes regardless of severity
///
/// Use this when you understand the risks and want to proceed anyway.
/// Activated with `--force-crd-update`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForceStrategy;

impl UpgradeStrategy for ForceStrategy {
    fn decide(&self, _analysis: &CrdAnalysis) -> UpgradeDecision {
        UpgradeDecision::Apply
    }

    fn name(&self) -> &'static str {
        "force"
    }
}

/// Skip strategy: never update CRDs
///
/// Use this when CRDs are managed externally (e.g., by a GitOps tool
/// or manual kubectl apply). Activated with `--skip-crd-update`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SkipStrategy;

impl UpgradeStrategy for SkipStrategy {
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision {
        if analysis.is_new {
            // Even skip strategy should note when it's skipping a new CRD
            UpgradeDecision::Reject {
                reason: "CRD updates skipped (--skip-crd-update)".to_string(),
                blocking_changes: vec![format!("New CRD: {}", analysis.crd_name)],
            }
        } else {
            UpgradeDecision::Reject {
                reason: "CRD updates skipped (--skip-crd-update)".to_string(),
                blocking_changes: vec![],
            }
        }
    }

    fn name(&self) -> &'static str {
        "skip"
    }
}

/// Cautious strategy: apply safe changes only, skip warnings and dangerous
///
/// This is more conservative than SafeStrategy - it won't even apply
/// warning-level changes without explicit confirmation.
#[derive(Debug, Default, Clone, Copy)]
pub struct CautiousStrategy;

impl UpgradeStrategy for CautiousStrategy {
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision {
        if analysis.is_new {
            return UpgradeDecision::Apply;
        }

        let skipped: Vec<SkippedChange> = analysis
            .changes
            .iter()
            .filter(|c| c.severity() >= ChangeSeverity::Warning)
            .map(|c| SkippedChange {
                path: c.path.clone(),
                reason: c.message.clone(),
                severity: c.severity(),
            })
            .collect();

        if skipped.is_empty() {
            UpgradeDecision::Apply
        } else {
            UpgradeDecision::ApplyPartial { skipped }
        }
    }

    fn name(&self) -> &'static str {
        "cautious"
    }
}

/// Custom strategy with configurable thresholds
///
/// Allows fine-grained control over which severity levels are accepted.
#[derive(Debug, Clone)]
pub struct CustomStrategy {
    /// Maximum severity to allow (anything above this is rejected)
    pub max_allowed_severity: ChangeSeverity,
    /// Whether to require confirmation for warnings
    pub require_confirmation_for_warnings: bool,
}

impl CustomStrategy {
    /// Create a new custom strategy
    pub fn new(max_allowed_severity: ChangeSeverity) -> Self {
        Self {
            max_allowed_severity,
            require_confirmation_for_warnings: false,
        }
    }

    /// Require confirmation for warning-level changes
    pub fn with_confirmation_for_warnings(mut self) -> Self {
        self.require_confirmation_for_warnings = true;
        self
    }
}

impl UpgradeStrategy for CustomStrategy {
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision {
        if analysis.is_new {
            return UpgradeDecision::Apply;
        }

        let max_severity = analysis.max_severity();

        if max_severity > self.max_allowed_severity {
            let blocking: Vec<String> = analysis
                .changes
                .iter()
                .filter(|c| c.severity() > self.max_allowed_severity)
                .map(|c| c.message.clone())
                .collect();

            UpgradeDecision::Reject {
                reason: format!(
                    "Changes exceed maximum allowed severity ({:?})",
                    self.max_allowed_severity
                ),
                blocking_changes: blocking,
            }
        } else {
            UpgradeDecision::Apply
        }
    }

    fn name(&self) -> &'static str {
        "custom"
    }
}

/// Create a strategy from CLI options
///
/// This is the main entry point for selecting a strategy based on
/// command-line flags.
pub fn strategy_from_options(skip_update: bool, force_update: bool) -> Box<dyn UpgradeStrategy> {
    if skip_update {
        Box::new(SkipStrategy)
    } else if force_update {
        Box::new(ForceStrategy)
    } else {
        Box::new(SafeStrategy)
    }
}

/// Builder for creating custom upgrade strategies
#[derive(Debug, Default)]
pub struct StrategyBuilder {
    max_severity: Option<ChangeSeverity>,
    skip_all: bool,
    force_all: bool,
}

impl StrategyBuilder {
    /// Create a new strategy builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Skip all CRD updates
    pub fn skip_all(mut self) -> Self {
        self.skip_all = true;
        self
    }

    /// Force all CRD updates
    pub fn force_all(mut self) -> Self {
        self.force_all = true;
        self
    }

    /// Set maximum allowed severity
    pub fn max_severity(mut self, severity: ChangeSeverity) -> Self {
        self.max_severity = Some(severity);
        self
    }

    /// Build the strategy
    pub fn build(self) -> Box<dyn UpgradeStrategy> {
        if self.skip_all {
            Box::new(SkipStrategy)
        } else if self.force_all {
            Box::new(ForceStrategy)
        } else if let Some(severity) = self.max_severity {
            Box::new(CustomStrategy::new(severity))
        } else {
            Box::new(SafeStrategy)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::analyzer::{ChangeKind, CrdChange};

    fn make_analysis(changes: Vec<(ChangeKind, &str)>) -> CrdAnalysis {
        CrdAnalysis {
            crd_name: "test.example.com".to_string(),
            changes: changes
                .into_iter()
                .map(|(kind, msg)| CrdChange {
                    kind,
                    path: "test.path".to_string(),
                    message: msg.to_string(),
                    old_value: None,
                    new_value: None,
                })
                .collect(),
            is_new: false,
        }
    }

    #[test]
    fn test_safe_strategy_allows_safe_changes() {
        let analysis = make_analysis(vec![
            (ChangeKind::AddOptionalField, "Added field"),
            (ChangeKind::AddVersion, "Added version"),
        ]);

        let strategy = SafeStrategy;
        let decision = strategy.decide(&analysis);

        assert!(decision.allows_apply());
        assert!(!decision.is_rejected());
    }

    #[test]
    fn test_safe_strategy_allows_warnings() {
        let analysis = make_analysis(vec![
            (ChangeKind::TightenValidation, "Validation tightened"),
            (ChangeKind::ChangeDefault, "Default changed"),
        ]);

        let strategy = SafeStrategy;
        let decision = strategy.decide(&analysis);

        assert!(decision.allows_apply());
    }

    #[test]
    fn test_safe_strategy_rejects_dangerous() {
        let analysis = make_analysis(vec![
            (ChangeKind::AddOptionalField, "Safe change"),
            (ChangeKind::RemoveVersion, "Dangerous change"),
        ]);

        let strategy = SafeStrategy;
        let decision = strategy.decide(&analysis);

        assert!(decision.is_rejected());
        if let UpgradeDecision::Reject {
            blocking_changes, ..
        } = decision
        {
            assert_eq!(blocking_changes.len(), 1);
            assert!(blocking_changes[0].contains("Dangerous"));
        }
    }

    #[test]
    fn test_force_strategy_allows_everything() {
        let analysis = make_analysis(vec![
            (ChangeKind::RemoveVersion, "Dangerous 1"),
            (ChangeKind::ChangeScope, "Dangerous 2"),
            (ChangeKind::ChangeFieldType, "Dangerous 3"),
        ]);

        let strategy = ForceStrategy;
        let decision = strategy.decide(&analysis);

        assert!(decision.allows_apply());
        assert!(!decision.is_rejected());
    }

    #[test]
    fn test_skip_strategy_rejects_everything() {
        let analysis = make_analysis(vec![(ChangeKind::AddOptionalField, "Safe change")]);

        let strategy = SkipStrategy;
        let decision = strategy.decide(&analysis);

        assert!(decision.is_rejected());
    }

    #[test]
    fn test_cautious_strategy_skips_warnings() {
        let analysis = make_analysis(vec![
            (ChangeKind::AddOptionalField, "Safe change"),
            (ChangeKind::TightenValidation, "Warning change"),
        ]);

        let strategy = CautiousStrategy;
        let decision = strategy.decide(&analysis);

        match decision {
            UpgradeDecision::ApplyPartial { skipped } => {
                assert_eq!(skipped.len(), 1);
                assert_eq!(skipped[0].severity, ChangeSeverity::Warning);
            }
            _ => panic!("Expected ApplyPartial"),
        }
    }

    #[test]
    fn test_new_crd_always_applies() {
        let analysis = CrdAnalysis::new_crd("test.example.com".to_string());

        // All strategies except Skip should allow new CRDs
        assert!(SafeStrategy.decide(&analysis).allows_apply());
        assert!(ForceStrategy.decide(&analysis).allows_apply());
        assert!(CautiousStrategy.decide(&analysis).allows_apply());
    }

    #[test]
    fn test_strategy_from_options() {
        let skip = strategy_from_options(true, false);
        assert_eq!(skip.name(), "skip");

        let force = strategy_from_options(false, true);
        assert_eq!(force.name(), "force");

        let safe = strategy_from_options(false, false);
        assert_eq!(safe.name(), "safe");
    }

    #[test]
    fn test_strategy_builder() {
        let skip = StrategyBuilder::new().skip_all().build();
        assert_eq!(skip.name(), "skip");

        let force = StrategyBuilder::new().force_all().build();
        assert_eq!(force.name(), "force");

        let custom = StrategyBuilder::new()
            .max_severity(ChangeSeverity::Warning)
            .build();
        assert_eq!(custom.name(), "custom");
    }

    #[test]
    fn test_custom_strategy_respects_max_severity() {
        let analysis = make_analysis(vec![
            (ChangeKind::TightenValidation, "Warning"),
            (ChangeKind::RemoveVersion, "Dangerous"),
        ]);

        // Allow up to warnings
        let strategy = CustomStrategy::new(ChangeSeverity::Warning);
        let decision = strategy.decide(&analysis);
        assert!(decision.is_rejected());

        // Allow up to dangerous
        let strategy = CustomStrategy::new(ChangeSeverity::Dangerous);
        let decision = strategy.decide(&analysis);
        assert!(decision.allows_apply());
    }
}
