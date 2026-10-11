//! Crate-internal typed error authority (#4859 / ERR1 under #2688).
//!
//! Git invocation timeout is the first migrated matchable family. Later
//! PR-sized slices add cancellation (#4860), oversized-diff (#4861), and
//! check-refusal (#6834) variants. Unmigrated failures travel as
//! [`CoreError::Message`] until those migrations; they are not a semantic
//! family.
//!
//! Public CLI, JSON, and LSP text is rendered at [`Display`] boundaries.
//! Semantic consumers match variants (including through
//! [`CoreError::with_context`]), not rendered prefixes. #2811 remains the
//! public/LSP projection owner: a typed timeout maps to the existing
//! `git_invocation_timeout` kind string.

use std::error::Error;
use std::fmt;

/// Public/LSP projection token for git invocation timeout (#2303 / #2811).
pub(crate) const GIT_INVOCATION_TIMEOUT_KIND: &str = "git_invocation_timeout";

/// Refusal identity for a `check --json` failure outside every named family
/// (#6834): the honest fallback, never a claim about what failed.
pub(crate) const ANALYSIS_FAILED_IDENTITY: &str = "analysis_failed";

/// Repair route for the [`ANALYSIS_FAILED_IDENTITY`] fallback.
pub(crate) const ANALYSIS_FAILED_REPAIR_ROUTE: &str = "analysis/failure";

/// Repair route for a typed git timeout inside a `check --json` refusal.
pub(crate) const GIT_TIMEOUT_REPAIR_ROUTE: &str = "analysis/git-timeout";

pub(crate) const GIT_TIMEOUT_REPAIR_GUIDANCE: &str = " Repair route: raise or disable the git deadline (0 disables it) — \
     --git-timeout SECS or RIPR_GIT_TIMEOUT=<seconds> for CLI runs, the \
     gitTimeoutMs initialization option for editor sessions — then re-run.";

/// Crate-internal error used for semantic control flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoreError {
    /// A git invocation that was given a zero deadline, or that exceeded its
    /// cooperative deadline and was terminated.
    GitInvocationTimeout {
        operation: String,
        timeout_ms: u128,
        spawned: bool,
    },
    /// Cooperative analysis cancellation observed at a checkpoint (#4860).
    /// Display keeps the public `analysis cancelled: <Kind>` wording.
    AnalysisCancelled(crate::analysis::cancellation::AnalysisCancellation),
    /// A named `check` failure family (#6834). The producing branch knows
    /// which input or operation failed, so it constructs this variant
    /// directly; consumers match [`CheckFailureKind`] structurally, never
    /// message text. `detail` is the exact human diagnostic also printed on
    /// stderr, kept verbatim so typed construction never rewords prose.
    CheckFailure {
        kind: CheckFailureKind,
        detail: String,
    },
    /// Unmigrated remainder. Display-only until a later family is typed.
    Message(String),
    /// Structured wrap that preserves the source kind.
    Context {
        context: String,
        source: Box<CoreError>,
    },
}

/// Named `check` failure families (#6834), in the owner-ruled vocabulary.
///
/// Each kind is both the refusal `run_status`/`category`/`limitation` value
/// and the `basis`: a refusal names the limiting condition, and there is no
/// separate budget authority behind it. Scope-guard identities stay owned by
/// #4861 and are not members here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckFailureKind {
    /// The requested base revision does not resolve to a commit.
    BaseUnresolvable,
    /// The repository root cannot be worked with: not a directory, not
    /// inside a Git work tree, or a repository Git cannot read.
    RepositoryRootUnusable,
    /// `ripr.toml` (or the candidate tree's config) failed to load or parse.
    ConfigInvalid,
    /// An explicit `--suppression-policy` file is missing or malformed.
    SuppressionPolicyInvalid,
}

impl CheckFailureKind {
    /// Stable wire identity for the `--json` refusal envelope.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::BaseUnresolvable => "base_unresolvable",
            Self::RepositoryRootUnusable => "repository_root_unusable",
            Self::ConfigInvalid => "config_invalid",
            Self::SuppressionPolicyInvalid => "suppression_policy_invalid",
        }
    }

    /// Stable repair route for the `--json` refusal envelope, in the
    /// existing `analysis/<kebab>` vocabulary.
    pub(crate) fn repair_route(self) -> &'static str {
        match self {
            Self::BaseUnresolvable => "analysis/base-resolution",
            Self::RepositoryRootUnusable => "analysis/repository-root",
            Self::ConfigInvalid => "analysis/config-load",
            Self::SuppressionPolicyInvalid => "analysis/suppression-policy",
        }
    }
}

/// Envelope projection for a `check --json` refusal (#6834): the identity,
/// repair route, and redaction duty the renderer needs, decided here so no
/// renderer matches error text or invents taxonomy.
pub(crate) struct CheckRefusal {
    pub(crate) identity: &'static str,
    pub(crate) repair_route: &'static str,
    /// Whether the envelope message must be the redacted config summary
    /// ([`crate::config::config_error_summary`]) instead of the full
    /// diagnostic: config file contents never enter machine output
    /// (RIPR-SPEC-0007). Every other family echoes the stderr diagnostic
    /// verbatim, exactly what the caller already sees.
    pub(crate) redact_message: bool,
}

impl CoreError {
    pub(crate) fn git_invocation_timeout(
        operation: impl Into<String>,
        timeout_ms: u128,
        spawned: bool,
    ) -> Self {
        Self::GitInvocationTimeout {
            operation: operation.into(),
            timeout_ms,
            spawned,
        }
    }

    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    /// A base revision that does not resolve to a commit (#6834). `detail`
    /// is the exact human diagnostic, kept verbatim.
    pub(crate) fn base_unresolvable(detail: impl Into<String>) -> Self {
        Self::CheckFailure {
            kind: CheckFailureKind::BaseUnresolvable,
            detail: detail.into(),
        }
    }

    /// A repository root Git cannot work with (#6834). `detail` is the
    /// exact human diagnostic, kept verbatim.
    pub(crate) fn repository_root_unusable(detail: impl Into<String>) -> Self {
        Self::CheckFailure {
            kind: CheckFailureKind::RepositoryRootUnusable,
            detail: detail.into(),
        }
    }

    /// A `ripr.toml` (or candidate-tree config) that failed to load (#6834).
    /// `detail` is the exact human diagnostic, kept verbatim; the refusal
    /// renderer redacts it for machine output.
    pub(crate) fn config_invalid(detail: impl Into<String>) -> Self {
        Self::CheckFailure {
            kind: CheckFailureKind::ConfigInvalid,
            detail: detail.into(),
        }
    }

    /// An explicit suppression policy that is missing or malformed (#6834).
    /// `detail` is the exact human diagnostic, kept verbatim.
    pub(crate) fn suppression_policy_invalid(detail: impl Into<String>) -> Self {
        Self::CheckFailure {
            kind: CheckFailureKind::SuppressionPolicyInvalid,
            detail: detail.into(),
        }
    }

    /// Attach human context without dropping a typed timeout kind.
    pub(crate) fn with_context(self, context: impl fmt::Display) -> Self {
        Self::Context {
            context: context.to_string(),
            source: Box::new(self),
        }
    }

    pub(crate) fn is_git_invocation_timeout(&self) -> bool {
        match self {
            Self::GitInvocationTimeout { .. } => true,
            Self::Context { source, .. } => source.is_git_invocation_timeout(),
            Self::AnalysisCancelled(_) | Self::CheckFailure { .. } | Self::Message(_) => false,
        }
    }

    /// True for a typed cancellation, including through `with_context`.
    /// Timeout is a distinct family and never reads as cancellation.
    pub(crate) fn is_analysis_cancelled(&self) -> bool {
        match self {
            Self::AnalysisCancelled(_) => true,
            Self::Context { source, .. } => source.is_analysis_cancelled(),
            Self::GitInvocationTimeout { .. } | Self::CheckFailure { .. } | Self::Message(_) => {
                false
            }
        }
    }

    /// The #2811 / component-outcome kind when this error is a git timeout.
    pub(crate) fn git_invocation_timeout_kind(&self) -> Option<&'static str> {
        self.is_git_invocation_timeout()
            .then_some(GIT_INVOCATION_TIMEOUT_KIND)
    }

    /// The named `check` failure family, including through `with_context`
    /// (#6834). A [`CoreError::Message`] is never a family, even when its
    /// text names one: lookalikes stay on the [`ANALYSIS_FAILED_IDENTITY`]
    /// fallback.
    pub(crate) fn check_failure_kind(&self) -> Option<CheckFailureKind> {
        match self {
            Self::CheckFailure { kind, .. } => Some(*kind),
            Self::Context { source, .. } => source.check_failure_kind(),
            Self::GitInvocationTimeout { .. } | Self::AnalysisCancelled(_) | Self::Message(_) => {
                None
            }
        }
    }

    /// The `--json` refusal projection for this error (#6834): the named
    /// family when one is typed, the existing timeout identity for a typed
    /// timeout, and [`ANALYSIS_FAILED_IDENTITY`] for everything else
    /// (unmigrated messages, cancellation, and any future family until it
    /// is typed). Purely structural: message text is never inspected, so a
    /// scope-guard message still reads as the fallback here — the CLI routes
    /// those to the frozen scope-guard renderer first, and #4861 owns typing
    /// them.
    pub(crate) fn check_refusal(&self) -> CheckRefusal {
        if let Some(kind) = self.check_failure_kind() {
            return CheckRefusal {
                identity: kind.as_str(),
                repair_route: kind.repair_route(),
                redact_message: matches!(kind, CheckFailureKind::ConfigInvalid),
            };
        }
        if self.is_git_invocation_timeout() {
            return CheckRefusal {
                identity: GIT_INVOCATION_TIMEOUT_KIND,
                repair_route: GIT_TIMEOUT_REPAIR_ROUTE,
                redact_message: false,
            };
        }
        CheckRefusal {
            identity: ANALYSIS_FAILED_IDENTITY,
            repair_route: ANALYSIS_FAILED_REPAIR_ROUTE,
            redact_message: false,
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitInvocationTimeout {
                operation,
                spawned: false,
                ..
            } => write!(
                formatter,
                "{GIT_INVOCATION_TIMEOUT_KIND}: {operation} was given a zero deadline (not spawned)"
            ),
            Self::GitInvocationTimeout {
                operation,
                timeout_ms,
                spawned: true,
            } => write!(
                formatter,
                "{GIT_INVOCATION_TIMEOUT_KIND}: {operation} exceeded the {timeout_ms}ms deadline (process terminated).{}",
                GIT_TIMEOUT_REPAIR_GUIDANCE
            ),
            Self::AnalysisCancelled(cancellation) => cancellation.fmt(formatter),
            Self::CheckFailure { detail, .. } => formatter.write_str(detail),
            Self::Message(message) => formatter.write_str(message),
            Self::Context { context, source } => write!(formatter, "{context}: {source}"),
        }
    }
}

impl Error for CoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Context { source, .. } => Some(source.as_ref()),
            Self::GitInvocationTimeout { .. }
            | Self::AnalysisCancelled(_)
            | Self::CheckFailure { .. }
            | Self::Message(_) => None,
        }
    }
}

impl From<crate::analysis::cancellation::AnalysisCancellation> for CoreError {
    fn from(cancellation: crate::analysis::cancellation::AnalysisCancellation) -> Self {
        Self::AnalysisCancelled(cancellation)
    }
}

impl From<String> for CoreError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for CoreError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

impl From<CoreError> for String {
    fn from(error: CoreError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{CoreError, GIT_INVOCATION_TIMEOUT_KIND};
    use std::error::Error;

    fn timeout() -> CoreError {
        CoreError::git_invocation_timeout("git -C /workspace [\"diff\"]", 30000, true)
    }

    #[test]
    fn display_parity_matches_the_public_timeout_wording() {
        let spawned = timeout();
        assert_eq!(
            spawned.to_string(),
            concat!(
                "git_invocation_timeout: git -C /workspace [\"diff\"] exceeded the 30000ms deadline (process terminated).",
                " Repair route: raise or disable the git deadline (0 disables it) — ",
                "--git-timeout SECS or RIPR_GIT_TIMEOUT=<seconds> for CLI runs, the ",
                "gitTimeoutMs initialization option for editor sessions — then re-run."
            )
        );
        let zero = CoreError::git_invocation_timeout("git -C /x [\"status\"]", 0, false);
        assert_eq!(
            zero.to_string(),
            "git_invocation_timeout: git -C /x [\"status\"] was given a zero deadline (not spawned)"
        );
    }

    #[test]
    fn lookalike_message_is_not_a_timeout() {
        // Control 3: an unrelated error whose Display begins with the legacy
        // prefix cannot trigger timeout control flow.
        let lookalike = CoreError::message("git_invocation_timeout: forged prefix must not match");
        assert!(
            lookalike
                .to_string()
                .starts_with(GIT_INVOCATION_TIMEOUT_KIND),
            "the lookalike Display must start with the legacy prefix so a restored prefix matcher would misfire"
        );
        assert!(
            !lookalike.is_git_invocation_timeout(),
            "Message must not be classified as a typed timeout"
        );
        assert_eq!(lookalike.git_invocation_timeout_kind(), None);
    }

    #[test]
    fn wrapped_timeout_keeps_its_kind_without_a_prefix_match() {
        // Controls 4 and 8: wrapping a real timeout must not lose the kind,
        // and restoring prefix matching on Display would fail this case.
        let wrapped = timeout().with_context("workspace analysis failed");
        assert!(wrapped.is_git_invocation_timeout());
        assert_eq!(
            wrapped.git_invocation_timeout_kind(),
            Some(GIT_INVOCATION_TIMEOUT_KIND)
        );
        assert!(
            !wrapped.to_string().starts_with(GIT_INVOCATION_TIMEOUT_KIND),
            "wrapped Display must not start with the legacy prefix; prefix matching would miss it"
        );
        assert!(
            wrapped.to_string().contains(GIT_INVOCATION_TIMEOUT_KIND),
            "inner public wording remains in the Display chain"
        );
    }

    #[test]
    fn source_chain_is_inspectable_without_command_output() {
        // Control 5: source() yields the typed timeout, not unbounded
        // stdout/stderr or extra private paths.
        let wrapped = timeout().with_context("failed to run git diff");
        let source = wrapped
            .source()
            .and_then(|source| source.downcast_ref::<CoreError>());
        assert!(
            source.is_some_and(CoreError::is_git_invocation_timeout),
            "context should expose the typed timeout as source"
        );
        if let Some(CoreError::GitInvocationTimeout {
            operation,
            timeout_ms,
            spawned,
        }) = source
        {
            assert_eq!(operation, "git -C /workspace [\"diff\"]");
            assert_eq!(*timeout_ms, 30000);
            assert!(*spawned);
            assert!(
                !operation.contains('\n'),
                "timeout source must not carry command output"
            );
        }
    }

    #[test]
    fn ordinary_message_with_timeout_words_is_not_typed() {
        // A Message remains ordinary even
        // when stderr mentions similar words.
        let failure = CoreError::message(
            "git -C /workspace [\"diff\"] failed\nstdout: \nstderr: git_invocation_timeout is not a git command",
        );
        assert!(!failure.is_git_invocation_timeout());
        assert!(failure.to_string().contains("git_invocation_timeout"));
    }

    #[test]
    fn prefix_matching_display_is_not_the_semantic_oracle() {
        // Control 8: if production restored `starts_with` on Display, the
        // wrapped timeout would be missed and the lookalike would fire.
        let wrapped = timeout().with_context("failed to run git diff");
        let lookalike = CoreError::message("git_invocation_timeout: forged");
        assert_ne!(
            wrapped.is_git_invocation_timeout(),
            wrapped.to_string().starts_with(GIT_INVOCATION_TIMEOUT_KIND)
        );
        assert_ne!(
            lookalike.is_git_invocation_timeout(),
            lookalike
                .to_string()
                .starts_with(GIT_INVOCATION_TIMEOUT_KIND)
        );
    }

    #[test]
    fn analysis_cancellation_is_typed_wrapped_and_distinct_from_timeout() {
        use crate::analysis::cancellation::{AnalysisAbortKind, AnalysisCancellation};
        let cancelled = CoreError::from(AnalysisCancellation {
            kind: AnalysisAbortKind::Superseded,
        });
        assert_eq!(cancelled.to_string(), "analysis cancelled: Superseded");
        assert!(cancelled.is_analysis_cancelled());
        assert!(!cancelled.is_git_invocation_timeout());
        assert_eq!(cancelled.git_invocation_timeout_kind(), None);

        let wrapped = cancelled.with_context("workspace analysis failed");
        assert!(wrapped.is_analysis_cancelled());
        assert!(!wrapped.to_string().starts_with("analysis cancelled:"));

        for lookalike in [
            CoreError::message("analysis cancelled: Superseded"),
            CoreError::from("analysis cancelled: forged".to_string()),
            CoreError::message("analysis cancelled: Superseded").with_context("outer"),
        ] {
            assert!(!lookalike.is_analysis_cancelled(), "{lookalike}");
        }

        // Timeout and cancellation never stand in for each other.
        assert!(!timeout().is_analysis_cancelled());
        assert!(!timeout().with_context("outer").is_analysis_cancelled());
    }

    #[test]
    fn check_failure_kinds_project_the_ruled_identity_route_and_redaction() {
        use super::{
            ANALYSIS_FAILED_IDENTITY, ANALYSIS_FAILED_REPAIR_ROUTE, CheckFailureKind,
            GIT_TIMEOUT_REPAIR_ROUTE,
        };
        let cases = [
            (
                CoreError::base_unresolvable("the base `x` does not resolve to a commit"),
                CheckFailureKind::BaseUnresolvable,
                "base_unresolvable",
                "analysis/base-resolution",
                false,
            ),
            (
                CoreError::repository_root_unusable("`/` is not inside a Git work tree"),
                CheckFailureKind::RepositoryRootUnusable,
                "repository_root_unusable",
                "analysis/repository-root",
                false,
            ),
            (
                CoreError::config_invalid("ripr.toml: invalid ripr.toml: expected table"),
                CheckFailureKind::ConfigInvalid,
                "config_invalid",
                "analysis/config-load",
                true,
            ),
            (
                CoreError::suppression_policy_invalid("suppression policy `p` is invalid"),
                CheckFailureKind::SuppressionPolicyInvalid,
                "suppression_policy_invalid",
                "analysis/suppression-policy",
                false,
            ),
        ];
        for (error, kind, identity, route, redact) in cases {
            assert_eq!(error.check_failure_kind(), Some(kind), "{error}");
            let refusal = error.check_refusal();
            assert_eq!(refusal.identity, identity, "{error}");
            assert_eq!(refusal.repair_route, route, "{error}");
            assert_eq!(refusal.redact_message, redact, "{error}");
            assert!(!error.is_git_invocation_timeout(), "{error}");
            assert!(!error.is_analysis_cancelled(), "{error}");
        }
        // The typed timeout keeps its existing identity with the new route,
        // and it is distinct from every check family.
        let refusal = timeout().check_refusal();
        assert_eq!(refusal.identity, GIT_INVOCATION_TIMEOUT_KIND);
        assert_eq!(refusal.repair_route, GIT_TIMEOUT_REPAIR_ROUTE);
        assert!(!refusal.redact_message);
        assert_eq!(timeout().check_failure_kind(), None);
        // The fallback covers the unmigrated remainder and cancellation.
        for error in [
            CoreError::message("some future analysis failure"),
            CoreError::from(crate::analysis::cancellation::AnalysisCancellation {
                kind: crate::analysis::cancellation::AnalysisAbortKind::Superseded,
            }),
        ] {
            let refusal = error.check_refusal();
            assert_eq!(refusal.identity, ANALYSIS_FAILED_IDENTITY, "{error}");
            assert_eq!(
                refusal.repair_route, ANALYSIS_FAILED_REPAIR_ROUTE,
                "{error}"
            );
            assert!(!refusal.redact_message, "{error}");
            assert_eq!(error.check_failure_kind(), None, "{error}");
        }
    }

    #[test]
    fn check_failure_detail_stays_verbatim_and_wrapped_kind_survives() {
        let detail = "the base `x` does not resolve to a commit (the analysis did not run).";
        let error = CoreError::base_unresolvable(detail);
        assert_eq!(error.to_string(), detail);
        let wrapped = error.with_context("workspace analysis failed");
        assert_eq!(
            wrapped.to_string(),
            format!("workspace analysis failed: {detail}")
        );
        assert_eq!(
            wrapped.check_failure_kind(),
            Some(super::CheckFailureKind::BaseUnresolvable)
        );
        assert_eq!(wrapped.check_refusal().identity, "base_unresolvable");
    }

    #[test]
    fn check_failure_lookalike_message_stays_on_the_fallback() {
        // #6834 negative control, text side: a Message whose Display names a
        // family must not claim that family's identity; restoring prefix
        // matching on Display would misfire here and on the wrapped case
        // above (whose Display names no family at the start).
        for text in [
            "base_unresolvable: forged prefix must not match",
            "config_invalid: forged prefix must not match",
            "repository_root_unusable: forged",
            "suppression_policy_invalid: forged",
        ] {
            let lookalike = CoreError::message(text);
            assert_eq!(lookalike.check_failure_kind(), None, "{text}");
            assert_eq!(
                lookalike.check_refusal().identity,
                super::ANALYSIS_FAILED_IDENTITY,
                "{text}"
            );
        }
        // Scope-guard messages are still untyped until #4861, so they read
        // as the fallback here; the CLI routes them to the frozen renderer
        // before this projection ever sees them.
        let guard = CoreError::message("diff_scope_oversized: 3 lines exceed the limit");
        assert_eq!(guard.check_failure_kind(), None);
        assert_eq!(
            guard.check_refusal().identity,
            super::ANALYSIS_FAILED_IDENTITY
        );
    }
}
