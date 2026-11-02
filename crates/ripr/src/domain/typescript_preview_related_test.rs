//! Family-aware TypeScript/JavaScript preview related-test selection (#5505).
//!
//! One row identity for the check headline, preview card, packet projection,
//! JSON oracle summary, verify-command inference, and miss explanation.

use super::{
    Finding, LanguageId, OracleKind, OracleStrength, ProbeFamily, RelatedTest,
    typescript_oracle_seam::ts_oracle_kind_matches_seam,
};

/// The one TypeScript/JavaScript preview related-test row shared by every
/// single-row projection surface (#11228 tie-break, #5505 family evidence).
pub(crate) fn typescript_script_preview_related_test(finding: &Finding) -> Option<&RelatedTest> {
    if !matches!(
        finding.language,
        Some(LanguageId::TypeScript | LanguageId::JavaScript)
    ) {
        return None;
    }
    let tests = &finding.related_tests;
    if tests.is_empty() {
        return None;
    }
    if let Some(row) = related_test_for_typescript_oracle_evidence_ref(finding) {
        return Some(row);
    }
    let family = &finding.probe.family;
    let mut best: Option<&RelatedTest> = None;
    for test in tests {
        if !typescript_row_carries_family_oracle(test, family) {
            continue;
        }
        if best.is_none_or(|current| test.oracle_strength.rank() > current.oracle_strength.rank()) {
            best = Some(test);
        }
    }
    best.or_else(|| tests.first())
}

fn related_test_for_typescript_oracle_evidence_ref(finding: &Finding) -> Option<&RelatedTest> {
    let reference = finding.evidence.iter().find_map(|line| {
        line.strip_prefix("typescript_oracle_evidence_ref: ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })?;
    let (file, line_text) = reference.rsplit_once(':')?;
    let evidence_line = line_text.parse::<usize>().ok()?;
    let file_norm = file.replace('\\', "/");
    let mut best: Option<&RelatedTest> = None;
    for test in finding.related_tests.iter() {
        if test.file.to_string_lossy().replace('\\', "/") != file_norm || test.line > evidence_line
        {
            continue;
        }
        if best.is_none_or(|current| test.line > current.line) {
            best = Some(test);
        }
    }
    best
}

fn typescript_row_carries_family_oracle(test: &RelatedTest, family: &ProbeFamily) -> bool {
    if test.oracle_strength == OracleStrength::None {
        return false;
    }
    if test.oracle_strength == OracleStrength::Unknown && test.oracle_kind == OracleKind::Unknown {
        return false;
    }
    ts_oracle_kind_matches_seam(&test.oracle_kind, family)
}
