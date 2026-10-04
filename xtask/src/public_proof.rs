//! Public proof page (`cargo xtask public-proof`).
//!
//! Renders `docs/PUBLIC_PROOF.md`, the page a skeptical developer reads before
//! adopting ripr, from committed receipts under `metrics/public-proof/` and the
//! pinned corpus manifest. Nothing on the page is typed by hand: every number,
//! trend and shortfall line is computed from a receipt, and a receipt that is
//! missing a field the page needs is a hard error, not a silent omission.
//!
//! `--check` fails when the committed page differs from what the receipts
//! render, or when a receipt has drifted from the canonical in-repo source it
//! was copied from. `--refresh-receipts` re-copies those canonical sources
//! before rendering. The same check runs as an xtask unit test, so the required
//! Rust gate fails on a stale page.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const PAGE: &str = "docs/PUBLIC_PROOF.md";
const RECEIPTS: &str = "metrics/public-proof";
const CORPUS_MANIFEST: &str = "benchmarks/rust_corpus/manifest.json";

/// Receipts that are verbatim copies of a canonical in-repo output. When the
/// canonical file exists it must equal the receipt byte for byte.
const CANONICAL_SOURCES: [(&str, &str); 2] = [
    ("dx-scoreboard.json", "metrics/dx-scoreboard/baseline.json"),
    (
        "verdict-corpus.json",
        "fixtures/rust-verdict-corpus/expected/report.json",
    ),
];

const NULL: &Value = &Value::Null;

struct Options {
    check: bool,
    refresh_receipts: bool,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let root = Path::new(".");
    if options.refresh_receipts {
        refresh_receipts(root)?;
    }
    let rendered = render(root)?;
    let page = root.join(PAGE);
    if options.check {
        check_page(root, &rendered)?;
        println!("{PAGE} matches its receipts.");
        return Ok(());
    }
    fs::write(&page, rendered).map_err(|err| format!("failed to write {PAGE}: {err}"))?;
    println!("wrote {PAGE}");
    Ok(())
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        check: false,
        refresh_receipts: false,
    };
    for arg in args {
        match arg.as_str() {
            "--check" => options.check = true,
            "--refresh-receipts" => options.refresh_receipts = true,
            other => {
                return Err(format!(
                    "unknown public-proof argument `{other}`; usage: cargo xtask public-proof [--check] [--refresh-receipts]"
                ));
            }
        }
    }
    if options.check && options.refresh_receipts {
        return Err(
            "`--check` only reads; run `--refresh-receipts` without it to update the receipts"
                .to_string(),
        );
    }
    Ok(options)
}

/// Fails with the next step when the page or a receipt is out of date.
fn check_page(root: &Path, rendered: &str) -> Result<(), String> {
    let drift = receipt_drift(root)?;
    if !drift.is_empty() {
        return Err(format!(
            "{}\nrun `cargo xtask public-proof --refresh-receipts` and commit the result",
            drift.join("\n")
        ));
    }
    let committed = fs::read_to_string(root.join(PAGE))
        .map_err(|err| format!("failed to read {PAGE}: {err}; run `cargo xtask public-proof`"))?;
    if committed != rendered {
        return Err(format!(
            "{PAGE} does not match its receipts; run `cargo xtask public-proof` and commit the result"
        ));
    }
    Ok(())
}

/// One line per receipt whose canonical source exists and differs.
fn receipt_drift(root: &Path) -> Result<Vec<String>, String> {
    let mut drift = Vec::new();
    for (receipt, source) in CANONICAL_SOURCES {
        let source_path = root.join(source);
        if !source_path.exists() {
            continue;
        }
        let canonical = read_bytes(&source_path)?;
        let copy = read_bytes(&root.join(RECEIPTS).join(receipt))?;
        if canonical != copy {
            drift.push(format!(
                "{RECEIPTS}/{receipt} differs from its canonical source {source}"
            ));
        }
    }
    Ok(drift)
}

fn refresh_receipts(root: &Path) -> Result<(), String> {
    for (receipt, source) in CANONICAL_SOURCES {
        let source_path = root.join(source);
        if !source_path.exists() {
            continue;
        }
        let bytes = read_bytes(&source_path)?;
        let target: PathBuf = root.join(RECEIPTS).join(receipt);
        fs::write(&target, bytes)
            .map_err(|err| format!("failed to write {}: {err}", target.display()))?;
        println!("refreshed {RECEIPTS}/{receipt} from {source}");
    }
    Ok(())
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|err| format!("{} is not JSON: {err}", path.display()))
}

struct Receipts {
    dx: Value,
    verdicts: Value,
    mutation: Value,
    first_previous: Value,
    first_current: Value,
    agent: Value,
    install: Value,
    corpus: Value,
}

fn load(root: &Path) -> Result<Receipts, String> {
    let receipt = |name: &str| read_json(&root.join(RECEIPTS).join(name));
    Ok(Receipts {
        dx: receipt("dx-scoreboard.json")?,
        verdicts: receipt("verdict-corpus.json")?,
        mutation: receipt("mutation-spot-check.json")?,
        first_previous: receipt("first-run-previous.json")?,
        first_current: receipt("first-run-current.json")?,
        agent: receipt("agent-as-user.json")?,
        install: receipt("install.json")?,
        corpus: read_json(&root.join(CORPUS_MANIFEST))?,
    })
}

// ---------------------------------------------------------------------------
// JSON access. A missing field the page depends on is an error naming it.

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(NULL)
}

fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    field(value, key)
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn req_arr<'a>(value: &'a Value, key: &str, ctx: &str) -> Result<&'a [Value], String> {
    field(value, key)
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{ctx}: missing array `{key}`"))
}

fn req_str(value: &Value, key: &str, ctx: &str) -> Result<String, String> {
    field(value, key)
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{ctx}: missing string `{key}`"))
}

fn req_f64(value: &Value, key: &str, ctx: &str) -> Result<f64, String> {
    field(value, key)
        .as_f64()
        .ok_or_else(|| format!("{ctx}: missing number `{key}`"))
}

fn text(value: &Value, key: &str) -> String {
    field(value, key).as_str().unwrap_or("").to_string()
}

fn item_text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(s) = value.get("code").and_then(Value::as_str) {
        return s.to_string();
    }
    value.to_string()
}

/// A rate recorded as `{numerator, denominator, rate}` with `rate` a decimal string.
struct Rate {
    numerator: u64,
    denominator: u64,
    rate: f64,
}

fn req_rate(value: &Value, key: &str, ctx: &str) -> Result<Rate, String> {
    let node = field(value, key);
    let numerator = field(node, "numerator")
        .as_u64()
        .ok_or_else(|| format!("{ctx}: `{key}` has no numerator"))?;
    let denominator = field(node, "denominator")
        .as_u64()
        .ok_or_else(|| format!("{ctx}: `{key}` has no denominator"))?;
    let rate = field(node, "rate")
        .as_str()
        .and_then(|raw| raw.parse::<f64>().ok())
        .ok_or_else(|| format!("{ctx}: `{key}` has no rate"))?;
    Ok(Rate {
        numerator,
        denominator,
        rate,
    })
}

// ---------------------------------------------------------------------------
// Formatting

fn num(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn percent(ratio: f64) -> String {
    format!("{:.1}%", ratio * 100.0)
}

fn fmt_value(value: f64, unit: &str) -> String {
    match unit {
        "ms" => format!("{:.1} s", value / 1000.0),
        "MB" => format!("{value:.0} MB"),
        "ratio" => percent(value),
        "s" => format!("{} s", num(value)),
        "" => num(value),
        "flag" => if value > 0.5 { "yes" } else { "no" }.to_string(),
        other => format!("{} {other}", num(value)),
    }
}

fn fmt_delta(delta: f64, unit: &str) -> String {
    let sign = if delta > 0.0 {
        "+"
    } else if delta < 0.0 {
        "-"
    } else {
        ""
    };
    format!("{sign}{}", fmt_value(delta.abs(), unit))
}

fn cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn short(sha: &str, len: usize) -> String {
    sha.chars().take(len).collect()
}

/// `ripr 0.11.0 (a7a089e1c51d...)` becomes `ripr 0.11.0 (a7a089e)`.
fn short_version(version: &str) -> String {
    match version.split_once('(') {
        Some((head, rest)) => {
            let sha = rest.trim_end_matches(')');
            format!("{}({})", head, short(sha, 7))
        }
        None => version.to_string(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Meets,
    Below,
    NotMeasured,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Meets => "meets the bar",
            Self::Below => "below the bar",
            Self::NotMeasured => "not measured",
        }
    }
}

struct Page(String);

impl Page {
    fn line(&mut self, line: impl AsRef<str>) {
        self.0.push_str(line.as_ref());
        self.0.push('\n');
    }

    fn blank(&mut self) {
        self.0.push('\n');
    }

    fn table(&mut self, head: &[&str], rows: &[Vec<String>]) {
        self.line(format!("| {} |", head.join(" | ")));
        self.line(format!("| {} |", vec!["---"; head.len()].join(" | ")));
        for row in rows {
            let cells: Vec<String> = row.iter().map(|c| cell(c)).collect();
            self.line(format!("| {} |", cells.join(" | ")));
        }
        self.blank();
    }
}

// ---------------------------------------------------------------------------
// Bars: one row per scoreboard metric, whatever receipt it comes from.

struct Bar {
    id: String,
    board: String,
    title: String,
    value: Option<f64>,
    unit: String,
    target: f64,
    lower_is_better: bool,
    status: Status,
    trend: String,
    basis: String,
}

fn board_name(id: &str) -> &str {
    match id {
        "speed" => "Speed and memory",
        "ci" => "CI adoption",
        "trust" => "Trust",
        "paste" => "Pasted commands",
        "first_run" => "First run",
        other => other,
    }
}

fn meets(value: f64, target: f64, lower_is_better: bool) -> bool {
    if lower_is_better {
        value <= target
    } else {
        value >= target
    }
}

fn is_unknown_verdict(verdict: &str) -> bool {
    verdict.ends_with("_unknown")
}

fn steps_of(case: &Value) -> &[Value] {
    items(case, "steps")
}

fn all_steps(receipt: &Value) -> Vec<&Value> {
    items(receipt, "cases").iter().flat_map(steps_of).collect()
}

fn friction_of(step: &Value) -> &[Value] {
    items(step, "friction")
}

/// First-run bars computed from a raw `first_run.v1` receipt, so they stay
/// comparable across the two receipts the page shows.
fn first_run_value(id: &str, receipt: &Value) -> Option<f64> {
    let steps = all_steps(receipt);
    let count = |n: usize| -> Option<f64> { u32::try_from(n).ok().map(f64::from) };
    match id {
        "first_run.unknown_verdicts" => count(
            items(receipt, "cases")
                .iter()
                .filter(|case| is_unknown_verdict(&text(case, "verdict")))
                .count(),
        ),
        "first_run.friction_events" => count(steps.iter().map(|s| friction_of(s).len()).sum()),
        "first_run.failed_steps" => count(
            steps
                .iter()
                .filter(|s| field(s, "exit").as_i64() != Some(0))
                .count(),
        ),
        "first_run.over_budget_steps" => count(
            steps
                .iter()
                .flat_map(|s| friction_of(s))
                .filter(|f| f.as_str().is_some_and(|t| t.contains("budget")))
                .count(),
        ),
        "first_run.walk_secs" => items(receipt, "cases")
            .iter()
            .map(|case| {
                steps_of(case)
                    .iter()
                    .filter_map(|s| field(s, "secs").as_f64())
                    .sum::<f64>()
            })
            .reduce(f64::max),
        _ => None,
    }
}

struct Derived {
    value: Option<f64>,
    trend: String,
    basis: String,
}

fn derived(id: &str, r: &Receipts) -> Result<Option<Derived>, String> {
    let first_run = |basis: &str| -> Derived {
        let value = first_run_value(id, &r.first_current);
        let previous = first_run_value(id, &r.first_previous);
        let before_label = short_version(&text(&r.first_previous, "ripr"));
        let trend = match (value, previous) {
            (Some(now), Some(before)) if (now - before).abs() < 1e-9 => {
                format!("unchanged from {before_label} ({})", fmt_value(before, ""))
            }
            (Some(_), Some(before)) => {
                format!("was {} in {before_label}", fmt_value(before, ""))
            }
            _ => "no earlier receipt".to_string(),
        };
        Derived {
            value,
            trend,
            basis: basis.to_string(),
        }
    };
    let first_receipt = |value: f64, basis: &str| -> Option<Derived> {
        Some(Derived {
            value: Some(value),
            trend: "first receipt".to_string(),
            basis: basis.to_string(),
        })
    };
    Ok(match id {
        "trust.false_verdict_rate" => {
            let rate = req_rate(&r.verdicts, "false_verdict_rate", "verdict-corpus")?;
            first_receipt(rate.rate, "verdict corpus")
        }
        "trust.discriminator_claim_agreement" => first_receipt(
            req_f64(
                field(
                    field(&r.mutation, "scored_families"),
                    "claims_discriminator",
                ),
                "agreement_rate",
                "mutation-spot-check",
            )?,
            "mutation spot check",
        ),
        "trust.gap_claim_agreement" => first_receipt(
            req_f64(
                field(
                    field(&r.mutation, "scored_families"),
                    "claims_no_discriminator",
                ),
                "agreement_rate",
                "mutation-spot-check",
            )?,
            "mutation spot check",
        ),
        "trust.mutation_join_coverage" => {
            let (precise, mutants) = mutation_join_totals(&r.mutation)?;
            first_receipt(
                if mutants == 0.0 {
                    0.0
                } else {
                    precise / mutants
                },
                "mutation spot check",
            )
        }
        "first_run.unknown_verdicts"
        | "first_run.friction_events"
        | "first_run.failed_steps"
        | "first_run.over_budget_steps"
        | "first_run.walk_secs" => Some(first_run("first-run receipts")),
        _ => None,
    })
}

/// Seam-precise joins and total mutants across the spot-check repositories.
fn mutation_join_totals(mutation: &Value) -> Result<(f64, f64), String> {
    let mut precise = 0.0;
    let mut mutants = 0.0;
    for repo in req_arr(mutation, "repos", "mutation-spot-check")? {
        let ctx = "mutation-spot-check repo";
        precise += req_f64(field(repo, "pairings"), "seam_precise", ctx).unwrap_or(0.0);
        mutants += req_f64(field(repo, "calibration_metrics"), "mutants_total", ctx)?;
    }
    Ok((precise, mutants))
}

fn bars(r: &Receipts) -> Result<Vec<Bar>, String> {
    let baseline_rev = short(
        field(field(&r.dx, "gate"), "baseline_revision")
            .as_str()
            .unwrap_or(""),
        7,
    );
    let mut out = Vec::new();
    for metric in req_arr(&r.dx, "metrics", "dx-scoreboard")? {
        let id = req_str(metric, "id", "dx-scoreboard metric")?;
        let unit = text(metric, "unit");
        let target = req_f64(metric, "target", &id)?;
        let lower_is_better = text(metric, "direction") != "higher_is_better";
        let mut value = field(metric, "value").as_f64();
        let mut trend = "no earlier measurement".to_string();
        let mut basis = text(metric, "source");
        if let Some(derived) = derived(&id, r)? {
            value = derived.value;
            trend = derived.trend;
            basis = derived.basis;
        } else if let Some(base) = metric.get("baseline")
            && base.get("comparable").and_then(Value::as_bool) == Some(true)
            && let (Some(before), Some(delta)) = (
                base.get("value").and_then(Value::as_f64),
                base.get("delta").and_then(Value::as_f64),
            )
        {
            trend = if delta == 0.0 {
                format!(
                    "unchanged since {baseline_rev} ({})",
                    fmt_value(before, &unit)
                )
            } else if unit == "flag" {
                format!(
                    "changed since {baseline_rev} (was {})",
                    fmt_value(before, &unit)
                )
            } else {
                format!(
                    "{} since {baseline_rev} (was {})",
                    fmt_delta(delta, &unit),
                    fmt_value(before, &unit)
                )
            };
        }
        let status = match value {
            Some(v) if meets(v, target, lower_is_better) => Status::Meets,
            Some(_) => Status::Below,
            None => Status::NotMeasured,
        };
        if status == Status::NotMeasured {
            basis = text(metric, "reason");
        }
        out.push(Bar {
            board: board_name(&text(metric, "board")).to_string(),
            id,
            title: text(metric, "title"),
            value,
            unit,
            target,
            lower_is_better,
            status,
            trend,
            basis,
        });
    }
    Ok(out)
}

fn bar_row(bar: &Bar) -> Vec<String> {
    let value = bar
        .value
        .map_or_else(|| "not measured".to_string(), |v| fmt_value(v, &bar.unit));
    let value = if bar.status == Status::Below {
        format!("**{value}**")
    } else {
        value
    };
    vec![
        bar.board.clone(),
        if bar.title.is_empty() {
            bar.id.clone()
        } else {
            bar.title.clone()
        },
        value,
        format!(
            "{} {}",
            if bar.lower_is_better { "<=" } else { ">=" },
            fmt_value(bar.target, &bar.unit)
        ),
        bar.status.label().to_string(),
        bar.trend.clone(),
    ]
}

// ---------------------------------------------------------------------------
// Rendering

fn render(root: &Path) -> Result<String, String> {
    let r = load(root)?;
    let bars = bars(&r)?;
    let mut page = Page(String::new());
    header(&mut page, &r);
    scoreboard(&mut page, &bars);
    shortfalls(&mut page, &r, &bars)?;
    mutation_section(&mut page, &r.mutation)?;
    verdict_section(&mut page, &r.verdicts)?;
    speed_section(&mut page, &r.dx)?;
    first_run_section(&mut page, &r.first_previous, &r.first_current, &r.install);
    agent_section(&mut page, &r.agent);
    corpus_section(&mut page, &r)?;
    boundaries(&mut page, &r);
    reproduce(&mut page);
    Ok(page.0)
}

fn header(page: &mut Page, r: &Receipts) {
    page.line("# Public proof");
    page.blank();
    page.line("<!-- Generated by `cargo xtask public-proof`. Do not edit by hand. -->");
    page.blank();
    page.line("This page is for a developer deciding whether to trust ripr. Every number comes from a committed receipt, names the revision it was measured at, and sits next to the bar we set for it. Where ripr misses a bar, the page says so before it says anything else.");
    page.blank();
    page.line("The page is generated. `cargo xtask public-proof --check` (also run by the xtask unit tests that CI requires) fails when the page no longer matches its receipts, or when a receipt has drifted from the in-repo output it was copied from.");
    page.blank();
    page.line("## Receipts");
    page.blank();
    let mutation_repos = items(&r.mutation, "repos");
    let mutation_tools: Vec<String> = {
        let mut seen: Vec<String> = Vec::new();
        for repo in mutation_repos {
            let version = text(repo, "cargo_mutants_version");
            if !seen.contains(&version) {
                seen.push(version);
            }
        }
        seen
    };
    let rows = vec![
        vec![
            "`metrics/public-proof/dx-scoreboard.json`".to_string(),
            "Speed, memory, CI adoption, pasted-command safety, self-contradictions".to_string(),
            short_version(&text(&r.dx, "analyzer_version")),
            format!("runner `{}`", text(&r.dx, "runner_class")),
        ],
        vec![
            "`metrics/public-proof/verdict-corpus.json`".to_string(),
            "Hand-labeled verdict corpus".to_string(),
            format!("corpus {}", text(&r.verdicts, "corpus_version")),
            text(&r.verdicts, "spec"),
        ],
        vec![
            "`metrics/public-proof/mutation-spot-check.json`".to_string(),
            "Agreement with real mutation runs".to_string(),
            format!("{} repositories at pinned revisions", mutation_repos.len()),
            format!("cargo-mutants {}", mutation_tools.join(", ")),
        ],
        vec![
            "`metrics/public-proof/first-run-previous.json`".to_string(),
            "New-developer walk, earlier release".to_string(),
            short_version(&text(&r.first_previous, "ripr")),
            format!("{} crates", items(&r.first_previous, "cases").len()),
        ],
        vec![
            "`metrics/public-proof/first-run-current.json`".to_string(),
            "New-developer walk, current build".to_string(),
            short_version(&text(&r.first_current, "ripr")),
            format!("{} crates", items(&r.first_current, "cases").len()),
        ],
        vec![
            "`metrics/public-proof/agent-as-user.json`".to_string(),
            "An agent using only ripr's help to close a real test gap".to_string(),
            text(&r.agent, "source"),
            text(&r.agent, "evidence"),
        ],
        vec![
            "`metrics/public-proof/install.json`".to_string(),
            "Time to install a prebuilt release".to_string(),
            text(&r.install, "source"),
            "one cloud container, not hosted CI".to_string(),
        ],
        vec![
            format!("`{CORPUS_MANIFEST}`"),
            "Pinned corpus the scoreboards draw from".to_string(),
            format!("corpus {}", text(&r.corpus, "corpus_version")),
            "read directly".to_string(),
        ],
    ];
    page.table(&["Receipt", "Measures", "Revision", "Detail"], &rows);
}

fn scoreboard(page: &mut Page, bars: &[Bar]) {
    let total = bars.len();
    let met = bars.iter().filter(|b| b.status == Status::Meets).count();
    let below = bars.iter().filter(|b| b.status == Status::Below).count();
    let unmeasured = bars
        .iter()
        .filter(|b| b.status == Status::NotMeasured)
        .count();
    page.line("## Scoreboard");
    page.blank();
    page.line(format!(
        "{total} bars. ripr meets {met}, is below the bar on {below}, and has not measured {unmeasured}. Bold values miss their bar. A trend compares against the earlier receipt named in the row; rows with no earlier receipt are first measurements."
    ));
    page.blank();
    let rows: Vec<Vec<String>> = bars.iter().map(bar_row).collect();
    page.table(&["Board", "Bar", "Now", "Target", "Status", "Trend"], &rows);
    let unmeasured_rows: Vec<&Bar> = bars
        .iter()
        .filter(|b| b.status == Status::NotMeasured)
        .collect();
    if !unmeasured_rows.is_empty() {
        page.line("Not measured, and why:");
        page.blank();
        for bar in unmeasured_rows {
            page.line(format!("- `{}`: {}", bar.id, bar.basis));
        }
        page.blank();
    }
}

fn shortfalls(page: &mut Page, r: &Receipts, bars: &[Bar]) -> Result<(), String> {
    page.line("## Where ripr falls short");
    page.blank();
    page.line("Each line below is computed from the receipts above. Detail sections follow.");
    page.blank();

    let rows = req_arr(&r.verdicts, "rows", "verdict-corpus")?;
    let false_actionable = req_rate(&r.verdicts, "false_actionable_rate", "verdict-corpus")?;
    let false_cases: Vec<String> = rows
        .iter()
        .filter(|row| text(row, "outcome") == "false_actionable")
        .map(|row| format!("`{}`", text(row, "case_id")))
        .collect();
    page.line(format!(
        "- **Wrong gaps.** On the labeled corpus ripr reported a gap on {} of {} changes whose tests caught every listed mutant ({}): {}.",
        false_actionable.numerator,
        false_actionable.denominator,
        percent(false_actionable.rate),
        false_cases.join(", ")
    ));

    let abstention = req_rate(&r.verdicts, "abstention_rate", "verdict-corpus")?;
    page.line(format!(
        "- **Mostly unsure.** It abstained on {} of {} corpus cases ({}). Abstaining is the safe failure, but each abstention is a change ripr gave the developer no help on.",
        abstention.numerator,
        abstention.denominator,
        percent(abstention.rate)
    ));

    let families = field(&r.mutation, "scored_families");
    let no_disc = field(families, "claims_no_discriminator");
    let counts = field(no_disc, "counts");
    let agree = field(counts, "agree").as_u64().unwrap_or(0);
    let false_gap = field(counts, "false_gap").as_u64().unwrap_or(0);
    let false_gap_repos: Vec<String> = {
        let mut seen: Vec<String> = Vec::new();
        for example in items(field(&r.mutation, "disagreement_examples"), "false_gap") {
            let repo = text(example, "repo");
            if !seen.contains(&repo) {
                seen.push(repo);
            }
        }
        seen
    };
    page.line(format!(
        "- **Real mutants disagree with \"no test would notice\".** Of {} mutants on seams ripr called ungripped, real mutation testing caught {} that ripr said nothing would catch; ripr agreed on {}. The recorded examples are all in: {}.",
        agree + false_gap,
        false_gap,
        agree,
        if false_gap_repos.is_empty() {
            "none recorded".to_string()
        } else {
            false_gap_repos.join(", ")
        }
    ));

    let (precise, mutants) = mutation_join_totals(&r.mutation)?;
    page.line(format!(
        "- **Thin ground truth.** Only {} of {} mutants ({}) join a ripr seam precisely enough to score, so the agreement figures rest on a small slice.",
        num(precise),
        num(mutants),
        percent(if mutants == 0.0 { 0.0 } else { precise / mutants })
    ));

    for bar in bars
        .iter()
        .filter(|b| b.status == Status::Below && b.board == "Speed and memory")
    {
        let worst = worst_sample(&r.dx, &bar.id).unwrap_or_else(|| {
            bar.value
                .map_or_else(|| "not measured".to_string(), |v| fmt_value(v, &bar.unit))
        });
        page.line(format!(
            "- **{}.** Worst repository: {worst}; the bar is {} {}.",
            bar.title,
            if bar.lower_is_better {
                "at most"
            } else {
                "at least"
            },
            fmt_value(bar.target, &bar.unit),
        ));
    }

    let unknown_now = bars
        .iter()
        .find(|b| b.id == "first_run.unknown_verdicts")
        .and_then(|b| b.value);
    let cases_now = items(&r.first_current, "cases").len();
    if let Some(unknown) = unknown_now {
        let unknown_before =
            first_run_value("first_run.unknown_verdicts", &r.first_previous).unwrap_or(0.0);
        page.line(format!(
            "- **First-run verdicts are unresolved.** {} of {} first-run crates ended in an `*_unknown` verdict on {}; {} of {} did on {}.",
            num(unknown),
            cases_now,
            short_version(&text(&r.first_current, "ripr")),
            num(unknown_before),
            items(&r.first_previous, "cases").len(),
            short_version(&text(&r.first_previous, "ripr"))
        ));
    }

    let swarm_dx = items(&r.dx, "repos").len();
    let manifest_repos = items(&r.corpus, "repos").len();
    page.line(format!(
        "- **Narrow coverage.** The speed scoreboard measured {swarm_dx} repositories and the first-run walk {cases_now} crates, out of {manifest_repos} pinned in the corpus."
    ));
    page.blank();
    Ok(())
}

/// The repository with the worst sample for a metric, as `repo (value)`.
fn worst_sample(dx: &Value, id: &str) -> Option<String> {
    let metric = items(dx, "metrics")
        .iter()
        .find(|m| field(m, "id").as_str() == Some(id))?;
    let unit = text(metric, "unit");
    let lower = text(metric, "direction") != "higher_is_better";
    items(metric, "samples")
        .iter()
        .filter_map(|s| Some((text(s, "repo"), field(s, "value").as_f64()?)))
        .reduce(|a, b| {
            let worse = if lower { b.1 > a.1 } else { b.1 < a.1 };
            if worse { b } else { a }
        })
        .map(|(repo, value)| format!("{repo} at {}", fmt_value(value, &unit)))
}

fn family_label(key: &str) -> String {
    match key {
        "claims_discriminator" => "ripr says a test would notice (`strongly_gripped`)".to_string(),
        "claims_no_discriminator" => "ripr says no test would notice (`ungripped`)".to_string(),
        other => other.to_string(),
    }
}

fn mutation_section(page: &mut Page, mutation: &Value) -> Result<(), String> {
    let ctx = "mutation-spot-check";
    page.line("## Mutation agreement");
    page.blank();
    page.line(format!(
        "{} Status of this receipt: `{}`.",
        req_str(mutation, "claim_boundary", ctx)?,
        text(mutation, "status")
    ));
    page.blank();
    let families = field(mutation, "scored_families");
    let family_rows: Vec<Vec<String>> = families
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, family)| {
                    let counts = field(family, "counts");
                    let count = |name: &str| field(counts, name).as_u64().unwrap_or(0).to_string();
                    vec![
                        family_label(key),
                        field(family, "seams_scored")
                            .as_u64()
                            .unwrap_or(0)
                            .to_string(),
                        field(family, "mutants_scored")
                            .as_u64()
                            .unwrap_or(0)
                            .to_string(),
                        count("agree"),
                        count("overclaim"),
                        count("false_gap"),
                        field(family, "agreement_rate")
                            .as_f64()
                            .map_or_else(|| "n/a".to_string(), percent),
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    if family_rows.is_empty() {
        return Err(format!("{ctx}: `scored_families` is missing or empty"));
    }
    page.table(
        &[
            "Verdict class",
            "Seams",
            "Mutants",
            "Agree",
            "Overclaim",
            "False gap",
            "Agreement",
        ],
        &family_rows,
    );
    page.line("An overclaim is a mutant ripr said a test would catch that no test caught. A false gap is a mutant ripr said nothing would catch that the tests did catch.");
    page.blank();

    let repo_rows: Vec<Vec<String>> = req_arr(mutation, "repos", ctx)?
        .iter()
        .map(|repo| {
            let metrics = field(repo, "calibration_metrics");
            let n = |value: &Value, key: &str| field(value, key).as_u64().unwrap_or(0).to_string();
            vec![
                text(repo, "name"),
                format!("`{}`", short(&text(repo, "revision"), 12)),
                text(repo, "cargo_mutants_version"),
                n(metrics, "mutants_total"),
                n(metrics, "matched_total"),
                n(metrics, "ambiguous_file_line_total"),
                n(metrics, "unmatched_mutants_total"),
                n(field(repo, "pairings"), "seam_precise"),
            ]
        })
        .collect();
    page.table(
        &[
            "Repository",
            "Revision",
            "cargo-mutants",
            "Mutants",
            "Joined",
            "Ambiguous",
            "Unmatched",
            "Seam-precise",
        ],
        &repo_rows,
    );

    let grip = field(
        field(mutation, "outcomes_by_pairing_and_grip_class"),
        "seam_precise",
    );
    let grip_rows: Vec<Vec<String>> = grip
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(class, outcomes)| {
                    let summary: Vec<String> = outcomes
                        .as_object()
                        .map(|o| {
                            o.iter()
                                .map(|(name, count)| {
                                    format!("{name} {}", count.as_u64().unwrap_or(0))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    vec![format!("`{class}`"), summary.join(", ")]
                })
                .collect()
        })
        .unwrap_or_default();
    page.line("Real mutation outcomes by ripr grip class, seam-precise joins only:");
    page.blank();
    page.table(&["Grip class", "Mutation outcomes"], &grip_rows);

    let examples = items(field(mutation, "disagreement_examples"), "false_gap");
    if !examples.is_empty() {
        page.line("False-gap examples, as recorded:");
        page.blank();
        for example in examples {
            page.line(format!(
                "- {} `{}:{}` {} `{}` is `{}`; mutant `{}` was {}.",
                text(example, "repo"),
                text(example, "file"),
                field(example, "line").as_u64().unwrap_or(0),
                text(example, "seam_kind"),
                text(example, "expression"),
                text(example, "grip_class"),
                text(example, "mutant")
                    .split_once(": ")
                    .map_or_else(|| text(example, "mutant"), |(_, rest)| rest.to_string()),
                text(example, "runtime_outcome"),
            ));
        }
        page.blank();
    }
    Ok(())
}

fn verdict_section(page: &mut Page, verdicts: &Value) -> Result<(), String> {
    let ctx = "verdict-corpus";
    page.line("## Verdict corpus");
    page.blank();
    page.line(format!(
        "{} hand-labeled changes (corpus {}, {}). Each has a ground-truth label from real mutants and an ideal verdict; ripr's observed verdict is compared against it. A false actionable verdict is a reported gap on a change whose tests caught every listed mutant. That is the failure that costs a developer's trust, so it is tracked on its own.",
        req_f64(verdicts, "cases_total", ctx)?,
        text(verdicts, "corpus_version"),
        text(verdicts, "spec")
    ));
    page.blank();
    let labels = [
        ("false_verdict_rate", "False verdicts (all cases)"),
        (
            "false_actionable_rate",
            "False actionable (of discriminated)",
        ),
        (
            "false_exposed_rate",
            "False exposed (of not fully discriminated)",
        ),
        (
            "false_silent_rate",
            "False silent (of not fully discriminated)",
        ),
        ("ideal_rate", "Ideal verdict"),
        (
            "abstention_rate",
            "Abstained (limited or silent where acceptable)",
        ),
        ("contradiction_rate", "Findings with a contradiction"),
    ];
    let mut rate_rows = Vec::new();
    for (key, label) in labels {
        let rate = req_rate(verdicts, key, ctx)?;
        rate_rows.push(vec![
            label.to_string(),
            format!("{}/{}", rate.numerator, rate.denominator),
            percent(rate.rate),
        ]);
    }
    page.table(&["Rate", "Count", "Share"], &rate_rows);
    let case_rows: Vec<Vec<String>> = req_arr(verdicts, "rows", ctx)?
        .iter()
        .map(|row| {
            let contradictions: Vec<String> =
                items(row, "contradictions").iter().map(item_text).collect();
            let classes: Vec<String> = items(row, "observed_classifications")
                .iter()
                .map(item_text)
                .collect();
            vec![
                format!("`{}`", text(row, "case_id")),
                text(row, "truth"),
                text(row, "ideal_verdict"),
                text(row, "observed_verdict"),
                classes.join(", "),
                text(row, "outcome"),
                if contradictions.is_empty() {
                    "none".to_string()
                } else {
                    contradictions.join(", ")
                },
            ]
        })
        .collect();
    page.table(
        &[
            "Case",
            "Truth",
            "Ideal",
            "Observed",
            "Static classes",
            "Outcome",
            "Contradictions",
        ],
        &case_rows,
    );
    let non_claims = items(verdicts, "non_claims");
    if !non_claims.is_empty() {
        page.line("What the corpus does not claim:");
        page.blank();
        for claim in non_claims {
            page.line(format!("- {}", item_text(claim)));
        }
        page.blank();
    }
    Ok(())
}

fn speed_section(page: &mut Page, dx: &Value) -> Result<(), String> {
    let ctx = "dx-scoreboard";
    page.line("## Speed and memory");
    page.blank();
    let ids = [
        "speed.cold_pilot_ms",
        "speed.cold_pilot_peak_rss_mb",
        "speed.warm_check_ms",
        "speed.warm_check_peak_rss_mb",
    ];
    let metrics = req_arr(dx, "metrics", ctx)?;
    let find = |id: &str| metrics.iter().find(|m| field(m, "id").as_str() == Some(id));
    let mut head = vec!["Repository".to_string()];
    let mut target_row = vec!["**Bar**".to_string()];
    for id in ids {
        let metric = find(id).ok_or_else(|| format!("{ctx}: metric `{id}` is missing"))?;
        head.push(text(metric, "title"));
        target_row.push(fmt_value(
            req_f64(metric, "target", id)?,
            &text(metric, "unit"),
        ));
    }
    let mut rows = vec![target_row];
    for repo in req_arr(dx, "repos", ctx)? {
        let mut row = vec![format!("{} ({})", text(repo, "id"), text(repo, "note"))];
        for id in ids {
            let entry = items(repo, "metrics")
                .iter()
                .find(|m| field(m, "metric").as_str() == Some(id));
            row.push(match entry {
                Some(m) => {
                    let value = field(m, "value").as_f64().map_or_else(
                        || "not measured".to_string(),
                        |v| fmt_value(v, &text(m, "unit")),
                    );
                    if text(m, "status") == "below_target" {
                        format!("**{value}**")
                    } else {
                        value
                    }
                }
                None => "not measured".to_string(),
            });
        }
        rows.push(row);
    }
    let head_refs: Vec<&str> = head.iter().map(String::as_str).collect();
    page.line(format!(
        "Measured on the pinned corpus repositories at the revisions in the receipt, with a release build, on runner class `{}`. Bold values miss the bar. Cold is a fresh cache; warm is the second `ripr check` against the last commit. Peak memory is the process's high-water mark sampled every 10 ms on Linux, a lower bound.",
        text(dx, "runner_class")
    ));
    page.blank();
    page.table(&head_refs, &rows);
    Ok(())
}

fn step_range(receipt: &Value, step: &str) -> String {
    let secs: Vec<f64> = items(receipt, "cases")
        .iter()
        .flat_map(steps_of)
        .filter(|s| field(s, "step").as_str() == Some(step))
        .filter_map(|s| field(s, "secs").as_f64())
        .collect();
    let min = secs.iter().copied().reduce(f64::min);
    let max = secs.iter().copied().reduce(f64::max);
    match (min, max) {
        (Some(lo), Some(hi)) if (hi - lo).abs() < 1e-9 => format!("{} s", num(lo)),
        (Some(lo), Some(hi)) => format!("{}-{} s", num(lo), num(hi)),
        _ => "not run".to_string(),
    }
}

fn friction_summary(receipt: &Value) -> Vec<String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for step in all_steps(receipt) {
        for flag in friction_of(step) {
            let flag = item_text(flag);
            match seen.iter_mut().find(|(text, _)| *text == flag) {
                Some((_, count)) => *count += 1,
                None => seen.push((flag, 1)),
            }
        }
    }
    seen.into_iter()
        .map(|(flag, count)| {
            if count > 1 {
                format!("{flag} (x{count})")
            } else {
                flag
            }
        })
        .collect()
}

fn first_run_section(page: &mut Page, previous: &Value, current: &Value, install: &Value) {
    let before = short_version(&text(previous, "ripr"));
    let after = short_version(&text(current, "ripr"));
    page.line("## First run");
    page.blank();
    page.line(
        "A scripted new developer runs `doctor`, `check`, `pilot`, the follow-up command `check` prints, and `init --ci github` against crates ripr was not tuned on, with one committed boundary edit each. The walk records timings and friction and does not judge verdict accuracy. Timings come from one Linux container. A standalone install timing is shown at the end of this section; it is not the time for the generated CI workflow to get ripr, which stays unmeasured."
    );
    page.blank();
    let mut verdict_rows = Vec::new();
    for case in items(current, "cases") {
        let name = text(case, "case");
        let earlier = items(previous, "cases")
            .iter()
            .find(|c| text(c, "case") == name);
        verdict_rows.push(vec![
            name,
            earlier.map_or_else(|| "not run".to_string(), |c| text(c, "verdict")),
            text(case, "verdict"),
            earlier.map_or_else(
                || "not run".to_string(),
                |c| field(c, "workflow_lines").as_u64().unwrap_or(0).to_string(),
            ),
            field(case, "workflow_lines")
                .as_u64()
                .unwrap_or(0)
                .to_string(),
        ]);
    }
    page.table(
        &[
            "Crate",
            &format!("Verdict, {before}"),
            &format!("Verdict, {after}"),
            &format!("Workflow lines, {before}"),
            &format!("Workflow lines, {after}"),
        ],
        &verdict_rows,
    );
    let mut step_names: Vec<String> = Vec::new();
    for receipt in [previous, current] {
        for step in all_steps(receipt) {
            let name = text(step, "step");
            if !step_names.contains(&name) {
                step_names.push(name);
            }
        }
    }
    let step_rows: Vec<Vec<String>> = step_names
        .iter()
        .map(|name| {
            vec![
                format!("`{name}`"),
                step_range(previous, name),
                step_range(current, name),
            ]
        })
        .collect();
    page.line("Seconds per step, range across crates:");
    page.blank();
    page.table(&["Step", &before, &after], &step_rows);
    for (label, receipt) in [(&before, previous), (&after, current)] {
        let flags = friction_summary(receipt);
        page.line(format!("Friction flagged on {label}:"));
        page.blank();
        if flags.is_empty() {
            page.line("- none");
        }
        for flag in flags {
            page.line(format!("- {flag}"));
        }
        page.blank();
    }
    page.line("Install, from its own receipt:");
    page.blank();
    page.line(format!("> {}", text(install, "evidence")));
    page.blank();
}

fn agent_section(page: &mut Page, agent: &Value) {
    page.line("## An agent using only ripr's help");
    page.blank();
    page.line(format!(
        "One agent per crate was given a real test gap and only ripr's own help output, and was scored against a mutation answer key. This is one run per crate, not a rate. Evidence: {}.",
        text(agent, "evidence")
    ));
    page.blank();
    let metrics = items(agent, "metrics");
    let value = |id: &str, repo: &str| -> String {
        metrics
            .iter()
            .find(|m| text(m, "id") == id && text(m, "repo") == repo)
            .and_then(|m| field(m, "value").as_f64())
            .map_or_else(|| "n/a".to_string(), num)
    };
    let mut repos: Vec<String> = Vec::new();
    for metric in metrics {
        let repo = text(metric, "repo");
        if !repo.is_empty() && !repos.contains(&repo) {
            repos.push(repo);
        }
    }
    let rows: Vec<Vec<String>> = repos
        .iter()
        .map(|repo| {
            vec![
                repo.clone(),
                value("agent.answer_key_mutants_caught", repo),
                value("agent.ripr_commands_to_fix", repo),
                value("agent.tool_steps_to_fix", repo),
                value("agent.false_weak_findings_after_fix", repo),
            ]
        })
        .collect();
    page.table(
        &[
            "Crate",
            "Answer-key mutants caught",
            "ripr commands to fix",
            "Tool steps to fix",
            "False weak findings after the fix",
        ],
        &rows,
    );
    let overall = |id: &str| -> String {
        metrics
            .iter()
            .find(|m| text(m, "id") == id && text(m, "repo").is_empty())
            .and_then(|m| field(m, "value").as_f64())
            .map_or_else(|| "n/a".to_string(), num)
    };
    page.line(format!(
        "Across the runs: fix success {}, stale re-check cycles {}, white-box tests written only to satisfy ripr {}.",
        overall("agent.fix_success_rate"),
        overall("agent.stale_recheck_cycles"),
        overall("agent.white_box_tests_to_satisfy_ripr"),
    ));
    page.blank();
}

fn corpus_section(page: &mut Page, r: &Receipts) -> Result<(), String> {
    let ctx = "rust-corpus manifest";
    let repos = req_arr(&r.corpus, "repos", ctx)?;
    let class_count = |class: &str| {
        repos
            .iter()
            .filter(|repo| field(field(repo, "profile"), "class").as_str() == Some(class))
            .count()
    };
    page.line("## The corpus behind the numbers");
    page.blank();
    page.line(format!(
        "{} real repositories are pinned to exact upstream commits (corpus {}): {} well-maintained, {} legacy, {} ordinary. The class is a judgment recorded per repository, not a measurement. Scoreboards run on subsets of it, so a number above describes the subset named in its section.",
        repos.len(),
        text(&r.corpus, "corpus_version"),
        class_count("good"),
        class_count("legacy"),
        class_count("common"),
    ));
    page.blank();
    let known_gaps = items(&r.corpus, "known_gaps");
    if !known_gaps.is_empty() {
        page.line("Known gaps in the corpus:");
        page.blank();
        for gap in known_gaps {
            page.line(format!("- {}", item_text(gap)));
        }
        page.blank();
    }
    let dx_rows: Vec<Vec<String>> = req_arr(&r.dx, "corpus", "dx-scoreboard")?
        .iter()
        .map(|repo| {
            vec![
                text(repo, "id"),
                format!("`{}`", short(&text(repo, "sha"), 12)),
                text(repo, "url"),
                text(repo, "status"),
            ]
        })
        .collect();
    page.line("Repositories the speed scoreboard ran on:");
    page.blank();
    page.table(
        &["Repository", "Pinned commit", "Source", "Status"],
        &dx_rows,
    );
    Ok(())
}

fn boundaries(page: &mut Page, r: &Receipts) {
    page.line("## What these numbers do not say");
    page.blank();
    page.line(format!("- {}", text(&r.dx, "claim_boundary")));
    page.line(format!("- {}", text(&r.mutation, "claim_boundary")));
    for claim in items(&r.verdicts, "non_claims") {
        page.line(format!("- {}", item_text(claim)));
    }
    page.line("- ripr reports static evidence. It does not run your tests or mutate your code, and no figure here is a runtime mutation result for your repository.");
    page.line("- Agent and first-run figures are single walks, not rates.");
    page.blank();
}

fn reproduce(page: &mut Page) {
    page.line("## Reproduce and refresh");
    page.blank();
    page.line("```bash");
    page.line(
        "cargo xtask dx-scoreboard --clone            # speed, memory, CI size, pasted commands",
    );
    page.line("cargo xtask mutation-spot-check              # agreement with real mutation runs");
    page.line("cargo xtask first-run                        # the new-developer walk");
    page.line(
        "cargo xtask public-proof --refresh-receipts  # re-copy canonical outputs, then render",
    );
    page.line("cargo xtask public-proof --check             # fail if this page is stale");
    page.line("```");
    page.blank();
    page.line("Receipts live in `metrics/public-proof/`. `dx-scoreboard.json` and `verdict-corpus.json` are verbatim copies of `metrics/dx-scoreboard/baseline.json` and `fixtures/rust-verdict-corpus/expected/report.json`; the check fails when either source moves ahead of its copy. The mutation, first-run and agent receipts are copied from the harness outputs named in their sections.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    #[test]
    fn committed_page_matches_its_receipts() -> Result<(), String> {
        let root = workspace_root();
        let rendered = render(&root)?;
        check_page(&root, &rendered)
    }

    #[test]
    fn page_names_its_shortfalls_before_its_detail() -> Result<(), String> {
        let rendered = render(&workspace_root())?;
        let shortfalls = rendered
            .find("## Where ripr falls short")
            .ok_or("no shortfalls section")?;
        let mutation = rendered
            .find("## Mutation agreement")
            .ok_or("no mutation section")?;
        assert!(shortfalls < mutation);
        Ok(())
    }

    #[test]
    fn stale_page_is_reported() {
        let result = check_page(&workspace_root(), "# not the page\n");
        assert!(result.is_err_and(|err| err.contains("does not match its receipts")));
    }

    #[test]
    fn unknown_arguments_are_refused() {
        let result = parse_options(&["--nope".to_string()]);
        assert!(result.is_err());
        let both = parse_options(&["--check".to_string(), "--refresh-receipts".to_string()]);
        assert!(both.is_err());
    }

    #[test]
    fn missing_receipt_field_is_an_error_not_a_blank() -> Result<(), String> {
        let mut receipts = load(&workspace_root())?;
        if let Some(object) = receipts.verdicts.as_object_mut() {
            object.remove("false_actionable_rate");
        }
        let mut page = Page(String::new());
        let result = shortfalls(&mut page, &receipts, &[]);
        assert!(result.is_err_and(|err| err.contains("false_actionable_rate")));
        Ok(())
    }

    #[test]
    fn formatting_keeps_units_readable() {
        assert_eq!(fmt_value(237_697.110_6, "ms"), "237.7 s");
        assert_eq!(fmt_value(3467.17, "MB"), "3467 MB");
        assert_eq!(fmt_value(0.043, "ratio"), "4.3%");
        assert_eq!(fmt_delta(-1220.0, "lines"), "-1220 lines");
        assert_eq!(
            short_version("ripr 0.11.0 (a7a089e1c51dfa51)"),
            "ripr 0.11.0 (a7a089e)"
        );
    }

    #[test]
    fn bar_status_follows_direction() {
        assert!(meets(0.96, 0.95, false));
        assert!(!meets(0.04, 0.8, false));
        assert!(meets(1.0, 2.0, true));
        assert!(!meets(3.0, 2.0, true));
    }

    #[test]
    fn drifted_receipt_is_reported() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-public-proof-{}", std::process::id()));
        let receipts = dir.join(RECEIPTS);
        fs::create_dir_all(&receipts).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("metrics/dx-scoreboard")).map_err(|e| e.to_string())?;
        fs::write(receipts.join("dx-scoreboard.json"), "{\"a\":1}").map_err(|e| e.to_string())?;
        fs::write(dir.join("metrics/dx-scoreboard/baseline.json"), "{\"a\":2}")
            .map_err(|e| e.to_string())?;
        fs::write(receipts.join("verdict-corpus.json"), "{}").map_err(|e| e.to_string())?;
        let drift = receipt_drift(&dir)?;
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        assert_eq!(drift.len(), 1);
        assert!(drift.iter().any(|line| line.contains("dx-scoreboard.json")));
        Ok(())
    }
}
