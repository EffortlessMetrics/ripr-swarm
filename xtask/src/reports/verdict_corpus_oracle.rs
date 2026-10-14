//! Oracle-strength accuracy of the Rust verdict corpus (RIPR-SPEC-0219).
//!
//! The line verdict cannot see an oracle error that leaves the verdict
//! unchanged: a test whose `assert_ne!` ripr reads as an `exact_value` /
//! strong oracle still scores `ideal` when the finding stays a gap. A case
//! may therefore label the oracle kind and strength a spec adopts for its
//! related test (`expected_oracle`). The harness reads what ripr reports for
//! that test on the anchored findings and scores the pair apart from the
//! verdict: `match`, `overclaim`, `underclaim`, `kind_mismatch` or
//! `unobserved`. These counts never enter the verdict rates.

use super::coverage::{LEDGER_FILE, Ledger, parse_example_id};
use super::{CORPUS_DIR, Case, Corpus, Ratio, ratio};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// ripr's `OracleKind` names (`crates/ripr/src/domain/evidence.rs`).
pub(crate) const ORACLE_KINDS: [&str; 10] = [
    "exact_value",
    "exact_error_variant",
    "whole_object_equality",
    "snapshot",
    "relational_check",
    "broad_error",
    "smoke_only",
    "mock_expectation",
    "guarded_result_match",
    "unknown",
];

/// The strengths a label may name. ripr also reports `unknown` and `none`,
/// which rank below `smoke` when observed; a spec never adopts them.
pub(crate) const EXPECTED_STRENGTHS: [&str; 4] = ["strong", "medium", "weak", "smoke"];

/// The oracle a spec adopts for the case's related test.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExpectedOracle {
    pub(crate) kind: String,
    pub(crate) strength: String,
    /// The related test's name as ripr reports it. Omitted, every related
    /// test on the anchored findings counts, so omit it only when the
    /// subject has one asserting test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) test: Option<String>,
    /// The spec acceptance example the label measures, `RIPR-SPEC-NNNN#K`.
    /// The example states the value or, where it states only the verdict,
    /// the spec's rules fix it (RIPR-SPEC-0219 records each such reading).
    pub(crate) spec: String,
}

/// The ceiling and floor `verdict-corpus check` protects, in the `[oracle]`
/// table of `spec-coverage.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OracleGate {
    pub(crate) match_floor: usize,
    pub(crate) overclaim_ceiling: usize,
}

/// smoke < weak < medium < strong; ripr's `unknown` and `none` sit below
/// smoke, as in its own `OracleStrength::rank`.
pub(crate) fn strength_rank(strength: &str) -> u8 {
    match strength {
        "strong" => 5,
        "medium" => 4,
        "weak" => 3,
        "smoke" => 2,
        "unknown" => 1,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OracleOutcome {
    Match,
    Overclaim,
    Underclaim,
    KindMismatch,
    Unobserved,
}

impl OracleOutcome {
    pub(crate) const ALL: [OracleOutcome; 5] = [
        Self::Match,
        Self::Overclaim,
        Self::Underclaim,
        Self::KindMismatch,
        Self::Unobserved,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::Overclaim => "overclaim",
            Self::Underclaim => "underclaim",
            Self::KindMismatch => "kind_mismatch",
            Self::Unobserved => "unobserved",
        }
    }
}

/// One related-test entry ripr reported on an anchored finding.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObservedOracle {
    pub(crate) test: String,
    pub(crate) kind: String,
    pub(crate) strength: String,
}

/// A labeled case's oracle score. `observed_*` is the claim a consumer sees
/// (the strongest entry); `observed_all` lists every distinct entry. Rows
/// round-trip through `expected/rows/<case>.json`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OracleRow {
    pub(crate) expected: ExpectedOracle,
    pub(crate) observed_kind: Option<String>,
    pub(crate) observed_strength: Option<String>,
    pub(crate) observed_all: Vec<ObservedOracle>,
    pub(crate) outcome: OracleOutcome,
}

/// Whether a reported test name is the labeled one. ripr reports the bare
/// function name; a label may spell it with its module path.
fn names_test(reported: &str, label: &str) -> bool {
    reported == label
        || label.ends_with(&format!("::{reported}"))
        || reported.ends_with(&format!("::{label}"))
}

/// Every distinct related-test oracle on the anchored findings, for the
/// labeled test or, without one, for every related test.
pub(crate) fn observed_oracles(findings: &[&Value], test: Option<&str>) -> Vec<ObservedOracle> {
    let mut seen = BTreeSet::new();
    for finding in findings {
        for entry in finding
            .get("related_tests")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or_default();
            let name = text("name");
            if test.is_some_and(|label| !names_test(name, label)) {
                continue;
            }
            seen.insert(ObservedOracle {
                test: name.to_string(),
                kind: text("oracle_kind").to_string(),
                strength: text("oracle_strength").to_string(),
            });
        }
    }
    seen.into_iter().collect()
}

/// The claim a consumer would see: the strongest reported strength. Among
/// entries of that strength the expected kind wins when present, so a test
/// that also carries the right reading is not called a kind mismatch;
/// otherwise the first in sorted order, which keeps the report stable.
pub(crate) fn claimed<'a>(
    observed: &'a [ObservedOracle],
    expected_kind: &str,
) -> Option<&'a ObservedOracle> {
    let top = observed.iter().map(|o| strength_rank(&o.strength)).max()?;
    let strongest: Vec<&ObservedOracle> = observed
        .iter()
        .filter(|o| strength_rank(&o.strength) == top)
        .collect();
    strongest
        .iter()
        .find(|o| o.kind == expected_kind)
        .or_else(|| strongest.first())
        .copied()
}

/// Strength decides first, so a stronger reading under another kind is an
/// overclaim, not a kind mismatch: the strength is what grants credit.
pub(crate) fn classify(expected: &ExpectedOracle, claim: Option<&ObservedOracle>) -> OracleOutcome {
    let Some(claim) = claim else {
        return OracleOutcome::Unobserved;
    };
    let (seen, want) = (
        strength_rank(&claim.strength),
        strength_rank(&expected.strength),
    );
    if seen > want {
        OracleOutcome::Overclaim
    } else if seen < want {
        OracleOutcome::Underclaim
    } else if claim.kind == expected.kind {
        OracleOutcome::Match
    } else {
        OracleOutcome::KindMismatch
    }
}

/// Scores one labeled case. A named test the JSON may have elided, or a
/// name that matches more than one reported test, is refused rather than
/// scored: either would be a false `unobserved` or another test's reading.
pub(crate) fn oracle_row(
    case_id: &str,
    expected: &ExpectedOracle,
    findings: &[&Value],
) -> Result<OracleRow, String> {
    if let Some(label) = expected.test.as_deref() {
        label_is_scorable(case_id, label, findings)?;
    }
    let observed_all = observed_oracles(findings, expected.test.as_deref());
    let claim = claimed(&observed_all, &expected.kind);
    Ok(OracleRow {
        expected: expected.clone(),
        observed_kind: claim.map(|c| c.kind.clone()),
        observed_strength: claim.map(|c| c.strength.clone()),
        outcome: classify(expected, claim),
        observed_all,
    })
}

/// The labeled test must name exactly one reported test (by name, file and
/// line), and a miss only counts when every finding rendered all of its
/// related tests: `ripr check` JSON caps the list and discloses the
/// pre-cap count in `related_tests_total`.
fn label_is_scorable(case_id: &str, label: &str, findings: &[&Value]) -> Result<(), String> {
    let mut identities = BTreeSet::new();
    let mut elided = false;
    for finding in findings {
        let entries = finding
            .get("related_tests")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let total = finding
            .get("related_tests_total")
            .and_then(Value::as_u64)
            .unwrap_or(entries.len() as u64);
        elided |= total > entries.len() as u64;
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if names_test(name, label) {
                identities.insert((
                    name.to_string(),
                    entry
                        .get("file")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    entry.get("line").and_then(Value::as_u64),
                ));
            }
        }
    }
    if identities.len() > 1 {
        let found: Vec<String> = identities
            .iter()
            .map(|(name, file, line)| format!("{name} ({file}:{})", line.unwrap_or_default()))
            .collect();
        return Err(format!(
            "case `{case_id}` expected_oracle test `{label}` matches {} reported tests ({}); name one test",
            identities.len(),
            found.join(", ")
        ));
    }
    if identities.is_empty() && elided {
        return Err(format!(
            "case `{case_id}` expected_oracle test `{label}` is not among the rendered related_tests and the JSON elided some; the score would be a false `unobserved`"
        ));
    }
    Ok(())
}

/// Label law for one case's `expected_oracle`; whether its spec example
/// exists is checked with the other citations in `oracle_ledger_violations`.
pub(crate) fn expected_violations(case: &Case) -> Vec<String> {
    let mut violations = Vec::new();
    let Some(expected) = &case.expected_oracle else {
        return violations;
    };
    let id = &case.case_id;
    if !ORACLE_KINDS.contains(&expected.kind.as_str()) {
        violations.push(format!(
            "case `{id}` expected_oracle kind `{}` is not a ripr oracle kind ({})",
            expected.kind,
            ORACLE_KINDS.join(", ")
        ));
    }
    if !EXPECTED_STRENGTHS.contains(&expected.strength.as_str()) {
        violations.push(format!(
            "case `{id}` expected_oracle strength `{}` is not one of {}",
            expected.strength,
            EXPECTED_STRENGTHS.join(", ")
        ));
    }
    if expected
        .test
        .as_deref()
        .is_some_and(|test| test.trim().is_empty())
    {
        violations.push(format!(
            "case `{id}` expected_oracle test is present but empty; name the related test or remove the field"
        ));
    }
    if parse_example_id(&expected.spec).is_none() {
        violations.push(format!(
            "case `{id}` expected_oracle spec `{}` is not `RIPR-SPEC-NNNN#K`",
            expected.spec
        ));
    }
    violations
}

/// The `[oracle]` gate table and each label's spec example against the
/// specs. A corpus with labels needs the table, and the floor and ceiling
/// cannot exceed the labeled count.
pub(crate) fn oracle_ledger_violations(
    corpus: &Corpus,
    ledger: &Ledger,
    specs: &super::coverage::SpecExamples,
) -> Vec<String> {
    let mut violations = Vec::new();
    let labeled = corpus
        .cases
        .iter()
        .filter(|c| c.expected_oracle.is_some())
        .count();
    for case in &corpus.cases {
        let Some(expected) = &case.expected_oracle else {
            continue;
        };
        let Some((spec, number)) = parse_example_id(&expected.spec) else {
            continue;
        };
        if !specs.get(&spec).is_some_and(|n| n.contains(&number)) {
            violations.push(format!(
                "case `{}` expected_oracle cites `{}`, which is not a numbered acceptance example in docs/specs",
                case.case_id, expected.spec
            ));
        }
    }
    match &ledger.oracle {
        None if labeled > 0 => violations.push(format!(
            "{labeled} cases carry expected_oracle but {LEDGER_FILE} has no [oracle] table; add `match_floor` and `overclaim_ceiling`"
        )),
        Some(gate) => {
            for (field, value) in [
                ("match_floor", gate.match_floor),
                ("overclaim_ceiling", gate.overclaim_ceiling),
            ] {
                if value > labeled {
                    violations.push(format!(
                        "{LEDGER_FILE} [oracle] {field} {value} exceeds the {labeled} cases with expected_oracle"
                    ));
                }
            }
        }
        None => {}
    }
    violations
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct OracleAccuracy {
    pub(crate) ledger: String,
    /// Cases with `expected_oracle`; the denominator of both rates.
    pub(crate) labeled: usize,
    pub(crate) by_outcome: BTreeMap<String, usize>,
    /// Matches over labeled cases.
    pub(crate) oracle_accuracy: Ratio,
    /// Overclaims over labeled cases.
    pub(crate) oracle_overclaim_rate: Ratio,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) match_floor: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) overclaim_ceiling: Option<usize>,
}

impl OracleAccuracy {
    pub(crate) fn is_unlabeled(&self) -> bool {
        self.labeled == 0
    }

    pub(crate) fn count(&self, outcome: OracleOutcome) -> usize {
        self.by_outcome
            .get(outcome.as_str())
            .copied()
            .unwrap_or_default()
    }
}

pub(crate) fn oracle_accuracy(outcomes: impl IntoIterator<Item = OracleOutcome>) -> OracleAccuracy {
    let mut by_outcome: BTreeMap<String, usize> = OracleOutcome::ALL
        .iter()
        .map(|o| (o.as_str().to_string(), 0))
        .collect();
    let mut labeled = 0;
    for outcome in outcomes {
        labeled += 1;
        *by_outcome.entry(outcome.as_str().to_string()).or_insert(0) += 1;
    }
    let get = |o: OracleOutcome| by_outcome.get(o.as_str()).copied().unwrap_or_default();
    OracleAccuracy {
        // `arm_gate` names the scored corpus's own ledger.
        ledger: format!("{CORPUS_DIR}/{LEDGER_FILE}"),
        labeled,
        oracle_accuracy: ratio(get(OracleOutcome::Match), labeled),
        oracle_overclaim_rate: ratio(get(OracleOutcome::Overclaim), labeled),
        by_outcome,
        match_floor: None,
        overclaim_ceiling: None,
    }
}

/// Name the scored corpus's ledger and, for a whole-corpus run with an
/// `[oracle]` table, arm its floor and ceiling. A case subset reports its
/// oracle rows but is never gated on whole-corpus counts.
pub(crate) fn arm_gate(
    accuracy: &mut OracleAccuracy,
    ledger: String,
    gate: Option<&OracleGate>,
    subset: bool,
) {
    accuracy.ledger = ledger;
    if let Some(gate) = gate.filter(|_| !subset) {
        accuracy.match_floor = Some(gate.match_floor);
        accuracy.overclaim_ceiling = Some(gate.overclaim_ceiling);
    }
}

/// A corpus without a spec-coverage ledger has no `[oracle]` gate and no
/// spec-example check, so a label there would be scored but never gated.
/// Refuse it until the corpus opts into a ledger.
pub(crate) fn ungated_label_violations(corpus: &Corpus, dir: &std::path::Path) -> Vec<String> {
    corpus
        .cases
        .iter()
        .filter(|case| case.expected_oracle.is_some())
        .map(|case| {
            format!(
                "case `{}` carries expected_oracle but {} has no {LEDGER_FILE} to gate it; add the ledger with an [oracle] table or remove the label",
                case.case_id,
                crate::normalize_path(dir)
            )
        })
        .collect()
}

/// The oracle gate. Fewer matches than the floor or more overclaims than
/// the ceiling fails with what to do; improvement passes and says which
/// committed number to tighten.
pub(crate) fn oracle_gate(accuracy: &OracleAccuracy) -> Result<Option<String>, String> {
    let (Some(floor), Some(ceiling)) = (accuracy.match_floor, accuracy.overclaim_ceiling) else {
        return Ok(None);
    };
    let matches = accuracy.count(OracleOutcome::Match);
    let overclaims = accuracy.count(OracleOutcome::Overclaim);
    let labeled = accuracy.labeled;
    let ledger = &accuracy.ledger;
    let mut failures = Vec::new();
    if overclaims > ceiling {
        failures.push(format!(
            "verdict-corpus: oracle overclaims rose to {overclaims}/{labeled} above the ceiling {ceiling} in {ledger}; a related test now reads stronger than its spec adopts, so restore the classifier reading (the report's oracle rows name each case), or relabel expected_oracle with the spec reason when the spec changed"
        ));
    }
    if matches < floor {
        failures.push(format!(
            "verdict-corpus: oracle matches fell to {matches}/{labeled} below the floor {floor} in {ledger}; restore the related-test reading the spec adopts (the report's oracle rows name each case), or lower `match_floor` with the reason in the PR when a label was deliberately retired"
        ));
    }
    if !failures.is_empty() {
        return Err(failures.join("\n"));
    }
    let mut notes = Vec::new();
    if overclaims < ceiling {
        notes.push(format!(
            "verdict-corpus: oracle overclaims {overclaims}/{labeled} are below the ceiling {ceiling}; lower `overclaim_ceiling` in {ledger} to {overclaims} to protect the improvement"
        ));
    }
    if matches > floor {
        notes.push(format!(
            "verdict-corpus: oracle matches {matches}/{labeled} are above the floor {floor}; raise `match_floor` in {ledger} to {matches} to protect them"
        ));
    }
    Ok((!notes.is_empty()).then(|| notes.join("\n")))
}

pub(crate) fn render_oracle_markdown(
    accuracy: &OracleAccuracy,
    rows: &[(&str, &OracleRow)],
) -> String {
    let mut out = String::new();
    out.push_str("\n## Oracle accuracy\n\n");
    let gate = match (accuracy.match_floor, accuracy.overclaim_ceiling) {
        (Some(floor), Some(ceiling)) => {
            format!(
                " Match floor {floor}; overclaim ceiling {ceiling} ({}).",
                accuracy.ledger
            )
        }
        _ => String::new(),
    };
    out.push_str(&format!(
        "The related-test oracle kind and strength ripr reports on the anchored findings, against the value a spec adopts (`expected_oracle`). Scored apart from the verdict: a wrong oracle that leaves the verdict unchanged still reads `ideal` above. Labeled {}. Accuracy {}/{} ({}); overclaim {}/{} ({}).{gate}\n\n",
        accuracy.labeled,
        accuracy.oracle_accuracy.numerator,
        accuracy.oracle_accuracy.denominator,
        accuracy.oracle_accuracy.rate,
        accuracy.oracle_overclaim_rate.numerator,
        accuracy.oracle_overclaim_rate.denominator,
        accuracy.oracle_overclaim_rate.rate,
    ));
    out.push_str("| Outcome | Cases |\n| --- | --- |\n");
    for outcome in OracleOutcome::ALL {
        out.push_str(&format!(
            "| {} | {} |\n",
            outcome.as_str(),
            accuracy.count(outcome)
        ));
    }
    if rows.is_empty() {
        return out;
    }
    out.push_str("\n| Case | Spec | Expected | Observed | All observed | Outcome |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for (case_id, row) in rows {
        let observed = match (&row.observed_kind, &row.observed_strength) {
            (Some(kind), Some(strength)) => format!("{kind} / {strength}"),
            _ => "none".to_string(),
        };
        let all = if row.observed_all.is_empty() {
            "none".to_string()
        } else {
            row.observed_all
                .iter()
                .map(|o| format!("{}: {} / {}", o.test, o.kind, o.strength))
                .collect::<Vec<_>>()
                .join("; ")
        };
        out.push_str(&format!(
            "| `{case_id}` | {} | {} / {} | {observed} | {all} | {} |\n",
            row.expected.spec,
            row.expected.kind,
            row.expected.strength,
            row.outcome.as_str()
        ));
    }
    out
}

#[cfg(test)]
#[path = "verdict_corpus_oracle_tests.rs"]
mod tests;
