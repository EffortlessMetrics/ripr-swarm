/// Advisory LSP codeLens for changed symbols.
///
/// RIPR-SPEC-0100: emits one advisory `CodeLens` per distinct line and title
/// among the `Finding`s whose probe location maps to the requested document URI. The title cites the finding's
/// `related_tests.len()` from the cached `AnalysisSnapshot` — the same source
/// that feeds `finding.class` — so the count is never fabricated.
///
/// Honesty invariants (enforced by unit tests):
/// - `snapshot == None` → empty Vec (absence of analysis ≠ fabricated 0-count).
/// - Count comes solely from `finding.related_tests.len()`, never from the
///   diagnostic projection or any runtime test runner output.
/// - Static-language vocab only (approved RIPR exposure terms); no runtime
///   mutation-testing vocabulary in any title.
/// - Preview-tier findings (TS / Python — `language_status == Some(Preview)`)
///   have advisory text prefixed with `preview:` and softened so the count is
///   not read as
///   confirmed by a static discriminator.
/// - N == 0 uses "no related tests found" + the class label, not the
///   forbidden unexercised-synonym that belongs to runtime mutation tooling.
use super::client_features::ClientFeatureProfile;
use super::uri::{file_uri_for_path, file_uris_match};
use super::{
    OPEN_RELATED_TEST_COMMAND, REFRESH_COMMAND, SHOW_RELATED_TEST_COMMAND, state::AnalysisSnapshot,
};
use crate::analysis_outcome::normalize_portable_analysis_path;
use crate::domain::{ExposureClass, Finding, LanguageStatus, OracleStrength, RelatedTest};
use std::path::Path;
use std::time::Duration;
use tower_lsp_server::ls_types::{CodeLens, Command, Position, Range, Uri};

/// Deterministic identity of the *visible* lens set for one committed
/// snapshot (#2032, RIPR-SPEC-0138).
///
/// The publish path compares this identity against the last one for which it
/// requested `workspace/codeLens/refresh` and sends the request only when the
/// identity changed. The identity is built from structured inputs only:
/// per-finding (finding id, canonical document URI, lens line, related-test
/// count, classification class, preview/limitation markers) plus the
/// snapshot's input identity. Wall-clock fields (snapshot age, the rendered
/// `· as of Xs ago` title suffix) are deliberately excluded — they change on
/// every snapshot without changing the semantic lens view, so comparing
/// rendered titles would falsely trigger a refresh on every commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LensViewIdentity {
    input_identity: Option<String>,
    items: Vec<LensViewItem>,
}

impl LensViewIdentity {
    /// The empty visible view: no snapshot is current (root removed,
    /// ambiguous, or analysis state cleared), so every lens should vanish.
    pub(super) fn cleared() -> Self {
        Self {
            input_identity: None,
            items: Vec::new(),
        }
    }
}

#[cfg(test)]
impl LensViewIdentity {
    /// The zero-based rows of the visible lens set, for row-owner tests.
    pub(super) fn rows(&self) -> Vec<u32> {
        self.items.iter().map(|item| item.line).collect()
    }
}

/// One finding's contribution to the visible lens set.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct LensViewItem {
    finding_id: String,
    document_uri: String,
    line: u32,
    related_test_count: usize,
    /// The related test a navigating lens opens, so a changed target is a
    /// lens-view change (`workspace/codeLens/refresh`).
    navigation_target: Option<(String, usize, String)>,
    class: String,
    preview: bool,
    static_limit_kind: Option<String>,
}

/// Compute the lens-view identity for a snapshot. Pure: reads only the
/// snapshot's structured fields, never the wall clock or rendered titles.
// #3282: the identity mirrors the visible lens set, so a change
// touching only non-actionable findings does not trigger a spurious
// workspace/codeLens/refresh.
pub(super) fn lens_view_identity(snapshot: &AnalysisSnapshot) -> LensViewIdentity {
    let mut items: Vec<LensViewItem> = snapshot
        .findings
        .iter()
        .filter(|finding| finding.is_candidate_actionable())
        .map(|finding| {
            let file = &finding.probe.location.file;
            let absolute = if file.is_absolute() {
                file.clone()
            } else {
                snapshot.root.join(file)
            };
            let document_uri = match file_uri_for_path(&absolute) {
                Ok(uri) => uri.as_str().to_string(),
                Err(_) => absolute.display().to_string(),
            };
            LensViewItem {
                finding_id: finding.id.clone(),
                document_uri,
                line: lens_line(snapshot, finding),
                related_test_count: finding.related_tests.len(),
                navigation_target: lens_navigation_test(snapshot, finding).map(|test| {
                    (
                        test.file.to_string_lossy().into_owned(),
                        test.line,
                        test.name.clone(),
                    )
                }),
                class: finding.class.as_str().to_string(),
                preview: finding
                    .language_status
                    .as_ref()
                    .is_some_and(|status| *status == LanguageStatus::Preview),
                static_limit_kind: finding
                    .static_limit_kind
                    .as_ref()
                    .map(|kind| kind.as_str().to_string()),
            }
        })
        .collect();
    // Sort so a findings-order change that does not alter the visible set
    // does not read as a lens-view change.
    items.sort();
    LensViewIdentity {
        input_identity: snapshot.input_identity_id(),
        items,
    }
}

/// Produce advisory `CodeLens` items for all findings that map to `uri`
/// inside `snapshot`.
///
/// Returns an empty Vec when `snapshot` is `None` — absence of an analysis
/// snapshot is not the same as confirmed absence of related tests.
pub(super) fn code_lens_response(
    uri: &Uri,
    snapshot: Option<&AnalysisSnapshot>,
    client_features: &ClientFeatureProfile,
) -> Vec<CodeLens> {
    let Some(snapshot) = snapshot else {
        return Vec::new();
    };
    let age = snapshot.refresh.age();
    let mut lenses: Vec<CodeLens> = Vec::new();
    for lens in lens_findings(uri, snapshot)
        .map(|finding| finding_to_code_lens(snapshot, finding, age, client_features))
    {
        // Several probes on one line (a predicate and its returned value) often
        // read identically; one lens per distinct line and title keeps the
        // editor from stacking repeated text. Hover still lists every finding.
        if !lenses
            .iter()
            .any(|seen| seen.range == lens.range && lens_title(seen) == lens_title(&lens))
        {
            lenses.push(lens);
        }
    }
    lenses
}

fn lens_title(lens: &CodeLens) -> Option<&str> {
    lens.command.as_ref().map(|command| command.title.as_str())
}

/// The findings that produce code lenses for `uri`, in snapshot order. Lenses
/// with an equal range and title collapse, so several findings can share one.
fn lens_findings<'a>(
    uri: &'a Uri,
    snapshot: &'a AnalysisSnapshot,
) -> impl Iterator<Item = &'a Finding> + 'a {
    snapshot
        .findings
        .iter()
        // Candidate-actionable eligibility (#3282, per RIPR-SPEC-0152):
        // a lens anchors an advisory at the finding's recorded line,
        // and a base-deleted finding's recorded line is the projected
        // new-side coordinate — not a candidate position to pin.
        .filter(|finding| finding.is_candidate_actionable())
        .filter(|finding| finding_matches_uri(finding, uri, &snapshot.root))
}

/// The findings whose code lens sits on the zero-based `line` of `uri`, so
/// hover can list every finding the lens (or lenses) on that line summarize.
pub(super) fn lens_findings_at_line<'a>(
    uri: &'a Uri,
    snapshot: &'a AnalysisSnapshot,
    line: u32,
) -> Vec<&'a Finding> {
    lens_findings(uri, snapshot)
        .filter(|finding| lens_line(snapshot, finding) == line)
        .collect()
}

/// The single row owner for code-lens placement, the semantic lens-view
/// identity and unpublished line-hover lookup (#10609, RIPR-SPEC-0100).
///
/// A finding with a captured Rust diagnostic origin uses that origin's editor
/// row, the same numeric row published diagnostics use, so standalone-CR files
/// agree across surfaces. The row is read from the snapshot, never from disk,
/// so a later save cannot move it, and it does not depend on whether a
/// diagnostic was published. Without an origin (non-Rust finding or a producer
/// that captured none) the fallback is the analyzer row; no text search is
/// performed and no exact span is claimed. A lens is always zero-width.
fn lens_line(snapshot: &AnalysisSnapshot, finding: &Finding) -> u32 {
    match snapshot.rust_diagnostic_origins.for_finding(finding) {
        Some(origin) => origin.line,
        None => finding.probe.location.line.saturating_sub(1) as u32,
    }
}

/// Build one `CodeLens` for a single finding.
///
/// A row with an honest related-test target opens it through the negotiated
/// navigation command; every other row refreshes saved-workspace analysis
/// through the registered server command. The title keeps the cached advisory
/// distinct from the action. No resolve round-trip is needed.
/// Honest advisory when the Rust adapter recorded a parse failure for this file.
const PARSE_FAILED_LENS_TITLE: &str =
    "ripr: related tests unknown, this file did not parse · fix the syntax error and save";

fn finding_portable_path(finding: &Finding, root: &Path) -> Option<String> {
    let file = &finding.probe.location.file;
    let relative = if file.is_absolute() {
        file.strip_prefix(root).ok()?.to_path_buf()
    } else {
        file.clone()
    };
    normalize_portable_analysis_path(relative.to_string_lossy().as_ref()).ok()
}

/// True when `snapshot.analysis_outcome` carries a language-adapter producer
/// failure limitation for the same portable path as `finding`.
fn file_parse_failed(snapshot: &AnalysisSnapshot, finding: &Finding) -> bool {
    let Some(finding_path) = finding_portable_path(finding, &snapshot.root) else {
        return false;
    };
    let Some(outcome) = snapshot.analysis_outcome.as_ref() else {
        return false;
    };
    outcome.limitations.iter().any(|limitation| {
        super::diagnostics::limitation_is_file_parse_failure(limitation)
            && limitation.path.as_deref() == Some(finding_path.as_str())
    })
}

/// The related test a lens click may open, or `None` when the row has no
/// honest target. Fail closed: a finding with no static path, no related test,
/// or a file that did not parse never navigates, and neither does a test
/// outside the workspace root. Prefers a test with a strong oracle, otherwise
/// the first listed (the same order the lens count comes from).
fn lens_navigation_test<'a>(
    snapshot: &AnalysisSnapshot,
    finding: &'a Finding,
) -> Option<&'a RelatedTest> {
    if matches!(finding.class, ExposureClass::NoStaticPath) || file_parse_failed(snapshot, finding)
    {
        return None;
    }
    let test = finding
        .related_tests
        .iter()
        .find(|test| test.oracle_strength == OracleStrength::Strong)
        .or_else(|| finding.related_tests.first())?;
    let path = if test.file.is_absolute() {
        test.file.clone()
    } else {
        snapshot.root.join(&test.file)
    };
    super::uri::path_is_within_root(snapshot.root.as_path(), &path).then_some(test)
}

/// The navigation command for `test`, negotiated exactly like the
/// `quickfix.ripr.navigate` code action (RIPR-SPEC-0129): the client-side
/// `ripr.openRelatedTest` when advertised, else the server-executed
/// `ripr.showRelatedTest` for a `window/showDocument` client bound to the
/// snapshot input identity. `None` when neither route exists.
fn lens_navigation_command(
    snapshot: &AnalysisSnapshot,
    test: &RelatedTest,
    client_features: &ClientFeatureProfile,
) -> Option<(&'static str, serde_json::Value)> {
    let path = if test.file.is_absolute() {
        test.file.clone()
    } else {
        snapshot.root.join(&test.file)
    };
    let uri = file_uri_for_path(&path).ok()?;
    let mut target = serde_json::json!({
        "uri": uri.as_str(),
        "line": test.line,
        "test_name": test.name.as_str(),
    });
    if client_features.supports_client_command(OPEN_RELATED_TEST_COMMAND) {
        return Some((OPEN_RELATED_TEST_COMMAND, target));
    }
    if client_features.show_document {
        let identity = snapshot.input_identity_id()?;
        target.as_object_mut()?.insert(
            "input_identity".to_string(),
            serde_json::Value::String(identity),
        );
        return Some((SHOW_RELATED_TEST_COMMAND, target));
    }
    None
}

fn finding_to_code_lens(
    snapshot: &AnalysisSnapshot,
    finding: &Finding,
    age: Option<Duration>,
    client_features: &ClientFeatureProfile,
) -> CodeLens {
    let line = lens_line(snapshot, finding);
    let range = Range {
        start: Position { line, character: 0 },
        end: Position { line, character: 0 },
    };
    let body = if file_parse_failed(snapshot, finding) {
        PARSE_FAILED_LENS_TITLE.to_string()
    } else {
        related_test_lens_title(finding, age)
    };
    let navigation = lens_navigation_test(snapshot, finding).and_then(|test| {
        lens_navigation_command(snapshot, test, client_features).map(|route| (test, route))
    });
    let command = match navigation {
        Some((test, (command, target))) => Command {
            title: format!("{body} · open {}", test.name),
            command: command.to_string(),
            arguments: Some(vec![target]),
        },
        None => Command {
            title: format!("{body} · refresh saved workspace"),
            command: REFRESH_COMMAND.to_string(),
            arguments: None,
        },
    };
    CodeLens {
        range,
        command: Some(command),
        data: None,
    }
}

/// Decide whether a `Finding` belongs to the requested document URI.
///
/// Uses `file_uri_for_path` (the same helper used by `diagnostics.rs`) to
/// build the finding's canonical file URI, then compares with
/// `file_uris_match` for cross-platform correctness (Windows drive-letter
/// case insensitivity, percent-encoding normalization).
fn finding_matches_uri(finding: &Finding, uri: &Uri, root: &Path) -> bool {
    let file = &finding.probe.location.file;
    let absolute = if file.is_absolute() {
        file.clone()
    } else {
        root.join(file)
    };
    match file_uri_for_path(&absolute) {
        Ok(finding_uri) => file_uris_match(&finding_uri, uri),
        Err(_) => false,
    }
}

/// Build the advisory lens title for a finding.
///
/// Rules (all vocabulary passes `check-static-language`):
///
/// - Preview tier (`language_status == Some(Preview)`): prefix `preview:`,
///   use softened phrasing — the count may be incomplete for preview adapters.
/// - `NoStaticPath`: `ripr: no test reaches this change (PLAIN) · static, cached`
///   regardless of file/name-token overlap in `related_tests` (count is not shown
///   as a positive related-test grip).
/// - N > 0 + discriminating class (`Exposed`):
///   `ripr: N related test(s) (exposed) · static, cached`
/// - N > 0 + non-discriminating class (anything except `Exposed`):
///   `ripr: N related test(s), no static check of this change (PLAIN) · static, cached`
/// - N == 0 (non-`NoStaticPath`): `ripr: no related tests found (PLAIN) · static, cached`
///   where `PLAIN` is the same plain vocabulary word (`exposed`, `weak`,
///   `unrevealed`, `no path`, `unknown`) the `ripr check` summary uses, not
///   the schema value (`as_str` stays in JSON / `human-full` / `explain`).
/// - Appends `· as of Xs ago` when the snapshot age is known.
///
/// Vocabulary enforcement: titles must not contain runtime mutation-testing
/// terms (the gate bans those words in all tracked prose). // ripr-allow: static-language: doc describing the vocabulary constraint, not emitting the terms
pub(super) fn related_test_lens_title(finding: &Finding, age: Option<Duration>) -> String {
    let is_preview = finding
        .language_status
        .as_ref()
        .is_some_and(|s| *s == LanguageStatus::Preview);
    let class_label = finding.class.plain_label();
    let count = finding.related_tests.len();

    let related_tests_phrase = |n: usize| {
        if n == 1 {
            "1 related test".to_string()
        } else {
            format!("{n} related tests")
        }
    };

    let body = if is_preview {
        // Preview tier: count may be incomplete; soften claim.
        if matches!(finding.class, ExposureClass::NoStaticPath) {
            format!("preview: no test reaches this change ({class_label}) · static, cached")
        } else if count == 0 {
            format!("preview: no related tests found ({class_label}) · static, cached")
        } else {
            format!(
                "preview: {} (preview, {class_label}) · static, cached",
                related_tests_phrase(count)
            )
        }
    } else if matches!(finding.class, ExposureClass::NoStaticPath) {
        format!("ripr: no test reaches this change ({class_label}) · static, cached")
    } else {
        match count {
            0 => format!("ripr: no related tests found ({class_label}) · static, cached"),
            n if finding.class == ExposureClass::Exposed => format!(
                "ripr: {} (exposed) · static, cached",
                related_tests_phrase(n)
            ),
            n => format!(
                "ripr: {}, no static check of this change ({class_label}) · static, cached",
                related_tests_phrase(n)
            ),
        }
    };

    match age {
        Some(d) => format!("{body} · as of {}s ago", d.as_secs()),
        None => body,
    }
}

/// Check that a generated lens title contains none of the forbidden
/// static-language vocabulary terms. Used by tests.
#[cfg(test)]
pub(super) fn lens_title_is_static_language_clean(title: &str) -> bool {
    // These string literals are test guards checking that the words do not
    // appear in production output — the strings themselves are not output.
    let forbidden = [
        "killed", // ripr-allow: static-language: test guard verifying this term does not appear in lens output
        "survived", // ripr-allow: static-language: test guard verifying this term does not appear in lens output
        "proven", // ripr-allow: static-language: test guard verifying this term does not appear in lens output
        "adequate", // ripr-allow: static-language: test guard verifying this term does not appear in lens output
        "covered", // ripr-allow: static-language: test guard verifying this term does not appear in lens output
        "passing", // ripr-allow: static-language: test guard verifying this term does not appear in lens output
        "untested", // ripr-allow: static-language: test guard verifying this term does not appear in lens output
    ];
    !forbidden.iter().any(|word| title.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis_outcome::{
        AnalysisIdentity, AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcome,
        AnalysisOutcomeCounts, AnalysisOutcomeKind, AnalysisRecovery, AnalysisRecoveryKind,
        AnalysisStage,
    };
    use crate::app::Mode;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, ExposureClass, Finding, LanguageStatus,
        OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest, RevealEvidence,
        RiprEvidence, SourceLocation, StageEvidence, StageState, SymbolId,
    };
    use crate::lsp::gap_artifacts::{GapArtifactRejection, ValidatedGapArtifact};
    use crate::lsp::state::{AnalysisSnapshot, RefreshMetadata};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use tower_lsp_server::ls_types::{Diagnostic, Position, Range};

    fn parse_uri(s: &str) -> Result<Uri, String> {
        s.parse()
            .map_err(|e| format!("failed to parse test URI {s}: {e}"))
    }

    fn make_related_test(i: usize) -> RelatedTest {
        RelatedTest {
            name: format!("test_{i}"),
            file: PathBuf::from("tests/lib.rs"),
            line: 10 + i,
            oracle: None,
            oracle_kind: OracleKind::Unknown,
            oracle_strength: OracleStrength::Weak,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }
    }

    fn make_finding(
        file: &str,
        line: usize,
        class: ExposureClass,
        related_test_count: usize,
        language_status: Option<LanguageStatus>,
    ) -> Finding {
        let related_tests = (0..related_test_count).map(make_related_test).collect();
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: format!("probe:{file}:{line}:test_family:aabbccdd"),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId(format!("probe:{file}:{line}:test_family:aabbccdd")),
                location: SourceLocation::new(file, line, 1),
                owner: Some(SymbolId("owner::fn".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Value,
                before: None,
                after: Some("true".to_string()),
                expression: "x > 0".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class,
            ripr: RiprEvidence {
                reach: StageEvidence::new(StageState::Yes, Confidence::High, "reachable"),
                infect: StageEvidence::new(StageState::Yes, Confidence::High, "infectable"),
                propagate: StageEvidence::new(StageState::Yes, Confidence::Medium, "propagatable"),
                reveal: RevealEvidence {
                    observe: StageEvidence::new(StageState::Weak, Confidence::Medium, "observed"),
                    discriminate: StageEvidence::new(
                        StageState::Weak,
                        Confidence::Medium,
                        "discriminated",
                    ),
                },
            },
            confidence: 1.0,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests,
            recommended_next_step: None,
            language: None,
            language_status,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    fn language_adapter_producer_failure_limitation(
        path: &str,
    ) -> Result<AnalysisLimitation, String> {
        let recovery = AnalysisRecovery::new(
            AnalysisRecoveryKind::InspectFailure,
            crate::analysis_outcome::RUST_PARSE_FAILURE_RECOVERY,
        )?;
        AnalysisLimitation::new(
            AnalysisLimitationKind::ProducerFailure,
            AnalysisStage::LanguageAdapter,
            recovery,
        )
        .with_path(path)
    }

    fn snapshot_with_parse_failure_limitation(
        root: &str,
        findings: Vec<Finding>,
        limitation: AnalysisLimitation,
    ) -> Result<AnalysisSnapshot, String> {
        let mut snapshot = make_snapshot_with_findings(root, findings);
        snapshot.analysis_outcome = Some(AnalysisOutcome::new(
            AnalysisOutcomeKind::PartialWithLimitations,
            AnalysisIdentity::default(),
            AnalysisOutcomeCounts {
                finding_count: snapshot.findings.len() as u64,
                ..AnalysisOutcomeCounts::default()
            },
            vec![limitation],
        )?);
        Ok(snapshot)
    }

    fn make_snapshot_with_findings(root: &str, findings: Vec<Finding>) -> AnalysisSnapshot {
        // Build a diagnostics_by_uri that satisfies is_consistent():
        // one diagnostic per finding, each carrying "finding_id".
        let mut diagnostics_by_uri = BTreeMap::new();
        for finding in &findings {
            let file = &finding.probe.location.file;
            let absolute = if file.is_absolute() {
                file.clone()
            } else {
                PathBuf::from(root).join(file)
            };
            if let Ok(uri) = file_uri_for_path(&absolute) {
                let line = finding.probe.location.line.saturating_sub(1) as u32;
                let diag = Diagnostic {
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position {
                            line,
                            character: 120,
                        },
                    },
                    severity: None,
                    code: None,
                    code_description: None,
                    source: Some("ripr".to_string()),
                    message: "test".to_string(),
                    related_information: None,
                    tags: None,
                    data: Some(serde_json::json!({ "finding_id": finding.id })),
                };
                diagnostics_by_uri
                    .entry(uri)
                    .or_insert_with(Vec::new)
                    .push(diag);
            }
        }
        AnalysisSnapshot {
            root: PathBuf::from(root),
            rust_consumed_sources: Default::default(),
            input_identity: None,
            base: None,
            mode: Mode::Draft,
            refresh: RefreshMetadata::default(),
            findings,
            analysis_outcome: None,
            diagnostic_profile: crate::config::LspDiagnosticProfile::Full,
            classified_seams: Vec::new(),
            repair_card_candidate_seams: None,
            gap_artifacts: Vec::<ValidatedGapArtifact>::new(),
            gap_artifact_rejections: Vec::<GapArtifactRejection>::new(),
            harness_facts: super::super::state::HarnessFactsOnSnapshot::NotRegistered,
            diagnostics_by_uri: diagnostics_by_uri.into(),
            diagnostic_uri_index: None,
            delivery_selection: None,
            seams_deferred: false,
            partial_scope: None,
            component_outcomes: Vec::new(),
            out_of_scope_test_file_findings: 0,
            unavailable_adapter: super::super::state::UnavailableAdapterCoverage::default(),
            language_view: None,
            rust_diagnostic_origins: Default::default(),
        }
    }

    #[test]
    fn lens_view_identity_ignores_wall_clock_age() -> Result<(), String> {
        // The wall-clock trap (#2032): a byte-identical re-commit differs only
        // in refresh metadata, which feeds the rendered `· as of Xs ago`
        // title suffix. The structured identity must NOT change, or every
        // snapshot would falsely trigger a codeLens refresh.
        let root = "/workspace";
        let finding = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 2, None);
        let first = make_snapshot_with_findings(root, vec![finding.clone()]);
        let mut second = make_snapshot_with_findings(root, vec![finding]);
        second.refresh.generated_at = first
            .refresh
            .generated_at
            .checked_add(Duration::from_hours(1))
            .ok_or_else(|| "failed to offset test timestamp".to_string())?;
        second.refresh.duration = Some(Duration::from_millis(42));

        if lens_view_identity(&first) != lens_view_identity(&second) {
            return Err(
                "wall-clock-only snapshot difference must not change the lens-view identity"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn lens_view_identity_tracks_semantic_lens_fields() -> Result<(), String> {
        let root = "/workspace";
        let base = lens_view_identity(&make_snapshot_with_findings(
            root,
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::Exposed,
                2,
                None,
            )],
        ));

        let count_changed = lens_view_identity(&make_snapshot_with_findings(
            root,
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::Exposed,
                3,
                None,
            )],
        ));
        if base == count_changed {
            return Err(
                "a related-test count change must change the lens-view identity".to_string(),
            );
        }

        let class_changed = lens_view_identity(&make_snapshot_with_findings(
            root,
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::WeaklyExposed,
                2,
                None,
            )],
        ));
        if base == class_changed {
            return Err("a classification change must change the lens-view identity".to_string());
        }

        let line_changed = lens_view_identity(&make_snapshot_with_findings(
            root,
            vec![make_finding(
                "src/lib.rs",
                11,
                ExposureClass::Exposed,
                2,
                None,
            )],
        ));
        if base == line_changed {
            return Err("a lens line change must change the lens-view identity".to_string());
        }

        let preview_changed = lens_view_identity(&make_snapshot_with_findings(
            root,
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::Exposed,
                2,
                Some(LanguageStatus::Preview),
            )],
        ));
        if base == preview_changed {
            return Err("a preview-marker change must change the lens-view identity".to_string());
        }

        // Findings order alone does not change the visible lens set.
        let finding_a = make_finding("src/a.rs", 10, ExposureClass::Exposed, 1, None);
        let finding_b = make_finding("src/b.rs", 20, ExposureClass::NoStaticPath, 0, None);
        let ordered = lens_view_identity(&make_snapshot_with_findings(
            root,
            vec![finding_a.clone(), finding_b.clone()],
        ));
        let reversed = lens_view_identity(&make_snapshot_with_findings(
            root,
            vec![finding_b, finding_a],
        ));
        if ordered != reversed {
            return Err("findings order must not change the lens-view identity".to_string());
        }

        // The snapshot's input identity is part of the view identity.
        let mut identified = make_snapshot_with_findings(
            root,
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::Exposed,
                2,
                None,
            )],
        );
        let config = crate::lsp::config::LspAnalysisConfig::default();
        identified.input_identity = Some(
            crate::lsp::input_identity::LspAnalysisInputIdentity::from_refresh_inputs(
                PathBuf::from(root),
                7,
                &config,
            ),
        );
        if lens_view_identity(&identified) == base {
            return Err("an input-identity change must change the lens-view identity".to_string());
        }
        Ok(())
    }

    #[test]
    fn code_lens_wire_advertises_an_honest_registered_refresh() -> Result<(), String> {
        let provider = super::super::capabilities::initialize_result()
            .capabilities
            .execute_command_provider
            .ok_or("missing actual command advertisement")?;
        for status in [None, Some(LanguageStatus::Preview)] {
            let preview = status.is_some();
            let finding = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 2, status);
            let snapshot = make_snapshot_with_findings("/workspace", vec![finding]);
            let uri = parse_uri("file:///workspace/src/lib.rs")?;
            let lenses =
                code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());
            if lenses.len() != 1 {
                return Err(format!(
                    "wire control must emit exactly one lens: {lenses:?}"
                ));
            }
            let lens = lenses.first().ok_or("missing actual emitted lens")?;
            let wire = serde_json::to_value(lens).map_err(|error| error.to_string())?;
            let command = wire.get("command").ok_or("missing wire command")?;
            let id = command
                .get("command")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing wire command identifier")?;
            if id != "ripr.refresh" {
                return Err(format!("lens must offer registered refresh, got {id:?}"));
            }
            if !provider.commands.iter().any(|advertised| advertised == id) {
                return Err(format!("emitted command was not advertised: {id}"));
            }
            let title = command
                .get("title")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing wire command title")?;
            if !title.contains("2 related tests")
                || !title.ends_with("· refresh saved workspace")
                || !title.contains("exposed")
                || !title.contains("static, cached")
                || preview != title.contains("preview")
            {
                return Err(format!(
                    "refresh action lost its honest cached summary: {title}"
                ));
            }
            if command
                .get("arguments")
                .is_some_and(|arguments| !arguments.is_null())
            {
                return Err("lens refresh must not introduce command arguments".into());
            }
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_collapses_identical_lenses_on_one_line() -> Result<(), String> {
        let first = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 3, None);
        // A distinct probe on the same expression line that renders the same
        // title: the case the user sees, so dedupe cannot key on finding id.
        let mut twin = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 3, None);
        twin.id = "probe:src/lib.rs:10:return_value:11223344".to_string();
        twin.probe.id = ProbeId(twin.id.clone());
        twin.probe.family = ProbeFamily::ReturnValue;
        let other_line = make_finding("src/lib.rs", 20, ExposureClass::Exposed, 3, None);
        let different = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 5, None);
        let snapshot =
            make_snapshot_with_findings("/workspace", vec![first, twin, other_line, different]);
        let uri = parse_uri("file:///workspace/src/lib.rs")?;

        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());

        // Twin on the same line with the same title collapses; a distinct line
        // and a distinct title on the same line both survive.
        let survivors: Vec<(u32, String)> = lenses
            .iter()
            .map(|lens| {
                let title = lens
                    .command
                    .as_ref()
                    .map(|command| command.title.clone())
                    .unwrap_or_default();
                (lens.range.start.line, title)
            })
            .collect();
        let lines: Vec<u32> = survivors.iter().map(|(line, _)| *line).collect();
        if lines != vec![9, 19, 9] {
            return Err(format!(
                "expected survivors on lines [9, 19, 9] in snapshot order, got {survivors:?}"
            ));
        }
        if survivors[0].1 == survivors[2].1 || !survivors[2].1.contains('5') {
            return Err(format!(
                "the distinct title on line 9 must survive: {survivors:?}"
            ));
        }
        if survivors[0].1 != survivors[1].1 {
            return Err(format!(
                "same-count lenses must share a title: {survivors:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_reports_real_related_test_count() -> Result<(), String> {
        let root = "/workspace";
        let finding = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 3, None);
        let snapshot = make_snapshot_with_findings(root, vec![finding]);
        let uri = parse_uri("file:///workspace/src/lib.rs")?;

        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());

        if lenses.len() != 1 {
            return Err(format!("expected exactly 1 lens, got {}", lenses.len()));
        }
        let title = lenses[0]
            .command
            .as_ref()
            .ok_or("expected command (title) in lens")?
            .title
            .clone();
        if !title.contains("3") {
            return Err(format!("title should cite 3 related tests, got: {title}"));
        }
        if !title.contains("exposed") {
            return Err(format!("title should cite class 'exposed', got: {title}"));
        }
        // Line should be 0-indexed (10 - 1 = 9)
        let line = lenses[0].range.start.line;
        if line != 9 {
            return Err(format!("expected lens at line 9 (0-indexed), got {line}"));
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_zero_related_tests_is_honest() -> Result<(), String> {
        let root = "/workspace";
        let finding = make_finding("src/lib.rs", 5, ExposureClass::NoStaticPath, 0, None);
        let snapshot = make_snapshot_with_findings(root, vec![finding]);
        let uri = parse_uri("file:///workspace/src/lib.rs")?;

        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());

        if lenses.len() != 1 {
            return Err(format!("expected 1 lens, got {}", lenses.len()));
        }
        let title = lenses[0]
            .command
            .as_ref()
            .ok_or("expected command in lens")?
            .title
            .clone();
        if !title.contains("no test reaches this change") {
            return Err(format!(
                "title must say no test reaches this change, got: {title}"
            ));
        }
        if !title.contains("no path") {
            return Err(format!(
                "title must include plain class label 'no path', got: {title}"
            ));
        }
        // Test guard: confirm the lens title uses none of the forbidden
        // static-language vocabulary (delegates to the shared helper, which
        // holds the flagged terms on stable, fmt-safe array lines).
        if !lens_title_is_static_language_clean(&title) {
            return Err(format!(
                "title must not use forbidden static-language vocabulary, got: {title}"
            ));
        }
        Ok(())
    }

    #[test]
    fn related_test_lens_title_no_static_path_never_claims_related_test_grip() -> Result<(), String>
    {
        for count in [0_usize, 1, 3] {
            let finding = make_finding("src/lib.rs", 5, ExposureClass::NoStaticPath, count, None);
            let title = related_test_lens_title(&finding, None);
            if !title.contains("no test reaches this change") {
                return Err(format!(
                    "NoStaticPath count={count} must not imply reach, got: {title}"
                ));
            }
            if title.contains("related test") {
                return Err(format!(
                    "NoStaticPath count={count} must not cite related-test count, got: {title}"
                ));
            }
            if !title.contains("no path") {
                return Err(format!(
                    "NoStaticPath must include plain class label, got: {title}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn related_test_lens_title_pluralizes_related_test_count() -> Result<(), String> {
        let one = related_test_lens_title(
            &make_finding("src/lib.rs", 1, ExposureClass::Exposed, 1, None),
            None,
        );
        if !one.contains("1 related test (exposed)") || one.contains("1 related tests") {
            return Err(format!("expected singular related test label, got: {one}"));
        }
        let many = related_test_lens_title(
            &make_finding("src/lib.rs", 1, ExposureClass::Exposed, 2, None),
            None,
        );
        if !many.contains("2 related tests (exposed)") {
            return Err(format!("expected plural related tests label, got: {many}"));
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_preview_finding_softens() -> Result<(), String> {
        let root = "/workspace";
        let finding = make_finding(
            "src/service.ts",
            20,
            ExposureClass::Exposed,
            1,
            Some(LanguageStatus::Preview),
        );
        let snapshot = make_snapshot_with_findings(root, vec![finding]);
        let uri = parse_uri("file:///workspace/src/service.ts")?;

        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());

        if lenses.len() != 1 {
            return Err(format!("expected 1 lens, got {}", lenses.len()));
        }
        let title = lenses[0]
            .command
            .as_ref()
            .ok_or("expected command in lens")?
            .title
            .clone();
        if !title.contains("preview") {
            return Err(format!(
                "preview finding must have 'preview' in title, got: {title}"
            ));
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_uncertain_class_does_not_claim_grip() -> Result<(), String> {
        let root = "/workspace";
        let finding = make_finding("src/lib.rs", 8, ExposureClass::ReachableUnrevealed, 2, None);
        let snapshot = make_snapshot_with_findings(root, vec![finding]);
        let uri = parse_uri("file:///workspace/src/lib.rs")?;

        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());

        if lenses.len() != 1 {
            return Err(format!("expected 1 lens, got {}", lenses.len()));
        }
        let title = lenses[0]
            .command
            .as_ref()
            .ok_or("expected command in lens")?
            .title
            .clone();
        if !title.contains("no static check of this change") {
            return Err(format!(
                "uncertain class must state 'no static check of this change', got: {title}"
            ));
        }
        if !title.contains("unrevealed") {
            return Err(format!(
                "title must include plain class label 'unrevealed', got: {title}"
            ));
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_no_snapshot_emits_nothing() -> Result<(), String> {
        let uri = parse_uri("file:///workspace/src/lib.rs")?;
        let lenses = code_lens_response(&uri, None, &ClientFeatureProfile::unsupported());
        if !lenses.is_empty() {
            return Err(
                "no snapshot must produce empty Vec, not a fabricated 0-count lens".to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_filters_to_requested_file() -> Result<(), String> {
        let root = "/workspace";
        let finding_a = make_finding("src/a.rs", 10, ExposureClass::Exposed, 2, None);
        let finding_b = make_finding("src/b.rs", 20, ExposureClass::NoStaticPath, 0, None);
        let snapshot = make_snapshot_with_findings(root, vec![finding_a, finding_b]);

        let uri_a = parse_uri("file:///workspace/src/a.rs")?;
        let lenses = code_lens_response(
            &uri_a,
            Some(&snapshot),
            &ClientFeatureProfile::unsupported(),
        );

        if lenses.len() != 1 {
            return Err(format!(
                "expected 1 lens for file a (only its finding), got {}",
                lenses.len()
            ));
        }
        let title = lenses[0]
            .command
            .as_ref()
            .ok_or("expected command in lens")?
            .title
            .clone();
        if !title.contains("2") {
            return Err(format!(
                "lens for a.rs must cite 2 related tests, got: {title}"
            ));
        }
        Ok(())
    }

    fn navigating_client(
        commands: &[&str],
        show_document: bool,
    ) -> Result<ClientFeatureProfile, String> {
        let params: tower_lsp_server::ls_types::InitializeParams =
            serde_json::from_value(serde_json::json!({
                "capabilities": {
                    "window": { "showDocument": { "support": show_document } },
                    "experimental": {
                        "riprEditor": {
                            "version": "0.10.0",
                            "commands": commands,
                            "guardedTestEdit": false
                        }
                    }
                }
            }))
            .map_err(|err| format!("fixture params must parse: {err}"))?;
        Ok(ClientFeatureProfile::from_initialize_params(&params))
    }

    fn only_lens(
        snapshot: &AnalysisSnapshot,
        file: &str,
        client: &ClientFeatureProfile,
    ) -> Result<Command, String> {
        let uri = parse_uri(&format!("file:///workspace/{file}"))?;
        let mut lenses = code_lens_response(&uri, Some(snapshot), client);
        if lenses.len() != 1 {
            return Err(format!("expected exactly one lens, got {lenses:?}"));
        }
        lenses
            .remove(0)
            .command
            .ok_or_else(|| "lens carries no command".to_string())
    }

    #[test]
    fn lens_click_opens_the_related_test_when_the_row_has_a_target() -> Result<(), String> {
        let snapshot = make_snapshot_with_findings(
            "/workspace",
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::Exposed,
                2,
                None,
            )],
        );
        let client = navigating_client(&[OPEN_RELATED_TEST_COMMAND], false)?;
        let command = only_lens(&snapshot, "src/lib.rs", &client)?;
        if command.command != OPEN_RELATED_TEST_COMMAND {
            return Err(format!("expected navigation, got {}", command.command));
        }
        let target = command
            .arguments
            .as_ref()
            .and_then(|args| args.first())
            .ok_or("navigation lens must carry its target")?;
        if target.get("uri").and_then(|v| v.as_str()) != Some("file:///workspace/tests/lib.rs")
            || target.get("line").and_then(|v| v.as_u64()) != Some(10)
            || target.get("test_name").and_then(|v| v.as_str()) != Some("test_0")
        {
            return Err(format!("unexpected navigation target: {target}"));
        }
        if !command.title.ends_with("· open test_0") || command.title.contains("refresh") {
            return Err(format!(
                "title must name the opened test: {}",
                command.title
            ));
        }
        if !lens_title_is_static_language_clean(&command.title) {
            return Err(format!("forbidden vocabulary: {}", command.title));
        }
        // Navigation only: no repair or actionability data on the wire.
        if target.get("input_identity").is_some() || target.get("repair").is_some() {
            return Err(format!("client-side target must stay minimal: {target}"));
        }
        Ok(())
    }

    #[test]
    fn lens_click_uses_server_navigation_for_show_document_clients() -> Result<(), String> {
        let mut snapshot = make_snapshot_with_findings(
            "/workspace",
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::WeaklyExposed,
                1,
                None,
            )],
        );
        let config = crate::lsp::config::LspAnalysisConfig::default();
        snapshot.input_identity = Some(
            crate::lsp::input_identity::LspAnalysisInputIdentity::from_refresh_inputs(
                PathBuf::from("/workspace"),
                7,
                &config,
            ),
        );
        let client = navigating_client(&[], true)?;
        let command = only_lens(&snapshot, "src/lib.rs", &client)?;
        if command.command != SHOW_RELATED_TEST_COMMAND {
            return Err(format!(
                "expected server navigation, got {}",
                command.command
            ));
        }
        let bound = command
            .arguments
            .as_ref()
            .and_then(|args| args.first())
            .and_then(|target| target.get("input_identity"))
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if bound != snapshot.input_identity_id() || bound.is_none() {
            return Err("server navigation must bind the snapshot input identity".to_string());
        }
        // Without an identity the server route cannot bind: refresh, not a stale target.
        let unbound = make_snapshot_with_findings(
            "/workspace",
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::WeaklyExposed,
                1,
                None,
            )],
        );
        let fallback = only_lens(&unbound, "src/lib.rs", &client)?;
        if fallback.command != REFRESH_COMMAND || fallback.arguments.is_some() {
            return Err(format!("unbound snapshot must refresh: {fallback:?}"));
        }
        Ok(())
    }

    #[test]
    fn lens_click_falls_back_to_refresh_without_an_honest_target() -> Result<(), String> {
        let navigating = navigating_client(&[OPEN_RELATED_TEST_COMMAND], true)?;
        let cases = [
            // No related tests.
            make_finding(
                "src/lib.rs",
                10,
                ExposureClass::ReachableUnrevealed,
                0,
                None,
            ),
            // No static path never navigates, even if related tests are listed.
            make_finding("src/lib.rs", 10, ExposureClass::NoStaticPath, 2, None),
        ];
        for finding in cases {
            let label = format!(
                "{:?} with {} tests",
                finding.class,
                finding.related_tests.len()
            );
            let snapshot = make_snapshot_with_findings("/workspace", vec![finding]);
            let command = only_lens(&snapshot, "src/lib.rs", &navigating)?;
            if command.command != REFRESH_COMMAND || command.arguments.is_some() {
                return Err(format!("{label} must refresh, got {command:?}"));
            }
            if !command.title.ends_with("· refresh saved workspace") {
                return Err(format!("{label} lost the refresh title: {}", command.title));
            }
        }
        // A related test outside the workspace root is never a target.
        let mut outside = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 1, None);
        outside.related_tests[0].file = PathBuf::from("/elsewhere/tests/lib.rs");
        let snapshot = make_snapshot_with_findings("/workspace", vec![outside]);
        let command = only_lens(&snapshot, "src/lib.rs", &navigating)?;
        if command.command != REFRESH_COMMAND {
            return Err(format!("outside-root test must refresh: {command:?}"));
        }
        // A generic client that advertised neither route keeps refresh.
        let generic = ClientFeatureProfile::unsupported();
        let snapshot = make_snapshot_with_findings(
            "/workspace",
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::Exposed,
                2,
                None,
            )],
        );
        let command = only_lens(&snapshot, "src/lib.rs", &generic)?;
        if command.command != REFRESH_COMMAND || command.arguments.is_some() {
            return Err(format!("generic client must refresh: {command:?}"));
        }
        // Parse-failed file: related tests are unknown, so nothing to open.
        let parse_failed = snapshot_with_parse_failure_limitation(
            "/workspace",
            vec![make_finding(
                "src/lib.rs",
                10,
                ExposureClass::Exposed,
                2,
                None,
            )],
            language_adapter_producer_failure_limitation("src/lib.rs")?,
        )?;
        let command = only_lens(&parse_failed, "src/lib.rs", &navigating)?;
        if command.command != REFRESH_COMMAND {
            return Err(format!("parse-failed file must refresh: {command:?}"));
        }
        Ok(())
    }

    #[test]
    fn lens_view_identity_tracks_the_navigation_target() -> Result<(), String> {
        let first = make_finding("src/lib.rs", 10, ExposureClass::Exposed, 2, None);
        let mut second = first.clone();
        second.related_tests[0].name = "other_test".to_string();
        let a = lens_view_identity(&make_snapshot_with_findings("/workspace", vec![first]));
        let b = lens_view_identity(&make_snapshot_with_findings("/workspace", vec![second]));
        if a == b {
            return Err("a changed navigation target must change the lens view".to_string());
        }
        Ok(())
    }

    #[test]
    fn code_lens_response_static_language_clean() -> Result<(), String> {
        let root = "/workspace";
        let classes: &[(ExposureClass, usize)] = &[
            (ExposureClass::Exposed, 3),
            (ExposureClass::WeaklyExposed, 1),
            (ExposureClass::ReachableUnrevealed, 2),
            (ExposureClass::NoStaticPath, 0),
            (ExposureClass::InfectionUnknown, 1),
            (ExposureClass::PropagationUnknown, 0),
            (ExposureClass::StaticUnknown, 0),
        ];
        for (i, (class, count)) in classes.iter().enumerate() {
            let finding = make_finding(&format!("src/file{i}.rs"), 10, class.clone(), *count, None);
            let snapshot = make_snapshot_with_findings(root, vec![finding]);
            let uri = parse_uri(&format!("file:///workspace/src/file{i}.rs"))?;
            let lenses =
                code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());
            for lens in &lenses {
                if let Some(cmd) = &lens.command
                    && !lens_title_is_static_language_clean(&cmd.title)
                {
                    return Err(format!(
                        "forbidden vocab in lens title for class {:?}: {}",
                        class.as_str(),
                        cmd.title
                    ));
                }
            }
        }
        // Also check preview tier.
        let preview_finding = make_finding(
            "src/preview.ts",
            5,
            ExposureClass::Exposed,
            1,
            Some(LanguageStatus::Preview),
        );
        let snapshot = make_snapshot_with_findings(root, vec![preview_finding]);
        let uri = parse_uri("file:///workspace/src/preview.ts")?;
        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());
        for lens in &lenses {
            if let Some(cmd) = &lens.command
                && !lens_title_is_static_language_clean(&cmd.title)
            {
                return Err(format!(
                    "forbidden vocab in preview lens title: {}",
                    cmd.title
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn parse_failed_file_lens_title_is_honest_not_zero_related_tests() -> Result<(), String> {
        let finding = make_finding("src/lib.rs", 10, ExposureClass::WeaklyExposed, 0, None);
        let limitation = language_adapter_producer_failure_limitation("src/lib.rs")?;
        let snapshot =
            snapshot_with_parse_failure_limitation("/workspace", vec![finding], limitation)?;
        let uri = parse_uri("file:///workspace/src/lib.rs")?;
        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());
        let title = lenses
            .first()
            .and_then(|lens| lens.command.as_ref())
            .map(|command| command.title.clone())
            .ok_or("expected one parse-failed lens")?;
        if title.contains("no related tests found") {
            return Err(format!(
                "parse-failed file must not claim no related tests found: {title}"
            ));
        }
        if !title.contains("did not parse") {
            return Err(format!(
                "parse-failed file lens must cite parse failure: {title}"
            ));
        }
        Ok(())
    }

    #[test]
    fn parse_failed_limitation_on_other_file_leaves_lens_unchanged() -> Result<(), String> {
        let finding = make_finding("src/lib.rs", 10, ExposureClass::WeaklyExposed, 0, None);
        let limitation = language_adapter_producer_failure_limitation("src/other.rs")?;
        let snapshot =
            snapshot_with_parse_failure_limitation("/workspace", vec![finding], limitation)?;
        let uri = parse_uri("file:///workspace/src/lib.rs")?;
        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());
        let title = lenses
            .first()
            .and_then(|lens| lens.command.as_ref())
            .map(|command| command.title.clone())
            .ok_or("expected one lens")?;
        if !title.contains("no related tests found") {
            return Err(format!(
                "limitation on another file must not change src/lib.rs lens: {title}"
            ));
        }
        Ok(())
    }

    #[test]
    fn parse_failed_lens_unchanged_for_non_matching_limitation() -> Result<(), String> {
        let finding = make_finding("src/lib.rs", 10, ExposureClass::WeaklyExposed, 0, None);
        let recovery = AnalysisRecovery::new(
            AnalysisRecoveryKind::InspectFailure,
            "Inspect the typed limitation before retrying.",
        )?;
        let wrong_stage = AnalysisLimitation::new(
            AnalysisLimitationKind::ProducerFailure,
            AnalysisStage::DiffParse,
            recovery.clone(),
        )
        .with_path("src/lib.rs")?;
        let snapshot_wrong_stage = snapshot_with_parse_failure_limitation(
            "/workspace",
            vec![finding.clone()],
            wrong_stage,
        )?;
        let wrong_kind = AnalysisLimitation::new(
            AnalysisLimitationKind::ProducerTimeout,
            AnalysisStage::LanguageAdapter,
            recovery,
        )
        .with_path("src/lib.rs")?;
        let snapshot_wrong_kind =
            snapshot_with_parse_failure_limitation("/workspace", vec![finding], wrong_kind)?;
        let uri = parse_uri("file:///workspace/src/lib.rs")?;
        for snapshot in [snapshot_wrong_stage, snapshot_wrong_kind] {
            let lenses =
                code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());
            let title = lenses
                .first()
                .and_then(|lens| lens.command.as_ref())
                .map(|command| command.title.clone())
                .ok_or("expected one lens")?;
            if !title.contains("no related tests found") {
                return Err(format!(
                    "non-matching limitation must keep zero-count lens: {title}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn per_file_producer_failure_that_is_not_a_parse_failure_keeps_the_lens() -> Result<(), String>
    {
        // `classification_incomplete`: a per-file language-adapter failure that
        // is an internal error, not a parse failure (#11799).
        let finding = make_finding("src/lib.rs", 10, ExposureClass::WeaklyExposed, 0, None);
        let recovery = AnalysisRecovery::new(
            AnalysisRecoveryKind::InspectFailure,
            "Report the failure with the file and diff, then re-run.",
        )?;
        let limitation = AnalysisLimitation::new(
            AnalysisLimitationKind::ProducerFailure,
            AnalysisStage::LanguageAdapter,
            recovery,
        )
        .with_path("src/lib.rs")?;
        let snapshot =
            snapshot_with_parse_failure_limitation("/workspace", vec![finding], limitation)?;
        let uri = parse_uri("file:///workspace/src/lib.rs")?;
        let title = code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported())
            .first()
            .and_then(|lens| lens.command.as_ref())
            .map(|command| command.title.clone())
            .ok_or("expected one lens")?;
        if title.contains("did not parse") {
            return Err(format!(
                "not a parse failure, lens must not say so: {title}"
            ));
        }
        Ok(())
    }

    #[test]
    fn parse_failed_lens_title_is_static_language_clean() -> Result<(), String> {
        if !lens_title_is_static_language_clean(PARSE_FAILED_LENS_TITLE) {
            return Err(format!(
                "parse-failed lens title must pass static-language guard: {PARSE_FAILED_LENS_TITLE}"
            ));
        }
        Ok(())
    }

    #[test]
    fn base_deleted_findings_get_no_code_lens() -> Result<(), String> {
        // #3282: a lens anchors an advisory at the finding's recorded line.
        // A base-deleted finding's recorded coordinate is the projected
        // new-side line (the #3212 incident shape — base line 29 against
        // a 13-line candidate), so pinning a lens there presents
        // deleted-side evidence at an impossible candidate position with
        // no revision marker. Candidate-actionable eligibility gates the
        // lens exactly as it gates diagnostics and annotations
        // (RIPR-SPEC-0152).
        let uri = parse_uri("file:///ws/src/lib.rs")?;
        let mut deleted = make_finding("src/lib.rs", 29, ExposureClass::NoStaticPath, 1, None);
        deleted.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        let current = make_finding("src/lib.rs", 3, ExposureClass::NoStaticPath, 1, None);
        let snapshot = make_snapshot_with_findings("/ws", vec![deleted, current]);
        let lenses =
            code_lens_response(&uri, Some(&snapshot), &ClientFeatureProfile::unsupported());
        let lines: Vec<u32> = lenses.iter().map(|lens| lens.range.start.line).collect();
        assert!(
            !lines.contains(&(29 - 1)),
            "base_deleted finding must not receive a lens at its projected coordinate: {lines:?}"
        );
        assert!(
            lines.contains(&(3 - 1)),
            "the candidate-current twin keeps its lens: {lines:?}"
        );
        Ok(())
    }
}
