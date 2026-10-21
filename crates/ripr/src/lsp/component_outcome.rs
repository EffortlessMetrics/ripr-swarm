//! Typed bounded component-outcome records for the LSP analysis pipeline
//! (#1997, RIPR-SPEC-0141).
//!
//! Every optional analysis component that can fail or degrade independently
//! of the diff analysis records one [`ComponentOutcome`] on the committed
//! [`super::state::AnalysisSnapshot`]. The snapshot is the single typed
//! authority: diagnostics severity policy, hover, code actions, standard
//! work-done progress, `ripr/analysisStatus`, and workspace status all read
//! the same records, so no degradation is reported only through process
//! stderr (the pre-#1997 hidden-stderr fallback).
//!
//! Vocabulary rules:
//!
//! - component states are the closed set `complete | unavailable | limited |
//!   failed | cancelled | deferred`; a degraded component (`limited` or
//!   `failed`) makes the shared run status `limited` via
//!   [`super::diagnostics::derive_run_status`] — never a silent `full`;
//! - messages are normalized, path-redacted, and size-bounded by
//!   [`bounded_message`] before they can reach any client surface;
//! - `findings_trustworthy` states explicitly whether ordinary diff-scoped
//!   findings remain usable evidence under the degradation; nothing about a
//!   degraded optional component downgrades the findings themselves.

/// One optional analysis component whose outcome is recorded per refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum AnalysisComponent {
    Diff,
    SeamInventory,
    GapLedger,
    CausalProjection,
    Cache,
    // NOTE (RIPR-SPEC-0141): `report` is deferred spec vocabulary and
    // intentionally absent from the code until a real producer exists —
    // out-of-scope projection suppression is disclosed typed via
    // `out_of_scope_test_file_findings`, not a fabricated report outcome.
}

impl AnalysisComponent {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::SeamInventory => "seam_inventory",
            Self::GapLedger => "gap_ledger",
            Self::CausalProjection => "causal_projection",
            Self::Cache => "cache",
        }
    }
}

/// Closed outcome-state vocabulary for one component (RIPR-SPEC-0141).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ComponentState {
    Complete,
    Unavailable,
    Limited,
    Failed,
    // NOTE (RIPR-SPEC-0141): `cancelled` is deferred spec vocabulary and
    // intentionally absent from the code until a real producer exists — a
    // cancelled attempt never commits a snapshot, so no producer constructs
    // this state today.
    Deferred,
}

impl ComponentState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Unavailable => "unavailable",
            Self::Limited => "limited",
            Self::Failed => "failed",
            Self::Deferred => "deferred",
        }
    }
}

/// The typed, bounded record of one component's outcome for one snapshot.
#[derive(Clone, Debug)]
pub(super) struct ComponentOutcome {
    pub(super) component: AnalysisComponent,
    pub(super) state: ComponentState,
    /// Stable machine-readable code for the specific outcome (for example
    /// `seam_inventory_failed`, `stale_artifact`). `None` for a plain
    /// `complete` outcome.
    pub(super) kind: Option<&'static str>,
    /// Bounded, path-redacted, single-line detail. `None` when the outcome
    /// needs no explanation beyond `component`/`state`/`kind`.
    pub(super) message: Option<String>,
    /// Whether ordinary diff-scoped findings remain trustworthy evidence
    /// under this outcome. Recorded explicitly so no consumer has to guess.
    pub(super) findings_trustworthy: bool,
    /// The safe recovery action a client may name to the user.
    pub(super) recovery: Option<&'static str>,
}

impl ComponentOutcome {
    pub(super) fn complete(component: AnalysisComponent) -> Self {
        Self {
            component,
            state: ComponentState::Complete,
            kind: None,
            message: None,
            findings_trustworthy: true,
            recovery: None,
        }
    }

    /// The component did not run for a named, repairable configuration
    /// reason (#6001): a normal, non-degraded `unavailable` state, but
    /// the wire names the switch that enables the component and the message
    /// names the blocking condition, instead of a bare `recovery: null` no
    /// agent consumer can act on.
    pub(super) fn unavailable_recoverable(
        component: AnalysisComponent,
        kind: &'static str,
        message: impl Into<String>,
        recovery: &'static str,
    ) -> Self {
        Self {
            component,
            state: ComponentState::Unavailable,
            kind: Some(kind),
            message: Some(bounded_message(&message.into())),
            findings_trustworthy: true,
            recovery: Some(recovery),
        }
    }

    /// The component was intentionally deferred to a later, explicit run
    /// (RIPR-SPEC-0105 seams deferral). Deferred is disclosed but not a
    /// degradation: it must not warn on every interactive refresh.
    pub(super) fn deferred(
        component: AnalysisComponent,
        kind: &'static str,
        recovery: &'static str,
    ) -> Self {
        Self {
            component,
            state: ComponentState::Deferred,
            kind: Some(kind),
            message: None,
            findings_trustworthy: true,
            recovery: Some(recovery),
        }
    }

    /// The component produced partial evidence; ordinary findings remain
    /// trustworthy unless the caller says otherwise.
    pub(super) fn limited(
        component: AnalysisComponent,
        kind: &'static str,
        message: impl Into<String>,
        findings_trustworthy: bool,
        recovery: &'static str,
    ) -> Self {
        Self {
            component,
            state: ComponentState::Limited,
            kind: Some(kind),
            message: Some(bounded_message(&message.into())),
            findings_trustworthy,
            recovery: Some(recovery),
        }
    }

    /// The component failed; ordinary findings remain trustworthy unless the
    /// caller says otherwise. The message is bounded and path-redacted at
    /// construction so no surface can leak unbounded error text.
    pub(super) fn failed(
        component: AnalysisComponent,
        kind: &'static str,
        message: impl Into<String>,
        findings_trustworthy: bool,
        recovery: &'static str,
    ) -> Self {
        Self {
            component,
            state: ComponentState::Failed,
            kind: Some(kind),
            message: Some(bounded_message(&message.into())),
            findings_trustworthy,
            recovery: Some(recovery),
        }
    }

    /// True when the component degraded the run (`limited` or `failed`).
    /// `unavailable`, `deferred`, `cancelled`, and `complete` are disclosed
    /// states, not degradations.
    pub(super) fn is_degraded(&self) -> bool {
        matches!(self.state, ComponentState::Limited | ComponentState::Failed)
    }

    /// One concise, single-line client-facing summary of the degraded
    /// outcome, naming the recovery route without overclaiming.
    pub(super) fn log_summary(&self) -> String {
        let kind = self.kind.unwrap_or(self.state.as_str());
        let detail = self
            .message
            .as_deref()
            .map(|message| format!(": {message}"))
            .unwrap_or_default();
        let recovery = self
            .recovery
            .map(|recovery| format!(" — recovery: {recovery}"))
            .unwrap_or_default();
        format!(
            "{} {} ({kind}){detail}{recovery}",
            self.component.as_str(),
            self.state.as_str()
        )
    }

    /// The typed payload exposed through `ripr/analysisStatus` (and embedded
    /// in workspace status). `snapshot_identity` binds the record to the
    /// snapshot/input identity that produced it.
    pub(super) fn status_payload(&self, snapshot_identity: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "component": self.component.as_str(),
            "state": self.state.as_str(),
            "kind": self.kind,
            "message": self.message,
            "findings_trustworthy": self.findings_trustworthy,
            "recovery": self.recovery,
            "snapshot_identity": snapshot_identity,
        })
    }

    /// The stable dedup fragment for this outcome: identical degradations on
    /// consecutive refreshes produce an identical fragment, which the backend
    /// uses to emit at most one warning per distinct degradation (#1997).
    /// The bounded message is part of the fragment: a NEW failure detail
    /// under the same kind is a different degradation, not a duplicate — its
    /// updated warning must be sent. Messages are already normalized,
    /// path-redacted, and bounded at construction, so a repeated failure
    /// repeats the same fragment.
    fn degradation_fragment(&self) -> Option<String> {
        if !self.is_degraded() {
            return None;
        }
        Some(format!(
            "{}={}:{}:{}",
            self.component.as_str(),
            self.state.as_str(),
            self.kind.unwrap_or("-"),
            self.message.as_deref().unwrap_or("-")
        ))
    }
}

/// The degradation signature for one snapshot's component outcomes: `None`
/// when nothing is degraded, otherwise a deterministic string over the
/// sorted degraded fragments. Byte-identical repeated degradations share one
/// signature, so the backend warns once per distinct degradation and logs one
/// recovery line when the signature clears.
pub(super) fn degradation_signature(outcomes: &[ComponentOutcome]) -> Option<String> {
    let mut fragments = outcomes
        .iter()
        .filter_map(ComponentOutcome::degradation_fragment)
        .collect::<Vec<_>>();
    if fragments.is_empty() {
        return None;
    }
    fragments.sort();
    Some(fragments.join("|"))
}

/// The concise warning text for one distinct degradation signature, naming
/// each degraded component and its recovery route.
pub(super) fn degradation_log_message(outcomes: &[ComponentOutcome]) -> Option<String> {
    let degraded = outcomes
        .iter()
        .filter(|outcome| outcome.is_degraded())
        .map(ComponentOutcome::log_summary)
        .collect::<Vec<_>>();
    if degraded.is_empty() {
        return None;
    }
    Some(format!("ripr analysis limited: {}", degraded.join("; ")))
}

/// Client-visible LSP detail budget (#1997, #8584). Truncation keeps a
/// one-character marker inside this count so `ripr/analysisStatus` summaries
/// never grow past the bound and never look complete when text was dropped.
pub(super) const CLIENT_VISIBLE_MESSAGE_CHAR_BOUND: usize = 240;

/// Normalize, path-redact, and size-bound one free-text detail string before
/// it can reach a client surface. Single source for the LSP path: the
/// backend's health-failure messages delegate here so error text is governed
/// once (#1997). Absolute path tokens remain redacted when they are wrapped
/// in quotes, backticks, or brackets; the wrappers stay in place (#8724).
/// When the redacted text exceeds the bound, the result ends with `…` and
/// still fits in the 240-character budget (#8584).
pub(super) fn bounded_message(message: &str) -> String {
    let normalized = message
        .chars()
        .map(|character| match character {
            '\r' | '\n' | '\t' => ' ',
            character => character,
        })
        .collect::<String>();
    let path_safe = normalized
        .split_whitespace()
        .map(redact_absolute_path_token)
        .collect::<Vec<_>>()
        .join(" ");
    if path_safe.chars().count() <= CLIENT_VISIBLE_MESSAGE_CHAR_BOUND {
        return path_safe;
    }
    let mut bounded = path_safe
        .chars()
        .take(CLIENT_VISIBLE_MESSAGE_CHAR_BOUND.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

/// Quote, backtick, and bracket wrappers that can hide an absolute path
/// at the start of a whitespace token (#8724).
const PATH_TOKEN_LEADING_WRAPPERS: &[char] = &['`', '\'', '"', '[', '{'];

/// Matching closers plus ordinary trailing sentence punctuation. The
/// wrappers are stripped only for the path test and then put back so
/// `` `/abs` `` becomes `` `<path>` `` rather than a rewritten sentence.
const PATH_TOKEN_TRAILING_PUNCTUATION: &[char] =
    &['`', '\'', '"', ']', '}', ',', '.', ';', '!', '?'];

fn redact_absolute_path_token(token: &str) -> std::borrow::Cow<'_, str> {
    let leading_len = token
        .chars()
        .take_while(|character| PATH_TOKEN_LEADING_WRAPPERS.contains(character))
        .map(char::len_utf8)
        .sum();
    let Some((leading, remainder)) = token.split_at_checked(leading_len) else {
        return std::borrow::Cow::Borrowed(token);
    };
    let trailing_len = remainder
        .chars()
        .rev()
        .take_while(|character| PATH_TOKEN_TRAILING_PUNCTUATION.contains(character))
        .map(char::len_utf8)
        .sum();
    let Some(core_end) = remainder.len().checked_sub(trailing_len) else {
        return std::borrow::Cow::Borrowed(token);
    };
    let Some((core, trailing)) = remainder.split_at_checked(core_end) else {
        return std::borrow::Cow::Borrowed(token);
    };
    if is_absolute_path_token(core) {
        std::borrow::Cow::Owned(format!("{leading}<path>{trailing}"))
    } else {
        std::borrow::Cow::Borrowed(token)
    }
}

fn is_absolute_path_token(token: &str) -> bool {
    token.starts_with('/')
        || token.starts_with('\\')
        || token
            .as_bytes()
            .get(1)
            .is_some_and(|character| *character == b':')
}

#[cfg(test)]
mod tests {
    use super::{AnalysisComponent, ComponentOutcome, ComponentState, degradation_signature};

    /// #4350: the editor bounds failure text, and the long-root failure
    /// reaches it behind the workspace-analysis and diff prefixes. The
    /// remedy must survive that cut, not only the diagnosis.
    #[test]
    fn bounded_long_root_failure_keeps_the_remedy() {
        // `check-local-context` forbids drive-letter literals; assemble one.
        let drive = "D:";
        let root = format!("{drive}\\{}", "long-path-segment\\".repeat(20));
        let describe = format!("git -C {root} [\"diff\", \"--no-color\"]");
        let err = std::io::Error::from_raw_os_error(267);
        let message = format!(
            "workspace analysis failed: failed to run git diff: {}",
            crate::git::windows_path_limit_message(
                "git",
                root.encode_utf16().count(),
                &err,
                &describe
            )
        );
        let bounded = super::bounded_message(&message);
        assert!(
            bounded.ends_with('…'),
            "fixture must exceed the bound: {bounded}"
        );
        assert!(
            bounded.contains("clone or move the repository to a shorter path"),
            "{bounded}"
        );
        assert!(bounded.contains("(MAX_PATH)"), "{bounded}");
    }

    #[test]
    fn degraded_states_are_exactly_limited_and_failed() {
        let mut degraded = ComponentOutcome::complete(AnalysisComponent::Diff);
        assert!(!degraded.is_degraded());
        degraded = ComponentOutcome::unavailable_recoverable(
            AnalysisComponent::SeamInventory,
            "disabled",
            "seam diagnostics are disabled by configuration",
            "set [lsp] seam_diagnostics = true in ripr.toml, then run ripr.refresh",
        );
        assert!(!degraded.is_degraded());
        degraded =
            ComponentOutcome::deferred(AnalysisComponent::SeamInventory, "defer", "retry refresh");
        assert!(!degraded.is_degraded());
        degraded = ComponentOutcome::limited(
            AnalysisComponent::Cache,
            "stale_artifact",
            "stale",
            true,
            "run ripr check",
        );
        assert!(degraded.is_degraded());
        degraded = ComponentOutcome::failed(
            AnalysisComponent::GapLedger,
            "gap_ledger_parse_failed",
            "parse failed",
            true,
            "run ripr check",
        );
        assert!(degraded.is_degraded());
    }

    #[test]
    fn failed_outcome_bounds_and_redacts_message() -> Result<(), String> {
        let long = format!(
            "read /workspace/example/ledger.json failed: {}",
            "x".repeat(400)
        );
        let outcome = ComponentOutcome::failed(
            AnalysisComponent::GapLedger,
            "gap_ledger_read_failed",
            long,
            true,
            "run ripr check to regenerate the gap decision ledger",
        );
        let message = outcome
            .message
            .as_deref()
            .ok_or_else(|| "failed outcome must carry a message".to_string())?;
        if message.contains("/home/user") {
            return Err(format!("message leaked an absolute path: {message}"));
        }
        if message.chars().count() > super::CLIENT_VISIBLE_MESSAGE_CHAR_BOUND {
            return Err(format!("message exceeded the bound: {message}"));
        }
        if !message.ends_with('…') {
            return Err(format!("truncated message must carry a marker: {message}"));
        }
        if !message.contains("<path>") {
            return Err(format!("message must redact the path token: {message}"));
        }
        Ok(())
    }

    /// #8584: clients must see that status-summary text was dropped, and the
    /// marker must stay inside the published 240-character bound.
    #[test]
    fn bounded_message_marks_truncation_inside_the_char_bound() {
        let message = format!("cargo metadata failed: {}", "x".repeat(400));
        let bounded = super::bounded_message(&message);
        assert!(
            bounded.ends_with('…'),
            "truncated status text must carry a marker: {bounded}"
        );
        assert_eq!(
            bounded.chars().count(),
            super::CLIENT_VISIBLE_MESSAGE_CHAR_BOUND,
            "the marker must stay inside the published bound: {bounded}"
        );
        assert!(
            !bounded.contains('\n') && !bounded.contains('\t'),
            "{bounded}"
        );
        assert!(bounded.starts_with("cargo metadata failed: "), "{bounded}");
    }

    #[test]
    fn bounded_message_leaves_an_exact_bound_text_unmarked() {
        let message = "a".repeat(super::CLIENT_VISIBLE_MESSAGE_CHAR_BOUND);
        let bounded = super::bounded_message(&message);
        assert_eq!(bounded, message);
        assert!(!bounded.ends_with('…'));
        assert_eq!(
            bounded.chars().count(),
            super::CLIENT_VISIBLE_MESSAGE_CHAR_BOUND
        );
    }

    #[test]
    fn bounded_message_redacts_a_long_path_instead_of_slicing_it() {
        let message = format!("/{}", "secret-segment/".repeat(40));
        let bounded = super::bounded_message(&message);
        assert_eq!(bounded, "<path>");
        assert!(!bounded.contains("secret-segment"));
        assert!(!bounded.ends_with('…'));
    }

    #[test]
    fn bounded_message_redacts_paths_that_remain_after_the_cut() {
        let message = format!(
            "read {} failed: {}",
            "/tmp/ripr-failure-secret.toml",
            "y".repeat(400)
        );
        let bounded = super::bounded_message(&message);
        assert!(bounded.contains("<path>"), "{bounded}");
        assert!(
            !bounded.contains("/tmp/ripr-failure-secret.toml"),
            "redaction must cover the text that remains: {bounded}"
        );
        assert!(bounded.ends_with('…'), "{bounded}");
        assert_eq!(
            bounded.chars().count(),
            super::CLIENT_VISIBLE_MESSAGE_CHAR_BOUND,
            "{bounded}"
        );
    }

    /// #8724: a backticked POSIX path starts with a backtick, so a
    /// leading-slash-only test left the absolute token intact.
    #[test]
    fn bounded_message_redacts_a_backticked_posix_path() {
        let leaked = "/abs/root/target/debug/missing-base";
        let message = format!("the base `{leaked}` does not resolve to a commit");
        let bounded = super::bounded_message(&message);
        assert_eq!(
            bounded, "the base `<path>` does not resolve to a commit",
            "{bounded}"
        );
        assert!(
            !bounded.contains("/abs/root"),
            "backticks must not hide the path token: {bounded}"
        );
    }

    /// #8724: a quoted Windows drive path starts with `"` rather than
    /// the drive letter. Assemble the drive so the local-context gate
    /// does not see a path literal.
    #[test]
    fn bounded_message_redacts_a_quoted_windows_path() {
        let drive = "D:";
        let leaked = format!(r#""{drive}\abs\root\file.rs""#);
        let message = format!("read {leaked} failed");
        let bounded = super::bounded_message(&message);
        assert_eq!(bounded, r#"read "<path>" failed"#, "{bounded}");
        assert!(
            !bounded.contains(r"\abs\root"),
            "quotes must not hide the path token: {bounded}"
        );
    }

    /// #8724: trailing sentence punctuation stays after `<path>`.
    #[test]
    fn bounded_message_redacts_a_path_and_keeps_trailing_punctuation() {
        let leaked = "/abs/root/ledger.json";
        let message = format!("read {leaked}, then retry");
        let bounded = super::bounded_message(&message);
        assert_eq!(bounded, "read <path>, then retry", "{bounded}");
        assert!(
            !bounded.contains("/abs/root"),
            "trailing punctuation must not hide the path token: {bounded}"
        );
    }

    #[test]
    fn status_payload_names_component_state_kind_and_recovery() -> Result<(), String> {
        let outcome = ComponentOutcome::failed(
            AnalysisComponent::CausalProjection,
            "causal_projection_unusable",
            "parse canonical delta failed",
            true,
            "run ripr check to regenerate the causal delta artifact",
        );
        let payload = outcome.status_payload(Some("snapshot:7"));
        if payload["component"].as_str() != Some("causal_projection")
            || payload["state"].as_str() != Some("failed")
            || payload["kind"].as_str() != Some("causal_projection_unusable")
            || payload["findings_trustworthy"].as_bool() != Some(true)
            || payload["recovery"].as_str()
                != Some("run ripr check to regenerate the causal delta artifact")
            || payload["snapshot_identity"].as_str() != Some("snapshot:7")
        {
            return Err(format!("unexpected status payload: {payload}"));
        }
        Ok(())
    }

    /// #6001: an unavailable component with a known enable switch must put
    /// that switch on the wire — `recovery: null` gives an agent consumer no
    /// path to the component, and the state must stay non-degraded.
    #[test]
    fn unavailable_recoverable_names_enable_route_without_degrading() -> Result<(), String> {
        let outcome = ComponentOutcome::unavailable_recoverable(
            AnalysisComponent::SeamInventory,
            "seam_diagnostics_not_enabled",
            "seam diagnostics require the full profile (current: actionable)",
            "set [lsp] diagnostic_profile = \"full\" in ripr.toml, then run ripr.refresh",
        );
        if outcome.is_degraded() {
            return Err("an unavailable outcome must never degrade the run".to_string());
        }
        let payload = outcome.status_payload(Some("snapshot:1"));
        if payload["state"].as_str() != Some("unavailable")
            || payload["kind"].as_str() != Some("seam_diagnostics_not_enabled")
            || payload["findings_trustworthy"].as_bool() != Some(true)
        {
            return Err(format!("unexpected unavailable payload: {payload}"));
        }
        let Some(recovery) = payload["recovery"].as_str() else {
            return Err(format!("recovery must name the enable route: {payload}"));
        };
        if !recovery.contains("diagnostic_profile") || !recovery.contains("ripr.refresh") {
            return Err(format!("recovery must name the enable route: {recovery}"));
        }
        let Some(message) = payload["message"].as_str() else {
            return Err(format!(
                "message must name the blocking condition: {payload}"
            ));
        };
        if !message.contains("actionable") {
            return Err(format!("message must name the current profile: {message}"));
        }
        Ok(())
    }

    #[test]
    fn degradation_signature_is_none_without_degradation_and_stable_when_degraded() {
        let clean = vec![
            ComponentOutcome::complete(AnalysisComponent::Diff),
            ComponentOutcome::deferred(
                AnalysisComponent::SeamInventory,
                "interactive_refresh_deferral",
                "run ripr.refreshDiagnostics for the full seam inventory",
            ),
        ];
        assert_eq!(degradation_signature(&clean), None);

        let degraded_a = vec![
            ComponentOutcome::complete(AnalysisComponent::Diff),
            ComponentOutcome::failed(
                AnalysisComponent::SeamInventory,
                "seam_inventory_failed",
                "walk failed",
                true,
                "retry ripr.refreshDiagnostics",
            ),
        ];
        let mut degraded_b = degraded_a.clone();
        degraded_b.reverse();
        let signature_a = degradation_signature(&degraded_a);
        assert_eq!(signature_a, degradation_signature(&degraded_b));
        assert_eq!(
            signature_a.as_deref(),
            Some("seam_inventory=failed:seam_inventory_failed:walk failed")
        );

        // A new failure detail under the same component/state/kind is a
        // different degradation, not a duplicate (#1997 review): its updated
        // warning must be sent.
        let degraded_new_detail = vec![
            ComponentOutcome::complete(AnalysisComponent::Diff),
            ComponentOutcome::failed(
                AnalysisComponent::SeamInventory,
                "seam_inventory_failed",
                "walk timed out",
                true,
                "retry ripr.refreshDiagnostics",
            ),
        ];
        assert_ne!(
            signature_a,
            degradation_signature(&degraded_new_detail),
            "a changed failure detail must change the dedup signature"
        );
    }

    #[test]
    fn degradation_log_message_names_component_and_recovery() -> Result<(), String> {
        let outcomes = vec![ComponentOutcome::failed(
            AnalysisComponent::SeamInventory,
            "seam_inventory_failed",
            "walk failed",
            true,
            "retry ripr.refreshDiagnostics",
        )];
        let message = super::degradation_log_message(&outcomes)
            .ok_or_else(|| "degraded outcomes must produce a log message".to_string())?;
        if !message.contains("seam_inventory failed")
            || !message.contains("retry ripr.refreshDiagnostics")
        {
            return Err(format!(
                "log message must name component and recovery: {message}"
            ));
        }
        Ok(())
    }

    #[test]
    fn deferred_and_unavailable_states_have_distinct_strings() {
        assert_ne!(
            ComponentState::Deferred.as_str(),
            ComponentState::Unavailable.as_str()
        );
        assert_ne!(
            ComponentState::Failed.as_str(),
            ComponentState::Limited.as_str()
        );
    }
}
