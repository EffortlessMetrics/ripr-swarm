//! Parsing utilities for the TypeScript preview adapter.

use super::*;

pub(crate) fn parse_error_reason(file: &Path, source: &str) -> Option<String> {
    if let Some(reason) = nesting_budget_trip(source) {
        return Some(reason);
    }
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type_for(file)).parse();
    if ret.errors.is_empty() {
        None
    } else {
        Some(format!("{} parser error(s)", ret.errors.len()))
    }
}

/// Maximum bracket-nesting depth the oxc parser may be asked to handle
/// (issue #4101). Past this depth a recursive-descent parse can overflow
/// the thread stack — an abort, not a catchable panic — so the adapter
/// declines the file with a typed budget reason instead. 128 sits 3x below
/// the lowest observed abort threshold (depth 400, Windows debug
/// main-thread stack) and far above legitimate nesting.
pub(crate) const MAX_TS_PARSE_NESTING_DEPTH: usize = 128;

/// Stable prefix of the nesting-budget trip reason. Consumers that render
/// trip-specific guidance match on this instead of re-deriving the trip.
pub(crate) const TS_PARSE_BUDGET_REASON_PREFIX: &str = "typescript parse budget exceeded";

/// Budget trip for a TypeScript/JavaScript source: `Some(reason)` when the
/// bracket-nesting depth exceeds [`MAX_TS_PARSE_NESTING_DEPTH`].
/// Single pass, comment-aware (`//` and `/* */` skipped); string contents
/// are counted literally — over-counting a bracket inside a string only
/// declines the file (fail-closed), while skipping strings risks missing
/// real nesting (unsound).
pub(crate) fn nesting_budget_trip(source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'/' && index + 1 < bytes.len() {
            if bytes[index + 1] == b'/' {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            if bytes[index + 1] == b'*' {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index += 2;
                continue;
            }
        }
        match byte {
            b'(' | b'[' | b'{' => {
                depth += 1;
                max_depth = max_depth.max(depth);
                if max_depth > MAX_TS_PARSE_NESTING_DEPTH {
                    // Trip at the first excess: the file is declined without
                    // parsing, so the reported fact is the exceeded budget,
                    // not the file's full depth.
                    return Some(format!(
                        "{TS_PARSE_BUDGET_REASON_PREFIX}: nesting depth exceeded {MAX_TS_PARSE_NESTING_DEPTH}"
                    ));
                }
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        index += 1;
    }
    None
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
            "gap_state: static_limitation".to_string(),
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
