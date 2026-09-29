//! Producer-owned numeric diagnostic origins for current Rust findings (#4464).
//!
//! Parser byte intervals travel with the seeded probe through final-ID
//! assignment. Exact geometry is admitted only when the captured analysis
//! bytes, cached file facts, candidate-current disposition, and the parser
//! span all agree. Projection later selects these numbers; it does not search
//! source text or reread the filesystem.
//!
//! Findings without a parser span keep the existing saved-line heuristic.
//! Non-current and refused Rust records carry a bounded zero-width range so a
//! deleted expression cannot paint a current line.

use super::facts::rust_source_text;
use super::rust_index::RustIndex;
use crate::domain::Finding;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Matches [`crate::lsp::position::MAX_LINE_SPAN_WIDTH`]. Analysis cannot
/// import the LSP adapter; position tests pin the two constants together.
pub(crate) const ORIGIN_MAX_SPAN_WIDTH: u32 = 120;

/// Parser-owned start inside the captured source. End is derived from the
/// finding expression only when that expression is a contiguous slice at this
/// offset (after the same leading trim the probe producer already applied).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParserByteSpan {
    pub start_byte: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginKind {
    Exact,
    CoarseZeroWidth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EncodedSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EncodedOrigin {
    /// 0-based LSP line.
    pub line: u32,
    pub utf8: EncodedSpan,
    pub utf16: EncodedSpan,
    pub utf32: EncodedSpan,
    pub kind: OriginKind,
}

impl EncodedOrigin {
    pub(crate) fn for_utf8(&self) -> EncodedSpan {
        self.utf8
    }

    pub(crate) fn for_utf16(&self) -> EncodedSpan {
        self.utf16
    }

    pub(crate) fn for_utf32(&self) -> EncodedSpan {
        self.utf32
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RustDiagnosticOrigins {
    records: BTreeMap<String, EncodedOrigin>,
}

impl RustDiagnosticOrigins {
    pub(crate) fn get(&self, probe_id: &str) -> Option<&EncodedOrigin> {
        self.records.get(probe_id)
    }

    pub(crate) fn for_finding(&self, finding: &Finding) -> Option<&EncodedOrigin> {
        self.get(&finding.id)
            .or_else(|| self.get(finding.probe.id.0.as_str()))
    }

    pub(crate) fn insert(&mut self, probe_id: String, origin: EncodedOrigin) {
        self.records.insert(probe_id, origin);
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Inputs already consumed by the Rust adapter for one analysis pass.
pub(crate) struct OriginBuildContext<'a> {
    pub root: &'a Path,
    pub loaded_files: &'a [(PathBuf, Vec<u8>)],
    pub index: &'a RustIndex,
    pub parser_spans: &'a BTreeMap<String, ParserByteSpan>,
}

pub(crate) fn origins_for_rust_findings(
    findings: &[Finding],
    context: &OriginBuildContext<'_>,
) -> RustDiagnosticOrigins {
    let mut captured = BTreeMap::<PathBuf, String>::new();
    for (path, bytes) in context.loaded_files {
        captured.insert(path.clone(), rust_source_text(bytes).text.into_owned());
    }

    let mut origins = RustDiagnosticOrigins::default();
    for finding in findings {
        let Some(relative) = relative_finding_path(context.root, finding) else {
            if should_record_without_path(finding, context.parser_spans) {
                origins.insert(finding.id.clone(), missing_input_origin());
            }
            continue;
        };
        let captured_source = captured.get(&relative).map(String::as_str);
        let facts = context.index.files.get(&relative);
        let span = parser_span_for_finding(finding, context.parser_spans);
        if let Some(origin) = origin_for_finding(finding, span, captured_source, facts) {
            origins.insert(finding.id.clone(), origin);
        }
    }
    origins
}

fn parser_span_for_finding(
    finding: &Finding,
    parser_spans: &BTreeMap<String, ParserByteSpan>,
) -> Option<ParserByteSpan> {
    parser_spans
        .get(finding.id.as_str())
        .copied()
        .or_else(|| parser_spans.get(finding.probe.id.0.as_str()).copied())
}

fn should_record_without_path(
    finding: &Finding,
    parser_spans: &BTreeMap<String, ParserByteSpan>,
) -> bool {
    !finding.source_currentness.permits_candidate_action()
        || parser_span_for_finding(finding, parser_spans).is_some()
}

fn relative_finding_path<'a>(root: &Path, finding: &'a Finding) -> Option<PathBuf> {
    let file = &finding.probe.location.file;
    if let Ok(relative) = file.strip_prefix(root) {
        return Some(relative.to_path_buf());
    }
    if file.is_relative() {
        return Some(file.clone());
    }
    None
}

fn origin_for_finding(
    finding: &Finding,
    span: Option<ParserByteSpan>,
    captured_source: Option<&str>,
    facts: Option<&super::rust_index::FileFacts>,
) -> Option<EncodedOrigin> {
    let Some(captured) = captured_source else {
        return if span.is_some() || !finding.source_currentness.permits_candidate_action() {
            Some(missing_input_origin())
        } else {
            None
        };
    };
    if !finding.source_currentness.permits_candidate_action() {
        return Some(coarse_on_line(captured, finding.probe.location.line));
    }
    let Some(span) = span else {
        return None;
    };
    if has_standalone_cr(captured) {
        return Some(missing_input_origin());
    }
    if facts.is_some_and(|facts| facts.used_lexical_fallback) {
        return Some(coarse_on_line(captured, finding.probe.location.line));
    }
    if facts.is_none_or(|facts| facts.source != captured) {
        return Some(coarse_on_line(captured, finding.probe.location.line));
    }
    match exact_origin(captured, span, finding) {
        Some(origin) => Some(origin),
        None => Some(coarse_on_line(captured, finding.probe.location.line)),
    }
}

fn missing_input_origin() -> EncodedOrigin {
    EncodedOrigin {
        line: 0,
        utf8: EncodedSpan { start: 0, end: 0 },
        utf16: EncodedSpan { start: 0, end: 0 },
        utf32: EncodedSpan { start: 0, end: 0 },
        kind: OriginKind::CoarseZeroWidth,
    }
}

fn coarse_on_line(source: &str, one_based_line: usize) -> EncodedOrigin {
    let lines = lsp_lines(source);
    let line = one_based_line.saturating_sub(1);
    if line < lines.len() {
        EncodedOrigin {
            line: line as u32,
            utf8: EncodedSpan { start: 0, end: 0 },
            utf16: EncodedSpan { start: 0, end: 0 },
            utf32: EncodedSpan { start: 0, end: 0 },
            kind: OriginKind::CoarseZeroWidth,
        }
    } else {
        missing_input_origin()
    }
}

fn exact_origin(source: &str, span: ParserByteSpan, finding: &Finding) -> Option<EncodedOrigin> {
    if finding.probe.expression.is_empty() {
        return None;
    }
    let start = trimmed_expression_start(source, span.start_byte, &finding.probe.expression)?;
    let end = start.checked_add(finding.probe.expression.len())?;
    if end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return None;
    }
    if source.get(start..end) != Some(finding.probe.expression.as_str()) {
        return None;
    }
    let lines = lsp_lines(source);
    let (line_index, line_span) = line_containing(start, &lines)?;
    if end > line_span.end {
        return None;
    }
    if finding.probe.location.line.saturating_sub(1) != line_index {
        return None;
    }
    let prefix = source.get(line_span.start..start)?;
    let expression = finding.probe.expression.as_str();
    Some(EncodedOrigin {
        line: line_index as u32,
        utf8: encoded_span(prefix, expression, Encoding::Utf8),
        utf16: encoded_span(prefix, expression, Encoding::Utf16),
        utf32: encoded_span(prefix, expression, Encoding::Utf32),
        kind: OriginKind::Exact,
    })
}

fn trimmed_expression_start(source: &str, start_byte: usize, expression: &str) -> Option<usize> {
    let rest = source.get(start_byte..)?;
    if !source.is_char_boundary(start_byte) {
        return None;
    }
    let skipped = rest.len() - rest.trim_start().len();
    let exact_start = start_byte.checked_add(skipped)?;
    source.get(exact_start..exact_start.checked_add(expression.len())?)?;
    Some(exact_start)
}

#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Utf16,
    Utf32,
}

fn encoded_span(prefix: &str, expression: &str, encoding: Encoding) -> EncodedSpan {
    let start = encoding_width(prefix, encoding);
    let width = capped_width(expression, encoding);
    EncodedSpan {
        start,
        end: start.saturating_add(width),
    }
}

fn encoding_width(text: &str, encoding: Encoding) -> u32 {
    match encoding {
        Encoding::Utf8 => text.len() as u32,
        Encoding::Utf32 => text.chars().count() as u32,
        Encoding::Utf16 => text.chars().map(|ch| ch.len_utf16() as u32).sum(),
    }
}

fn capped_width(expression: &str, encoding: Encoding) -> u32 {
    let mut width = 0u32;
    for ch in expression.chars() {
        let extra = match encoding {
            Encoding::Utf8 => ch.len_utf8() as u32,
            Encoding::Utf16 => ch.len_utf16() as u32,
            Encoding::Utf32 => 1,
        };
        if width.saturating_add(extra) > ORIGIN_MAX_SPAN_WIDTH {
            break;
        }
        width = width.saturating_add(extra);
    }
    width
}

#[derive(Clone, Copy)]
struct LineSpan {
    start: usize,
    end: usize,
}

fn has_standalone_cr(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 2;
            } else {
                return true;
            }
        } else {
            index += 1;
        }
    }
    false
}

fn lsp_lines(source: &str) -> Vec<LineSpan> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            lines.push(LineSpan { start, end: index });
            index += 2;
            start = index;
        } else if bytes[index] == b'\n' {
            lines.push(LineSpan { start, end: index });
            index += 1;
            start = index;
        } else {
            index += 1;
        }
    }
    if start < bytes.len() || bytes.is_empty() || source.ends_with('\n') || source.ends_with("\r\n")
    {
        lines.push(LineSpan {
            start,
            end: bytes.len(),
        });
    }
    if lines.is_empty() {
        lines.push(LineSpan { start: 0, end: 0 });
    }
    lines
}

fn line_containing(offset: usize, lines: &[LineSpan]) -> Option<(usize, LineSpan)> {
    lines
        .iter()
        .enumerate()
        .find(|(_, line)| offset >= line.start && offset <= line.end)
        .map(|(index, line)| (index, *line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::FileFacts;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, ExposureClass, Probe, ProbeFamily, ProbeId,
        RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation, StageEvidence, StageState,
    };
    use std::path::PathBuf;

    const PREDICATE: &str = "montant_é > discount_threshold";

    fn stage() -> StageEvidence {
        StageEvidence::new(StageState::Unknown, Confidence::Unknown, "origin test")
    }

    fn finding_on(
        id: &str,
        relative: &str,
        line: usize,
        expression: &str,
        currentness: SourceCurrentness,
    ) -> Finding {
        Finding {
            id: id.to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId(id.to_string()),
                location: SourceLocation::new(PathBuf::from(relative), line, 1),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: Some(expression.to_string()),
                expression: expression.to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::NoStaticPath,
            ripr: RiprEvidence {
                reach: stage(),
                infect: stage(),
                propagate: stage(),
                reveal: RevealEvidence {
                    observe: stage(),
                    discriminate: stage(),
                },
            },
            confidence: 0.0,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests: Vec::new(),
            recommended_next_step: None,
            language: Some(crate::domain::LanguageId::Rust),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: currentness,
        }
    }

    fn facts_for(source: &str) -> FileFacts {
        FileFacts {
            path: PathBuf::from("src/lib.rs"),
            source: source.to_string(),
            ..FileFacts::default()
        }
    }

    fn index_with(source: &str) -> RustIndex {
        let mut index = RustIndex::default();
        index
            .files
            .insert(PathBuf::from("src/lib.rs"), facts_for(source));
        index
    }

    fn origins_for(
        finding: &Finding,
        source: &str,
        span: Option<ParserByteSpan>,
        facts_source: Option<&str>,
    ) -> RustDiagnosticOrigins {
        let mut index = RustIndex::default();
        if let Some(facts_source) = facts_source {
            index
                .files
                .insert(PathBuf::from("src/lib.rs"), facts_for(facts_source));
        } else {
            index = index_with(source);
        }
        let mut spans = BTreeMap::new();
        if let Some(span) = span {
            spans.insert(finding.id.clone(), span);
        }
        let loaded = vec![(PathBuf::from("src/lib.rs"), source.as_bytes().to_vec())];
        origins_for_rust_findings(
            std::slice::from_ref(finding),
            &OriginBuildContext {
                root: Path::new("/workspace"),
                loaded_files: &loaded,
                index: &index,
                parser_spans: &spans,
            },
        )
    }

    fn covered<'a>(source: &'a str, origin: &EncodedOrigin) -> &'a str {
        let line = lsp_lines(source)
            .into_iter()
            .nth(origin.line as usize)
            .expect("line");
        let line_text = &source[line.start..line.end];
        let start = origin.utf8.start as usize;
        let end = origin.utf8.end as usize;
        &line_text[start..end]
    }

    #[test]
    fn exact_origin_skips_indent_and_if_keyword() {
        let source =
            "fn price() {\n    if montant_é > discount_threshold {\n        true\n    }\n}\n";
        let start = source.find(PREDICATE).expect("predicate");
        let finding = finding_on(
            "probe:first",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(
            &finding,
            source,
            Some(ParserByteSpan { start_byte: start }),
            None,
        );
        let origin = origins.get("probe:first").expect("origin");
        assert_eq!(origin.kind, OriginKind::Exact);
        assert_eq!(origin.line, 1);
        assert_eq!(covered(source, origin), PREDICATE);
        assert_eq!(origin.utf16.start, 7);
        assert_eq!(
            origin.utf16.end,
            origin.utf16.start + encoding_width(PREDICATE, Encoding::Utf16)
        );
        assert_eq!(origin.utf32.start, 7);
        assert_eq!(origin.utf8.start, "    if ".len() as u32);
    }

    #[test]
    fn first_substring_on_the_line_is_not_the_producer_span() {
        let source = concat!(
            "fn price() {\n",
            "    let _ = \"montant_é > discount_threshold\"; if montant_é > discount_threshold {\n",
            "        true\n",
            "    }\n",
            "}\n"
        );
        let decoy = source.find(PREDICATE).expect("decoy");
        let producer = source[decoy + 1..]
            .find(PREDICATE)
            .map(|offset| decoy + 1 + offset)
            .expect("producer");
        assert!(decoy < producer);
        let finding = finding_on(
            "probe:second",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(
            &finding,
            source,
            Some(ParserByteSpan {
                start_byte: producer,
            }),
            None,
        );
        let origin = origins.get("probe:second").expect("origin");
        assert_eq!(origin.kind, OriginKind::Exact);
        assert_eq!(covered(source, origin), PREDICATE);
        let line = source.lines().nth(1).expect("line");
        let first_match = line.find(PREDICATE).expect("first match");
        assert_ne!(origin.utf8.start as usize, first_match);
        assert_eq!(
            origin.utf8.start as usize,
            producer - (source.find('\n').expect("nl") + 1)
        );
    }

    #[test]
    fn cached_facts_from_another_source_cannot_authorize_captured_input() {
        let captured = "fn a() {\n    if montant_é > discount_threshold { true }\n}\n";
        let cached = "fn b() {\n    if montant_é > discount_threshold { true }\n    if montant_é > discount_threshold { false }\n}\n";
        let start = cached.rfind(PREDICATE).expect("b offset");
        let finding = finding_on(
            "probe:a",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(
            &finding,
            captured,
            Some(ParserByteSpan { start_byte: start }),
            Some(cached),
        );
        let origin = origins.get("probe:a").expect("origin");
        assert_eq!(origin.kind, OriginKind::CoarseZeroWidth);
        assert_eq!(origin.utf8.start, 0);
        assert_eq!(origin.utf8.end, 0);
        assert_eq!(origin.utf16.end, 0);
        assert_eq!(origin.utf32.end, 0);
    }

    #[test]
    fn candidate_current_without_parser_span_keeps_heuristic() {
        let source = "fn price() {\n    if montant_é > discount_threshold { true }\n}\n";
        let finding = finding_on(
            "probe:line",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(&finding, source, None, None);
        assert!(origins.get("probe:line").is_none());
        assert!(origins.is_empty());
    }

    #[test]
    fn base_deleted_is_zero_width_on_the_current_line() {
        let source = "fn price() {\n    true\n}\n";
        let finding = finding_on(
            "probe:deleted",
            "src/lib.rs",
            2,
            "very_long_call(montant_é, discount_threshold, other_argument)",
            SourceCurrentness::BaseDeleted,
        );
        let origins = origins_for(&finding, source, None, None);
        let origin = origins.get("probe:deleted").expect("origin");
        assert_eq!(origin.kind, OriginKind::CoarseZeroWidth);
        assert_eq!(origin.line, 1);
        assert_eq!(origin.utf8.end, origin.utf8.start);
        assert_eq!(origin.utf16.end, 0);
        assert_eq!(origin.utf32.end, 0);
    }

    #[test]
    fn standalone_cr_refuses_precision() {
        let source = "fn price() {\r    if montant_é > discount_threshold { true }\r}\r";
        let start = source.find(PREDICATE).expect("predicate");
        let finding = finding_on(
            "probe:cr",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(
            &finding,
            source,
            Some(ParserByteSpan { start_byte: start }),
            None,
        );
        let origin = origins.get("probe:cr").expect("origin");
        assert_eq!(origin.kind, OriginKind::CoarseZeroWidth);
        assert_eq!(origin.line, 0);
    }

    #[test]
    fn crlf_admits_exact_geometry() {
        let source = "fn price() {\r\n    if montant_é > discount_threshold {\r\n        true\r\n    }\r\n}\r\n";
        let start = source.find(PREDICATE).expect("predicate");
        let finding = finding_on(
            "probe:crlf",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(
            &finding,
            source,
            Some(ParserByteSpan { start_byte: start }),
            None,
        );
        let origin = origins.get("probe:crlf").expect("origin");
        assert_eq!(origin.kind, OriginKind::Exact);
        assert_eq!(origin.line, 1);
        assert_eq!(covered(source, origin), PREDICATE);
    }

    #[test]
    fn mid_scalar_start_stays_coarse() {
        let source = "fn price() {\n    if montant_é > discount_threshold { true }\n}\n";
        let start = source.find("é").expect("accent") + 1;
        assert!(!source.is_char_boundary(start));
        let finding = finding_on(
            "probe:mid",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(
            &finding,
            source,
            Some(ParserByteSpan { start_byte: start }),
            None,
        );
        let origin = origins.get("probe:mid").expect("origin");
        assert_eq!(origin.kind, OriginKind::CoarseZeroWidth);
    }

    #[test]
    fn tab_cjk_astral_and_combining_prefixes_count_all_encodings() {
        let prefix = "\tlet 日本語🎉e\u{0301} = 1; if ";
        let source = format!("fn price() {{\n{prefix}{PREDICATE} {{\n        true\n    }}\n}}\n");
        let start = source.find(PREDICATE).expect("predicate");
        let finding = finding_on(
            "probe:prefix",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let origins = origins_for(
            &finding,
            &source,
            Some(ParserByteSpan { start_byte: start }),
            None,
        );
        let origin = origins.get("probe:prefix").expect("origin");
        assert_eq!(origin.kind, OriginKind::Exact);
        assert_eq!(covered(&source, origin), PREDICATE);
        assert_eq!(origin.utf8.start, encoding_width(prefix, Encoding::Utf8));
        assert_eq!(origin.utf16.start, encoding_width(prefix, Encoding::Utf16));
        assert_eq!(origin.utf32.start, encoding_width(prefix, Encoding::Utf32));
        assert_ne!(origin.utf8.start, origin.utf16.start);
        assert_ne!(origin.utf16.start, origin.utf32.start);
    }

    #[test]
    fn missing_loaded_source_uses_line_zero() {
        let finding = finding_on(
            "probe:missing",
            "src/lib.rs",
            2,
            PREDICATE,
            SourceCurrentness::CandidateCurrent,
        );
        let mut spans = BTreeMap::new();
        spans.insert(finding.id.clone(), ParserByteSpan { start_byte: 4 });
        let index = RustIndex::default();
        let origins = origins_for_rust_findings(
            std::slice::from_ref(&finding),
            &OriginBuildContext {
                root: Path::new("/workspace"),
                loaded_files: &[],
                index: &index,
                parser_spans: &spans,
            },
        );
        let origin = origins.get("probe:missing").expect("origin");
        assert_eq!(origin, &missing_input_origin());
    }

    #[test]
    fn scalar_cap_does_not_split_an_astral_character() {
        let expression = format!("{}{}", "a".repeat(119), "🎉");
        let width = capped_width(&expression, Encoding::Utf16);
        assert_eq!(width, 119);
        let utf8 = capped_width(&expression, Encoding::Utf8);
        assert_eq!(utf8, 119);
    }
}
