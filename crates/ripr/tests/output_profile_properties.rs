//! Rendering-profile properties of `ripr check` over generated workspaces.
//!
//! The check report has several presentations of one document. Each one
//! promises that nothing is lost:
//!
//! 1. `json-compact` parses to the same value as `json`;
//! 2. `sarif-compact` parses to the same value as `sarif`;
//! 3. `json-terse` decodes, by the rules `docs/OUTPUT_SCHEMA.md` publishes
//!    under "json-terse", to exactly the `json` document;
//! 4. `json-terse` keeps the identity and verdict fields of every finding
//!    verbatim, before any decoding: `id`, `classification`, `severity`,
//!    `confidence`, `probe`, and each `ripr` stage `state` and `confidence`.
//!
//! The decoder below is written from the published rules, not from the
//! producer, so relation 3 holds the documentation and the renderer to each
//! other: a field the renderer codes or drops without a published way back
//! fails it, and so does a published rule the renderer does not follow.
//!
//! The suite drives the public library API ([`ripr::check_workspace`] and
//! [`ripr::render_check`]) over generated Rust workspaces with one to four
//! changed comparisons. Related tests repeat across findings, so the terse
//! profile has prose to code, rows to hoist and aliases to drop. Every case
//! first asserts its fixture: one predicate finding at each changed line.
//! A fixed subject shows the generator reaches each terse coding path, and
//! controls show each comparison can fail. These are static classes; nothing
//! here claims a runtime mutation outcome.
//!
//! The default case count is small so the required test run stays cheap. The
//! scheduled and manual `fuzz.yml` workflow raises it with `PROPTEST_CASES`
//! (400). Raise it locally with `PROPTEST_CASES=200 cargo test -p ripr --test
//! output_profile_properties`.

use proptest::prelude::*;
use proptest::test_runner::Config;
use ripr::{
    CheckInput, CheckOutput, Mode, OutputFormat, ProbeFamily, check_workspace, render_check,
};
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_CASES: u32 = 8;
const CRATE: &str = "output_profile_subject";

fn config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CASES),
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..Config::default()
    }
}

/// One changed comparison in its own module file.
#[derive(Clone, Debug)]
struct Module {
    threshold: u64,
    /// Whether the module's own test pins the boundary value.
    pinned: bool,
    /// Whether the module's own test asserts equality instead of truth.
    equality: bool,
}

/// A generated workspace: one to four module files, each with one changed
/// comparison, and one integration test file.
#[derive(Clone, Debug)]
struct Subject {
    modules: Vec<Module>,
    /// Whether one extra test calls every gate, so its row repeats across
    /// findings.
    shared: bool,
}

impl Subject {
    fn path(index: usize) -> String {
        format!("src/part{index}.rs")
    }

    /// Module source and the 1-based line of the changed expression.
    fn source(index: usize, module: &Module, changed: bool) -> (String, usize) {
        let op = if changed { ">=" } else { ">" };
        let text = format!(
            "pub fn gate{index}(amount: u64) -> bool {{\n    amount {op} {}\n}}\n",
            module.threshold
        );
        (text, 2)
    }

    fn diff(&self) -> String {
        let mut diff = String::new();
        for (index, module) in self.modules.iter().enumerate() {
            let (old, _) = Self::source(index, module, false);
            let (new, _) = Self::source(index, module, true);
            let path = Self::path(index);
            diff.push_str(&format!(
                "diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n"
            ));
            for (before, after) in old.lines().zip(new.lines()) {
                if before == after {
                    diff.push_str(&format!(" {before}\n"));
                } else {
                    diff.push_str(&format!("-{before}\n+{after}\n"));
                }
            }
        }
        diff
    }

    fn files(&self) -> Vec<(String, String)> {
        let mut lib = String::new();
        let mut tests = format!("use {CRATE}::*;\n");
        let mut files = Vec::new();
        for (index, module) in self.modules.iter().enumerate() {
            lib.push_str(&format!("mod part{index};\npub use part{index}::*;\n"));
            let input = if module.pinned {
                module.threshold
            } else {
                module.threshold + 7
            };
            let assertion = if module.equality {
                format!("assert_eq!(gate{index}({input}), true);")
            } else {
                format!("assert!(gate{index}({input}));")
            };
            tests.push_str(&format!(
                "\n#[test]\nfn gate{index}_checked() {{\n    {assertion}\n}}\n"
            ));
            files.push((Self::path(index), Self::source(index, module, true).0));
        }
        if self.shared {
            tests.push_str("\n#[test]\nfn every_gate_opens_far_above() {\n");
            for index in 0..self.modules.len() {
                tests.push_str(&format!("    assert!(gate{index}(100_000));\n"));
            }
            tests.push_str("}\n");
        }
        files.push(("src/lib.rs".to_string(), lib));
        files.push(("tests/case.rs".to_string(), tests));
        files.push((
            "Cargo.toml".to_string(),
            "[package]\nname = \"output-profile-subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n"
                .to_string(),
        ));
        files
    }
}

fn subject() -> impl Strategy<Value = Subject> {
    (
        prop::collection::vec((2u64..500, any::<bool>(), any::<bool>()), 1..5),
        any::<bool>(),
    )
        .prop_map(|(modules, shared)| Subject {
            modules: modules
                .into_iter()
                .map(|(threshold, pinned, equality)| Module {
                    threshold,
                    pinned,
                    equality,
                })
                .collect(),
            shared,
        })
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(files: &[(String, String)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-fuzz-output-profile-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self { root };
        for (name, content) in files {
            repo.write(name, content)?;
        }
        Ok(repo)
    }

    fn write(&self, name: &str, content: &str) -> Result<(), String> {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
        }
        std::fs::write(&path, content)
            .map_err(|error| format!("write {} failed: {error}", path.display()))
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Analyzes a subject and asserts its fixture: exactly one predicate finding
/// at each changed line, and none away from the changed lines.
fn analyzed(subject: &Subject) -> Result<CheckOutput, String> {
    let repo = TempRepo::create(&subject.files())?;
    repo.write("change.diff", &subject.diff())?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        diff_file: Some(repo.root.join("change.diff")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    for (index, module) in subject.modules.iter().enumerate() {
        let path = Subject::path(index);
        let (_, line) = Subject::source(index, module, true);
        let count = output
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::Predicate
                    && finding.probe.location.line == line
                    && finding.probe.location.file.ends_with(&path)
            })
            .count();
        if count != 1 {
            return Err(format!(
                "fixture produced {count} predicate findings at {path}:{line}"
            ));
        }
    }
    if let Some(stray) = output.findings.iter().find(|finding| {
        finding.probe.location.line != 2
            || !finding
                .probe
                .location
                .file
                .to_string_lossy()
                .replace('\\', "/")
                .contains("src/part")
    }) {
        return Err(format!(
            "fixture produced a finding away from the changed lines: {}:{}",
            stray.probe.location.file.display(),
            stray.probe.location.line
        ));
    }
    Ok(output)
}

fn parsed(output: &CheckOutput, format: &OutputFormat) -> Result<Value, String> {
    let text = render_check(output, format)?;
    serde_json::from_str(&text).map_err(|error| format!("{format:?} did not parse: {error}"))
}

/// Collects the JSON paths at which two documents differ.
fn collect_differences(path: &str, before: &Value, after: &Value, found: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in left {
                match right.get(key) {
                    Some(other) => {
                        collect_differences(&format!("{path}.{key}"), value, other, found);
                    }
                    None => found.push(format!("{path}.{key} (only in the first)")),
                }
            }
            for key in right.keys().filter(|key| !left.contains_key(*key)) {
                found.push(format!("{path}.{key} (only in the second)"));
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (value, other)) in left.iter().zip(right).enumerate() {
                collect_differences(&format!("{path}[{index}]"), value, other, found);
            }
        }
        _ => {
            if before != after {
                found.push(path.to_string());
            }
        }
    }
}

fn same(label: &str, expected: &Value, actual: &Value) -> Result<(), String> {
    let mut found = Vec::new();
    collect_differences("", expected, actual, &mut found);
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!("{label} differs at: {found:?}"))
    }
}

// The published `json-terse` rules (docs/OUTPUT_SCHEMA.md), as a consumer
// would apply them.

const RENDERING: &str = "rendering";
const LEGEND: &str = "evidence_legend";
const DROPPED_ALIASES: &str = "dropped_aliases";
const TABLE: &str = "related_test_table";
const TABLE_ROW: &str = "table_row";
const STAGES: [&str; 5] = ["reach", "infect", "propagate", "observe", "discriminate"];

/// Replaces a coded string by its legend template.
fn expand(value: &mut Value, legend: &Map<String, Value>) {
    if let Some(text) = value.as_str()
        && let Some(template) = legend.get(text)
    {
        *value = template.clone();
    }
}

/// Expands every string in an array or in the values of an object.
fn expand_all(value: Option<&mut Value>, legend: &Map<String, Value>) {
    match value {
        Some(Value::Array(items)) => items.iter_mut().for_each(|item| expand(item, legend)),
        Some(Value::Object(entries)) => entries.values_mut().for_each(|item| expand(item, legend)),
        _ => {}
    }
}

/// Decodes a `json-terse` document into the `json` document it encodes.
fn decode_terse(terse: &Value) -> Result<Value, String> {
    let mut document = terse.clone();
    let head = document
        .as_object_mut()
        .ok_or("terse document is not an object")?;
    if head.remove(RENDERING) != Some(Value::from("terse")) {
        return Err("terse document does not name its rendering".to_string());
    }
    let legend = match head.remove(LEGEND) {
        Some(Value::Object(legend)) => legend,
        None => Map::new(),
        Some(other) => return Err(format!("evidence_legend is not an object: {other}")),
    };
    let dropped: Vec<String> = match head.remove(DROPPED_ALIASES) {
        Some(Value::Array(names)) => names
            .iter()
            .map(|name| name.as_str().map(str::to_string).ok_or("alias name"))
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
        Some(other) => return Err(format!("dropped_aliases is not an array: {other}")),
    };
    let table = match head.remove(TABLE) {
        Some(Value::Array(rows)) => Some(rows),
        None => None,
        Some(other) => return Err(format!("related_test_table is not an array: {other}")),
    };
    let findings = head
        .get_mut("findings")
        .and_then(Value::as_array_mut)
        .ok_or("terse document has no findings array")?;
    for finding in findings {
        let finding = finding.as_object_mut().ok_or("finding is not an object")?;
        for key in ["evidence_path", "evidence", "missing", "assertion_texts"] {
            expand_all(finding.get_mut(key), &legend);
        }
        if let Some(ripr) = finding.get_mut("ripr").and_then(Value::as_object_mut) {
            for stage in ripr.values_mut() {
                if let Some(summary) = stage.get_mut("summary") {
                    expand(summary, &legend);
                }
            }
        }
        if let Some(rows) = finding
            .get_mut("related_tests")
            .and_then(Value::as_array_mut)
        {
            for row in rows {
                let row = row.as_object_mut().ok_or("related test row")?;
                if let Some(table) = &table
                    && let Some(index) = row.remove(TABLE_ROW)
                {
                    let core = index
                        .as_u64()
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| table.get(index))
                        .and_then(Value::as_object)
                        .ok_or_else(|| format!("table_row {index} has no table entry"))?;
                    for (key, value) in core {
                        if row.insert(key.clone(), value.clone()).is_some() {
                            return Err(format!("table row and its core both carry {key}"));
                        }
                    }
                }
                for key in ["oracle", "why"] {
                    if let Some(value) = row.get_mut(key) {
                        expand(value, &legend);
                    }
                }
            }
        }
        restore_aliases(finding, &dropped)?;
    }
    Ok(document)
}

/// Restores the aliases the terse profile is allowed to omit.
fn restore_aliases(finding: &mut Map<String, Value>, dropped: &[String]) -> Result<(), String> {
    let declared = |name: &str| dropped.iter().any(|alias| alias == name);
    if declared("suggested_next_action") && !finding.contains_key("suggested_next_action") {
        let owner = finding
            .get("recommended_next_step")
            .cloned()
            .ok_or("suggested_next_action dropped without its owner")?;
        finding.insert("suggested_next_action".to_string(), owner);
    }
    for alias in ["observed_values", "missing_discriminators"] {
        if declared(alias) && !finding.contains_key(alias) {
            let owner = finding
                .get("activation")
                .and_then(|activation| activation.get(alias))
                .cloned()
                .ok_or_else(|| format!("{alias} dropped without its owner"))?;
            finding.insert(alias.to_string(), owner);
        }
    }
    if declared("canonical_gap_id") && !finding.contains_key("canonical_gap_id") {
        let owner = finding
            .get("canonical_gap")
            .and_then(|gap| gap.get("id"))
            .cloned()
            .ok_or("canonical_gap_id dropped without its owner")?;
        finding.insert("canonical_gap_id".to_string(), owner);
    }
    if declared("evidence_stages") && !finding.contains_key("evidence_stages") {
        let ripr = finding
            .get("ripr")
            .cloned()
            .ok_or("evidence_stages dropped without ripr")?;
        let rows = STAGES
            .iter()
            .map(|stage| {
                serde_json::json!({
                    "stage": stage,
                    "state": ripr[stage]["state"],
                    "confidence": ripr[stage]["confidence"],
                })
            })
            .collect();
        finding.insert("evidence_stages".to_string(), Value::Array(rows));
    }
    Ok(())
}

/// Relation 4: the identity and verdict fields of every finding, read
/// straight from a document without decoding.
fn verdicts(document: &Value) -> Result<Vec<Value>, String> {
    let findings = document
        .get("findings")
        .and_then(Value::as_array)
        .ok_or("document has no findings array")?;
    Ok(findings
        .iter()
        .map(|finding| {
            let stages: Vec<Value> = STAGES
                .iter()
                .map(|stage| {
                    serde_json::json!([
                        finding["ripr"][stage]["state"],
                        finding["ripr"][stage]["confidence"],
                    ])
                })
                .collect();
            serde_json::json!({
                "id": finding["id"],
                "classification": finding["classification"],
                "severity": finding["severity"],
                "confidence": finding["confidence"],
                "probe": finding["probe"],
                "stages": stages,
            })
        })
        .collect())
}

/// Every profile relation over one analysis.
fn profiles_agree(output: &CheckOutput) -> Result<(), String> {
    let json = parsed(output, &OutputFormat::Json)?;
    same(
        "json-compact against json",
        &json,
        &parsed(output, &OutputFormat::JsonCompact)?,
    )?;
    same(
        "sarif-compact against sarif",
        &parsed(output, &OutputFormat::Sarif)?,
        &parsed(output, &OutputFormat::SarifCompact)?,
    )?;
    let terse = parsed(output, &OutputFormat::JsonTerse)?;
    same(
        "decoded json-terse against json",
        &json,
        &decode_terse(&terse)?,
    )?;
    if verdicts(&json)? != verdicts(&terse)? {
        return Err("json-terse changed an identity or verdict field".to_string());
    }
    Ok(())
}

fn fail(message: String) -> TestCaseError {
    TestCaseError::fail(message)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn every_profile_carries_the_same_report(subject in subject()) {
        let output = analyzed(&subject).map_err(fail)?;
        profiles_agree(&output).map_err(fail)?;
    }
}

/// A subject with repeated related tests, used by the coverage and control
/// tests below.
fn rich_subject() -> Subject {
    let module = |threshold, pinned, equality| Module {
        threshold,
        pinned,
        equality,
    };
    Subject {
        modules: vec![
            module(10, true, false),
            module(20, false, true),
            module(30, false, false),
            module(40, true, true),
        ],
        shared: true,
    }
}

/// The generator reaches every terse coding path, so relation 3 decodes
/// legend codes, hoisted rows and dropped aliases, not just verbatim text.
#[test]
fn the_rich_subject_reaches_every_terse_coding_path() -> Result<(), String> {
    let output = analyzed(&rich_subject())?;
    let terse = parsed(&output, &OutputFormat::JsonTerse)?;
    let legend = terse
        .get(LEGEND)
        .and_then(Value::as_object)
        .ok_or("no evidence_legend")?;
    if legend.is_empty() {
        return Err("evidence_legend is empty".to_string());
    }
    let table = terse
        .get(TABLE)
        .and_then(Value::as_array)
        .ok_or("no related_test_table")?;
    if table.is_empty() {
        return Err("related_test_table is empty".to_string());
    }
    let dropped = terse
        .get(DROPPED_ALIASES)
        .and_then(Value::as_array)
        .ok_or("no dropped_aliases")?;
    if dropped.is_empty() {
        return Err("dropped_aliases is empty".to_string());
    }
    profiles_agree(&output)
}

/// Control: removing one legend entry leaves a code undecoded, and the
/// decoded document no longer matches.
#[test]
fn a_missing_legend_entry_is_reported() -> Result<(), String> {
    let output = analyzed(&rich_subject())?;
    let json = parsed(&output, &OutputFormat::Json)?;
    let mut terse = parsed(&output, &OutputFormat::JsonTerse)?;
    let legend = terse
        .get_mut(LEGEND)
        .and_then(Value::as_object_mut)
        .ok_or("no evidence_legend")?;
    let first = legend.keys().next().cloned().ok_or("empty legend")?;
    legend.remove(&first);
    match same(
        "decoded json-terse against json",
        &json,
        &decode_terse(&terse)?,
    ) {
        Err(_) => Ok(()),
        Ok(()) => Err(format!("removing legend entry {first} went unnoticed")),
    }
}

/// Control: a field omitted without a published rule is reported.
#[test]
fn an_undeclared_omission_is_reported() -> Result<(), String> {
    let output = analyzed(&rich_subject())?;
    let json = parsed(&output, &OutputFormat::Json)?;
    let mut terse = parsed(&output, &OutputFormat::JsonTerse)?;
    let finding = terse
        .pointer_mut("/findings/0")
        .and_then(Value::as_object_mut)
        .ok_or("no first finding")?;
    finding
        .remove("language")
        .ok_or("first finding has no language")?;
    match same(
        "decoded json-terse against json",
        &json,
        &decode_terse(&terse)?,
    ) {
        Err(_) => Ok(()),
        Ok(()) => Err("dropping language went unnoticed".to_string()),
    }
}

/// Control: a verdict changed only in the terse profile is reported by
/// relation 4 even when the decoder would not touch that field.
#[test]
fn a_changed_verdict_is_reported() -> Result<(), String> {
    let output = analyzed(&rich_subject())?;
    let json = parsed(&output, &OutputFormat::Json)?;
    let mut terse = parsed(&output, &OutputFormat::JsonTerse)?;
    let classification = terse
        .pointer_mut("/findings/0/classification")
        .ok_or("no first classification")?;
    *classification = Value::from("static_unknown");
    if json.pointer("/findings/0/classification") == Some(&Value::from("static_unknown")) {
        return Err("control premise: the first finding is already static_unknown".to_string());
    }
    if verdicts(&json)? == verdicts(&terse)? {
        return Err("a changed classification went unnoticed".to_string());
    }
    Ok(())
}
