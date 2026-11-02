//! Public application error boundary for library consumers (#1986 / #1914).
//!
//! [`CoreError`] remains the crate-internal authority (#4859). This module
//! projects the first migrated vertical — `app::check` — onto a stable,
//! `#[non_exhaustive]` public surface without a repository-wide
//! `Result<_, String>` conversion.

use std::error::Error;
use std::fmt;

use crate::core_error::{CheckFailureKind, CoreError};

/// High-level failure category for programmatic handling (#1986).
///
/// Categories are stable across releases until explicitly versioned; new
/// categories may appear without breaking source compatibility because this
/// enum is `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiprErrorCategory {
    Configuration,
    RepositoryOrGit,
    DiffOrInput,
    Analysis,
    OutputOrSchema,
    PolicyOrGate,
    ReceiptOrArtifact,
    UnsupportedOrLimited,
    CancelledOrTimedOut,
    InternalInvariant,
}

impl RiprErrorCategory {
    /// Stable snake_case category token for JSON and agent consumers.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::RepositoryOrGit => "repository_or_git",
            Self::DiffOrInput => "diff_or_input",
            Self::Analysis => "analysis",
            Self::OutputOrSchema => "output_or_schema",
            Self::PolicyOrGate => "policy_or_gate",
            Self::ReceiptOrArtifact => "receipt_or_artifact",
            Self::UnsupportedOrLimited => "unsupported_or_limited",
            Self::CancelledOrTimedOut => "cancelled_or_timed_out",
            Self::InternalInvariant => "internal_invariant",
        }
    }
}

/// Public library error for the first migrated check vertical (#1986).
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq)]
pub enum RiprError {
    GitInvocationTimeout {
        operation: String,
        timeout_ms: u128,
        spawned: bool,
    },
    AnalysisCancelled {
        abort: &'static str,
    },
    CheckFailure {
        category: RiprErrorCategory,
        code: &'static str,
        detail: String,
    },
    Unclassified {
        detail: String,
    },
    Context {
        context: String,
        source: Box<RiprError>,
    },
}

impl RiprError {
    /// Projects a crate-internal [`CoreError`] onto the public boundary.
    pub(crate) fn from_core(error: CoreError) -> Self {
        match error {
            CoreError::GitInvocationTimeout {
                operation,
                timeout_ms,
                spawned,
            } => Self::GitInvocationTimeout {
                operation,
                timeout_ms,
                spawned,
            },
            CoreError::AnalysisCancelled(cancellation) => Self::AnalysisCancelled {
                abort: cancellation_abort_label(cancellation.kind),
            },
            CoreError::CheckFailure { kind, detail } => {
                let (category, code) = check_failure_projection(kind);
                Self::CheckFailure {
                    category,
                    code,
                    detail,
                }
            }
            CoreError::Message(message) => Self::Unclassified { detail: message },
            CoreError::Context { context, source } => Self::Context {
                context,
                source: Box::new(Self::from_core(*source)),
            },
        }
    }

    /// High-level category for this error, including through [`Self::Context`].
    pub fn category(&self) -> RiprErrorCategory {
        match self {
            Self::GitInvocationTimeout { .. } | Self::AnalysisCancelled { .. } => {
                RiprErrorCategory::CancelledOrTimedOut
            }
            Self::CheckFailure { category, .. } => *category,
            Self::Unclassified { .. } => RiprErrorCategory::InternalInvariant,
            Self::Context { source, .. } => source.category(),
        }
    }

    /// Stable machine code aligned with check JSON refusal identities where
    /// applicable (#6834 / RIPR-SPEC-0241).
    pub fn code(&self) -> &'static str {
        match self {
            Self::GitInvocationTimeout { .. } => crate::core_error::GIT_INVOCATION_TIMEOUT_KIND,
            Self::AnalysisCancelled { .. } => "analysis_cancelled",
            Self::CheckFailure { code, .. } => code,
            Self::Unclassified { .. } => crate::core_error::ANALYSIS_FAILED_IDENTITY,
            Self::Context { source, .. } => source.code(),
        }
    }

    /// True when this failure is cooperative cancellation, not an internal bug.
    pub fn is_analysis_cancelled(&self) -> bool {
        match self {
            Self::AnalysisCancelled { .. } => true,
            Self::Context { source, .. } => source.is_analysis_cancelled(),
            _ => false,
        }
    }

    /// True when this failure names a static limitation rather than an internal defect.
    pub fn is_supported_limitation(&self) -> bool {
        match self {
            Self::CheckFailure { code, .. } => {
                check_failure_kind_from_code(code) == Some(CheckFailureKind::SeamNotActionable)
            }
            Self::Context { source, .. } => source.is_supported_limitation(),
            _ => false,
        }
    }

    /// True when this failure must not be classified as an internal invariant.
    pub fn is_user_or_operational(&self) -> bool {
        self.is_analysis_cancelled()
            || self.is_supported_limitation()
            || matches!(
                self.category(),
                RiprErrorCategory::Configuration
                    | RiprErrorCategory::RepositoryOrGit
                    | RiprErrorCategory::DiffOrInput
                    | RiprErrorCategory::PolicyOrGate
                    | RiprErrorCategory::ReceiptOrArtifact
                    | RiprErrorCategory::CancelledOrTimedOut
            )
    }

    fn human_message(&self) -> String {
        self.to_core_error().to_string()
    }

    fn to_core_error(&self) -> CoreError {
        match self {
            Self::GitInvocationTimeout {
                operation,
                timeout_ms,
                spawned,
            } => CoreError::git_invocation_timeout(operation.clone(), *timeout_ms, *spawned),
            Self::AnalysisCancelled { abort } => {
                CoreError::message(format!("analysis cancelled: {abort}"))
            }
            Self::CheckFailure { code, detail, .. } => {
                reconstruct_check_failure(code, detail.clone())
            }
            Self::Unclassified { detail } => CoreError::message(detail.clone()),
            Self::Context { context, source } => {
                Self::to_core_error(source).with_context(context.clone())
            }
        }
    }
}

fn cancellation_abort_label(
    kind: crate::analysis::cancellation::AnalysisAbortKind,
) -> &'static str {
    match kind {
        crate::analysis::cancellation::AnalysisAbortKind::Superseded => "Superseded",
        crate::analysis::cancellation::AnalysisAbortKind::Cancelled => "Cancelled",
        crate::analysis::cancellation::AnalysisAbortKind::DeadlineExceeded => "DeadlineExceeded",
    }
}

fn check_failure_projection(kind: CheckFailureKind) -> (RiprErrorCategory, &'static str) {
    let code = kind.as_str();
    let category = match kind {
        CheckFailureKind::BaseUnresolvable | CheckFailureKind::RepositoryRootUnusable => {
            RiprErrorCategory::RepositoryOrGit
        }
        CheckFailureKind::ConfigInvalid => RiprErrorCategory::Configuration,
        CheckFailureKind::SuppressionPolicyInvalid | CheckFailureKind::SeamPolicyOmitted => {
            RiprErrorCategory::PolicyOrGate
        }
        CheckFailureKind::CheckArtifactUnusable | CheckFailureKind::GapLedgerUnusable => {
            RiprErrorCategory::ReceiptOrArtifact
        }
        CheckFailureKind::SeamIdUnknown | CheckFailureKind::SelectorMiss => {
            RiprErrorCategory::Analysis
        }
        CheckFailureKind::SeamNotActionable => RiprErrorCategory::UnsupportedOrLimited,
        CheckFailureKind::AnalysisTimeout => RiprErrorCategory::CancelledOrTimedOut,
        CheckFailureKind::DiffInputUnsupported => RiprErrorCategory::DiffOrInput,
        CheckFailureKind::AnalysisWorkerStopped => RiprErrorCategory::InternalInvariant,
    };
    (category, code)
}

fn check_failure_kind_from_code(code: &str) -> Option<CheckFailureKind> {
    match code {
        "base_unresolvable" => Some(CheckFailureKind::BaseUnresolvable),
        "repository_root_unusable" => Some(CheckFailureKind::RepositoryRootUnusable),
        "config_invalid" => Some(CheckFailureKind::ConfigInvalid),
        "suppression_policy_invalid" => Some(CheckFailureKind::SuppressionPolicyInvalid),
        "check_artifact_unusable" => Some(CheckFailureKind::CheckArtifactUnusable),
        "gap_ledger_unusable" => Some(CheckFailureKind::GapLedgerUnusable),
        "seam_id_unknown" => Some(CheckFailureKind::SeamIdUnknown),
        "seam_policy_omitted" => Some(CheckFailureKind::SeamPolicyOmitted),
        "seam_not_actionable" => Some(CheckFailureKind::SeamNotActionable),
        "selector_miss" => Some(CheckFailureKind::SelectorMiss),
        "analysis_timeout" => Some(CheckFailureKind::AnalysisTimeout),
        "diff_input_unsupported" => Some(CheckFailureKind::DiffInputUnsupported),
        "analysis_worker_stopped" => Some(CheckFailureKind::AnalysisWorkerStopped),
        _ => None,
    }
}

fn reconstruct_check_failure(code: &str, detail: String) -> CoreError {
    let Some(kind) = check_failure_kind_from_code(code) else {
        return CoreError::message(detail);
    };
    match kind {
        CheckFailureKind::BaseUnresolvable => CoreError::base_unresolvable(detail),
        CheckFailureKind::RepositoryRootUnusable => CoreError::repository_root_unusable(detail),
        CheckFailureKind::ConfigInvalid => CoreError::config_invalid(detail),
        CheckFailureKind::SuppressionPolicyInvalid => CoreError::suppression_policy_invalid(detail),
        CheckFailureKind::CheckArtifactUnusable => CoreError::check_artifact_unusable(detail),
        CheckFailureKind::GapLedgerUnusable => CoreError::gap_ledger_unusable(detail),
        CheckFailureKind::SeamIdUnknown => CoreError::seam_id_unknown(detail),
        CheckFailureKind::SeamPolicyOmitted => CoreError::seam_policy_omitted(detail),
        CheckFailureKind::SeamNotActionable => CoreError::seam_not_actionable(detail),
        CheckFailureKind::SelectorMiss => CoreError::selector_miss(detail),
        CheckFailureKind::AnalysisTimeout => CoreError::analysis_timeout(detail),
        CheckFailureKind::DiffInputUnsupported => CoreError::diff_input_unsupported(detail),
        CheckFailureKind::AnalysisWorkerStopped => CoreError::analysis_worker_stopped(detail),
    }
}

impl fmt::Display for RiprError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.human_message())
    }
}

impl fmt::Debug for RiprError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitInvocationTimeout {
                operation,
                timeout_ms,
                spawned,
            } => formatter
                .debug_struct("RiprError::GitInvocationTimeout")
                .field("operation", operation)
                .field("timeout_ms", timeout_ms)
                .field("spawned", spawned)
                .finish(),
            Self::AnalysisCancelled { abort } => formatter
                .debug_struct("RiprError::AnalysisCancelled")
                .field("abort", abort)
                .finish(),
            Self::CheckFailure {
                category,
                code,
                detail,
            } if *category == RiprErrorCategory::Configuration => formatter
                .debug_struct("RiprError::CheckFailure")
                .field("category", category)
                .field("code", code)
                .field(
                    "detail",
                    &crate::config::config_error_summary(detail.as_str()),
                )
                .finish(),
            Self::CheckFailure {
                category,
                code,
                detail,
            } => formatter
                .debug_struct("RiprError::CheckFailure")
                .field("category", category)
                .field("code", code)
                .field("detail", detail)
                .finish(),
            Self::Unclassified { detail } => formatter
                .debug_struct("RiprError::Unclassified")
                .field("detail", detail)
                .finish(),
            Self::Context { context, source } => formatter
                .debug_struct("RiprError::Context")
                .field("context", context)
                .field("source", source)
                .finish(),
        }
    }
}

impl Error for RiprError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Context { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<RiprError> for String {
    fn from(error: RiprError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{RiprError, RiprErrorCategory};
    use crate::core_error::CoreError;
    use std::error::Error;

    #[test]
    fn categories_and_codes_do_not_depend_on_message_text() {
        let error = RiprError::from_core(CoreError::base_unresolvable(
            "the base `main` does not resolve to a commit",
        ));
        assert_eq!(error.category(), RiprErrorCategory::RepositoryOrGit);
        assert_eq!(error.code(), "base_unresolvable");
        let rewritten =
            RiprError::from_core(CoreError::base_unresolvable("completely different prose"));
        assert_eq!(error.category(), rewritten.category());
        assert_eq!(error.code(), rewritten.code());
    }

    #[test]
    fn human_output_matches_core_display_for_check_failures() {
        let detail = "ripr.toml: invalid ripr.toml: key=value\nsecret=1";
        let core = CoreError::config_invalid(detail);
        let public = RiprError::from_core(core.clone());
        assert_eq!(public.to_string(), core.to_string());
    }

    #[test]
    fn cancellation_is_not_an_internal_invariant() {
        let cancelled = RiprError::from_core(CoreError::from(
            crate::analysis::cancellation::AnalysisCancellation {
                kind: crate::analysis::cancellation::AnalysisAbortKind::Superseded,
            },
        ));
        assert!(cancelled.is_analysis_cancelled());
        assert_ne!(cancelled.category(), RiprErrorCategory::InternalInvariant);
        assert_eq!(cancelled.code(), "analysis_cancelled");
    }

    #[test]
    fn limitation_is_not_an_internal_invariant() {
        let limited = RiprError::from_core(CoreError::seam_not_actionable(
            "seam s is not actionable under current policy",
        ));
        assert!(limited.is_supported_limitation());
        assert_eq!(limited.category(), RiprErrorCategory::UnsupportedOrLimited);
    }

    #[test]
    fn wrapped_errors_preserve_source_and_category() {
        let wrapped = RiprError::from_core(
            CoreError::base_unresolvable("missing base").with_context("check failed"),
        );
        assert_eq!(wrapped.category(), RiprErrorCategory::RepositoryOrGit);
        assert_eq!(wrapped.code(), "base_unresolvable");
        assert!(wrapped.source().is_some());
    }

    #[test]
    fn debug_redacts_config_excerpt() {
        let secret = "ripr.toml: invalid ripr.toml: expected table\npassword=\"secret\"";
        let error = RiprError::from_core(CoreError::config_invalid(secret));
        let debug = format!("{error:?}");
        assert!(!debug.contains("password"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn unknown_category_arm_is_required_for_forward_compat() {
        fn category_name(category: RiprErrorCategory) -> &'static str {
            match category {
                RiprErrorCategory::Configuration => "configuration",
                RiprErrorCategory::RepositoryOrGit
                | RiprErrorCategory::DiffOrInput
                | RiprErrorCategory::Analysis
                | RiprErrorCategory::OutputOrSchema
                | RiprErrorCategory::PolicyOrGate
                | RiprErrorCategory::ReceiptOrArtifact
                | RiprErrorCategory::UnsupportedOrLimited
                | RiprErrorCategory::CancelledOrTimedOut
                | RiprErrorCategory::InternalInvariant => "known",
                #[expect(
                    unreachable_patterns,
                    reason = "policy:1986-forward-compat: non_exhaustive category arm for downstream-style tests"
                )]
                _ => "future",
            }
        }
        assert_eq!(
            category_name(RiprErrorCategory::Configuration),
            "configuration"
        );
    }
}
