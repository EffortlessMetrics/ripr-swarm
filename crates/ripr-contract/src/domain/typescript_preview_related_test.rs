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
///
/// Producer `typescript_oracle_evidence_ref` may name a different family-
/// relevant row than raw strength ranking, but it must not displace a related
/// test that already matches the missing-check family at equal strength
/// (RIPR-SPEC-0224 / #5505). The producer currently emits the last strongest
/// family-matching assertion, which is often a later same-file row that does
/// not observe the missing boundary better than the first listed match.
pub fn typescript_script_preview_related_test(finding: &Finding) -> Option<&RelatedTest> {
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
    let family = &finding.probe.family;
    let family_ranked = strongest_family_relevant_related_test(tests, family);
    if let Some(row) = related_test_for_typescript_oracle_evidence_ref(finding)
        && oracle_ref_row_matches_missing_check_family(finding, row, family_ranked)
    {
        return Some(row);
    }
    family_ranked.or_else(|| tests.first())
}

/// Strongest related-test row whose collapsed oracle can observe `family`.
/// Equal rank keeps the first listed row (#11228).
fn strongest_family_relevant_related_test<'a>(
    tests: &'a [RelatedTest],
    family: &ProbeFamily,
) -> Option<&'a RelatedTest> {
    let mut best: Option<&RelatedTest> = None;
    for test in tests {
        if !typescript_row_carries_family_oracle(test, family) {
            continue;
        }
        if best.is_none_or(|current| test.oracle_strength.rank() > current.oracle_strength.rank()) {
            best = Some(test);
        }
    }
    best
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

/// Oracle-ref may override prior ranking only when the named row matches the
/// missing-check family and does not merely reorder equal-strength family
/// matches onto a later same-file test.
fn oracle_ref_row_matches_missing_check_family(
    finding: &Finding,
    row: &RelatedTest,
    family_ranked: Option<&RelatedTest>,
) -> bool {
    if !typescript_row_carries_family_oracle(row, &finding.probe.family) {
        return false;
    }
    match family_ranked {
        None => true,
        Some(best) if std::ptr::eq(best, row) => true,
        Some(best)
            if row.oracle_strength.rank() < best.oracle_strength.rank()
                && oracle_ref_observes_missing_check(finding) =>
        {
            // Producer named a weaker family-relevant row that actually
            // supplies the missing-check / gap evidence.
            true
        }
        Some(_) => false,
    }
}

fn oracle_ref_observes_missing_check(finding: &Finding) -> bool {
    let Some(missing) = finding
        .activation
        .missing_discriminators
        .first()
        .map(|fact| fact.value.as_str())
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let Some(observed) = finding.evidence.iter().find_map(|line| {
        line.strip_prefix("typescript_oracle_observed: ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }) else {
        return false;
    };
    observed_call_matches_missing_check(observed, missing)
}

/// Cheap evidence-only check: the producer-observed call is a plausible
/// observer of the missing equality/boundary, not a far-from-boundary input.
fn observed_call_matches_missing_check(observed: &str, missing: &str) -> bool {
    let Some((_, args_text)) = observed.split_once('(') else {
        return observed.contains(missing);
    };
    let Some(args_text) = args_text.rsplit_once(')').map(|(args, _)| args) else {
        return observed.contains(missing);
    };
    let args: Vec<&str> = args_text
        .split(',')
        .map(str::trim)
        .filter(|arg| !arg.is_empty())
        .collect();
    if missing.contains("==") {
        let parts: Vec<&str> = missing.split("==").map(str::trim).collect();
        if parts.len() == 2 {
            if parts[0] == parts[1] {
                return true;
            }
            if args.len() >= 2 && args[0] == args[1] {
                return true;
            }
            if args.iter().any(|arg| *arg == parts[0] || *arg == parts[1]) {
                // A named constant or literal from the missing check appears
                // in the observed call (`discountedTotal(DISCOUNT_THRESHOLD)`).
                return true;
            }
        }
    }
    args.iter().any(|arg| missing.contains(arg)) || observed.contains(missing)
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

#[cfg(test)]
mod observed_call_matches_missing_check_tests {
    use super::observed_call_matches_missing_check;

    #[test]
    fn far_from_boundary_input_does_not_match_equality_missing_check() {
        assert!(!observed_call_matches_missing_check(
            "applyDiscount(10000, 100)",
            "amount == threshold"
        ));
        assert!(!observed_call_matches_missing_check(
            "discountedTotal(20000)",
            "amount == DISCOUNT_THRESHOLD"
        ));
    }

    #[test]
    fn equal_literal_args_match_equality_missing_check() {
        assert!(observed_call_matches_missing_check(
            "applyDiscount(100, 100)",
            "amount == threshold"
        ));
    }

    #[test]
    fn named_constant_in_call_matches_missing_check() {
        assert!(observed_call_matches_missing_check(
            "discountedTotal(DISCOUNT_THRESHOLD)",
            "amount == DISCOUNT_THRESHOLD"
        ));
    }
}
