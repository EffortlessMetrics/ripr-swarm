//! Spec-example coverage of the Rust verdict corpus (RIPR-SPEC-0219).
//!
//! The unit is one numbered item under a spec's `## Acceptance Examples`
//! heading, named `RIPR-SPEC-NNNN#K`. Cases cite the examples they label in
//! `spec_examples`; `spec-coverage.toml` puts every spec with numbered
//! examples in or out of scope, waives in-scope examples a corpus case
//! cannot label, and records the covered count the gate protects. Coverage
//! says which spec examples carry a runtime-labeled case; it says nothing
//! about whether ripr's verdict on that case is right.

use super::{Corpus, Ratio, ratio};
use crate::normalize_path;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const LEDGER_FILE: &str = "spec-coverage.toml";
pub(crate) const SPECS_DIR: &str = "docs/specs";
const LEDGER_SCHEMA: &str = "ripr_verdict_corpus_spec_coverage.v1";
const SPEC_PREFIX: &str = "RIPR-SPEC-";
const EXAMPLES_HEADING: &str = "## Acceptance Examples";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ledger {
    pub(crate) schema_version: String,
    /// The covered-example count `verdict-corpus check` protects.
    pub(crate) floor: usize,
    #[serde(default)]
    pub(crate) spec: Vec<LedgerSpec>,
    /// In-scope specs whose acceptance examples are prose, not numbered:
    /// listed so the limit stays visible, never counted.
    #[serde(default)]
    pub(crate) unmeasured: Vec<UnmeasuredSpec>,
    /// The oracle-accuracy gate (`verdict_corpus_oracle.rs`).
    #[serde(default)]
    pub(crate) oracle: Option<super::oracle::OracleGate>,
    /// The operator-grid and trap gate (`verdict_corpus_grid.rs`).
    #[serde(default)]
    pub(crate) grid: Option<super::grid::GridGate>,
    /// Every false verdict the corpus still scores, each with its owner
    /// (`verdict_corpus_known_false.rs`).
    #[serde(default)]
    pub(crate) known_false: Vec<super::known_false::KnownFalse>,
    /// The retired shared ceiling line. Present only so a corpus that still
    /// carries it is refused with a pointer to `missed-credit.toml` (#10222).
    #[serde(default)]
    pub(crate) missed_credit_ceiling: Option<usize>,
    /// Abstained rows whose tests do discriminate the change, each accepted by
    /// an `[[entry]]` in `missed-credit.toml` (`verdict_corpus_known_false.rs`).
    #[serde(skip)]
    pub(crate) missed_credit: Vec<super::known_false::MissedCredit>,
    /// Coverage axes beyond spec examples: language shapes, test-library
    /// idioms and hostile inputs, each with the case count `check` protects.
    #[serde(default)]
    pub(crate) axis: Vec<LedgerAxis>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LedgerAxis {
    pub(crate) id: String,
    pub(crate) description: String,
    /// Fewest cases tagged with this axis. Raise it when coverage grows;
    /// lowering it is a reviewed edit with a reason.
    pub(crate) floor: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LedgerSpec {
    pub(crate) id: String,
    pub(crate) scope: Scope,
    #[serde(default)]
    pub(crate) reason: Option<String>,
    #[serde(default)]
    pub(crate) waived: Vec<Waiver>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    In,
    Out,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Waiver {
    pub(crate) example: ExampleKey,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnmeasuredSpec {
    pub(crate) id: String,
    pub(crate) reason: String,
}

/// One acceptance example of a spec: a legacy hand-numbered item (`K. `) or
/// a slug-keyed item (``- example `slug`: ``). New examples take a slug, so two PRs
/// adding examples never fight over the next number or the end of the list.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub(crate) enum ExampleKey {
    Number(usize),
    Slug(String),
}

impl std::fmt::Display for ExampleKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(number) => write!(f, "{number}"),
            Self::Slug(slug) => f.write_str(slug),
        }
    }
}

/// Spec id to its acceptance examples (numbered and slug-keyed); a spec
/// without any maps to an empty set.
pub(crate) type SpecExamples = BTreeMap<String, BTreeSet<ExampleKey>>;

/// A slug is lowercase ASCII words joined by single hyphens, starting with a
/// letter, so one example has one spelling and never reads as a number.
fn is_slug(text: &str) -> bool {
    text.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && !text.ends_with('-')
        && !text.contains("--")
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The items of `## Acceptance Examples` in file order: `K. ` numbered
/// items and ``- example `slug`: `` slug items at the start of a line. Continuation
/// lines are indented, so they never start an item; fenced code is skipped.
pub(crate) fn acceptance_items(markdown: &str) -> Vec<ExampleKey> {
    let mut found = Vec::new();
    let mut inside = false;
    // The open fence's character and length: Markdown fences use backticks or
    // tildes, and only a run of the same character at least as long closes one.
    let mut fence: Option<(char, usize)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let run = |c: char| trimmed.chars().take_while(|&x| x == c).count();
        match fence {
            Some((c, len)) => {
                if run(c) >= len && trimmed[run(c) * c.len_utf8()..].trim().is_empty() {
                    fence = None;
                }
                continue;
            }
            None => {
                if let Some(c) = ['`', '~'].into_iter().find(|&c| run(c) >= 3) {
                    fence = Some((c, run(c)));
                    continue;
                }
            }
        }
        if line.starts_with("## ") {
            inside = line.trim_end() == EXAMPLES_HEADING;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(slug) = line
            .strip_prefix("- example `")
            .and_then(|rest| rest.split_once("`:"))
            .map(|(slug, _)| slug)
            .filter(|slug| is_slug(slug))
        {
            found.push(ExampleKey::Slug(slug.to_string()));
            continue;
        }
        let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty()
            && line
                .get(digits.len()..)
                .is_some_and(|rest| rest.starts_with(". "))
            && let Ok(number) = digits.parse::<usize>()
        {
            found.push(ExampleKey::Number(number));
        }
    }
    found
}

/// The numbers of the items written `K. ` under `## Acceptance Examples`.
#[cfg(test)]
pub(crate) fn numbered_examples(markdown: &str) -> BTreeSet<usize> {
    acceptance_items(markdown)
        .into_iter()
        .filter_map(|key| match key {
            ExampleKey::Number(number) => Some(number),
            ExampleKey::Slug(_) => None,
        })
        .collect()
}

/// Slug items must be unique and listed alphabetically: an example inserted
/// where its slug sorts lands away from every other PR's insertion, so the
/// list has no shared end to conflict on. Returns the first violation.
fn slug_order_violation(items: &[ExampleKey]) -> Option<String> {
    let slugs: Vec<&str> = items
        .iter()
        .filter_map(|key| match key {
            ExampleKey::Slug(slug) => Some(slug.as_str()),
            ExampleKey::Number(_) => None,
        })
        .collect();
    slugs.windows(2).find_map(|pair| {
        if pair[0] == pair[1] {
            Some(format!("slug example `{}` is listed twice", pair[0]))
        } else if pair[0] > pair[1] {
            Some(format!(
                "slug example `{}` is listed before `{}`; keep slug examples alphabetical",
                pair[0], pair[1]
            ))
        } else {
            None
        }
    })
}

/// The spec id a file name carries: `RIPR-SPEC-NNNN-<slug>.md` or
/// `RIPR-SPEC-NNNN.md`.
fn spec_id_of(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".md")?;
    let digits = stem.strip_prefix(SPEC_PREFIX)?;
    let number = digits.get(..4)?;
    let rest = digits.get(4..)?;
    (number.chars().all(|c| c.is_ascii_digit()) && (rest.is_empty() || rest.starts_with('-')))
        .then(|| format!("{SPEC_PREFIX}{number}"))
}

pub(crate) fn scan_specs(dir: &Path) -> Result<SpecExamples, String> {
    let mut specs = SpecExamples::new();
    let entries =
        fs::read_dir(dir).map_err(|err| format!("read {}: {err}", normalize_path(dir)))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read {}: {err}", normalize_path(dir)))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = spec_id_of(&name) else {
            continue;
        };
        let path = entry.path();
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("read {}: {err}", normalize_path(&path)))?;
        let items = acceptance_items(&text);
        if let Some(problem) = slug_order_violation(&items) {
            return Err(format!("{}: {problem}", normalize_path(&path)));
        }
        if specs
            .insert(id.clone(), items.into_iter().collect())
            .is_some()
        {
            return Err(format!("two spec files carry the id `{id}`"));
        }
    }
    Ok(specs)
}

pub(crate) fn load_ledger(corpus_dir: &Path) -> Result<Ledger, String> {
    let path = corpus_dir.join(LEDGER_FILE);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("read {}: {err}", normalize_path(&path)))?;
    let mut ledger: Ledger =
        toml::from_str(&text).map_err(|err| format!("parse {}: {err}", normalize_path(&path)))?;
    if !ledger.known_false.is_empty() {
        return Err(format!(
            "{} lists `[[known_false]]` inline; one file per row under `{}/` keeps parallel fixes from conflicting",
            normalize_path(&path),
            super::known_false::KNOWN_FALSE_DIR
        ));
    }
    if ledger.missed_credit_ceiling.is_some() {
        return Err(format!(
            "{} carries `missed_credit_ceiling`; sorted `[[entry]]` blocks in `{}` replace that shared line, which every ceiling-moving PR conflicted on",
            normalize_path(&path),
            super::known_false::MISSED_CREDIT_FILE
        ));
    }
    ledger.known_false = super::known_false::load_entries(corpus_dir)?;
    ledger.missed_credit = super::known_false::load_missed_credit_entries(corpus_dir)?;
    Ok(ledger)
}

/// `RIPR-SPEC-NNNN#K` or `RIPR-SPEC-NNNN#slug` as (`RIPR-SPEC-NNNN`, key).
/// K is a positive integer without leading zeros and a slug is lowercase
/// hyphenated words, so one example has one spelling.
pub(crate) fn parse_example_id(id: &str) -> Option<(String, ExampleKey)> {
    let (spec, example) = id.split_once('#')?;
    let digits = spec.strip_prefix(SPEC_PREFIX)?;
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let key = if is_slug(example) {
        ExampleKey::Slug(example.to_string())
    } else if !example.is_empty()
        && !example.starts_with('0')
        && example.chars().all(|c| c.is_ascii_digit())
    {
        ExampleKey::Number(example.parse().ok()?)
    } else {
        return None;
    };
    Some((spec.to_string(), key))
}

/// Every spec example a case cites, after id validation.
fn citations(corpus: &Corpus) -> BTreeMap<(String, ExampleKey), Vec<String>> {
    let mut cited: BTreeMap<(String, ExampleKey), Vec<String>> = BTreeMap::new();
    for case in &corpus.cases {
        for id in &case.spec_examples {
            if let Some(key) = parse_example_id(id) {
                cited.entry(key).or_default().push(case.case_id.clone());
            }
        }
    }
    cited
}

fn examples_text(examples: &BTreeSet<ExampleKey>) -> String {
    if examples.is_empty() {
        return "none".to_string();
    }
    examples
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ledger and citation law. A spec with numbered examples that the ledger
/// does not name fails here, so a new spec cannot leave the denominator
/// silently.
pub(crate) fn coverage_violations(
    corpus: &Corpus,
    ledger: &Ledger,
    specs: &SpecExamples,
) -> Vec<String> {
    let mut violations = Vec::new();
    if ledger.schema_version != LEDGER_SCHEMA {
        violations.push(format!(
            "{LEDGER_FILE} schema_version `{}` is not `{LEDGER_SCHEMA}`",
            ledger.schema_version
        ));
    }
    let mut named = BTreeSet::new();
    let mut scopes: BTreeMap<&str, &LedgerSpec> = BTreeMap::new();
    for entry in &ledger.spec {
        if !named.insert(entry.id.as_str()) {
            violations.push(format!("{LEDGER_FILE} names `{}` twice", entry.id));
        }
        scopes.insert(entry.id.as_str(), entry);
        let Some(examples) = specs.get(&entry.id) else {
            violations.push(format!(
                "{LEDGER_FILE} names `{}`, but no docs/specs file carries that id",
                entry.id
            ));
            continue;
        };
        if examples.is_empty() {
            violations.push(format!(
                "{LEDGER_FILE} scopes `{}`, which has no numbered acceptance examples; list it under [[unmeasured]] or remove it",
                entry.id
            ));
        }
        let reason_empty = entry
            .reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty());
        if entry.scope == Scope::Out && reason_empty {
            violations.push(format!(
                "{LEDGER_FILE} puts `{}` out of scope without a reason",
                entry.id
            ));
        }
        if entry.scope == Scope::Out && !entry.waived.is_empty() {
            violations.push(format!(
                "{LEDGER_FILE} waives examples of out-of-scope `{}`; waivers apply only to in-scope specs",
                entry.id
            ));
        }
        let mut waived = BTreeSet::new();
        for waiver in &entry.waived {
            if !waived.insert(waiver.example.clone()) {
                violations.push(format!(
                    "{LEDGER_FILE} waives `{}#{}` twice",
                    entry.id, waiver.example
                ));
            }
            if !examples.contains(&waiver.example) {
                violations.push(format!(
                    "{LEDGER_FILE} waives `{}#{}`, which is not an acceptance example (examples: {})",
                    entry.id,
                    waiver.example,
                    examples_text(examples)
                ));
            }
            if waiver.reason.trim().is_empty() {
                violations.push(format!(
                    "{LEDGER_FILE} waives `{}#{}` without a reason",
                    entry.id, waiver.example
                ));
            }
        }
    }
    for entry in &ledger.unmeasured {
        if !named.insert(entry.id.as_str()) {
            violations.push(format!("{LEDGER_FILE} names `{}` twice", entry.id));
        }
        match specs.get(&entry.id) {
            None => violations.push(format!(
                "{LEDGER_FILE} lists unmeasured `{}`, but no docs/specs file carries that id",
                entry.id
            )),
            Some(examples) if !examples.is_empty() => violations.push(format!(
                "{LEDGER_FILE} lists `{}` as unmeasured, but it now has numbered acceptance examples ({}); scope it under [[spec]]",
                entry.id,
                examples_text(examples)
            )),
            Some(_) => {}
        }
        if entry.reason.trim().is_empty() {
            violations.push(format!(
                "{LEDGER_FILE} lists unmeasured `{}` without a reason",
                entry.id
            ));
        }
    }
    for (id, examples) in specs {
        if !examples.is_empty() && !scopes.contains_key(id.as_str()) {
            violations.push(format!(
                "`{id}` has numbered acceptance examples but is missing from {LEDGER_FILE}; add a [[spec]] entry with scope \"in\" or \"out\""
            ));
        }
    }
    for case in &corpus.cases {
        let mut seen = BTreeSet::new();
        for id in &case.spec_examples {
            if !seen.insert(id.as_str()) {
                violations.push(format!("case `{}` cites `{id}` twice", case.case_id));
            }
            let Some((spec, number)) = parse_example_id(id) else {
                violations.push(format!(
                    "case `{}` spec example `{id}` is not `RIPR-SPEC-NNNN#K` or `RIPR-SPEC-NNNN#slug`",
                    case.case_id
                ));
                continue;
            };
            let Some(examples) = specs.get(&spec) else {
                violations.push(format!(
                    "case `{}` cites `{id}`, but no docs/specs file carries `{spec}`",
                    case.case_id
                ));
                continue;
            };
            if !examples.contains(&number) {
                violations.push(format!(
                    "case `{}` cites `{id}`, but `{spec}` has no acceptance example {number} (examples: {})",
                    case.case_id,
                    examples_text(examples)
                ));
                continue;
            }
            match scopes.get(spec.as_str()) {
                Some(entry) if entry.scope == Scope::Out => violations.push(format!(
                    "case `{}` cites `{id}`, but {LEDGER_FILE} puts `{spec}` out of scope; scope it in or drop the citation",
                    case.case_id
                )),
                Some(entry) if entry.waived.iter().any(|w| w.example == number) => {
                    violations.push(format!(
                        "`{id}` is both waived in {LEDGER_FILE} and covered by case `{}`; drop the waiver",
                        case.case_id
                    ))
                }
                _ => {}
            }
        }
    }
    violations.extend(axis_violations(corpus, ledger));
    let coverage = spec_example_coverage(corpus, ledger, specs);
    if ledger.floor > coverage.coverage.denominator {
        violations.push(format!(
            "{LEDGER_FILE} floor {} exceeds the {} coverable examples",
            ledger.floor, coverage.coverage.denominator
        ));
    }
    violations
}

/// Cases per declared axis, in ledger order.
fn axis_counts(corpus: &Corpus, ledger: &Ledger) -> Vec<(String, usize, usize)> {
    ledger
        .axis
        .iter()
        .map(|axis| {
            let count = corpus
                .cases
                .iter()
                .filter(|case| case.axes.iter().any(|tag| tag == &axis.id))
                .count();
            (axis.id.clone(), count, axis.floor)
        })
        .collect()
}

/// Axis law: every tag a case carries is declared once in the ledger, and
/// no axis holds fewer cases than its floor. A deleted or retagged case
/// fails here, so an axis cannot shrink silently.
fn axis_violations(corpus: &Corpus, ledger: &Ledger) -> Vec<String> {
    let mut violations = Vec::new();
    let mut declared = BTreeSet::new();
    for axis in &ledger.axis {
        if !declared.insert(axis.id.as_str()) {
            violations.push(format!("{LEDGER_FILE} declares axis `{}` twice", axis.id));
        }
        if axis.id.trim().is_empty() {
            violations.push(format!("{LEDGER_FILE} declares an axis without an id"));
        }
        if axis.description.trim().is_empty() {
            violations.push(format!(
                "{LEDGER_FILE} declares axis `{}` without a description",
                axis.id
            ));
        }
    }
    for case in &corpus.cases {
        let mut seen = BTreeSet::new();
        for tag in &case.axes {
            if !seen.insert(tag.as_str()) {
                violations.push(format!(
                    "case `{}` carries axis `{tag}` twice",
                    case.case_id
                ));
            }
            if !declared.contains(tag.as_str()) {
                violations.push(format!(
                    "case `{}` carries axis `{tag}`, which {LEDGER_FILE} does not declare",
                    case.case_id
                ));
            }
        }
    }
    for (id, count, floor) in axis_counts(corpus, ledger) {
        if count < floor {
            violations.push(format!(
                "axis `{id}` holds {count} cases, below its {LEDGER_FILE} floor {floor}"
            ));
        }
    }
    violations
}

/// The one-line axis summary `check` prints, for a ledgered corpus that
/// declares axes. A `--cases` run (`selected`) prints the counts of what it
/// ran without the floors, which it does not enforce.
pub(crate) fn axis_note(
    corpus_dir: &Path,
    corpus: &Corpus,
    selected: bool,
) -> Result<Option<String>, String> {
    if !corpus_dir.join(LEDGER_FILE).is_file() {
        return Ok(None);
    }
    let ledger = load_ledger(corpus_dir)?;
    if ledger.axis.is_empty() {
        return Ok(None);
    }
    let parts: Vec<String> = axis_counts(corpus, &ledger)
        .into_iter()
        .map(|(id, count, floor)| {
            if selected {
                format!("{id} {count}")
            } else {
                format!("{id} {count} (floor {floor})")
            }
        })
        .collect();
    let scope = if selected {
        " (selected cases only)"
    } else {
        ""
    };
    Ok(Some(format!("coverage axes{scope} {}", parts.join(", "))))
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SpecExampleCoverage {
    pub(crate) ledger: String,
    pub(crate) floor: usize,
    /// Covered over in-scope examples minus waived ones.
    pub(crate) coverage: Ratio,
    /// Covered plus waived over all in-scope examples. Waived examples are
    /// accounted for, never covered.
    pub(crate) accounted: Ratio,
    pub(crate) in_scope_examples: usize,
    pub(crate) covered_examples: usize,
    pub(crate) waived_examples: usize,
    pub(crate) in_scope_specs: usize,
    pub(crate) out_of_scope_specs: usize,
    pub(crate) unmeasured_specs: Vec<String>,
    pub(crate) by_spec: Vec<SpecCoverageRow>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SpecCoverageRow {
    pub(crate) spec: String,
    pub(crate) in_scope_examples: usize,
    pub(crate) covered: usize,
    pub(crate) waived: usize,
    pub(crate) uncovered: Vec<ExampleKey>,
}

/// Coverage for one corpus directory without going through `expected_report()`.
///
/// The dx-scoreboard reads this so tempdir corpora that only need row rates
/// never scan `docs/specs`. Specs come from the repository that owns
/// `fixtures/<corpus>/`, not from CWD or an ancestor walk that would hit
/// this repo's specs when tests write under `target/` (#7134). Corpus
/// `validate` and `coverage_violations` run first: an invalid case or
/// ledger is a failed instrument, not a measured rate.
pub(crate) fn spec_example_coverage_for_corpus(
    corpus_dir: &Path,
) -> Result<SpecExampleCoverage, String> {
    let corpus = super::load_corpus(corpus_dir)?;
    let ledger = load_ledger(corpus_dir)?;
    let specs = scan_specs(&specs_dir_for_corpus(corpus_dir)?)?;
    let mut violations = super::validate(&corpus, corpus_dir);
    violations.extend(coverage_violations(&corpus, &ledger, &specs));
    violations.extend(super::oracle::oracle_ledger_violations(
        &corpus, &ledger, &specs,
    ));
    if !violations.is_empty() {
        return Err(format!(
            "verdict corpus is invalid:\n- {}",
            violations.join("\n- ")
        ));
    }
    let mut coverage = spec_example_coverage(&corpus, &ledger, &specs);
    coverage.ledger = ledger_label(corpus_dir);
    Ok(coverage)
}

/// The ledger path messages name: `fixtures/<corpus>/spec-coverage.toml`
/// for the corpus actually read, so a Python floor failure does not point
/// at the Rust ledger.
pub(crate) fn ledger_label(corpus_dir: &Path) -> String {
    match corpus_dir.file_name().and_then(|name| name.to_str()) {
        Some(name) => format!("fixtures/{name}/{LEDGER_FILE}"),
        None => normalize_path(&corpus_dir.join(LEDGER_FILE)),
    }
}

fn specs_dir_for_corpus(corpus_dir: &Path) -> Result<PathBuf, String> {
    if let Some(root) = corpus_dir
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "fixtures"))
        .and_then(Path::parent)
    {
        let specs = root.join(SPECS_DIR);
        if specs.is_dir() {
            return Ok(specs);
        }
    }
    Err(format!(
        "no {SPECS_DIR} beside {}; spec-example coverage reads numbered examples from the repository that owns fixtures/<corpus>/, not from CWD or expected_report",
        normalize_path(corpus_dir)
    ))
}

pub(crate) fn spec_example_coverage(
    corpus: &Corpus,
    ledger: &Ledger,
    specs: &SpecExamples,
) -> SpecExampleCoverage {
    let cited = citations(corpus);
    let mut by_spec = Vec::new();
    let (mut total, mut covered, mut waived) = (0, 0, 0);
    let mut in_scope_specs = 0;
    let mut out_of_scope_specs = 0;
    for entry in &ledger.spec {
        if entry.scope == Scope::Out {
            out_of_scope_specs += 1;
            continue;
        }
        in_scope_specs += 1;
        let examples = specs.get(&entry.id).cloned().unwrap_or_default();
        let waivers: BTreeSet<ExampleKey> = entry
            .waived
            .iter()
            .map(|w| w.example.clone())
            .filter(|n| examples.contains(n))
            .collect();
        let mut row_covered = 0;
        let mut uncovered = Vec::new();
        for number in &examples {
            if waivers.contains(number) {
                continue;
            }
            if cited.contains_key(&(entry.id.clone(), number.clone())) {
                row_covered += 1;
            } else {
                uncovered.push(number.clone());
            }
        }
        total += examples.len();
        covered += row_covered;
        waived += waivers.len();
        by_spec.push(SpecCoverageRow {
            spec: entry.id.clone(),
            in_scope_examples: examples.len(),
            covered: row_covered,
            waived: waivers.len(),
            uncovered,
        });
    }
    by_spec.sort_by(|a, b| a.spec.cmp(&b.spec));
    let mut unmeasured: Vec<String> = ledger.unmeasured.iter().map(|u| u.id.clone()).collect();
    unmeasured.sort();
    SpecExampleCoverage {
        ledger: format!("{}/{LEDGER_FILE}", super::CORPUS_DIR),
        floor: ledger.floor,
        coverage: ratio(covered, total - waived),
        accounted: ratio(covered + waived, total),
        in_scope_examples: total,
        covered_examples: covered,
        waived_examples: waived,
        in_scope_specs,
        out_of_scope_specs,
        unmeasured_specs: unmeasured,
        by_spec,
    }
}

/// The floor gate. Below the floor fails with what to do; above it passes
/// and says the floor can be raised.
pub(crate) fn floor_gate(coverage: &SpecExampleCoverage) -> Result<Option<String>, String> {
    let covered = coverage.covered_examples;
    let floor = coverage.floor;
    if covered < floor {
        return Err(format!(
            "verdict-corpus: spec-example coverage fell to {covered}/{} below the floor {floor} in {}; restore the `spec_examples` citations a case lost, or lower `floor` with the reason in the PR when a case was deliberately retired",
            coverage.coverage.denominator, coverage.ledger
        ));
    }
    if covered > floor {
        return Ok(Some(format!(
            "verdict-corpus: spec-example coverage {covered}/{} is above the floor {floor}; raise `floor` in {} to {covered} to protect it",
            coverage.coverage.denominator, coverage.ledger
        )));
    }
    Ok(None)
}

pub(crate) fn render_coverage_markdown(coverage: &SpecExampleCoverage) -> String {
    let mut out = String::new();
    out.push_str("\n## Spec example coverage\n\n");
    out.push_str(&format!(
        "Numbered acceptance examples of in-scope specs that at least one case cites in `spec_examples` ({}). Covered {}/{} ({}); floor {}. Waived {} and accounted (covered plus waived) {}/{}. In-scope specs {}, out-of-scope specs {}.\n",
        coverage.ledger,
        coverage.coverage.numerator,
        coverage.coverage.denominator,
        coverage.coverage.rate,
        coverage.floor,
        coverage.waived_examples,
        coverage.accounted.numerator,
        coverage.accounted.denominator,
        coverage.in_scope_specs,
        coverage.out_of_scope_specs,
    ));
    out.push_str(&format!(
        "\nUnmeasured in-scope specs (prose acceptance examples, not counted): {}.\n",
        if coverage.unmeasured_specs.is_empty() {
            "none".to_string()
        } else {
            coverage.unmeasured_specs.join(", ")
        }
    ));
    out.push_str("\n| Spec | In-scope examples | Covered | Waived | Uncovered |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for row in &coverage.by_spec {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            row.spec,
            row.in_scope_examples,
            row.covered,
            row.waived,
            if row.uncovered.is_empty() {
                "none".to_string()
            } else {
                row.uncovered
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
    }
    out
}

#[cfg(test)]
#[path = "verdict_corpus_coverage_tests.rs"]
mod tests;
