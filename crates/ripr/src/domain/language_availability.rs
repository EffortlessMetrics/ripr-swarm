//! Shared per-language availability report and effective-enablement resolver.
//!
//! Detection, compiled availability, configured intent and effective
//! enablement are different facts (#2615). This module is the single pure
//! owner that resolves them into one [`LanguageAvailabilityReport`] so
//! config diagnostics, `ripr check`, `ripr doctor` and the editor can project
//! the same semantic state instead of inferring availability from file
//! extensions or Cargo features alone. It performs no I/O: callers supply the
//! observed facts, and it never promotes a support tier or claims an adapter
//! ran.

use super::LanguageId;

/// Support tier a language is documented at. The resolver never promotes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportTier {
    Stable,
    UsableAlpha,
    Preview,
    Planned,
}

impl SupportTier {
    pub fn as_str(self) -> &'static str {
        match self {
            SupportTier::Stable => "stable",
            SupportTier::UsableAlpha => "usable_alpha",
            SupportTier::Preview => "preview",
            SupportTier::Planned => "planned",
        }
    }

    /// Conservative tier for a language: only Rust is stable.
    pub fn for_language(language: LanguageId) -> Self {
        match language {
            LanguageId::Rust => SupportTier::Stable,
            _ => SupportTier::Preview,
        }
    }
}

/// What the user said in `[languages]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfiguredIntent {
    /// The language is not mentioned; policy decides.
    Unspecified,
    Enabled,
    /// Explicitly disabled; overrides auto-detection.
    Disabled,
}

/// Whether detection alone may enable a language when the user is silent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoDetectionPolicy {
    /// Detection is reported but never enables the language.
    Never,
    /// A detected, compiled language is enabled when not configured.
    WhenDetected,
}

impl AutoDetectionPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            AutoDetectionPolicy::Never => "never",
            AutoDetectionPolicy::WhenDetected => "when_detected",
        }
    }
}

/// Observed facts for one language. Built by the caller from config, the
/// workspace scan and the binary; the resolver adds no facts of its own.
#[derive(Clone, Debug)]
pub struct LanguageAvailabilityInput {
    pub language: LanguageId,
    pub support: SupportTier,
    /// Relevant project markers or files are present.
    pub project_detected: bool,
    pub file_count: usize,
    /// Relevant files in the current diff scope, when a scope exists.
    pub changed_file_count: Option<usize>,
    /// The binary contains the adapter.
    pub compiled_available: bool,
    pub configured: ConfiguredIntent,
    pub auto_policy: AutoDetectionPolicy,
    /// `Some(true)` when the adapter ran and disclosed limitations,
    /// `Some(false)` when it ran cleanly, `None` before any analysis.
    pub adapter_ran_with_limitations: Option<bool>,
}

impl LanguageAvailabilityInput {
    /// Input with the compiled fact read from this binary's features.
    pub fn for_language(language: LanguageId) -> Self {
        Self {
            language,
            support: SupportTier::for_language(language),
            project_detected: false,
            file_count: 0,
            changed_file_count: None,
            compiled_available: language.is_available(),
            configured: ConfiguredIntent::Unspecified,
            auto_policy: AutoDetectionPolicy::Never,
            adapter_ran_with_limitations: None,
        }
    }
}

/// The user-facing availability state. Closed vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvailabilityState {
    DetectedCompiledEnabled,
    DetectedCompiledDisabled,
    DetectedNotCompiled,
    ConfiguredEnabledNotDetected,
    ConfiguredEnabledButUnavailable,
    AutoDetectedEnabled,
    SupportedButNoCurrentScope,
    AdapterRanWithLimitations,
    /// Not enabled and nothing to analyze: a named state, never a clean
    /// result.
    NotEnabled,
}

impl AvailabilityState {
    pub fn as_str(self) -> &'static str {
        match self {
            AvailabilityState::DetectedCompiledEnabled => "detected_compiled_enabled",
            AvailabilityState::DetectedCompiledDisabled => "detected_compiled_disabled",
            AvailabilityState::DetectedNotCompiled => "detected_not_compiled",
            AvailabilityState::ConfiguredEnabledNotDetected => "configured_enabled_not_detected",
            AvailabilityState::ConfiguredEnabledButUnavailable => {
                "configured_enabled_but_unavailable"
            }
            AvailabilityState::AutoDetectedEnabled => "auto_detected_enabled",
            AvailabilityState::SupportedButNoCurrentScope => "supported_but_no_current_scope",
            AvailabilityState::AdapterRanWithLimitations => "adapter_ran_with_limitations",
            AvailabilityState::NotEnabled => "not_enabled",
        }
    }
}

/// One resolved availability report for a language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageAvailabilityReport {
    pub language: LanguageId,
    pub support: SupportTier,
    pub project_detected: bool,
    pub file_count: usize,
    pub changed_file_count: Option<usize>,
    pub compile_feature: &'static str,
    pub compiled_available: bool,
    pub configured_enabled: bool,
    pub configured_disabled: bool,
    pub auto_detection_policy: AutoDetectionPolicy,
    /// Auto-detection (not configuration) is what enabled the language.
    pub auto_detected: bool,
    pub effective_enabled: bool,
    pub state: AvailabilityState,
    /// Ordered remediation steps, complete for the state; empty when none.
    pub remediation: Vec<String>,
    pub non_claims: Vec<&'static str>,
}

/// Resolve the effective enablement of one language from observed facts.
///
/// Policy: an explicit disable always wins; an explicit enable needs the
/// adapter compiled; with no configuration, only
/// [`AutoDetectionPolicy::WhenDetected`] plus detection plus a compiled
/// adapter enables the language. Effective enablement never implies the
/// adapter ran.
pub fn resolve_language_availability(
    input: &LanguageAvailabilityInput,
) -> LanguageAvailabilityReport {
    let language = input.language;
    let configured_enabled = input.configured == ConfiguredIntent::Enabled;
    let configured_disabled = input.configured == ConfiguredIntent::Disabled;
    let auto_detected = input.configured == ConfiguredIntent::Unspecified
        && input.auto_policy == AutoDetectionPolicy::WhenDetected
        && input.project_detected
        && input.compiled_available;
    let effective_enabled =
        input.compiled_available && !configured_disabled && (configured_enabled || auto_detected);

    let state = if configured_enabled && !input.compiled_available {
        AvailabilityState::ConfiguredEnabledButUnavailable
    } else if input.project_detected && !input.compiled_available && !configured_disabled {
        AvailabilityState::DetectedNotCompiled
    } else if configured_disabled && input.project_detected && input.compiled_available {
        AvailabilityState::DetectedCompiledDisabled
    } else if effective_enabled {
        match input.adapter_ran_with_limitations {
            Some(true) => AvailabilityState::AdapterRanWithLimitations,
            _ if input.changed_file_count == Some(0) => {
                AvailabilityState::SupportedButNoCurrentScope
            }
            _ if auto_detected => AvailabilityState::AutoDetectedEnabled,
            _ if input.project_detected => AvailabilityState::DetectedCompiledEnabled,
            _ => AvailabilityState::ConfiguredEnabledNotDetected,
        }
    } else {
        AvailabilityState::NotEnabled
    };

    let remediation = remediation_for(input, state);
    let mut non_claims = vec![
        "availability does not mean the adapter ran",
        "availability does not promote the support tier",
    ];
    if !effective_enabled {
        non_claims.push(
            "a disabled or unavailable language is not analyzed; silence is not a clean result",
        );
    }

    LanguageAvailabilityReport {
        language,
        support: input.support,
        project_detected: input.project_detected,
        file_count: input.file_count,
        changed_file_count: input.changed_file_count,
        compile_feature: language.required_feature(),
        compiled_available: input.compiled_available,
        configured_enabled,
        configured_disabled,
        auto_detection_policy: input.auto_policy,
        auto_detected,
        effective_enabled,
        state,
        remediation,
        non_claims,
    }
}

/// Resolve every input, preserving input order so each language keeps its
/// own independent state in a mixed-language workspace.
pub fn resolve_language_availability_all(
    inputs: &[LanguageAvailabilityInput],
) -> Vec<LanguageAvailabilityReport> {
    inputs.iter().map(resolve_language_availability).collect()
}

/// The shared unavailable-adapter recovery text, or `None` when the report's
/// adapter route is not blocked on a missing build. The single place
/// surfaces read the build prerequisite from, so none re-derives it from
/// `is_available` alone.
pub fn unavailable_recovery(report: &LanguageAvailabilityReport) -> Option<&str> {
    let blocked = matches!(
        report.state,
        AvailabilityState::DetectedNotCompiled | AvailabilityState::ConfiguredEnabledButUnavailable
    );
    if blocked {
        report.remediation.first().map(String::as_str)
    } else {
        None
    }
}

fn enable_step(language: LanguageId) -> String {
    format!(
        "enable `{}` under `[languages]` in ripr.toml (`enabled = [..., \"{}\"]`)",
        language.as_str(),
        language.as_str()
    )
}

fn leave_disabled_step(language: LanguageId) -> String {
    format!(
        "or leave {} disabled intentionally; its files are then not analyzed",
        language.display_name()
    )
}

fn remediation_for(input: &LanguageAvailabilityInput, state: AvailabilityState) -> Vec<String> {
    let language = input.language;
    let mut steps = Vec::new();
    match state {
        AvailabilityState::DetectedNotCompiled
        | AvailabilityState::ConfiguredEnabledButUnavailable => {
            steps.push(language.unavailable_adapter_recovery());
            if input.configured != ConfiguredIntent::Enabled
                && input.auto_policy == AutoDetectionPolicy::Never
            {
                steps.push(enable_step(language));
            }
            steps.push(leave_disabled_step(language));
        }
        AvailabilityState::DetectedCompiledDisabled => {
            steps.push(enable_step(language));
            steps.push(leave_disabled_step(language));
        }
        AvailabilityState::NotEnabled
            if input.project_detected && input.configured == ConfiguredIntent::Unspecified =>
        {
            steps.push(enable_step(language));
        }
        _ => {}
    }
    if input.compiled_available
        && matches!(
            state,
            AvailabilityState::DetectedCompiledDisabled
                | AvailabilityState::DetectedCompiledEnabled
                | AvailabilityState::AutoDetectedEnabled
                | AvailabilityState::ConfiguredEnabledNotDetected
        )
        && let Some(prerequisite) = language.enable_prerequisite()
    {
        steps.push(prerequisite);
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        language: LanguageId,
        detected: bool,
        compiled: bool,
        configured: ConfiguredIntent,
        policy: AutoDetectionPolicy,
    ) -> LanguageAvailabilityInput {
        LanguageAvailabilityInput {
            project_detected: detected,
            file_count: usize::from(detected),
            compiled_available: compiled,
            configured,
            auto_policy: policy,
            ..LanguageAvailabilityInput::for_language(language)
        }
    }

    fn resolve(i: &LanguageAvailabilityInput) -> LanguageAvailabilityReport {
        resolve_language_availability(i)
    }

    const PREVIEW: [LanguageId; 3] = [LanguageId::Python, LanguageId::TypeScript, LanguageId::Perl];

    #[test]
    fn detected_compiled_enabled() {
        for l in PREVIEW {
            let r = resolve(&input(
                l,
                true,
                true,
                ConfiguredIntent::Enabled,
                AutoDetectionPolicy::Never,
            ));
            assert_eq!(r.state, AvailabilityState::DetectedCompiledEnabled);
            assert!(r.effective_enabled && r.configured_enabled && !r.auto_detected);
        }
    }

    #[test]
    fn detected_compiled_disabled_overrides_auto_detection() {
        for l in PREVIEW {
            let r = resolve(&input(
                l,
                true,
                true,
                ConfiguredIntent::Disabled,
                AutoDetectionPolicy::WhenDetected,
            ));
            assert_eq!(r.state, AvailabilityState::DetectedCompiledDisabled);
            assert!(!r.effective_enabled && !r.auto_detected && r.configured_disabled);
            assert!(r.remediation.iter().any(|s| s.contains("[languages]")));
            assert!(r.non_claims.iter().any(|s| s.contains("not analyzed")));
        }
    }

    #[test]
    fn detected_not_compiled_names_complete_route_and_never_enables() {
        for l in PREVIEW {
            let r = resolve(&input(
                l,
                true,
                false,
                ConfiguredIntent::Unspecified,
                AutoDetectionPolicy::WhenDetected,
            ));
            assert_eq!(r.state, AvailabilityState::DetectedNotCompiled);
            assert!(!r.effective_enabled && !r.auto_detected);
            assert!(r.remediation[0].contains(l.required_feature()));
        }
        // Opt-in policy: both the build and the config step are named.
        let r = resolve(&input(
            LanguageId::Python,
            true,
            false,
            ConfiguredIntent::Unspecified,
            AutoDetectionPolicy::Never,
        ));
        assert!(r.remediation.iter().any(|s| s.contains("lang-python")));
        assert!(r.remediation.iter().any(|s| s.contains("[languages]")));
    }

    #[test]
    fn configured_enabled_without_adapter_is_unavailable_not_silent() {
        let r = resolve(&input(
            LanguageId::Perl,
            false,
            false,
            ConfiguredIntent::Enabled,
            AutoDetectionPolicy::Never,
        ));
        assert_eq!(r.state, AvailabilityState::ConfiguredEnabledButUnavailable);
        assert!(!r.effective_enabled);
        assert!(r.remediation[0].contains("lang-perl"));
    }

    #[test]
    fn configured_enabled_not_detected() {
        let r = resolve(&input(
            LanguageId::Python,
            false,
            true,
            ConfiguredIntent::Enabled,
            AutoDetectionPolicy::Never,
        ));
        assert_eq!(r.state, AvailabilityState::ConfiguredEnabledNotDetected);
    }

    #[test]
    fn auto_detection_is_visible_when_it_changes_enablement() {
        let r = resolve(&input(
            LanguageId::Python,
            true,
            true,
            ConfiguredIntent::Unspecified,
            AutoDetectionPolicy::WhenDetected,
        ));
        assert_eq!(r.state, AvailabilityState::AutoDetectedEnabled);
        assert!(r.auto_detected && r.effective_enabled && !r.configured_enabled);
        // Discriminating negative: opt-in policy does not auto-enable.
        let r = resolve(&input(
            LanguageId::TypeScript,
            true,
            true,
            ConfiguredIntent::Unspecified,
            AutoDetectionPolicy::Never,
        ));
        assert!(!r.effective_enabled && !r.auto_detected);
        assert_eq!(r.state, AvailabilityState::NotEnabled);
        assert!(r.remediation.iter().any(|s| s.contains("[languages]")));
    }

    #[test]
    fn empty_scope_and_adapter_limitations_are_named() {
        let mut i = input(
            LanguageId::Python,
            true,
            true,
            ConfiguredIntent::Enabled,
            AutoDetectionPolicy::Never,
        );
        i.changed_file_count = Some(0);
        assert_eq!(
            resolve(&i).state,
            AvailabilityState::SupportedButNoCurrentScope
        );
        i.changed_file_count = Some(2);
        i.adapter_ran_with_limitations = Some(true);
        assert_eq!(
            resolve(&i).state,
            AvailabilityState::AdapterRanWithLimitations
        );
        i.adapter_ran_with_limitations = Some(false);
        assert_eq!(
            resolve(&i).state,
            AvailabilityState::DetectedCompiledEnabled
        );
    }

    #[test]
    fn perl_enabled_still_names_fact_packet_prerequisite() {
        let r = resolve(&input(
            LanguageId::Perl,
            true,
            true,
            ConfiguredIntent::Enabled,
            AutoDetectionPolicy::Never,
        ));
        assert!(r.remediation.iter().any(|s| s.contains("fact packet")));
        let py = resolve(&input(
            LanguageId::Python,
            true,
            true,
            ConfiguredIntent::Enabled,
            AutoDetectionPolicy::Never,
        ));
        assert!(py.remediation.is_empty());
    }

    #[test]
    fn mixed_workspace_keeps_independent_states_and_tier() {
        let reports = resolve_language_availability_all(&[
            input(
                LanguageId::Python,
                true,
                true,
                ConfiguredIntent::Enabled,
                AutoDetectionPolicy::Never,
            ),
            input(
                LanguageId::TypeScript,
                true,
                false,
                ConfiguredIntent::Unspecified,
                AutoDetectionPolicy::Never,
            ),
            input(
                LanguageId::Perl,
                true,
                true,
                ConfiguredIntent::Disabled,
                AutoDetectionPolicy::Never,
            ),
        ]);
        let states: Vec<_> = reports.iter().map(|r| r.state.as_str()).collect();
        assert_eq!(
            states,
            [
                "detected_compiled_enabled",
                "detected_not_compiled",
                "detected_compiled_disabled"
            ]
        );
        assert!(reports.iter().all(|r| r.support == SupportTier::Preview));
        assert_eq!(
            SupportTier::for_language(LanguageId::Rust),
            SupportTier::Stable
        );
        assert_eq!(SupportTier::Planned.as_str(), "planned");
        assert_eq!(SupportTier::UsableAlpha.as_str(), "usable_alpha");
        assert_eq!(AutoDetectionPolicy::Never.as_str(), "never");
    }

    #[test]
    fn explicit_disable_outranks_missing_adapter_and_detection() {
        // Detected + not compiled + explicitly disabled: not enabled, and no
        // build or enable route is pushed at a user who opted out.
        let r = resolve(&input(
            LanguageId::TypeScript,
            true,
            false,
            ConfiguredIntent::Disabled,
            AutoDetectionPolicy::WhenDetected,
        ));
        assert_eq!(r.state, AvailabilityState::NotEnabled);
        assert!(!r.effective_enabled && r.configured_disabled);
        assert!(r.remediation.is_empty());
        assert!(r.non_claims.iter().any(|s| s.contains("not analyzed")));
        // Undetected + not compiled + disabled is the same named state.
        let r = resolve(&input(
            LanguageId::Perl,
            false,
            false,
            ConfiguredIntent::Disabled,
            AutoDetectionPolicy::Never,
        ));
        assert_eq!(r.state, AvailabilityState::NotEnabled);
        assert!(r.remediation.is_empty());
    }

    #[test]
    fn adapter_limitation_cannot_enable_an_unavailable_or_undetected_language() {
        // A stale "ran with limitations" fact never overrides enablement.
        let mut i = input(
            LanguageId::Python,
            false,
            true,
            ConfiguredIntent::Unspecified,
            AutoDetectionPolicy::WhenDetected,
        );
        i.adapter_ran_with_limitations = Some(true);
        let r = resolve(&i);
        assert_eq!(r.state, AvailabilityState::NotEnabled);
        assert!(!r.effective_enabled);
        i.project_detected = true;
        i.compiled_available = false;
        assert_eq!(resolve(&i).state, AvailabilityState::DetectedNotCompiled);
        // Enabled but undetected: the ran-with-limitations fact is honored.
        i.compiled_available = true;
        i.project_detected = false;
        i.configured = ConfiguredIntent::Enabled;
        assert_eq!(
            resolve(&i).state,
            AvailabilityState::AdapterRanWithLimitations
        );
    }

    #[test]
    fn unavailable_recovery_is_only_for_missing_builds() {
        for l in PREVIEW {
            let missing = resolve(&input(
                l,
                true,
                false,
                ConfiguredIntent::Unspecified,
                AutoDetectionPolicy::Never,
            ));
            assert_eq!(
                unavailable_recovery(&missing),
                Some(l.unavailable_adapter_recovery().as_str())
            );
            let configured = resolve(&input(
                l,
                false,
                false,
                ConfiguredIntent::Enabled,
                AutoDetectionPolicy::Never,
            ));
            assert_eq!(
                unavailable_recovery(&configured),
                Some(l.unavailable_adapter_recovery().as_str())
            );
            // Compiled adapters and explicit opt-outs never report a build gap.
            for configured in [ConfiguredIntent::Unspecified, ConfiguredIntent::Disabled] {
                let ok = resolve(&input(
                    l,
                    true,
                    true,
                    configured,
                    AutoDetectionPolicy::Never,
                ));
                assert_eq!(unavailable_recovery(&ok), None);
            }
            let opted_out = resolve(&input(
                l,
                true,
                false,
                ConfiguredIntent::Disabled,
                AutoDetectionPolicy::Never,
            ));
            assert_eq!(unavailable_recovery(&opted_out), None);
        }
    }

    #[test]
    fn available_states_carry_no_remediation() {
        let r = resolve(&input(
            LanguageId::TypeScript,
            true,
            true,
            ConfiguredIntent::Enabled,
            AutoDetectionPolicy::Never,
        ));
        assert!(r.remediation.is_empty());
    }
}
