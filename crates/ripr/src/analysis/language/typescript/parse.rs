//! Parsing utilities for the TypeScript preview adapter.
//!
//! Every oxc parse in this adapter runs through [`parse_on_worker`]:
//!
//! 1. A cheap single-pass nesting scan estimates the deepest expression
//!    nesting in the source before any parse is attempted (issue #4101).
//!    Sources past [`PARSE_NESTING_BUDGET`] are refused with a typed
//!    `expression_nesting_budget` static-limit reason instead of being
//!    parsed, so no input can abort the process and every refusal carries
//!    the same `unsupported_syntax` disclosure contract as a parser error.
//! 2. Accepted sources are parsed on a dedicated worker thread with a large
//!    explicit stack ([`PARSE_WORKER_STACK_BYTES`]), so the recursive oxc
//!    parser no longer depends on the caller's (often 1 MiB) stack. The
//!    budget stays far below the worker's own measured overflow depth, so
//!    inputs under the budget keep parsing — including deep but legitimate
//!    files that previously aborted the process.

use super::*;
use crate::domain::EvidenceState;

/// Stack size for the dedicated parse worker thread.
///
/// The oxc 0.130 parser is recursive-descent; its recursion depth is bounded
/// only by the thread stack it runs on. The main thread's stack is small and
/// platform-dependent (1 MiB on Windows), where paren nesting around 400
/// already aborts the process (issue #4101). A dedicated worker with an
/// explicit 256 MiB stack lifts the practical overflow depth to the tens of
/// thousands under Windows debug builds (the worst frame sizes observed for
/// this fix; release builds recurse further per stack byte). The reservation
/// is address-space only: the worker touches pages as the parse actually
/// recurses, and at most one worker is alive at a time because the adapter
/// parses workspace files sequentially.
pub(crate) const PARSE_WORKER_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Maximum estimated expression nesting depth accepted for a parse.
///
/// Justification (issue #4101):
///
/// - Abort floor: on the unguarded main-thread path, paren nesting of 350
///   parsed cleanly and 400 aborted on Windows debug (1 MiB stack); unary
///   `!` / `await` chains and ternary chains aborted around n≈2000 there.
///   The budget sits above every historically aborting input, so those
///   inputs are now refused with a typed disclosure instead of being parsed
///   on a small stack.
/// - Headroom: the 256 MiB worker still parses nesting far beyond the
///   budget (measured on Windows debug; see the `parse_depth_tests` module),
///   so the budget is a disclosed safety envelope far below the worker's
///   own overflow depth, not the worker's limit.
/// - Real-world code: expression nesting beyond a few dozen levels is
///   dominated by iterative binary/member chains, which do not count toward
///   the estimate; recursive paren/ternary/unary nesting at 2000+ levels is
///   generated/minified pathology, not reviewable source.
pub(crate) const PARSE_NESTING_BUDGET: usize = 2_000;

/// Typed refusal when the worker thread cannot be spawned.
const PARSE_WORKER_UNAVAILABLE: &str =
    "static limit parse_worker_unavailable: TypeScript parse worker thread could not start";

/// Typed refusal when the worker thread died (for example an oxc panic).
const PARSE_WORKER_FAILED: &str = "static limit parse_worker_failed: TypeScript parse worker thread failed; syntax facts unavailable";

/// Whether `reason` (an `Err` from [`parse_on_worker`]) means the worker thread
/// could not run, which depends on the environment rather than on the source.
/// The nesting-budget refusal is deterministic and returns false.
pub(crate) fn is_worker_failure(reason: &str) -> bool {
    reason == PARSE_WORKER_FAILED || reason.starts_with(PARSE_WORKER_UNAVAILABLE)
}

pub(crate) fn parse_error_reason(file: &Path, source: &str) -> Option<String> {
    parse_on_worker(file, source, |file, source, allocator| {
        let ret = Parser::new(allocator, source, source_type_for(file)).parse();
        if ret.errors.is_empty() {
            None
        } else {
            // Include the first parser message so the limitation is actionable
            // ("1 parser error(s): Unexpected token") instead of a bare count.
            // oxc diagnostics implement Display as their message text.
            Some(format!(
                "{} parser error(s): {}",
                ret.errors.len(),
                ret.errors[0]
            ))
        }
    })
    .unwrap_or_else(Some)
}

pub(crate) fn parse_limit_for_file<'a>(
    file: &Path,
    limits: &'a [TypeScriptParseLimit],
) -> Option<&'a TypeScriptParseLimit> {
    let changed_file = normalized_path(file);
    limits
        .iter()
        .find(|limit| normalized_path(&limit.file) == changed_file)
}

/// Capping reasons for a `static_unknown` preview finding with no extracted
/// owner or related test (#10700): the discriminator is not established.
fn unmodeled_confidence_basis() -> Vec<crate::domain::ConfidenceCap> {
    crate::domain::ConfidenceCap::for_preview(
        &ExposureClass::StaticUnknown,
        &crate::domain::ConfidenceBasisInputs {
            has_related_test: false,
            strong_oracle_present: false,
            boundary_input_matched: true,
        },
    )
}

pub(crate) fn unsupported_syntax_finding(
    file: &Path,
    line: usize,
    line_text: &str,
    limit: &TypeScriptParseLimit,
) -> Finding {
    let id_path: String = file
        .display()
        .to_string()
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    let unsup_probe_id = fingerprint_probe_id(
        "probe",
        &id_path,
        "typescript_preview_unsupported_syntax",
        "",
        &normalize_expression(line_text),
        1,
    );
    let probe = Probe {
        id: unsup_probe_id.clone(),
        location: SourceLocation::new(file.to_string_lossy().as_ref(), line, 1),
        owner: None,
        family: ProbeFamily::StaticUnknown,
        delta: DeltaKind::Unknown,
        before: None,
        after: Some(line_text.to_string()),
        expression: line_text.to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    };
    let summary = format!(
        "TypeScript preview parser could not build syntax facts for `{}`: {}",
        normalized_path(file),
        limit.reason
    );
    let stage = StageEvidence::new(StageState::Unknown, Confidence::Low, &summary);
    let missing = format!(
        "Static limit `unsupported_syntax`: malformed TypeScript/JavaScript prevented syntax-first owner, test, and probe extraction for `{}`. Repair route: fix or isolate the unsupported syntax before relying on repair guidance.",
        normalized_path(file)
    );
    let why_not_actionable = format!(
        "static limit `unsupported_syntax` prevents bounded TypeScript repair guidance: {}",
        limit.reason
    );
    let repair_route =
        "fix or isolate the unsupported syntax before relying on repair guidance".to_string();
    let recommended = "TypeScript preview advisory: static limit `unsupported_syntax`; Repair route: fix or isolate the unsupported syntax before relying on repair guidance; no actionable repair packet is emitted.".to_string();
    // Resolved from the probe's own delta evidence (#3281) before the probe
    // moves into the finding.
    let source_currentness = crate::domain::SourceCurrentness::from_probe_delta(
        probe.before.as_deref(),
        probe.after.as_deref(),
    );

    Finding {
        analysis_scope: crate::domain::AnalysisScope::Diff,
        confidence_basis: unmodeled_confidence_basis(),
        id: probe.id.0.clone(),
        canonical_gap: None,
        probe,
        class: ExposureClass::StaticUnknown,
        ripr: RiprEvidence {
            reach: stage.clone(),
            infect: stage.clone(),
            propagate: stage.clone(),
            reveal: RevealEvidence {
                observe: stage.clone(),
                discriminate: stage,
            },
        },
        confidence: 0.2,
        evidence: vec![
            format!("static_limit unsupported_syntax: {}", limit.reason),
            format!("gap_state: {}", EvidenceState::StaticLimitation.as_str()),
            "actionability_category: unsupported_syntax".to_string(),
            format!("why_not_actionable: {why_not_actionable}"),
            format!("repair_route: {repair_route}"),
            "evidence_needed_to_promote: resolve the named static limit and re-run TypeScript preview evidence extraction".to_string(),
            typescript_raw_evidence_ref(
                file,
                line,
                None,
                &unsup_probe_id.0,
            ),
        ],
        missing: vec![
            missing,
            format!(
                "TypeScript preview actionability `static_limitation` / `unsupported_syntax`: {why_not_actionable}. Repair route: {repair_route}"
            ),
        ],
        flow_sinks: Vec::new(),
        activation: Default::default(),
        stop_reasons: vec![StopReason::StaticProbeUnknown],
        related_tests_matched_total: None,
        related_tests: Vec::new(),
        recommended_next_step: Some(recommended),
        language: Some(output_language_for(file)),
        language_status: Some(LanguageStatus::Preview),
        owner_kind: None,
        static_limit_kind: Some(StaticLimitKind::UnsupportedSyntax),
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness,
    }
}

/// A `static_unknown` finding for a changed runtime enum member (#9436).
///
/// The owner extractor indexes no enum, so the classifier yields nothing for
/// `High = 2,` -> `High = 3,` and the edit would read as silence. Whether any
/// test observes the member's numeric value is not statically known, so the
/// honest verdict is limited, never exposed and never actionable.
pub(crate) fn enum_member_finding(
    file: &Path,
    line: usize,
    line_text: &str,
    identity: &str,
) -> Finding {
    let id_path: String = normalized_path(file)
        .to_string()
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    let probe_id = fingerprint_probe_id(
        "probe",
        &id_path,
        "typescript_preview_enum_member",
        // Enum::Member keeps identical member text in two enums apart without
        // tying the id to a source line.
        identity,
        &normalize_expression(line_text),
        1,
    );
    let probe = Probe {
        id: probe_id.clone(),
        location: SourceLocation::new(file.to_string_lossy().as_ref(), line, 1),
        owner: None,
        family: ProbeFamily::StaticUnknown,
        delta: DeltaKind::Unknown,
        before: None,
        after: Some(line_text.to_string()),
        expression: line_text.to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    };
    let summary = format!(
        "TypeScript preview does not index enum members, so no test relation is known for `{}`",
        normalized_path(file)
    );
    let stage = StageEvidence::new(StageState::Unknown, Confidence::Low, &summary);
    let why_not_actionable = "static limit `typescript_owner_extraction_partial` (enum declaration): the changed enum member has no extracted owner, so whether a test observes its value is not statically known".to_string();
    let repair_route =
        "add or strengthen a test that reads the changed member's value, then re-run".to_string();
    let source_currentness = crate::domain::SourceCurrentness::from_probe_delta(
        probe.before.as_deref(),
        probe.after.as_deref(),
    );
    Finding {
        analysis_scope: crate::domain::AnalysisScope::Diff,
        confidence_basis: unmodeled_confidence_basis(),
        id: probe.id.0.clone(),
        canonical_gap: None,
        probe,
        class: ExposureClass::StaticUnknown,
        ripr: RiprEvidence {
            reach: stage.clone(),
            infect: stage.clone(),
            propagate: stage.clone(),
            reveal: RevealEvidence {
                observe: stage.clone(),
                discriminate: stage,
            },
        },
        confidence: 0.2,
        evidence: vec![
            "static_limit typescript_owner_extraction_partial: enum member".to_string(),
            format!("gap_state: {}", EvidenceState::StaticLimitation.as_str()),
            "actionability_category: owner_shape_unextracted".to_string(),
            format!("why_not_actionable: {why_not_actionable}"),
            format!("repair_route: {repair_route}"),
            "evidence_needed_to_promote: extract enum members as owners, then re-run TypeScript preview evidence".to_string(),
            typescript_raw_evidence_ref(file, line, None, &probe_id.0),
        ],
        missing: vec![format!(
            "Static limit `typescript_owner_extraction_partial`: {why_not_actionable}. Repair route: {repair_route}"
        )],
        flow_sinks: Vec::new(),
        activation: Default::default(),
        stop_reasons: vec![StopReason::StaticProbeUnknown],
        related_tests_matched_total: None,
        related_tests: Vec::new(),
        recommended_next_step: Some(format!(
            "TypeScript preview advisory: {why_not_actionable}; no actionable repair packet is emitted."
        )),
        language: Some(output_language_for(file)),
        language_status: Some(LanguageStatus::Preview),
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness,
    }
}

/// Parse `source` for `file` behind the nesting budget and the large-stack
/// worker thread, then run `analyze` over the parsed program inside the
/// worker. Returns `Err` with a typed static-limit reason when the budget
/// refuses the source or the worker cannot run; `Ok` otherwise.
pub(crate) fn parse_on_worker<T, F>(file: &Path, source: &str, analyze: F) -> Result<T, String>
where
    F: FnOnce(&Path, &str, &Allocator) -> T + Send + 'static,
    T: Send + 'static,
{
    if let Some(estimated) = nesting_over_budget(source, source_may_contain_jsx(file)) {
        return Err(nesting_budget_refusal_reason(estimated));
    }
    let owned_file = file.to_path_buf();
    let owned_source = source.to_string();
    #[cfg(test)]
    let injection_owner = std::thread::current().id();
    let spawn = std::thread::Builder::new()
        .name("ripr-ts-oxc-parse".to_string())
        .stack_size(PARSE_WORKER_STACK_BYTES)
        .spawn(move || {
            #[cfg(test)]
            injected_worker_panic::fire(injection_owner, &owned_file);
            let allocator = Allocator::default();
            analyze(&owned_file, &owned_source, &allocator)
        });
    match spawn {
        Ok(handle) => handle
            .join()
            .map_err(|_join_error| PARSE_WORKER_FAILED.to_string()),
        Err(err) => Err(format!("{PARSE_WORKER_UNAVAILABLE}: {err}")),
    }
}

/// Test-only fault injection (#11132): a panic on the parse worker for an
/// armed file path, raised before the caller's analysis closure runs, so a
/// test drives the production join-and-disclose path with the same thread
/// death an oxc or AST-consumer panic produces. Keyed by the initiating
/// thread and path; its identity is passed explicitly to the parse worker so
/// overlapping analyses on different test threads cannot affect each other.
#[cfg(test)]
pub(crate) mod injected_worker_panic {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex};
    use std::thread::ThreadId;

    /// Initiating thread and path -> parses allowed to succeed first.
    static ARMED: LazyLock<Mutex<HashMap<(ThreadId, PathBuf), usize>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    /// Disarms the path when dropped.
    pub(crate) struct Armed((ThreadId, PathBuf));

    impl Drop for Armed {
        fn drop(&mut self) {
            if let Ok(mut armed) = ARMED.lock() {
                armed.remove(&self.0);
            }
        }
    }

    /// Panic on the worker for every parse of `path` after `allow` successes.
    pub(crate) fn arm(path: &Path, allow: usize) -> Armed {
        let key = (std::thread::current().id(), path.to_path_buf());
        if let Ok(mut armed) = ARMED.lock() {
            armed.insert(key.clone(), allow);
        }
        Armed(key)
    }

    pub(super) fn fire(owner: ThreadId, path: &Path) {
        let due = match ARMED.lock() {
            Ok(mut armed) => match armed.get_mut(&(owner, path.to_path_buf())) {
                Some(0) => true,
                Some(remaining) => {
                    *remaining -= 1;
                    false
                }
                None => false,
            },
            Err(_) => false,
        };
        if due {
            std::panic::resume_unwind(Box::new("injected TypeScript parse worker panic (#11132)"));
        }
    }
}

/// Typed `expression_nesting_budget` static-limit reason for a source whose
/// estimated nesting depth is over [`PARSE_NESTING_BUDGET`].
fn nesting_budget_refusal_reason(estimated: usize) -> String {
    format!(
        "static limit expression_nesting_budget: estimated expression nesting depth {estimated} exceeds the {PARSE_NESTING_BUDGET}-level parse budget; parse refused to avoid a parser stack overflow"
    )
}

/// `Some(estimated_depth)` when the cheap nesting scan estimates the source
/// over [`PARSE_NESTING_BUDGET`], else `None`.
fn nesting_over_budget(source: &str, jsx: bool) -> Option<usize> {
    let estimated = max_expression_nesting_estimate_with(source, jsx);
    (estimated > PARSE_NESTING_BUDGET).then_some(estimated)
}

/// Whether `file` can hold JSX elements (`.ts`/`.mts`/`.cts` cannot, so a
/// `<Type>value` assertion there is never an element).
fn source_may_contain_jsx(file: &Path) -> bool {
    !matches!(
        file.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "mts" | "cts")
    )
}

/// What the last significant identifier/number word was, so regex-vs-division
/// and prefix-operator decisions can see keywords instead of only bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LastWord {
    None,
    /// `await` / `typeof` / `void` / `delete` / `new`.
    PrefixOperator,
    /// Keywords that put a following `/` into expression position
    /// (`return`, `case`, `throw`, ...).
    ExpressionKeyword,
    Operand,
}

/// Estimate the deepest expression nesting in `source` with a single
/// left-to-right byte scan (issue #4101).
///
/// The scan is a lexical approximation, not a parse. It tracks three
/// contributors that all add recursive-descent frames in the oxc parser and
/// returns the maximum of their running sum:
///
/// - bracket depth: `(`, `[`, `{`;
/// - open ternary chain: `?` conditionals not yet closed by their `:`
///   (object-literal / type-annotation colons can pop a ternary early,
///   which under-counts; the bracket terms keep those sources covered);
/// - prefix-operator run: consecutive `!`/`~`, prefix `+`/`-`, and prefix
///   `await`/`typeof`/`void`/`delete`/`new` with no operand between them;
/// - open JSX elements (only when `jsx`): a `<` in expression position
///   followed by a name or `>` opens one, `</` and `/>` close one; and every
///   template interpolation `${` adds a level like a bracket does.
///
/// Long binary/member chains are iterative in the parser and contribute
/// nothing. Line/block comments, string and template literals, and regex
/// literals are skipped so their contents cannot inflate the estimate; the
/// regex-vs-division choice uses the standard expression-position heuristic,
/// and a regex that never terminates on its line falls back to code.
#[cfg(test)]
pub(crate) fn max_expression_nesting_estimate(source: &str) -> usize {
    max_expression_nesting_estimate_with(source, true)
}

/// [`max_expression_nesting_estimate`] with JSX element counting selected by
/// `jsx` (off for plain TypeScript files, where `<T>x` is a type assertion).
pub(crate) fn max_expression_nesting_estimate_with(source: &str, jsx: bool) -> usize {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Code,
        Template,
        LineComment,
        BlockComment,
        Str(u8),
        Regex { in_class: bool },
    }

    fn is_word_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
    }

    let mut mode = Mode::Code;
    // Brace depth recorded when each template interpolation `${` opened; a
    // `}` that brings the depth back to the recorded value closes it.
    let mut interpolations: Vec<usize> = Vec::new();
    let mut depth: usize = 0;
    let mut ternary_open: usize = 0;
    let mut unary_run: usize = 0;
    let mut jsx_open: usize = 0;
    // Open `<` run (type arguments and chained comparisons: `a<a<a<..`,
    // `Array<Array<..`), which oxc parses recursively. `>` closes one; the run
    // resets at statement, list and block boundaries.
    let mut angle_run: usize = 0;
    let mut max_estimate: usize = 0;
    let mut last_word = LastWord::None;
    // Last significant byte inside code, for the position heuristics.
    // `None` means start of input.
    let mut prev: Option<u8> = None;

    let bytes = source.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        match mode {
            Mode::LineComment => {
                if byte == b'\n' {
                    mode = Mode::Code;
                }
            }
            Mode::BlockComment => {
                if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    index += 1;
                    mode = Mode::Code;
                }
            }
            Mode::Str(quote) => {
                if byte == b'\\' {
                    index += 1;
                } else if byte == quote {
                    mode = Mode::Code;
                    last_word = LastWord::Operand;
                    prev = Some(quote);
                }
            }
            Mode::Template => match byte {
                b'\\' => index += 1,
                b'`' => {
                    mode = Mode::Code;
                    last_word = LastWord::Operand;
                    prev = Some(b'`');
                }
                b'$' if bytes.get(index + 1) == Some(&b'{') => {
                    index += 1;
                    // The interpolation is a nesting level of its own for the
                    // recursive parser (nested templates parse recursively).
                    depth = depth.saturating_add(1);
                    interpolations.push(depth);
                    max_estimate = max_estimate.max(
                        depth
                            .saturating_add(ternary_open)
                            .saturating_add(unary_run)
                            .saturating_add(jsx_open),
                    );
                    mode = Mode::Code;
                    last_word = LastWord::None;
                    prev = Some(b'{');
                }
                _ => {}
            },
            Mode::Regex { in_class } => match byte {
                b'\\' => index += 1,
                b'[' if !in_class => mode = Mode::Regex { in_class: true },
                b']' => mode = Mode::Regex { in_class: false },
                b'\n' => {
                    // A regex cannot span lines: the heuristic misjudged a
                    // division, so fall back to code scanning.
                    mode = Mode::Code;
                    last_word = LastWord::Operand;
                    prev = Some(b'\n');
                }
                b'/' if !in_class => {
                    mode = Mode::Code;
                    last_word = LastWord::Operand;
                    prev = Some(b'/');
                }
                _ => {}
            },
            Mode::Code => {
                if byte.is_ascii_whitespace() {
                    index += 1;
                    continue;
                }
                if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
                    mode = Mode::LineComment;
                    index += 2;
                    continue;
                }
                if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    mode = Mode::BlockComment;
                    index += 2;
                    continue;
                }
                if is_word_byte(byte) {
                    let mut end = index;
                    while end < bytes.len() && is_word_byte(bytes[end]) {
                        end += 1;
                    }
                    let word = &source[index..end];
                    last_word = if byte.is_ascii_digit() {
                        LastWord::Operand
                    } else if prev != Some(b'.')
                        && matches!(word, "await" | "typeof" | "void" | "delete" | "new")
                    {
                        LastWord::PrefixOperator
                    } else if matches!(
                        word,
                        "return"
                            | "case"
                            | "throw"
                            | "in"
                            | "of"
                            | "instanceof"
                            | "do"
                            | "else"
                            | "yield"
                    ) {
                        LastWord::ExpressionKeyword
                    } else {
                        LastWord::Operand
                    };
                    if last_word == LastWord::PrefixOperator && expression_position(prev, last_word)
                    {
                        unary_run = unary_run.saturating_add(1);
                    } else {
                        unary_run = 0;
                    }
                    prev = bytes.get(end.wrapping_sub(1)).copied();
                    index = end;
                    max_estimate = max_estimate.max(
                        depth
                            .saturating_add(ternary_open)
                            .saturating_add(unary_run)
                            .saturating_add(jsx_open),
                    );
                    continue;
                }
                match byte {
                    b'\'' | b'"' => {
                        mode = Mode::Str(byte);
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'`' => {
                        mode = Mode::Template;
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'<' if jsx => {
                        let next = bytes.get(index + 1).copied();
                        if next == Some(b'/') {
                            // `</name>` / `</>`: consume the slash so it is
                            // not read as a regex start.
                            jsx_open = jsx_open.saturating_sub(1);
                            index += 1;
                        } else if expression_position(prev, last_word)
                            && next.is_some_and(|n| {
                                n.is_ascii_alphabetic() || matches!(n, b'>' | b'_' | b'$')
                            })
                        {
                            jsx_open = jsx_open.saturating_add(1);
                        } else {
                            angle_run = angle_run.saturating_add(1);
                        }
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'<' => {
                        angle_run = angle_run.saturating_add(1);
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'>' => {
                        if prev != Some(b'=') {
                            angle_run = angle_run.saturating_sub(1);
                        }
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b';' | b',' => {
                        angle_run = 0;
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'/' if jsx && bytes.get(index + 1) == Some(&b'>') => {
                        // Self-closing `/>`.
                        jsx_open = jsx_open.saturating_sub(1);
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'/' if expression_position(prev, last_word) => {
                        mode = Mode::Regex { in_class: false };
                        last_word = LastWord::None;
                        // Regex contents are skipped; the literal becomes
                        // an operand when it closes.
                    }
                    b'/' => {
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'(' | b'[' | b'{' => {
                        depth = depth.saturating_add(1);
                        if byte == b'{' {
                            angle_run = 0;
                        }
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b')' | b']' => {
                        depth = depth.saturating_sub(1);
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'}' => {
                        if interpolations.last() == Some(&depth) {
                            interpolations.pop();
                            depth = depth.saturating_sub(1);
                            mode = Mode::Template;
                            // The interpolation continues template text: this
                            // brace closes the level `${` opened.
                        } else {
                            depth = depth.saturating_sub(1);
                            angle_run = 0;
                            unary_run = 0;
                            prev = Some(byte);
                        }
                        last_word = LastWord::None;
                    }
                    b'?' => match bytes.get(index + 1) {
                        Some(b'.') | Some(b'?') | Some(b'=') => {
                            index += 1;
                            unary_run = 0;
                            last_word = LastWord::None;
                            prev = Some(b'?');
                        }
                        _ => {
                            ternary_open = ternary_open.saturating_add(1);
                            last_word = LastWord::None;
                            prev = Some(byte);
                        }
                    },
                    b':' => {
                        ternary_open = ternary_open.saturating_sub(1);
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'!' | b'~' => {
                        unary_run = unary_run.saturating_add(1);
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    b'+' | b'-' => {
                        if expression_position(prev, last_word) {
                            unary_run = unary_run.saturating_add(1);
                        } else {
                            unary_run = 0;
                        }
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                    _ => {
                        unary_run = 0;
                        last_word = LastWord::None;
                        prev = Some(byte);
                    }
                }
                max_estimate = max_estimate.max(
                    depth
                        .saturating_add(ternary_open)
                        .saturating_add(unary_run)
                        .saturating_add(jsx_open)
                        .saturating_add(angle_run),
                );
            }
        }
        index += 1;
    }
    max_estimate
}

/// Whether the position after `prev`/`last_word` starts an expression: a `/`
/// there begins a regex literal and a `+`/`-`/prefix keyword there is a
/// unary operator.
fn expression_position(prev: Option<u8>, last_word: LastWord) -> bool {
    match last_word {
        LastWord::PrefixOperator | LastWord::ExpressionKeyword => return true,
        LastWord::Operand => return false,
        LastWord::None => {}
    }
    match prev {
        // Start of input is an expression position.
        None => true,
        // After an operand or a closer, a `/` divides and `+`/`-` are binary.
        Some(b')') | Some(b']') | Some(b'}') => false,
        Some(b'"') | Some(b'\'') | Some(b'`') => false,
        Some(b'/') => false,
        Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' => false,
        // Operators, openers, and punctuation are expression positions.
        Some(_) => true,
    }
}

#[cfg(test)]
mod parse_depth_tests {
    #[test]
    fn unmodeled_producers_carry_a_discriminator_not_established_basis() {
        use crate::domain::ConfidenceCap;
        let path = std::path::Path::new("src/a.ts");
        let limit = TypeScriptParseLimit {
            file: path.to_path_buf(),
            reason: "depth".to_string(),
        };
        let unsupported = unsupported_syntax_finding(path, 1, "x", &limit);
        let member = enum_member_finding(path, 1, "High = 2,", "Level::High");
        for finding in [&unsupported, &member] {
            assert_eq!(
                finding.confidence_basis.last(),
                Some(&ConfidenceCap::DiscriminatorNotEstablished)
            );
        }
    }

    /// Producer-wide invariant: every committed TypeScript/JavaScript preview
    /// finding in the fixture goldens carries a non-empty `confidence_basis`
    /// and a `confidence_why_capped` line, whichever producer built it.
    #[test]
    fn every_ts_js_preview_finding_in_the_goldens_has_a_confidence_basis() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        let mut checked = 0usize;
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
            let path = entry
                .map_err(|e| e.to_string())?
                .path()
                .join("expected/check.json");
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| e.to_string())?;
            for finding in value["findings"].as_array().into_iter().flatten() {
                let language = finding["language"].as_str().unwrap_or("");
                if language != "typescript" && language != "javascript" {
                    continue;
                }
                checked += 1;
                let has_basis = finding["confidence_basis"]
                    .as_array()
                    .is_some_and(|basis| !basis.is_empty());
                if !has_basis || finding["confidence_why_capped"].as_str().is_none() {
                    missing.push(format!("{} {}", path.display(), finding["id"]));
                }
            }
        }
        assert!(
            checked > 20,
            "expected many TS/JS golden findings, saw {checked}"
        );
        assert!(missing.is_empty(), "findings without a basis: {missing:#?}");
        Ok(())
    }

    use super::*;

    /// Wrap `inner` in `depth` balanced parentheses.
    fn nested(depth: usize, inner: &str) -> String {
        let mut source = String::new();
        source.extend(std::iter::repeat_n('(', depth));
        source.push_str(inner);
        source.extend(std::iter::repeat_n(')', depth));
        source
    }

    /// #11661 family, TypeScript: oxc parses `a < a < a ..` (speculative
    /// type arguments) and `Array<Array<..` recursively; 300,000 links
    /// overflowed even the 256 MiB worker in a release build.
    #[test]
    fn nesting_estimate_counts_angle_bracket_runs() {
        let comparisons = format!("const v = a{};", " < a".repeat(5_000));
        let generics = format!(
            "let y: {}number{};",
            "Array<".repeat(5_000),
            ">".repeat(5_000)
        );
        for source in [comparisons, generics] {
            assert!(
                max_expression_nesting_estimate_with(&source, false) > PARSE_NESTING_BUDGET,
                "{}",
                &source[..30]
            );
        }
        // Ordinary generics, comparisons and arrows stay far below the budget.
        let ordinary = "const f = (a: Map<string, Array<number>>) => a.size < 3 && a.size > 1 ? new Set<Array<Map<K, V>>>() : [1 < 2, 3 > 2];";
        assert!(max_expression_nesting_estimate_with(ordinary, false) < 20);
    }

    #[test]
    fn nesting_estimate_counts_recursive_expression_shapes() {
        assert_eq!(max_expression_nesting_estimate("((((a))))"), 4);
        assert_eq!(max_expression_nesting_estimate("[[[{a}]]]"), 4);
        // Nested ternaries: each `?` opens a chain link, each `:` closes one.
        assert_eq!(max_expression_nesting_estimate("a ? b ? c : d : e"), 2);
        // Prefix operator runs.
        assert_eq!(max_expression_nesting_estimate("!!!!value"), 4);
        assert_eq!(max_expression_nesting_estimate("- - -value"), 3);
        assert_eq!(
            max_expression_nesting_estimate("await await await value"),
            3
        );
        // Combined contributors sum while they nest together.
        assert_eq!(max_expression_nesting_estimate("!!(answer ? yes : no)"), 3);
    }

    #[test]
    fn nesting_estimate_ignores_lexical_shells() {
        // String, comment, template, and regex contents do not nest.
        assert_eq!(
            max_expression_nesting_estimate("const s = ')))((( (( ';"),
            0
        );
        assert_eq!(
            max_expression_nesting_estimate("/* ((( (( */ const a = 1; // ((( "),
            0
        );
        assert_eq!(
            max_expression_nesting_estimate("const t = `a ${f((g))} b )(` ;"),
            3
        );
        assert_eq!(max_expression_nesting_estimate("const r = /a(b)c/ ;"), 0);
        assert_eq!(max_expression_nesting_estimate("return /(((/;"), 0);
        // Division chains are iterative in the parser and count for nothing.
        assert_eq!(max_expression_nesting_estimate("const q = a / b / c;"), 0);
        // Binary/member chains are iterative in the parser and count for nothing.
        assert_eq!(
            max_expression_nesting_estimate("a.b + c.d + e.f + g.h + i.j + k.l;"),
            0
        );
    }

    /// Hostile-input regression: nested template interpolations and JSX
    /// elements recurse in the parser but were invisible to the estimate, so a
    /// 50k-deep template hung (quadratic) and a 20k-deep JSX tree overflowed
    /// the stack.
    #[test]
    fn nesting_estimate_counts_template_interpolations_and_jsx_elements() {
        assert_eq!(max_expression_nesting_estimate("`${`${`${x}`}`}`"), 3);
        let templates = format!("{}x{}", "`${".repeat(5000), "}`".repeat(5000));
        assert_eq!(max_expression_nesting_estimate(&templates), 5000);
        let elements = format!("return {}x{};", "<a>".repeat(5000), "</a>".repeat(5000));
        assert_eq!(max_expression_nesting_estimate(&elements), 5000);
        // Siblings, self-closing tags and fragments do not accumulate.
        let flat = "return <><a/><b></b><c x=\"1\" /></>;";
        assert_eq!(max_expression_nesting_estimate(flat), 2);
        // An open `<` counts while unclosed (#11769) and `>` closes it, so a
        // comparison adds one transient level and generics add their depth.
        assert_eq!(max_expression_nesting_estimate("if (a <b && c > d) {}"), 2);
        assert_eq!(
            max_expression_nesting_estimate("const x = f<Array<T>>(y);"),
            2
        );
        // Plain TypeScript: `<T>value` is an assertion, never an element (it
        // adds one transient angle level that `>` closes, never a JSX level).
        assert_eq!(
            max_expression_nesting_estimate_with("x = <T>y; z = <U>w;", false),
            1
        );
        for file in ["deep.ts", "deep.tsx", "deep.js"] {
            let reason = parse_on_worker(Path::new(file), &templates, |_, _, _| ());
            assert!(
                reason.is_err_and(|r| r.contains("expression_nesting_budget")),
                "{file}"
            );
        }
        let jsx_refused = parse_on_worker(
            Path::new("deep.tsx"),
            &format!(
                "export const v = {}x{};",
                "<a>".repeat(5000),
                "</a>".repeat(5000)
            ),
            |_, _, _| (),
        );
        assert!(jsx_refused.is_err_and(|r| r.contains("expression_nesting_budget")));
    }

    #[test]
    fn nesting_budget_refuses_over_budget_source_with_typed_reason() {
        // Balanced at budget+1: the oxc worker would parse this cleanly, so
        // any non-`None` reason can only come from the pre-parse budget.
        let source = format!(
            "const over = {}v{};",
            "(".repeat(PARSE_NESTING_BUDGET + 1),
            ")".repeat(PARSE_NESTING_BUDGET + 1)
        );
        let reason = parse_error_reason(Path::new("src/over.ts"), &source);
        let reason = reason.unwrap_or_default();
        assert!(
            reason.contains("expression_nesting_budget"),
            "typed budget refusal expected, got: {reason}"
        );
        assert!(
            !reason.contains("parser error"),
            "budget refusal must not masquerade as a parser error: {reason}"
        );
    }

    #[test]
    fn nesting_budget_admits_deep_legitimate_source_on_worker_stack() {
        // Exactly at budget: admitted, and the large-stack worker parses it
        // cleanly (None) — the parse previously ran on the caller's stack,
        // where far shallower nesting already aborted (issue #4101).
        let source = nested(PARSE_NESTING_BUDGET, "value");
        assert_eq!(
            max_expression_nesting_estimate(&source),
            PARSE_NESTING_BUDGET
        );
        let reason = parse_error_reason(Path::new("src/deep.ts"), &source);
        assert_eq!(reason, None);
    }

    #[test]
    fn previously_aborting_depth_now_parses_and_classifies() {
        // Depth 400 aborted the unguarded Windows-debug main-thread parse
        // (issue #4101 red witness). It must now parse cleanly.
        let source = format!(
            "export function deep(v: number): number {{\n  const r = {};\n  return r;\n}}\n",
            nested(400, "v")
        );
        let reason = parse_error_reason(Path::new("src/deep.ts"), &source);
        assert_eq!(reason, None);
        let owners = extract_owners(Path::new("src/deep.ts"), &source);
        assert_eq!(owners.len(), 1);
        assert_eq!(
            owners.first().map(|owner| owner.name.as_str()),
            Some("deep")
        );
    }

    #[test]
    fn unsupported_syntax_finding_carries_budget_reason() {
        let reason = nesting_budget_refusal_reason(PARSE_NESTING_BUDGET + 1);
        let limit = TypeScriptParseLimit {
            file: PathBuf::from("src/over.ts"),
            reason: reason.clone(),
        };
        let finding =
            unsupported_syntax_finding(Path::new("src/over.ts"), 2, "  const over = ...;", &limit);
        assert!(matches!(finding.class, ExposureClass::StaticUnknown));
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::UnsupportedSyntax)
        );
        let joined = finding.evidence.join("\n");
        assert!(
            joined.contains(
                "static_limit unsupported_syntax: static limit expression_nesting_budget"
            ),
            "budget reason must ride the unsupported_syntax evidence line: {joined}"
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains(reason.as_str()))
        );
    }
}
