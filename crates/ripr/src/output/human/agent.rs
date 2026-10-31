//! `ripr check --format agent`: a compact, legend-free, line-oriented view of
//! the same triage the default human format prints (#11580).
//!
//! The report is a pure projection. Which finding is the top gap, whether the
//! run is limited, and which command to run next all come from the human
//! triage selector and the canonical next action; this module adds no
//! classification, ranking, or validator of its own. It only decides how much
//! of the already-selected evidence fits on a bounded line, so an agent can
//! read it for a few hundred bytes instead of the 1 KB human digest or the
//! 57-110 KB JSON document.
//!
//! Shape (one record per line, `|`-separated fields, no legend):
//!
//! ```text
//! #ripr agent static-advisory base=origin/main findings=6 gaps=2 exposed=4 weakly_exposed=2
//! weakly_exposed|src/lib.rs:404|major: self.major|missing=major: self.major|test=t1|id=probe:...
//! t1=tests/test_version.rs:254 test_next_minor exact_value `assert_eq!(next.minor, 3);`
//! next=ripr explain --root /repo probe:...
//! ```
//!
//! Terminal states are one explicit line: `done=` when nothing is left to fix
//! from the test side, `stop=` when ripr cannot offer a test-side fix.

use super::triage::{self, HumanTriage};
use crate::app::{CheckDiffProvenance, CheckOutput, FindingDrillIn};
use crate::config::RiprConfig;
use crate::domain::{ExposureClass, Finding, NextActionCheckCase, OracleStrength, RelatedTest};
use crate::output::next_action::render_next_action_plain_at_root;
use crate::output::path::repository_display_path;
use std::collections::BTreeMap;

/// Gap lines printed before the explicit `+N more` line.
pub(super) const MAX_GAP_LINES: usize = 5;
/// Character budget for the changed expression and the oracle text.
const FIELD_BUDGET: usize = 80;

pub(super) fn render(
    output: &CheckOutput,
    config: &RiprConfig,
    drill_in: Option<&FindingDrillIn>,
    provenance: CheckDiffProvenance,
) -> String {
    let triage = triage::select_human_triage(output, config);
    let considered: Vec<&Finding> = triage
        .selected
        .into_iter()
        .chain(triage.omitted.iter().copied())
        .collect();
    // Test-side gaps are independent of which finding triage selected.
    // A selected static-limited finding must not hide omitted weakly_exposed
    // or reachable_unrevealed work, or `stop=` would tell an agent to quit
    // while a test-side gap is still open (#11825).
    let gaps = gap_findings(&triage);
    let mut out = header_line(output, &considered, gaps.len());
    let next = next_line(&triage, output, &gaps, drill_in, provenance);
    if output.findings.is_empty() && !output.no_scope_provided {
        out.push_str(
            "done=no changed behavior found in scope; static advisory, not runtime proof\n",
        );
        return out;
    }
    if should_emit_stop(&gaps) {
        out.push_str(&terminal_line(&triage, output));
        push_limits(&mut out, &considered, &[]);
        if let Some(next) = next {
            out.push_str(&next);
            out.push('\n');
        }
        return out;
    }

    let mut tests: Vec<TestRef> = Vec::new();
    for finding in gaps.iter().take(MAX_GAP_LINES) {
        let test_ref = strongest_test(finding).map(|test| intern_test(&mut tests, output, test));
        out.push_str(&gap_line(output, finding, test_ref));
        out.push('\n');
    }
    for (index, test) in tests.iter().enumerate() {
        out.push_str(&format!("t{}={}\n", index + 1, test.line));
    }
    if gaps.len() > MAX_GAP_LINES {
        out.push_str(&format!(
            "+{} more gaps not shown; `--format json` lists all\n",
            gaps.len() - MAX_GAP_LINES
        ));
    }
    push_limits(&mut out, &considered, &gaps);
    if let Some(line) = next {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Gap-eligible findings in triage order: the selected top gap first, then the
/// remaining candidate-current, non-limited, non-exposed findings in the
/// producer's own finding order. Anything the triage treats as static-limited,
/// preview-only, exposed or non-current is not a test-side gap.
fn gap_findings<'a>(triage: &HumanTriage<'a>) -> Vec<&'a Finding> {
    triage
        .selected
        .into_iter()
        .chain(triage.omitted.iter().copied())
        .filter(|finding| is_test_side_gap(finding))
        .collect()
}

fn is_test_side_gap(finding: &Finding) -> bool {
    finding.is_candidate_actionable()
        && matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
        && !triage::is_static_limited(finding)
        && !triage::is_preview_limited(finding)
}

/// `stop=` is the terminal "nothing more is fixable from the test side"
/// flag. It is true only when no test-side gap remains; a selected
/// static-limited finding is not enough to set it.
fn should_emit_stop(gaps: &[&Finding]) -> bool {
    gaps.is_empty()
}

fn header_line(output: &CheckOutput, considered: &[&Finding], gaps: usize) -> String {
    let mut by_class: BTreeMap<&'static str, usize> = BTreeMap::new();
    for finding in considered {
        *by_class.entry(finding.class.as_str()).or_default() += 1;
    }
    let mut line = format!(
        "#ripr agent static-advisory base={} findings={} gaps={gaps}",
        output.base.as_deref().unwrap_or("-"),
        considered.len(),
    );
    for (class, count) in by_class {
        line.push_str(&format!(" {class}={count}"));
    }
    line.push('\n');
    line
}

fn terminal_line(triage: &HumanTriage<'_>, output: &CheckOutput) -> String {
    match triage::check_case_for_triage(triage, output) {
        NextActionCheckCase::TopGap => {
            // The selected finding is limited or preview-only, so it is not a
            // test-side gap even though the triage names it.
            "stop=no test-side gap is offered for the selected finding; see `limits=`\n".to_string()
        }
        NextActionCheckCase::NoDiffFinding => {
            "done=no weak or unobserved gap left to fix from the test side; static advisory, not runtime proof\n"
                .to_string()
        }
        NextActionCheckCase::CandidateFilterHidAll => {
            "stop=every finding is base-side or unresolved, none is a current test-side gap\n"
                .to_string()
        }
        NextActionCheckCase::SuppressedByPolicy => {
            "stop=all findings are suppressed by policy\n".to_string()
        }
        NextActionCheckCase::StaticLimited => {
            "stop=ripr cannot tell from reading the code whether tests would notice; nothing more is fixable from the test side\n"
                .to_string()
        }
        NextActionCheckCase::PreviewAdvisory => {
            "stop=preview-language evidence is advisory only; nothing more is fixable from the test side\n"
                .to_string()
        }
        NextActionCheckCase::ScopeMissing => {
            "stop=nothing was analyzed (no scope); pass --base REV, --diff FILE or --worktree\n"
                .to_string()
        }
    }
}

/// `limits=` names the considered findings that are not test-side gaps, by
/// class, so a clean-looking gap list never hides them (the `github` format
/// drops them silently).
fn push_limits(out: &mut String, considered: &[&Finding], gaps: &[&Finding]) {
    let mut by_class: BTreeMap<&'static str, usize> = BTreeMap::new();
    for finding in considered {
        if finding.class == ExposureClass::Exposed {
            continue;
        }
        if gaps.iter().any(|gap| gap.id == finding.id) {
            continue;
        }
        *by_class.entry(finding.class.as_str()).or_default() += 1;
    }
    if by_class.is_empty() {
        return;
    }
    let parts: Vec<String> = by_class
        .into_iter()
        .map(|(class, count)| format!("{class}:{count}"))
        .collect();
    out.push_str(&format!("limits={}\n", parts.join(",")));
}

/// The command to run next. A top gap drills in with the navigation's
/// scope-bound `ripr explain` (the same route the human digest prints, and
/// the canonical action's `detail_route`), because a bare finding id does not
/// resolve outside the run that produced it. Other states take the canonical
/// next action's own command, when it has one; a non-executable action prints
/// nothing here because the `done=`/`stop=` line already says why.
fn next_line(
    triage: &HumanTriage<'_>,
    output: &CheckOutput,
    gaps: &[&Finding],
    drill_in: Option<&FindingDrillIn>,
    provenance: CheckDiffProvenance,
) -> Option<String> {
    if let Some(gap) = gaps.first() {
        return Some(match drill_in {
            Some(FindingDrillIn::Commands(navigation)) => {
                format!("next={}", navigation.explain_command(&gap.id))
            }
            None => format!("next=ripr explain {}", gap.id),
        });
    }
    let action =
        triage::canonical_next_action_for_triage(triage, output, drill_in, provenance).ok()?;
    action.command()?;
    let lines = render_next_action_plain_at_root(&action, Some(&output.root));
    let command = lines.first()?.trim().strip_prefix("Next:")?.trim();
    Some(format!("next={command}"))
}

struct TestRef {
    key: (String, usize, String),
    line: String,
}

fn strongest_test(finding: &Finding) -> Option<&RelatedTest> {
    finding
        .related_tests
        .iter()
        .filter(|test| test.is_contributing())
        .min_by_key(|test| strength_rank(&test.oracle_strength))
}

fn strength_rank(strength: &OracleStrength) -> u8 {
    match strength {
        OracleStrength::Strong => 0,
        OracleStrength::Medium => 1,
        OracleStrength::Weak => 2,
        OracleStrength::Smoke => 3,
        OracleStrength::None => 4,
        OracleStrength::Unknown => 5,
    }
}

/// Index (1-based) of `test` in the shared table, adding it on first use so a
/// test that grips several gaps is written once.
fn intern_test(tests: &mut Vec<TestRef>, output: &CheckOutput, test: &RelatedTest) -> usize {
    let key = (
        repository_display_path(&output.root, &test.file),
        test.line,
        test.name.clone(),
    );
    if let Some(index) = tests.iter().position(|known| known.key == key) {
        return index + 1;
    }
    let mut line = format!(
        "{}:{} {} {}",
        key.0,
        key.1,
        field(&test.name),
        test.oracle_kind.as_str()
    );
    if let Some(oracle) = test
        .oracle
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        line.push_str(&format!(" `{}`", clip(oracle)));
    }
    if let Some(miss) = test.miss {
        line.push_str(&format!(" miss={}", miss.as_str()));
    }
    tests.push(TestRef { key, line });
    tests.len()
}

fn gap_line(output: &CheckOutput, finding: &Finding, test_ref: Option<usize>) -> String {
    let location = format!(
        "{}:{}",
        repository_display_path(&output.root, &finding.probe.location.file),
        finding.probe.location.line
    );
    let missing = finding
        .activation
        .missing_discriminators
        .first()
        .map(|fact| fact.value.as_str())
        .or_else(|| finding.missing.first().map(String::as_str))
        .unwrap_or("-");
    let test = test_ref.map_or_else(|| "-".to_string(), |index| format!("t{index}"));
    format!(
        "{}|{}|{}|missing={}|test={}|id={}",
        finding.class.as_str(),
        field(&location),
        field(&clip(&finding.probe.expression)),
        field(&clip(missing)),
        test,
        finding.id
    )
}

/// One line, `|`-safe: whitespace collapses and the field separator is
/// escaped so a record always splits into the same number of fields.
fn field(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|")
}

fn clip(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= FIELD_BUDGET {
        return collapsed;
    }
    let mut clipped: String = collapsed.chars().take(FIELD_BUDGET).collect();
    clipped.push('…');
    clipped
}
