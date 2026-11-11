//! Diff-scope properties of `ripr check` over generated workspaces.
//!
//! `ripr` answers a question about the behavior changed in a diff, so the
//! answer may depend only on the changed analyzable source. The neighbouring
//! suites cover other relations: `stage_metamorphic` edits unrelated code in
//! the changed file, `metamorphic_relations` edits the tests, and xtask
//! `output_properties` checks that finding ids survive reordering. Nothing
//! generated covered the diff itself, so this suite drives the public library
//! API ([`ripr::check_workspace`]) over one generated workspace and varies only
//! the diff text:
//!
//! 1. a diff that changes no analyzable source (empty, blank, or only
//!    documentation, data and binary sections) reports no findings;
//! 2. adding sections for unrelated documentation, data and binary files to a
//!    diff leaves the report unchanged;
//! 3. the order of the file sections in a diff leaves the report unchanged,
//!    including the order of the findings;
//! 4. how much unchanged context a hunk carries, and whether the diff uses
//!    CRLF line endings, leaves the report unchanged;
//! 5. a second run over the same bytes renders the same JSON, byte for byte.
//!
//! "The report" is the whole rendered `--format json` document, not only the
//! findings. Relations 2 to 4 compare every field and name the few that
//! describe the diff text itself and so may move: the input identity hash
//! always, and the changed file and line counts when sections are added.
//!
//! Every case first asserts its fixture: the source diff alone produces
//! exactly one predicate finding at each generated changed line, over the
//! intended expression. A relation can therefore never hold over a workspace
//! that was not analyzed. A control test shows the comparison is able to
//! fail: dropping one source section from the diff is reported as a
//! difference. These are static classes; nothing here claims a runtime
//! mutation outcome.
//!
//! The default case count is small so the required test run stays cheap. The
//! scheduled and manual `fuzz.yml` workflow raises it with `PROPTEST_CASES`
//! (400).
//! Raise it locally with `PROPTEST_CASES=500 cargo test -p ripr --test
//! diff_scope_properties`.

use proptest::prelude::*;
use proptest::test_runner::Config;
use ripr::{
    CheckInput, CheckOutput, Mode, OutputFormat, ProbeFamily, check_workspace, render_check,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_CASES: u32 = 12;
const CRATE: &str = "diff_scope_subject";

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
    /// Whether the test file pins the boundary value for this module.
    pinned: bool,
    /// Unrelated lines before the changed function.
    prelude: u8,
}

/// A generated workspace: two to four module files, each with one changed
/// comparison, and one integration test file.
#[derive(Clone, Debug)]
struct Subject {
    modules: Vec<Module>,
}

impl Subject {
    fn path(index: usize) -> String {
        format!("src/part{index}.rs")
    }

    fn function(index: usize) -> String {
        format!("gate{index}")
    }

    fn expression(module: &Module) -> String {
        format!("amount >= {}", module.threshold)
    }

    /// Module source and the 1-based line of the changed expression.
    fn source(index: usize, module: &Module, changed: bool) -> (String, usize) {
        let op = if changed { ">=" } else { ">" };
        let mut lines: Vec<String> = (0..module.prelude)
            .map(|line| format!("// unrelated note {line}"))
            .collect();
        lines.push(format!(
            "pub fn {}(amount: u64) -> bool {{",
            Self::function(index)
        ));
        let line = lines.len() + 1;
        lines.push(format!("    amount {op} {}", module.threshold));
        lines.push("}".to_string());
        (lines.join("\n") + "\n", line)
    }

    /// Whole-file diff section for one module.
    fn section(index: usize, module: &Module) -> String {
        let (old, line) = Self::source(index, module, false);
        let (new, _) = Self::source(index, module, true);
        let mut body = String::new();
        for (at, (before, after)) in old.lines().zip(new.lines()).enumerate() {
            if at + 1 == line {
                body.push_str(&format!("-{before}\n+{after}\n"));
            } else {
                body.push_str(&format!(" {before}\n"));
            }
        }
        let count = old.lines().count();
        let path = Self::path(index);
        format!(
            "diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,{count} +1,{count} @@\n{body}"
        )
    }

    /// The same change as [`Self::section`], as a hunk carrying at most
    /// `context` unchanged lines on each side of the changed line.
    fn narrow_section(index: usize, module: &Module, context: usize, crlf: bool) -> String {
        let (old, line) = Self::source(index, module, false);
        let (new, _) = Self::source(index, module, true);
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        let first = line.saturating_sub(context).max(1);
        let last = (line + context).min(old_lines.len());
        let mut body = String::new();
        for at in first..=last {
            if at == line {
                body.push_str(&format!("-{}\n+{}\n", old_lines[at - 1], new_lines[at - 1]));
            } else {
                body.push_str(&format!(" {}\n", old_lines[at - 1]));
            }
        }
        let count = last - first + 1;
        let path = Self::path(index);
        let section = format!(
            "diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n@@ -{first},{count} +{first},{count} @@\n{body}"
        );
        if crlf {
            section.replace('\n', "\r\n")
        } else {
            section
        }
    }

    fn sections(&self) -> Vec<String> {
        self.modules
            .iter()
            .enumerate()
            .map(|(index, module)| Self::section(index, module))
            .collect()
    }

    fn files(&self) -> Vec<(String, String)> {
        let mut lib = String::new();
        let mut tests = format!("use {CRATE}::*;\n");
        let mut files = Vec::new();
        for (index, module) in self.modules.iter().enumerate() {
            lib.push_str(&format!("mod part{index};\npub use part{index}::*;\n"));
            let name = Self::function(index);
            let assertion = if module.pinned {
                format!("assert!({name}({}));", module.threshold)
            } else {
                format!("assert!({name}({}));", module.threshold + 7)
            };
            tests.push_str(&format!(
                "\n#[test]\nfn {name}_checked() {{\n    {assertion}\n}}\n"
            ));
            files.push((Self::path(index), Self::source(index, module, true).0));
        }
        files.push(("src/lib.rs".to_string(), lib));
        files.push(("tests/case.rs".to_string(), tests));
        files.push((
            "Cargo.toml".to_string(),
            "[package]\nname = \"diff-scope-subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n"
                .to_string(),
        ));
        files
    }
}

/// A diff section for a file `ripr` does not analyze.
#[derive(Clone, Debug)]
enum Noise {
    /// An existing documentation file gains a line.
    Modified(u8),
    /// A new data file appears.
    Added(u8),
    /// A documentation file is deleted.
    Deleted(u8),
    /// A binary asset changes; the section carries no text hunk.
    Binary(u8),
    /// A documentation file changes mode only; the section carries no hunk.
    ModeOnly(u8),
    /// A documentation file is renamed without a content change.
    Renamed(u8),
}

const NOISE_EXTENSIONS: [&str; 4] = ["md", "txt", "json", "csv"];

impl Noise {
    fn path(kind: &str, pick: u8, slot: usize) -> String {
        let extension = NOISE_EXTENSIONS[usize::from(pick) % NOISE_EXTENSIONS.len()];
        format!("notes/{kind}-{slot}.{extension}")
    }

    /// The diff section, and the file it leaves in the working tree if any.
    fn render(&self, slot: usize) -> (String, Option<(String, String)>) {
        match self {
            Noise::Modified(pick) => {
                let path = Self::path("modified", *pick, slot);
                let section = format!(
                    "diff --git a/{path} b/{path}\nindex 3333333..4444444 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,1 +1,2 @@\n first line\n+second line {slot}\n"
                );
                let content = format!("first line\nsecond line {slot}\n");
                (section, Some((path, content)))
            }
            Noise::Added(pick) => {
                let path = Self::path("added", *pick, slot);
                let section = format!(
                    "diff --git a/{path} b/{path}\nnew file mode 100644\nindex 0000000..5555555\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,2 @@\n+amount >= {slot}\n+if amount > 3 {{ return true; }}\n"
                );
                let content = format!("amount >= {slot}\nif amount > 3 {{ return true; }}\n");
                (section, Some((path, content)))
            }
            Noise::Deleted(pick) => {
                let path = Self::path("deleted", *pick, slot);
                let section = format!(
                    "diff --git a/{path} b/{path}\ndeleted file mode 100644\nindex 6666666..0000000\n--- a/{path}\n+++ /dev/null\n@@ -1,1 +0,0 @@\n-amount > {slot}\n"
                );
                (section, None)
            }
            Noise::Binary(pick) => {
                let path = format!("notes/asset-{slot}-{pick}.png");
                let section = format!(
                    "diff --git a/{path} b/{path}\nindex 7777777..8888888 100644\nBinary files a/{path} and b/{path} differ\n"
                );
                (section, Some((path, "not really a png\n".to_string())))
            }
            Noise::ModeOnly(pick) => {
                let path = Self::path("mode", *pick, slot);
                let section =
                    format!("diff --git a/{path} b/{path}\nold mode 100644\nnew mode 100755\n");
                (section, Some((path, "first line\n".to_string())))
            }
            Noise::Renamed(pick) => {
                let from = Self::path("before", *pick, slot);
                let to = Self::path("after", *pick, slot);
                let section = format!(
                    "diff --git a/{from} b/{to}\nsimilarity index 100%\nrename from {from}\nrename to {to}\n"
                );
                (section, Some((to, "first line\n".to_string())))
            }
        }
    }
}

fn noise() -> impl Strategy<Value = Noise> {
    prop_oneof![
        any::<u8>().prop_map(Noise::Modified),
        any::<u8>().prop_map(Noise::Added),
        any::<u8>().prop_map(Noise::Deleted),
        any::<u8>().prop_map(Noise::Binary),
        any::<u8>().prop_map(Noise::ModeOnly),
        any::<u8>().prop_map(Noise::Renamed),
    ]
}

fn subject() -> impl Strategy<Value = Subject> {
    prop::collection::vec((2u64..500, any::<bool>(), 0u8..4), 2..5).prop_map(|modules| Subject {
        modules: modules
            .into_iter()
            .map(|(threshold, pinned, prelude)| Module {
                threshold,
                pinned,
                prelude,
            })
            .collect(),
    })
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(files: &[(String, String)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-fuzz-diff-scope-{}-{}",
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

    /// Runs the analysis over `diff`, always from the same file name so the
    /// diff text is the only thing that differs between two runs.
    fn check(&self, diff: &str) -> Result<CheckOutput, String> {
        self.write("change.diff", diff)?;
        check_workspace(CheckInput {
            root: self.root.clone(),
            diff_file: Some(self.root.join("change.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The rendered report, as the JSON renderer writes it.
fn report(output: &CheckOutput) -> Result<Value, String> {
    let text = render_check(output, &OutputFormat::Json)?;
    serde_json::from_str(&text).map_err(|error| format!("check JSON did not parse: {error}"))
}

fn findings(report: &Value) -> Result<&Vec<Value>, String> {
    report
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "check JSON has no findings array".to_string())
}

/// The hash of the diff bytes. It moves whenever the diff text does.
const INPUT_IDENTITY: &str = ".analysis_outcome.outcome.identity.input_identity";
/// Counts of the files and lines the diff names, analyzable or not.
const CHANGED_FILE_COUNT: &str = ".analysis_outcome.outcome.counts.changed_file_count";
const CHANGED_LINE_COUNT: &str = ".analysis_outcome.outcome.counts.changed_line_count";

/// Collects the JSON paths at which two reports differ.
fn collect_differences(path: &str, before: &Value, after: &Value, found: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in left {
                match right.get(key) {
                    Some(other) => {
                        collect_differences(&format!("{path}.{key}"), value, other, found);
                    }
                    None => found.push(format!("{path}.{key}")),
                }
            }
            for key in right.keys().filter(|key| !left.contains_key(*key)) {
                found.push(format!("{path}.{key}"));
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

/// The paths at which two reports differ, other than the `allowed` fields
/// that describe the diff text itself.
fn differences(before: &Value, after: &Value, allowed: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    collect_differences("", before, after, &mut found);
    found.retain(|path| !allowed.contains(&path.as_str()));
    found
}

/// Asserts the source diff alone analyzed every generated change: exactly one
/// predicate finding per module, at the changed line, over the changed
/// expression.
fn assert_fixture(subject: &Subject, output: &CheckOutput) -> Result<(), String> {
    for (index, module) in subject.modules.iter().enumerate() {
        let path = Subject::path(index);
        let (_, line) = Subject::source(index, module, true);
        let mut found = output.findings.iter().filter(|finding| {
            finding.probe.family == ProbeFamily::Predicate
                && finding.probe.location.line == line
                && finding.probe.location.file.ends_with(&path)
        });
        let finding = found
            .next()
            .ok_or_else(|| format!("fixture produced no predicate finding in {path}:{line}"))?;
        if found.next().is_some() {
            return Err(format!(
                "fixture produced duplicate predicate findings in {path}"
            ));
        }
        let expression = Subject::expression(module);
        if finding.probe.expression.trim() != expression {
            return Err(format!(
                "finding in {path} is `{}`, not the changed `{expression}`",
                finding.probe.expression
            ));
        }
    }
    Ok(())
}

/// A workspace on disk, its source-only diff sections, and the baseline run.
struct Baseline {
    repo: TempRepo,
    sections: Vec<String>,
    report: Value,
}

fn baseline(subject: &Subject, noise: &[Noise]) -> Result<Baseline, String> {
    let mut files = subject.files();
    for (slot, item) in noise.iter().enumerate() {
        if let (_, Some(file)) = item.render(slot) {
            files.push(file);
        }
    }
    let repo = TempRepo::create(&files)?;
    let sections = subject.sections();
    let output = repo.check(&sections.concat())?;
    assert_fixture(subject, &output)?;
    let report = report(&output)?;
    Ok(Baseline {
        repo,
        sections,
        report,
    })
}

fn fail(message: String) -> TestCaseError {
    TestCaseError::fail(message)
}

proptest! {
    #![proptest_config(config())]

    /// Relation 1: a diff with no analyzable source change reports nothing,
    /// in a workspace where the source diff does report findings.
    #[test]
    fn a_diff_without_source_changes_reports_no_findings(
        subject in subject(),
        noise in prop::collection::vec(noise(), 0..4),
        blank in 0u8..3,
    ) {
        let base = baseline(&subject, &noise).map_err(fail)?;
        let diff: String = if noise.is_empty() {
            "\n".repeat(usize::from(blank))
        } else {
            noise.iter().enumerate().map(|(slot, item)| item.render(slot).0).collect()
        };
        let output = base.repo.check(&diff).map_err(fail)?;
        let report = report(&output).map_err(fail)?;
        let findings = findings(&report).map_err(fail)?;
        prop_assert!(
            findings.is_empty(),
            "diff without source changes reported {} findings for {:?} with {:?}: {:?}",
            findings.len(),
            subject,
            noise,
            findings
        );
    }

    /// Relation 2: unrelated documentation, data and binary sections, placed
    /// anywhere among the source sections, leave the report unchanged.
    #[test]
    fn unrelated_file_sections_leave_the_report_unchanged(
        subject in subject(),
        noise in prop::collection::vec(noise(), 1..4),
        places in prop::collection::vec(any::<prop::sample::Index>(), 3),
    ) {
        let base = baseline(&subject, &noise).map_err(fail)?;
        let mut sections = base.sections.clone();
        for (slot, item) in noise.iter().enumerate() {
            let at = places[slot].index(sections.len() + 1);
            sections.insert(at, item.render(slot).0);
        }
        let output = base.repo.check(&sections.concat()).map_err(fail)?;
        let changed = differences(
            &base.report,
            &report(&output).map_err(fail)?,
            &[INPUT_IDENTITY, CHANGED_FILE_COUNT, CHANGED_LINE_COUNT],
        );
        prop_assert!(
            changed.is_empty(),
            "unrelated sections changed {:?} for {:?} with {:?}",
            changed,
            subject,
            noise
        );
    }

    /// Relation 3: the order of the file sections does not change the report,
    /// including the order in which the findings are listed.
    #[test]
    fn section_order_leaves_the_report_unchanged(
        (subject, order) in subject().prop_flat_map(|subject| {
            let count = subject.modules.len();
            (Just(subject), Just((0..count).collect::<Vec<usize>>()).prop_shuffle())
        }),
    ) {
        let base = baseline(&subject, &[]).map_err(fail)?;
        let diff: String = order.iter().map(|index| base.sections[*index].as_str()).collect();
        let output = base.repo.check(&diff).map_err(fail)?;
        let changed = differences(&base.report, &report(&output).map_err(fail)?, &[INPUT_IDENTITY]);
        prop_assert!(
            changed.is_empty(),
            "section order {:?} changed {:?} for {:?}",
            order,
            changed,
            subject
        );
    }

    /// Relation 4: how much unchanged context a hunk carries, and whether the
    /// diff uses CRLF line endings, does not change the report.
    #[test]
    fn hunk_context_and_line_endings_leave_the_report_unchanged(
        subject in subject(),
        shapes in prop::collection::vec((0usize..5, any::<bool>()), 4),
    ) {
        let base = baseline(&subject, &[]).map_err(fail)?;
        let diff: String = subject
            .modules
            .iter()
            .enumerate()
            .map(|(index, module)| {
                let (context, crlf) = shapes[index];
                Subject::narrow_section(index, module, context, crlf)
            })
            .collect();
        let output = base.repo.check(&diff).map_err(fail)?;
        let changed = differences(&base.report, &report(&output).map_err(fail)?, &[INPUT_IDENTITY]);
        prop_assert!(
            changed.is_empty(),
            "hunk shapes {:?} changed {:?} for {:?}",
            shapes,
            changed,
            subject
        );
    }

    /// Relation 5: the same bytes render the same JSON on a second run.
    #[test]
    fn a_second_run_renders_the_same_json(subject in subject()) {
        let base = baseline(&subject, &[]).map_err(fail)?;
        let diff = base.sections.concat();
        let first = render_check(&base.repo.check(&diff).map_err(fail)?, &OutputFormat::Json)
            .map_err(fail)?;
        let second = render_check(&base.repo.check(&diff).map_err(fail)?, &OutputFormat::Json)
            .map_err(fail)?;
        prop_assert_eq!(first, second, "two runs differed for {:?}", subject);
    }
}

fn control_subject() -> Subject {
    Subject {
        modules: vec![
            Module {
                threshold: 18,
                pinned: true,
                prelude: 0,
            },
            Module {
                threshold: 40,
                pinned: false,
                prelude: 2,
            },
        ],
    }
}

/// Control: the comparison the relations rely on is able to fail. Dropping
/// one source section from the diff removes a finding, and the comparison
/// reports it even with every diff-describing field allowed to move.
#[test]
fn dropping_a_source_section_is_reported_as_a_difference() -> Result<(), String> {
    let subject = control_subject();
    let base = baseline(&subject, &[])?;
    let partial = report(&base.repo.check(&base.sections[0])?)?;
    if findings(&partial)?.len() >= findings(&base.report)?.len() {
        return Err("dropping a source section did not remove a finding".to_string());
    }
    let changed = differences(
        &base.report,
        &partial,
        &[INPUT_IDENTITY, CHANGED_FILE_COUNT, CHANGED_LINE_COUNT],
    );
    if changed.is_empty() {
        return Err("the comparison missed a dropped source section".to_string());
    }
    Ok(())
}

/// Control: the fields the relations allow to move are real report fields,
/// so a renamed field cannot silently turn an allowance into a no-op, and
/// the diff-describing counts do count the unrelated sections.
#[test]
fn allowed_fields_exist_and_describe_the_diff() -> Result<(), String> {
    let subject = control_subject();
    let noise = [Noise::Modified(0)];
    let base = baseline(&subject, &noise)?;
    let mut diff = base.sections.concat();
    diff.push_str(&noise[0].render(0).0);
    let noisy = report(&base.repo.check(&diff)?)?;
    let mut found = Vec::new();
    collect_differences("", &base.report, &noisy, &mut found);
    found.sort();
    let mut expected = vec![CHANGED_FILE_COUNT, CHANGED_LINE_COUNT, INPUT_IDENTITY];
    expected.sort();
    if found != expected {
        return Err(format!(
            "one unrelated section moved {found:?}, expected {expected:?}"
        ));
    }
    Ok(())
}
