//! RIPR-SPEC-0240: unknown, not a gap.
//!
//! A `reachable_unrevealed` finding says the tests that reach a change do
//! not check it. ripr may make that claim only when the failing step was
//! established from evidence it could read. When every related `assert_eq!`
//! was refused because of a limit in ripr's own reading (an unparsed or
//! unplaced file, an unidentified test, a feature `cfg`, a binding that only
//! may rebind the macro), the finding becomes `static_unknown` with a named
//! `static_limit_kind`, so no renderer, gate, or repair route treats an
//! analyzer blind spot as a missing test.
//!
//! A refusal for a shape that can keep the assertion from running (a branch,
//! a closure, an opaque macro, a gated module, a real rebinding) keeps the
//! gap: runtime controls in `tests/owner_pin_execution.rs` prove those gaps.
//!
//! This is the single owner of that decision for Rust findings. It runs after
//! every other limit post-pass and never upgrades a class: it only withholds a
//! gap, which keeps the actionability flip fail-closed.

use super::decision::confidence_score;
use super::reveal::ASSERTION_CONTEXT_UNESTABLISHED;
use crate::analysis::facts::{FunctionContainer, TestFact};
use crate::analysis::rust_index::RustIndex;
use crate::domain::*;

/// Classifier-to-post-pass marker: every refused related `assert_eq!` was
/// refused for an analyzer limit. The post-pass removes it from the finding.
pub(in crate::analysis) const REFUSALS_ARE_ANALYZER_LIMITS: &str =
    "gap_admission: every refused related assertion rests on an analyzer limit";

const LIMIT: StaticLimitKind = StaticLimitKind::RustAssertionContextUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether the refused related assertions run as the standard `assert_eq!`";
const JUDGED_TEST_LIMIT: usize = 3;

/// Withhold a Rust gap whose evidence rests on an analyzer limit. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_unsupported_gap(finding: &mut Finding) -> bool {
    let before = finding.evidence.len();
    finding
        .evidence
        .retain(|line| line != REFUSALS_ARE_ANALYZER_LIMITS);
    let marked = finding.evidence.len() != before;
    if !(marked && gap_rests_on_refusal(finding)) {
        return false;
    }
    let judged = judged_tests(finding);
    withhold_gap(
        finding,
        LIMIT,
        FIRST_UNRESOLVED_EDGE,
        next_step(&judged),
        &judged,
    );
    true
}

/// Rewrite a gap finding to `static_unknown` under the named `limit`:
/// rescore, record the stop reason and the limitation edges, and drop the
/// absence claims that would feed repair routes.
fn withhold_gap(
    finding: &mut Finding,
    limit: StaticLimitKind,
    first_unresolved_edge: &str,
    next_step: String,
    judged: &[String],
) {
    let withheld_class = finding.class.as_str();
    finding.class = ExposureClass::StaticUnknown;
    // The classifier scored this finding as a gap, and a gap class earns a
    // bonus for a confident negative claim. Rescore for the class it now
    // has; `min` keeps any lower score an earlier limit pass assigned.
    let ripr = &finding.ripr;
    finding.confidence = finding.confidence.min(confidence_score(
        &ripr.reach,
        &ripr.infect,
        &ripr.propagate,
        &ripr.reveal.observe,
        &ripr.reveal.discriminate,
        &finding.class,
    ));
    finding.static_limit_kind = Some(limit);
    if !finding
        .stop_reasons
        .contains(&StopReason::GapEvidenceUnresolved)
    {
        finding.stop_reasons.push(StopReason::GapEvidenceUnresolved);
    }
    finding.evidence.push(format!(
        "gap_withheld: {withheld_class} is not claimed; {first_unresolved_edge} ({})",
        limit.as_str()
    ));
    finding.evidence.extend([
        format!(
            "{LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX}{}",
            last_established_edge(finding, judged)
        ),
        format!("{LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX}{first_unresolved_edge}"),
        format!("{LIMITATION_ANALYZER_ROUTE_PREFIX}analysis/classify/gap-admission"),
        format!(
            "{LIMITATION_NON_CLAIM_PREFIX}no missing test, no weak oracle, and no test adequacy is established"
        ),
    ]);
    // The absence lines ("no assertion observes ...") and the missing
    // discriminators are the unestablished claim itself. Keeping them would
    // feed repair placement and agent packets a gap this policy withheld.
    finding.missing = vec![limit.describe().to_string()];
    finding.activation.missing_discriminators.clear();
    finding.recommended_next_step = Some(next_step);
}

const LOG_LIMIT: StaticLimitKind = StaticLimitKind::RustLogCaptureUnresolved;
const LOG_FIRST_UNRESOLVED_EDGE: &str =
    "whether the captured log record carries the changed value the test asserts";
const LOG_MACROS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

/// #7133: withhold the gap on a changed `log::` record when a
/// related test installs a capturing logger before it calls the owner and
/// asserts exactly on a value read from that logger's store. ripr reads
/// neither the logger nor the format template, so the comparison may well
/// discriminate; it is a named limit, never credit. Only a free-function
/// owner (`owner_name`) counts: a bare `owner(` call cannot name a method.
/// Returns `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_log_capture_gap(
    finding: &mut Finding,
    index: &RustIndex,
    owner_name: Option<&str>,
) -> bool {
    let Some(owner) = owner_name else {
        return false;
    };
    if finding.static_limit_kind.is_some()
        || finding.class != ExposureClass::WeaklyExposed
        || !matches!(
            finding.probe.family,
            ProbeFamily::SideEffect | ProbeFamily::CallDeletion
        )
        || !is_qualified_log_macro(&finding.probe.expression)
    {
        return false;
    }
    let capturing: Vec<String> = finding
        .related_tests
        .iter()
        .filter(|related| {
            index.tests().iter().any(|test| {
                test.name == related.name
                    && test.file == related.file
                    && test_asserts_captured_log(test, owner, index)
            })
        })
        .map(|related| {
            format!(
                "`{}` ({}:{})",
                related.name,
                related.file.display().to_string().replace('\\', "/"),
                related.line
            )
        })
        .collect();
    if capturing.is_empty() {
        return false;
    }
    let next_step = format!(
        "{} installs a capturing logger and asserts on the captured records, which ripr does not resolve, so it does not report a gap. Check that the assertion compares the changed log record; add a test only if none does.",
        capturing.join(", ")
    );
    withhold_gap(
        finding,
        LOG_LIMIT,
        LOG_FIRST_UNRESOLVED_EDGE,
        next_step,
        &capturing,
    );
    true
}

/// `log::info!(..)` and the other `log` level macros, by their qualified
/// path only: a bare `info!` may be any macro. `tracing::` events reach a
/// `log` logger only through tracing's `log` feature, which ripr does not
/// read, so they keep their gap.
fn is_qualified_log_macro(expression: &str) -> bool {
    let trimmed = expression.trim();
    trimmed.strip_prefix("log::").is_some_and(|rest| {
        LOG_MACROS.iter().any(|level| {
            rest.strip_prefix(level)
                .is_some_and(|after| after.trim_start().starts_with('!'))
        })
    })
}

/// Whether `test` installs a capturing logger, then calls `owner`, then
/// asserts with an exact oracle on a captured value. The install is
/// `log::set_logger(&STORE)` / `log::set_boxed_logger(STORE)` with a plain
/// binding as argument, or `let store = helper();` where `helper` is in the
/// same file, installs a logger and returns a type with a same-file
/// `impl log::Log`. The store also extends one
/// hop to a binding read from it after the owner call
/// (`let lines = logger.0.lock()..;`). Every owner call is tried, so a
/// re-call after a late capture still counts. The assertion must carry a
/// string literal and no count or status projection: `lines.len()` or
/// `installed.is_ok()` does not compare the record's text.
fn test_asserts_captured_log(test: &TestFact, owner: &str, index: &RustIndex) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(&test.body);
    let same_file = || {
        index
            .functions()
            .iter()
            .filter(|function| function.file == test.file)
    };
    let logger_types: Vec<&str> = same_file()
        .filter_map(|function| match &function.item.container {
            FunctionContainer::TraitImpl {
                trait_path,
                self_ty,
            } if matches!(trait_path.as_str(), "log::Log" | "Log") => Some(self_ty.as_str()),
            _ => None,
        })
        .collect();
    let helpers: Vec<&str> = same_file()
        .filter(|function| returns_installed_logger(function.body.as_str(), &logger_types))
        .map(|function| function.name.as_str())
        .collect();
    free_call_offsets(&masked, owner)
        .into_iter()
        .any(|call_at| captured_and_asserted_around(test, &masked, call_at, &helpers))
}

fn captured_and_asserted_around(
    test: &TestFact,
    masked: &str,
    call_at: usize,
    helpers: &[&str],
) -> bool {
    let call_line = test.start_line + masked[..call_at].matches('\n').count();
    let before = &masked[..call_at];
    let mut stores: Vec<String> = install_arguments(before);
    for helper in helpers {
        for at in free_call_offsets(before, helper) {
            stores.extend(let_binding_at(masked, at));
        }
    }
    if stores.is_empty() {
        return false;
    }
    // One hop: a binding read from the store after the owner call.
    let mut read_from_store = Vec::new();
    for statement in masked[call_at..].split(';') {
        let Some(binding) = let_binding_at(statement, statement.len()) else {
            continue;
        };
        let initializer = statement.split_once('=').map_or("", |(_, rest)| rest);
        if stores.iter().any(|store| names(initializer, store)) {
            read_from_store.push(binding);
        }
    }
    stores.extend(read_from_store);
    test.assertions.iter().any(|assertion| {
        let masked_assertion = crate::analysis::extract::mask_comments_and_strings(&assertion.text);
        assertion.line > call_line
            && matches!(
                assertion.kind,
                OracleKind::ExactValue | OracleKind::WholeObjectEquality
            )
            && assertion.text.contains('"')
            && !COUNT_OR_STATUS_PROJECTIONS
                .iter()
                .any(|projection| masked_assertion.contains(projection))
            && stores.iter().any(|store| names(&masked_assertion, store))
    })
}

/// Projections that read a count or a status, not the captured text.
const COUNT_OR_STATUS_PROJECTIONS: [&str; 6] = [
    ".len()",
    ".is_empty()",
    ".count()",
    ".is_ok()",
    ".is_err()",
    ".is_some()",
];

/// Plain-binding arguments of `log::set_logger(..)` /
/// `log::set_boxed_logger(..)` calls in masked `code` (`&LOGGER` names
/// `LOGGER`). The call's own result is never a store.
fn install_arguments(code: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    for install in ["log::set_logger(", "log::set_boxed_logger("] {
        for (at, call) in code.match_indices(install) {
            let argument = code[at + call.len()..]
                .split([')', ','])
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches('&')
                .trim();
            if is_ident(argument) {
                arguments.push(argument.to_string());
            }
        }
    }
    arguments
}

/// Whether a helper's body installs a logger through `log::` and its
/// return type names one of `logger_types` (a same-file type with an
/// `impl log::Log`), so a `let` of its result is the capture store. A helper
/// that installs a logger but returns some other value is not a store.
fn returns_installed_logger(body: &str, logger_types: &[&str]) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let installs = ["log::set_logger(", "log::set_boxed_logger("]
        .iter()
        .any(|install| masked.contains(install));
    let signature = masked.split('{').next().unwrap_or_default();
    let Some((_, return_type)) = signature.split_once("->") else {
        return false;
    };
    installs
        && logger_types.iter().any(|logger| {
            let name = logger.rsplit("::").next().unwrap_or(logger);
            names(return_type, name)
        })
}

/// Offsets of `name(` calls in masked `code` that are not method calls,
/// path calls (`other::name(`) or part of a longer identifier.
fn free_call_offsets(code: &str, name: &str) -> Vec<usize> {
    let call = format!("{name}(");
    code.match_indices(&call)
        .filter(|(at, _)| {
            !code[..*at].ends_with("::")
                && !code[..*at].chars().next_back().is_some_and(|before| {
                    before.is_alphanumeric() || before == '_' || before == '.'
                })
        })
        .map(|(at, _)| at)
        .collect()
}

/// The binding of the `let [mut] binding = ..` statement that contains
/// offset `at` of masked `code`.
fn let_binding_at(code: &str, at: usize) -> Option<String> {
    let before = code.get(..at)?;
    let start = before.rfind([';', '{', '}']).map_or(0, |index| index + 1);
    let statement = before[start..].trim_start();
    let rest = statement.strip_prefix("let ")?.trim_start();
    let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
    let end = rest
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len());
    let binding = &rest[..end];
    (is_ident(binding) && rest[end..].trim_start().starts_with(['=', ':']))
        .then(|| binding.to_string())
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
        && text != "_"
}

fn names(text: &str, ident: &str) -> bool {
    text.match_indices(ident).any(|(at, name)| {
        let boundary = |ch: char| !(ch.is_alphanumeric() || ch == '_');
        text[..at].chars().next_back().is_none_or(boundary)
            && text[at + name.len()..].chars().next().is_none_or(boundary)
    })
}

fn gap_rests_on_refusal(finding: &Finding) -> bool {
    // A producer that already named a limit owns the finding's explanation.
    finding.static_limit_kind.is_none()
        && finding.class == ExposureClass::ReachableUnrevealed
        && finding.ripr.reveal.observe.state == StageState::No
        && finding.ripr.reveal.observe.summary == ASSERTION_CONTEXT_UNESTABLISHED
}

/// Refusal scope is computed per finding, so every related test is a
/// candidate source of the refused assertions. The rows are already ranked
/// strongest relation first.
fn judged_tests(finding: &Finding) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for test in &finding.related_tests {
        let label = format!(
            "`{}` ({}:{})",
            test.name,
            test.file.display().to_string().replace('\\', "/"),
            test.line
        );
        if !names.contains(&label) {
            names.push(label);
        }
        if names.len() == JUDGED_TEST_LIMIT {
            break;
        }
    }
    names
}

fn last_established_edge(finding: &Finding, judged: &[String]) -> String {
    if judged.is_empty() {
        format!(
            "reach {}: {}",
            finding.ripr.reach.state.as_str(),
            finding.ripr.reach.summary
        )
    } else {
        format!("related tests {}", judged.join(", "))
    }
}

fn next_step(judged: &[String]) -> String {
    let tests = if judged.is_empty() {
        "the tests that call this code".to_string()
    } else {
        judged.join(", ")
    };
    format!(
        "ripr could not read whether the assertions in {tests} run as the standard `assert_eq!`, so it does not report a gap. Check that one of them compares the changed value; add a test only if none does."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    fn related(name: &str) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            line: 40,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::None,
            relation_reason: Some(RelationReason::DirectOwnerCall),
            relation_confidence: Some(RelationConfidence::High),
            miss: None,
        }
    }

    /// The serde `format_u8` shape: a feature-gated test whose loop
    /// `assert_eq!` was refused, marked by the classifier.
    fn marked() -> Finding {
        let yes = || stage(StageState::Yes, "yes");
        let mut activation = ActivationEvidence::default();
        activation
            .missing_discriminators
            .push(MissingDiscriminatorFact {
                value: "n == 100".to_string(),
                reason: "predicate boundary equality is not asserted".to_string(),
                flow_sink: None,
            });
        Finding {
            id: "probe:gap-admission".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:gap-admission".to_string()),
                location: SourceLocation::new("src/lib.rs", 10, 1),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "n > 99".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::ReachableUnrevealed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, "Related tests appear to reach owner"),
                infect: yes(),
                propagate: yes(),
                reveal: RevealEvidence {
                    observe: stage(StageState::No, ASSERTION_CONTEXT_UNESTABLISHED),
                    discriminate: stage(StageState::No, "unestablished"),
                },
            },
            confidence: 0.79,
            evidence: vec![
                "assertion not credited: `assert_eq!` in test_format_u8 at src/lib.rs:41: the test carries `#[cfg(feature = \"std\")]`".to_string(),
                REFUSALS_ARE_ANALYZER_LIMITS.to_string(),
            ],
            missing: vec!["No relevant oracle was detected".to_string()],
            flow_sinks: Vec::new(),
            activation,
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: vec![related("test_format_u8")],
            recommended_next_step: None,
            language: Some(LanguageId::Rust),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    #[test]
    fn analyzer_limit_refusals_withhold_the_gap_and_name_the_test() {
        let mut finding = marked();
        assert!(withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(finding.static_limit_kind, Some(LIMIT));
        assert_eq!(
            finding.stop_reasons,
            vec![StopReason::GapEvidenceUnresolved]
        );
        assert_eq!(finding.missing, vec![LIMIT.describe().to_string()]);
        assert!(finding.activation.missing_discriminators.is_empty());
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line == REFUSALS_ARE_ANALYZER_LIMITS),
            "the marker is consumed"
        );
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("gap_withheld: reachable_unrevealed")),
            "{:?}",
            finding.evidence
        );
        assert_eq!(
            finding.recommended_next_step.as_deref(),
            Some(
                "ripr could not read whether the assertions in `test_format_u8` (src/lib.rs:40) run as the standard `assert_eq!`, so it does not report a gap. Check that one of them compares the changed value; add a test only if none does."
            )
        );
    }

    #[test]
    fn withholding_drops_the_gap_confidence_bonus_and_never_raises_a_score() {
        // reach/infect/propagate yes (0.2 each), observe/discriminate no
        // (0.02 each) score 0.64; reachable_unrevealed adds 0.15.
        let mut scored_as_gap = marked();
        assert!(withhold_unsupported_gap(&mut scored_as_gap));
        assert!((scored_as_gap.confidence - 0.64).abs() < 1e-6);

        let mut already_lower = marked();
        already_lower.confidence = 0.5;
        assert!(withhold_unsupported_gap(&mut already_lower));
        assert!((already_lower.confidence - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_unmarked_refusal_keeps_the_gap() {
        // A local-shape refusal (`if` branch, uncalled closure) leaves no
        // marker: the runtime controls prove those gaps.
        let mut finding = marked();
        finding
            .evidence
            .retain(|line| line != REFUSALS_ARE_ANALYZER_LIMITS);
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        assert_eq!(finding.static_limit_kind, None);
    }

    #[test]
    fn a_marker_without_the_refused_observe_keeps_the_gap_and_is_consumed() {
        let mut finding = marked();
        finding.ripr.reveal.observe = stage(
            StageState::No,
            "Related tests were found, but no assertion appears to observe the changed value, error, field, or effect",
        );
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line == REFUSALS_ARE_ANALYZER_LIMITS)
        );
    }

    #[test]
    fn an_existing_named_limit_and_non_gap_classes_are_untouched() {
        let mut named = marked();
        named.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
        assert!(!withhold_unsupported_gap(&mut named));
        assert_eq!(named.class, ExposureClass::ReachableUnrevealed);

        for class in [
            ExposureClass::Exposed,
            ExposureClass::WeaklyExposed,
            ExposureClass::NoStaticPath,
            ExposureClass::InfectionUnknown,
            ExposureClass::PropagationUnknown,
            ExposureClass::StaticUnknown,
        ] {
            let mut finding = marked();
            finding.class = class.clone();
            assert!(!withhold_unsupported_gap(&mut finding));
            assert_eq!(finding.class, class);
            assert_eq!(finding.static_limit_kind, None);
        }
    }

    #[test]
    fn withholding_twice_adds_one_stop_reason() {
        let mut finding = marked();
        assert!(withhold_unsupported_gap(&mut finding));
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(
            finding.stop_reasons,
            vec![StopReason::GapEvidenceUnresolved]
        );
    }

    const LOG_SOURCE: &str = r#"
pub fn audit(amount: u32) {
    log::info!("audited {}", amount * 7);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Capture(std::sync::Mutex<Vec<String>>);

    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            self.0.lock().unwrap().push(record.args().to_string());
        }
        fn flush(&self) {}
    }

    fn capture() -> &'static Capture {
        let logger: &'static Capture = Box::leak(Box::new(Capture(Default::default())));
        log::set_logger(logger).unwrap();
        logger
    }

    #[test]
    fn audit_logs_the_scaled_amount() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_runs() {
        audit(6);
    }

    #[test]
    fn audit_then_capture() {
        audit(6);
        let logger = capture();
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_asserts_elsewhere() {
        let _logger = capture();
        audit(6);
        let total = 6 * 7;
        assert_eq!(total, 42);
    }

    static LOGGER: Capture = Capture(std::sync::Mutex::new(Vec::new()));

    #[test]
    fn audit_direct_install() {
        log::set_logger(&LOGGER).unwrap();
        audit(6);
        assert_eq!(*LOGGER.0.lock().unwrap(), vec!["audited 42".to_string()]);
    }

    fn capture_once() -> &'static Capture {
        static ONCE: std::sync::OnceLock<&'static Capture> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            let logger: &'static Capture = Box::leak(Box::new(Capture(Default::default())));
            let _ = log::set_logger(logger);
            logger
        })
    }

    #[test]
    fn audit_through_once_lock() {
        let logger = capture_once();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert!(lines.iter().any(|l| l == "audited 42"), "{lines:?}");
    }

    #[test]
    fn audit_again_after_late_capture() {
        audit(1);
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_counts_lines() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines.len(), 1, "one line");
    }

    #[test]
    fn audit_checks_line_width() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines.iter().next().map(String::len), Some(10));
    }

    #[test]
    fn audit_checks_install_result() {
        let installed = log::set_logger(&LOGGER);
        audit(6);
        assert_eq!(installed.is_ok(), true);
    }

    fn silence() -> &'static str {
        log::set_logger(&LOGGER).ok();
        "six"
    }

    #[test]
    fn audit_with_helper_value() {
        let word = silence();
        audit(6);
        assert_eq!(word, "six");
    }

    mod other {
        pub fn audit(_amount: u32) {}
    }

    #[test]
    fn audit_path_call() {
        let logger = capture();
        other::audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_asserts_before_call() {
        let logger = capture();
        assert_eq!(*logger.0.lock().unwrap(), Vec::<String>::new(), "empty");
        audit(6);
    }

    #[test]
    fn audit_relational() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert!(lines.contains(&"audited 42".to_string()));
    }
}
"#;

    fn log_index() -> RustIndex {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            LOG_SOURCE.to_string(),
        );
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from("src/lib.rs"), facts);
        index
    }

    /// The cell-0096 shape: a weakly exposed `log::info!` record with one
    /// related test of the given name.
    fn log_finding(test: &str, expression: &str) -> Finding {
        let mut finding = marked();
        finding.class = ExposureClass::WeaklyExposed;
        finding.probe.family = ProbeFamily::SideEffect;
        finding.probe.expression = expression.to_string();
        finding.evidence.clear();
        finding.related_tests = vec![related(test)];
        finding
    }

    const AUDIT_LOG: &str = "log::info!(\"audited {}\", amount * 7);";

    #[test]
    fn a_captured_log_assertion_withholds_the_gap_under_its_own_limit() {
        let index = log_index();
        for test in [
            "audit_logs_the_scaled_amount",
            "audit_direct_install",
            "audit_again_after_late_capture",
            "audit_through_once_lock",
        ] {
            let mut finding = log_finding(test, AUDIT_LOG);
            assert!(
                withhold_log_capture_gap(&mut finding, &index, Some("audit")),
                "{test}"
            );
            assert_eq!(finding.class, ExposureClass::StaticUnknown, "{test}");
            assert_eq!(
                finding.static_limit_kind,
                Some(StaticLimitKind::RustLogCaptureUnresolved),
                "{test}"
            );
            assert_eq!(
                finding.stop_reasons,
                vec![StopReason::GapEvidenceUnresolved],
                "{test}"
            );
            assert!(finding.activation.missing_discriminators.is_empty());
            assert!(
                finding
                    .recommended_next_step
                    .as_deref()
                    .is_some_and(|step| step.contains(&format!("`{test}`"))),
                "{test}: {:?}",
                finding.recommended_next_step
            );
            assert!(
                finding
                    .evidence
                    .iter()
                    .any(|line| line.contains(LOG_FIRST_UNRESOLVED_EDGE)),
                "{test}"
            );
        }
    }

    #[test]
    fn an_uncaptured_or_misordered_log_test_keeps_the_gap() {
        let index = log_index();
        for test in [
            "audit_runs",
            "audit_then_capture",
            "audit_asserts_elsewhere",
            "audit_counts_lines",
            "audit_checks_install_result",
            "audit_with_helper_value",
            "audit_path_call",
            "audit_asserts_before_call",
            "audit_relational",
            "audit_checks_line_width",
        ] {
            let mut finding = log_finding(test, AUDIT_LOG);
            assert!(
                !withhold_log_capture_gap(&mut finding, &index, Some("audit")),
                "{test}"
            );
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{test}");
            assert_eq!(finding.static_limit_kind, None, "{test}");
        }
    }

    #[test]
    fn only_a_qualified_log_record_on_a_free_owner_is_withheld() {
        let index = log_index();
        let test = "audit_logs_the_scaled_amount";
        let mut bare = log_finding(test, "info!(\"audited {}\", amount * 7);");
        assert!(!withhold_log_capture_gap(&mut bare, &index, Some("audit")));
        let mut println = log_finding(test, "log::println!(\"{}\", amount * 7);");
        assert!(!withhold_log_capture_gap(
            &mut println,
            &index,
            Some("audit")
        ));
        let mut method = log_finding(test, AUDIT_LOG);
        assert!(!withhold_log_capture_gap(&mut method, &index, None));
        let mut other_owner = log_finding(test, AUDIT_LOG);
        assert!(!withhold_log_capture_gap(
            &mut other_owner,
            &index,
            Some("report")
        ));
        let mut predicate = log_finding(test, AUDIT_LOG);
        predicate.probe.family = ProbeFamily::Predicate;
        assert!(!withhold_log_capture_gap(
            &mut predicate,
            &index,
            Some("audit")
        ));
        let mut gap = log_finding(test, AUDIT_LOG);
        gap.class = ExposureClass::ReachableUnrevealed;
        assert!(!withhold_log_capture_gap(&mut gap, &index, Some("audit")));
        let mut limited = log_finding(test, AUDIT_LOG);
        limited.static_limit_kind = Some(StaticLimitKind::Metaprogramming);
        assert!(!withhold_log_capture_gap(
            &mut limited,
            &index,
            Some("audit")
        ));
        let mut tracing = log_finding(test, "tracing::warn!(\"audited {}\", amount * 7);");
        assert!(!withhold_log_capture_gap(
            &mut tracing,
            &index,
            Some("audit")
        ));
    }
}
