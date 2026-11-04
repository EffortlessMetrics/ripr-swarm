//! `cargo xtask slow-test-report` — the slowest tests and the per-test time
//! budget (issue #12905, slice 1).
//!
//! Reads the nextest `ci` profile JUnit XML (`target/nextest/ci/junit.xml`,
//! uploaded by `test-analytics.yml` as the `rust-junit` artifact) and writes the
//! slowest tests with per-test seconds, judged against the budget in
//! `policy/test-time-budget.toml`.
//!
//! The policy records the decision: `mode = "advisory"` reports only;
//! `mode = "enforced"` fails when a test over budget is not in the committed
//! baseline allowlist. The baseline lists the tests that were over budget when
//! the policy was written, so enforcement can be switched on without a flood of
//! failures and then ratcheted down by deleting entries.
//!
//! The command never runs tests; it reads a JUnit file a prior run produced.
//! A missing or malformed JUnit file or policy is an instrument failure and is
//! reported as an error, never as an empty pass.

use serde_json::{Value, json};
use std::collections::BTreeSet;

const DEFAULT_JUNIT: &str = "target/nextest/ci/junit.xml";
const DEFAULT_POLICY: &str = "policy/test-time-budget.toml";
const DEFAULT_LIMIT: usize = 50;
const POLICY_SCHEMA: &str = "ripr_slow_test_budget.v1";
const REPORT_SCHEMA: &str = "ripr_slow_test_report.v1";
const REPORT_JSON: &str = "slow-tests.json";
const REPORT_MD: &str = "slow-tests.md";
const USAGE: &str =
    "usage: cargo xtask slow-test-report [--junit <path>] [--policy <path>] [--limit <n>]";

pub(crate) fn slow_test_report(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_options(args)?;
    let junit = std::fs::read_to_string(&options.junit)
        .map_err(|err| format!("failed to read JUnit XML {}: {err}", options.junit))?;
    let policy = std::fs::read_to_string(&options.policy)
        .map_err(|err| format!("failed to read policy {}: {err}", options.policy))?;
    let report = build_report(&junit, &policy, options.limit)?;
    let json_text = serde_json::to_string_pretty(&report_json(&report))
        .map_err(|err| format!("serialize slow-test report: {err}"))?;
    crate::write_report(REPORT_JSON, &format!("{json_text}\n"))?;
    crate::write_report(REPORT_MD, &report_markdown(&report))?;
    println!("Wrote target/ripr/reports/{REPORT_JSON}");
    println!("Wrote target/ripr/reports/{REPORT_MD}");
    if report.fails_enforcement() {
        return Err(format!(
            "slow-test budget is enforced and {} test(s) exceed {}s without a baseline entry: {}",
            report.new_over_budget().len(),
            report.budget_seconds,
            report
                .new_over_budget()
                .iter()
                .map(|test| test.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct Options {
    junit: String,
    policy: String,
    limit: usize,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        junit: DEFAULT_JUNIT.to_string(),
        policy: DEFAULT_POLICY.to_string(),
        limit: DEFAULT_LIMIT,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .filter(|value| !value.is_empty())
                .cloned()
                .ok_or_else(|| format!("slow-test-report {flag} requires a value; {USAGE}"))
        };
        match arg.as_str() {
            "--junit" => options.junit = value("--junit")?,
            "--policy" => options.policy = value("--policy")?,
            "--limit" => {
                options.limit = value("--limit")?
                    .parse::<usize>()
                    .ok()
                    .filter(|limit| *limit > 0)
                    .ok_or_else(|| {
                        "slow-test-report --limit must be a positive integer".to_string()
                    })?;
            }
            other => {
                return Err(format!(
                    "unknown slow-test-report argument `{other}`; {USAGE}"
                ));
            }
        }
    }
    Ok(options)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Advisory,
    Enforced,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::Enforced => "enforced",
        }
    }
}

#[derive(Debug)]
struct BaselineEntry {
    test: String,
    recorded_seconds: f64,
}

#[derive(Debug)]
struct Policy {
    mode: Mode,
    budget_seconds: f64,
    ratchet: String,
    baseline: Vec<BaselineEntry>,
}

fn parse_policy(text: &str) -> Result<Policy, String> {
    let value = toml::Value::Table(
        text.parse::<toml::Table>()
            .map_err(|err| format!("slow-test policy is not valid TOML: {err}"))?,
    );
    let schema = value.get("schema").and_then(toml::Value::as_str);
    if schema != Some(POLICY_SCHEMA) {
        return Err(format!(
            "slow-test policy schema must be `{POLICY_SCHEMA}`, found {schema:?}"
        ));
    }
    let decision = value
        .get("decision")
        .and_then(toml::Value::as_table)
        .ok_or("slow-test policy needs a [decision] table")?;
    let mode = match decision.get("mode").and_then(toml::Value::as_str) {
        Some("advisory") => Mode::Advisory,
        Some("enforced") => Mode::Enforced,
        other => {
            return Err(format!(
                "slow-test policy decision.mode must be `advisory` or `enforced`, found {other:?}"
            ));
        }
    };
    let budget_seconds = decision
        .get("per_test_budget_seconds")
        .and_then(|budget| {
            budget
                .as_float()
                .or_else(|| budget.as_integer().map(|n| n as f64))
        })
        .filter(|budget| budget.is_finite() && *budget > 0.0)
        .ok_or("slow-test policy decision.per_test_budget_seconds must be a positive number")?;
    let ratchet = decision
        .get("ratchet")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|ratchet| !ratchet.is_empty())
        .ok_or("slow-test policy decision.ratchet must record the ratchet decision")?
        .to_string();
    let mut baseline = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in value
        .get("baseline")
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let test = entry
            .get("test")
            .and_then(toml::Value::as_str)
            .filter(|test| !test.is_empty())
            .ok_or("slow-test baseline entry needs a non-empty `test`")?
            .to_string();
        let recorded_seconds = entry
            .get("recorded_seconds")
            .and_then(|seconds| {
                seconds
                    .as_float()
                    .or_else(|| seconds.as_integer().map(|n| n as f64))
            })
            .filter(|seconds| seconds.is_finite())
            .ok_or_else(|| format!("slow-test baseline `{test}` needs numeric recorded_seconds"))?;
        if recorded_seconds <= budget_seconds {
            return Err(format!(
                "slow-test baseline `{test}` recorded {recorded_seconds}s is not over the {budget_seconds}s budget; delete the entry"
            ));
        }
        if !seen.insert(test.clone()) {
            return Err(format!("slow-test baseline lists `{test}` twice"));
        }
        baseline.push(BaselineEntry {
            test,
            recorded_seconds,
        });
    }
    Ok(Policy {
        mode,
        budget_seconds,
        ratchet,
        baseline,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Standing {
    WithinBudget,
    OverBudgetBaselined,
    OverBudgetNew,
}

impl Standing {
    fn as_str(self) -> &'static str {
        match self {
            Self::WithinBudget => "within_budget",
            Self::OverBudgetBaselined => "over_budget_baselined",
            Self::OverBudgetNew => "over_budget_new",
        }
    }
}

#[derive(Debug)]
pub(crate) struct TestTime {
    pub(crate) id: String,
    pub(crate) seconds: f64,
    pub(crate) standing: Standing,
}

#[derive(Debug)]
pub(crate) struct SlowTestReport {
    mode: Mode,
    budget_seconds: f64,
    ratchet: String,
    tests_measured: usize,
    tests_skipped: usize,
    over_budget_total: usize,
    limit: usize,
    slowest: Vec<TestTime>,
    new_over_budget_all: Vec<TestTime>,
    /// Baselined tests that are now within budget: delete the entry.
    ratchet_candidates: Vec<(String, f64, f64)>,
    /// Baselined tests absent from this JUnit file (subset run or renamed).
    baseline_not_observed: Vec<String>,
}

impl SlowTestReport {
    fn new_over_budget(&self) -> &[TestTime] {
        &self.new_over_budget_all
    }

    fn fails_enforcement(&self) -> bool {
        self.mode == Mode::Enforced && !self.new_over_budget_all.is_empty()
    }
}

pub(crate) fn build_report(
    junit: &str,
    policy: &str,
    limit: usize,
) -> Result<SlowTestReport, String> {
    let policy = parse_policy(policy)?;
    let document = roxmltree::Document::parse(junit)
        .map_err(|err| format!("JUnit XML is not well formed: {err}"))?;
    let mut measured: Vec<(String, f64)> = Vec::new();
    let mut skipped = 0usize;
    for case in document
        .descendants()
        .filter(|node| node.has_tag_name("testcase"))
    {
        let name = case
            .attribute("name")
            .ok_or("JUnit testcase without a name attribute")?;
        let id = match case.attribute("classname") {
            Some(class) if !class.is_empty() => format!("{class} {name}"),
            _ => name.to_string(),
        };
        if case.children().any(|child| child.has_tag_name("skipped")) {
            skipped += 1;
            continue;
        }
        let seconds = case
            .attribute("time")
            .and_then(|time| time.parse::<f64>().ok())
            .filter(|time| time.is_finite() && *time >= 0.0)
            .ok_or_else(|| format!("JUnit testcase `{id}` has no valid time attribute"))?;
        measured.push((id, seconds));
    }
    if measured.is_empty() {
        return Err(
            "JUnit XML contains no measured testcase; slowness was not established".to_string(),
        );
    }
    let baselined: BTreeSet<&str> = policy
        .baseline
        .iter()
        .map(|entry| entry.test.as_str())
        .collect();
    let mut tests: Vec<TestTime> = measured
        .into_iter()
        .map(|(id, seconds)| {
            let standing = if seconds <= policy.budget_seconds {
                Standing::WithinBudget
            } else if baselined.contains(id.as_str()) {
                Standing::OverBudgetBaselined
            } else {
                Standing::OverBudgetNew
            };
            TestTime {
                id,
                seconds,
                standing,
            }
        })
        .collect();
    // Slowest first; ties by id so the report is deterministic.
    tests.sort_by(|a, b| {
        b.seconds
            .total_cmp(&a.seconds)
            .then_with(|| a.id.cmp(&b.id))
    });
    let tests_measured = tests.len();
    let over_budget_total = tests
        .iter()
        .filter(|test| test.standing != Standing::WithinBudget)
        .count();
    let ratchet_candidates = policy
        .baseline
        .iter()
        .filter_map(|entry| {
            tests
                .iter()
                .find(|test| test.id == entry.test && test.standing == Standing::WithinBudget)
                .map(|test| (entry.test.clone(), entry.recorded_seconds, test.seconds))
        })
        .collect();
    let baseline_not_observed = policy
        .baseline
        .iter()
        .filter(|entry| !tests.iter().any(|test| test.id == entry.test))
        .map(|entry| entry.test.clone())
        .collect();
    let new_over_budget_all = tests
        .iter()
        .filter(|test| test.standing == Standing::OverBudgetNew)
        .map(|test| TestTime {
            id: test.id.clone(),
            seconds: test.seconds,
            standing: test.standing,
        })
        .collect();
    tests.truncate(limit);
    Ok(SlowTestReport {
        mode: policy.mode,
        budget_seconds: policy.budget_seconds,
        ratchet: policy.ratchet,
        tests_measured,
        tests_skipped: skipped,
        over_budget_total,
        limit,
        slowest: tests,
        new_over_budget_all,
        ratchet_candidates,
        baseline_not_observed,
    })
}

fn status(report: &SlowTestReport) -> &'static str {
    if report.new_over_budget_all.is_empty() {
        "pass"
    } else if report.mode == Mode::Enforced {
        "fail"
    } else {
        "warn"
    }
}

fn report_json(report: &SlowTestReport) -> Value {
    let row = |rank: usize, test: &TestTime| {
        json!({
            "rank": rank,
            "test": test.id,
            "seconds": test.seconds,
            "standing": test.standing.as_str(),
        })
    };
    json!({
        "schema": REPORT_SCHEMA,
        "status": status(report),
        "mode": report.mode.as_str(),
        "ratchet": report.ratchet,
        "per_test_budget_seconds": report.budget_seconds,
        "tests_measured": report.tests_measured,
        "tests_skipped": report.tests_skipped,
        "over_budget_total": report.over_budget_total,
        "new_over_budget": report.new_over_budget_all.len(),
        "limit": report.limit,
        "slowest": report
            .slowest
            .iter()
            .enumerate()
            .map(|(index, test)| row(index + 1, test))
            .collect::<Vec<_>>(),
        "new_over_budget_tests": report
            .new_over_budget_all
            .iter()
            .enumerate()
            .map(|(index, test)| row(index + 1, test))
            .collect::<Vec<_>>(),
        "ratchet_candidates": report
            .ratchet_candidates
            .iter()
            .map(|(test, recorded, observed)| json!({
                "test": test,
                "recorded_seconds": recorded,
                "observed_seconds": observed,
            }))
            .collect::<Vec<_>>(),
        "baseline_not_observed": report.baseline_not_observed,
    })
}

fn report_markdown(report: &SlowTestReport) -> String {
    let mut body = format!(
        "# ripr slow-test report\n\nStatus: {}\n\nMode: {}\n\nPer-test budget: {}s\n\nRatchet decision: {}\n\n## Summary\n\n- Tests measured: {}\n- Tests skipped (excluded): {}\n- Over budget: {}\n- Over budget and not in baseline: {}\n\n",
        status(report),
        report.mode.as_str(),
        report.budget_seconds,
        markdown_cell(&report.ratchet),
        report.tests_measured,
        report.tests_skipped,
        report.over_budget_total,
        report.new_over_budget_all.len(),
    );
    body.push_str(&format!(
        "## {} slowest tests\n\n| Rank | Seconds | Standing | Test |\n| ---: | ---: | --- | --- |\n",
        report.slowest.len()
    ));
    for (index, test) in report.slowest.iter().enumerate() {
        body.push_str(&format!(
            "| {} | {:.3} | {} | `{}` |\n",
            index + 1,
            test.seconds,
            test.standing.as_str(),
            markdown_cell(&test.id)
        ));
    }
    if !report.new_over_budget_all.is_empty() {
        body.push_str("\n## Over budget and not in baseline\n\n");
        for test in &report.new_over_budget_all {
            body.push_str(&format!(
                "- `{}` took {:.3}s\n",
                markdown_cell(&test.id),
                test.seconds
            ));
        }
    }
    if !report.ratchet_candidates.is_empty() {
        body.push_str("\n## Ratchet candidates (within budget; delete from baseline)\n\n");
        for (test, recorded, observed) in &report.ratchet_candidates {
            body.push_str(&format!(
                "- `{}` recorded {recorded:.1}s, now {observed:.3}s\n",
                markdown_cell(test)
            ));
        }
    }
    if !report.baseline_not_observed.is_empty() {
        body.push_str("\n## Baseline entries not observed in this JUnit file\n\n");
        for test in &report.baseline_not_observed {
            body.push_str(&format!("- `{}`\n", markdown_cell(test)));
        }
    }
    body.push_str("\nRegenerate with `cargo xtask slow-test-report` after a `cargo nextest run --profile ci`.\n");
    body
}

fn markdown_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("slow_tests/sample_junit.xml");
    const POLICY: &str = r#"
schema = "ripr_slow_test_budget.v1"
[decision]
mode = "advisory"
per_test_budget_seconds = 30.0
ratchet = "advisory until the baseline shrinks"
[[baseline]]
test = "xtask::bin/xtask tests::long_baselined"
recorded_seconds = 150.0
[[baseline]]
test = "xtask::bin/xtask tests::improved_since_baseline"
recorded_seconds = 40.0
[[baseline]]
test = "xtask::bin/xtask tests::renamed_away"
recorded_seconds = 99.0
"#;

    fn standing_of(report: &SlowTestReport, id: &str) -> Option<Standing> {
        report
            .slowest
            .iter()
            .find(|test| test.id == id)
            .map(|test| test.standing)
    }

    #[test]
    fn ranks_slowest_first_with_per_test_seconds_and_excludes_skipped() -> Result<(), String> {
        let report = build_report(SAMPLE, POLICY, 50)?;
        let ids: Vec<&str> = report.slowest.iter().map(|test| test.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "xtask::bin/xtask tests::long_baselined",
                "ripr::cli_smoke check_json_timeout",
                "xtask::bin/xtask tests::newly_slow",
                "xtask::bin/xtask tests::exactly_at_budget",
                "xtask::bin/xtask tests::improved_since_baseline",
                "ripr::cli_smoke fast_check",
            ]
        );
        assert_eq!(report.slowest[0].seconds, 120.0);
        assert_eq!(report.tests_measured, 6);
        assert_eq!(report.tests_skipped, 1);
        Ok(())
    }

    #[test]
    fn limit_truncates_the_list_but_not_the_counts() -> Result<(), String> {
        let report = build_report(SAMPLE, POLICY, 2)?;
        assert_eq!(report.slowest.len(), 2);
        assert_eq!(report.tests_measured, 6);
        assert_eq!(report.over_budget_total, 3);
        Ok(())
    }

    #[test]
    fn budget_standing_separates_baselined_new_and_within() -> Result<(), String> {
        let report = build_report(SAMPLE, POLICY, 50)?;
        assert_eq!(
            standing_of(&report, "xtask::bin/xtask tests::long_baselined"),
            Some(Standing::OverBudgetBaselined)
        );
        assert_eq!(
            standing_of(&report, "xtask::bin/xtask tests::newly_slow"),
            Some(Standing::OverBudgetNew)
        );
        // 45.5s in ripr::cli_smoke is over budget and not baselined.
        assert_eq!(
            standing_of(&report, "ripr::cli_smoke check_json_timeout"),
            Some(Standing::OverBudgetNew)
        );
        // Exactly at budget is within budget.
        assert_eq!(
            standing_of(&report, "xtask::bin/xtask tests::exactly_at_budget"),
            Some(Standing::WithinBudget)
        );
        assert_eq!(report.new_over_budget().len(), 2);
        Ok(())
    }

    #[test]
    fn improved_baseline_entries_are_ratchet_candidates_and_missing_ones_are_named()
    -> Result<(), String> {
        let report = build_report(SAMPLE, POLICY, 50)?;
        assert_eq!(
            report.ratchet_candidates,
            vec![(
                "xtask::bin/xtask tests::improved_since_baseline".to_string(),
                40.0,
                2.0
            )]
        );
        assert_eq!(
            report.baseline_not_observed,
            vec!["xtask::bin/xtask tests::renamed_away".to_string()]
        );
        Ok(())
    }

    #[test]
    fn advisory_mode_never_fails_but_enforced_mode_fails_on_new_over_budget() -> Result<(), String>
    {
        let advisory = build_report(SAMPLE, POLICY, 50)?;
        assert!(!advisory.fails_enforcement());
        assert_eq!(status(&advisory), "warn");
        let enforced = POLICY.replace("mode = \"advisory\"", "mode = \"enforced\"");
        let enforced = build_report(SAMPLE, &enforced, 50)?;
        assert!(enforced.fails_enforcement());
        assert_eq!(status(&enforced), "fail");
        Ok(())
    }

    #[test]
    fn enforced_mode_passes_when_every_over_budget_test_is_baselined() -> Result<(), String> {
        let policy = POLICY.replace("mode = \"advisory\"", "mode = \"enforced\"")
            + "[[baseline]]\ntest = \"xtask::bin/xtask tests::newly_slow\"\nrecorded_seconds = 31.0\n\
               [[baseline]]\ntest = \"ripr::cli_smoke check_json_timeout\"\nrecorded_seconds = 45.5\n";
        let report = build_report(SAMPLE, &policy, 50)?;
        assert!(!report.fails_enforcement());
        assert_eq!(status(&report), "pass");
        Ok(())
    }

    #[test]
    fn report_outputs_carry_rank_seconds_and_decision() -> Result<(), String> {
        let report = build_report(SAMPLE, POLICY, 3)?;
        let json = report_json(&report);
        assert_eq!(json["schema"], REPORT_SCHEMA);
        assert_eq!(json["mode"], "advisory");
        assert_eq!(json["slowest"][0]["rank"], 1);
        assert_eq!(json["slowest"][0]["seconds"], 120.0);
        assert_eq!(json["slowest"][2]["standing"], "over_budget_new");
        let markdown = report_markdown(&report);
        assert!(markdown.contains("Ratchet decision: advisory until the baseline shrinks"));
        assert!(markdown.contains("| 1 | 120.000 | over_budget_baselined |"));
        Ok(())
    }

    #[test]
    fn instrument_failures_are_errors_not_empty_passes() {
        assert!(build_report("<testsuites/>", POLICY, 50).is_err());
        assert!(build_report("not xml", POLICY, 50).is_err());
        let no_time = r#"<testsuites><testsuite name="a"><testcase name="t" classname="a"/></testsuite></testsuites>"#;
        assert!(build_report(no_time, POLICY, 50).is_err());
    }

    #[test]
    fn invalid_policies_are_rejected() {
        let bad_mode = POLICY.replace("mode = \"advisory\"", "mode = \"warn\"");
        assert!(build_report(SAMPLE, &bad_mode, 50).is_err());
        let no_ratchet = POLICY.replace(
            "ratchet = \"advisory until the baseline shrinks\"",
            "ratchet = \" \"",
        );
        assert!(build_report(SAMPLE, &no_ratchet, 50).is_err());
        let zero_budget = POLICY.replace("30.0", "0.0");
        assert!(build_report(SAMPLE, &zero_budget, 50).is_err());
        // A baseline entry that is not over budget is dead weight.
        let under = POLICY.replace("recorded_seconds = 40.0", "recorded_seconds = 5.0");
        assert!(build_report(SAMPLE, &under, 50).is_err());
        let duplicate = format!(
            "{POLICY}[[baseline]]\ntest = \"xtask::bin/xtask tests::long_baselined\"\nrecorded_seconds = 150.0\n"
        );
        assert!(build_report(SAMPLE, &duplicate, 50).is_err());
    }

    #[test]
    fn options_parse_defaults_and_reject_bad_input() -> Result<(), String> {
        let defaults = parse_options(&[])?;
        assert_eq!(defaults.junit, DEFAULT_JUNIT);
        assert_eq!(defaults.limit, 50);
        let args: Vec<String> = ["--limit", "5", "--junit", "x.xml"]
            .iter()
            .map(|arg| arg.to_string())
            .collect();
        let parsed = parse_options(&args)?;
        assert_eq!((parsed.limit, parsed.junit.as_str()), (5, "x.xml"));
        for bad in [vec!["--limit", "0"], vec!["--limit"], vec!["--nope"]] {
            let bad: Vec<String> = bad.iter().map(|arg| arg.to_string()).collect();
            assert!(parse_options(&bad).is_err());
        }
        Ok(())
    }

    #[test]
    fn committed_policy_parses_and_names_only_over_budget_baseline_tests() -> Result<(), String> {
        let text = include_str!("../../../policy/test-time-budget.toml");
        let policy = parse_policy(text)?;
        assert_eq!(policy.mode, Mode::Advisory);
        assert!(!policy.baseline.is_empty());
        Ok(())
    }
}
