//! Output-edge properties over real analysis results (#12919).
//!
//! `check-static-language` scans tracked source text; it cannot see what the
//! renderers emit for an arbitrary analysis result. These properties generate
//! small Rust workspaces and changed-predicate diffs, run the production
//! `check` path, and assert three contracts on the bytes the renderers write:
//!
//! 1. every renderer (human, full human, JSON, GitHub, SARIF, context packet)
//!    uses only the static class vocabulary and none of the runtime-outcome
//!    words owned by `check-static-language`;
//! 2. finding ids are unique within a run and stable under reordering the diff
//!    and under unrelated line shifts;
//! 3. JSON output validates against the committed `check.schema.json`, and
//!    SARIF output validates against the SARIF 2.1.0 envelope subset below
//!    (no SARIF schema is committed; the subset is checked by the same
//!    repository validator).
//!
//! The forbidden-word list and word matcher are the gate's own
//! (`forbidden_static_terms`, `no_panic::contains_word`), not a copy.

use crate::exact_schema_json;
use crate::verification_contracts::validate_exact_value_against_schema;
use proptest::prelude::*;
use ripr::config::RiprConfig;
use ripr::domain::ExposureClass;
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The seven static classes, matched exhaustively so a new class breaks this
/// build until the vocabulary property is reviewed.
fn static_class_vocabulary() -> BTreeSet<&'static str> {
    [
        ExposureClass::Exposed,
        ExposureClass::WeaklyExposed,
        ExposureClass::ReachableUnrevealed,
        ExposureClass::NoStaticPath,
        ExposureClass::InfectionUnknown,
        ExposureClass::PropagationUnknown,
        ExposureClass::StaticUnknown,
    ]
    .into_iter()
    .inspect(|class| match class {
        ExposureClass::Exposed
        | ExposureClass::WeaklyExposed
        | ExposureClass::ReachableUnrevealed
        | ExposureClass::NoStaticPath
        | ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown
        | ExposureClass::StaticUnknown => {}
    })
    .map(|class| class.as_str())
    .collect()
}

/// One generated function: a predicate boundary the diff moves from `>` to
/// `>=`, with or without a test that pins the boundary exactly.
#[derive(Clone, Debug)]
struct FnSpec {
    threshold: u8,
    then_value: u8,
    else_value: u8,
    pinned: bool,
}

/// One generated source file holding several functions.
#[derive(Clone, Debug)]
struct FileSpec {
    fns: Vec<FnSpec>,
}

fn fn_spec() -> impl Strategy<Value = FnSpec> {
    (1u8..50, 1u8..50, 51u8..100, any::<bool>()).prop_map(
        |(threshold, then_value, else_value, pinned)| FnSpec {
            threshold,
            then_value,
            else_value,
            pinned,
        },
    )
}

fn workspace_spec() -> impl Strategy<Value = Vec<FileSpec>> {
    prop::collection::vec(
        prop::collection::vec(fn_spec(), 1..4).prop_map(|fns| FileSpec { fns }),
        1..4,
    )
}

/// A generated workspace plus the unified diff that changes every boundary.
struct Workspace {
    files: Vec<(String, String)>,
    /// One diff section per file, in file order; callers permute them.
    sections: Vec<String>,
}

/// `lead` unrelated comment lines precede the code, shifting every line.
fn build_workspace(specs: &[FileSpec], lead: usize) -> Workspace {
    let mut files = Vec::new();
    let mut sections = Vec::new();
    let mut lib = String::new();
    for (file_index, spec) in specs.iter().enumerate() {
        let path = format!("src/m{file_index}.rs");
        lib.push_str(&format!("pub mod m{file_index};\n"));
        let mut lines: Vec<String> = (0..lead)
            .map(|n| format!("// unrelated note {n}"))
            .collect();
        let mut hunks = String::new();
        let mut tests = String::from("#[cfg(test)]\nmod tests {\n    use super::*;\n");
        for (fn_index, f) in spec.fns.iter().enumerate() {
            let name = format!("f{file_index}_{fn_index}");
            lines.push(format!("pub fn {name}(x: i32) -> i32 {{"));
            let line_no = lines.len() + 1;
            let new = format!("    if x >= {} {{", f.threshold);
            let old = format!("    if x > {} {{", f.threshold);
            lines.push(new.clone());
            lines.push(format!("        {}", f.then_value));
            lines.push("    } else {".to_string());
            lines.push(format!("        {}", f.else_value));
            lines.push("    }".to_string());
            lines.push("}".to_string());
            lines.push(String::new());
            hunks.push_str(&format!(
                "@@ -{line_no},1 +{line_no},1 @@\n-{old}\n+{new}\n"
            ));
            let assertion = if f.pinned {
                format!("assert_eq!({name}({}), {});", f.threshold, f.then_value)
            } else {
                format!("assert!({name}({}) > 0);", f.threshold + 1)
            };
            tests.push_str(&format!(
                "    #[test]\n    fn {name}_checked() {{\n        {assertion}\n    }}\n"
            ));
        }
        tests.push_str("}\n");
        let mut source = lines.join("\n");
        source.push('\n');
        source.push_str(&tests);
        files.push((path.clone(), source));
        sections.push(format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n{hunks}"
        ));
    }
    files.push(("src/lib.rs".to_string(), lib));
    files.push((
        "Cargo.toml".to_string(),
        "[package]\nname = \"output_props\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n"
            .to_string(),
    ));
    Workspace { files, sections }
}

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-output-props-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The production check result and every renderer's bytes for it.
struct Rendered {
    root: PathBuf,
    output: ripr::CheckOutput,
    by_format: Vec<(&'static str, String)>,
}

fn analyze(workspace: &Workspace, order: &[usize]) -> Result<Rendered, String> {
    let scratch = Scratch::create()?;
    for (path, contents) in &workspace.files {
        std::fs::write(scratch.0.join(path), contents).map_err(|error| error.to_string())?;
    }
    let diff: String = order
        .iter()
        .map(|index| workspace.sections[*index].as_str())
        .collect();
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let output = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut by_format = Vec::new();
    for (name, format) in [
        ("human", OutputFormat::Human),
        ("human_full", OutputFormat::HumanFull),
        ("json", OutputFormat::Json),
        ("github", OutputFormat::Github),
        ("sarif", OutputFormat::Sarif),
    ] {
        by_format.push((name, render_check(&output, &format)?));
    }
    let config = RiprConfig::default();
    for finding in &output.findings {
        by_format.push((
            "packet",
            ripr::output::json::render_context_packet(finding, 3, &config, &scratch.0),
        ));
    }
    Ok(Rendered {
        root: scratch.0.clone(),
        output,
        by_format,
    })
}

fn rendered_text<'a>(rendered: &'a Rendered, format: &str) -> Result<&'a str, String> {
    rendered
        .by_format
        .iter()
        .find(|(name, _)| *name == format)
        .map(|(_, text)| text.as_str())
        .ok_or_else(|| format!("no {format} render"))
}

/// Property 1: no runtime-outcome word in any renderer's output, and every
/// class token in structured output is in the static vocabulary.
fn check_vocabulary(rendered: &Rendered, forbidden: &[String]) -> Result<(), String> {
    let vocabulary = static_class_vocabulary();
    let root = rendered.root.display().to_string().to_ascii_lowercase();
    for (format, text) in &rendered.by_format {
        // The scratch root is an absolute temp path the analysis echoes; it
        // is not renderer vocabulary, so strip it before matching words.
        let lower = text.to_ascii_lowercase().replace(&root, "");
        for term in forbidden {
            if crate::no_panic::contains_word(&lower, term) {
                return Err(format!(
                    "{format} output contains forbidden word `{term}`:\n{text}"
                ));
            }
        }
    }
    let value: Value = serde_json::from_str(rendered_text(rendered, "json")?)
        .map_err(|error| error.to_string())?;
    for finding in value["findings"].as_array().into_iter().flatten() {
        let class = finding["classification"].as_str().unwrap_or("<missing>");
        if !vocabulary.contains(class) {
            return Err(format!(
                "json classification `{class}` is outside the vocabulary"
            ));
        }
    }
    let sarif: Value = serde_json::from_str(rendered_text(rendered, "sarif")?)
        .map_err(|error| error.to_string())?;
    for result in sarif["runs"][0]["results"].as_array().into_iter().flatten() {
        let rule = result["ruleId"].as_str().unwrap_or("<missing>");
        let Some(class) = rule.strip_prefix("ripr.finding.") else {
            return Err(format!("sarif ruleId `{rule}` is not a finding rule"));
        };
        if !vocabulary.contains(class) {
            return Err(format!(
                "sarif ruleId class `{class}` is outside the vocabulary"
            ));
        }
    }
    for finding in &rendered.output.findings {
        if !vocabulary.contains(finding.class.as_str()) {
            return Err(format!(
                "domain class `{}` is outside the vocabulary",
                finding.class.as_str()
            ));
        }
    }
    Ok(())
}

/// Property 2: ids are unique within the run, in the domain findings and in
/// the JSON render.
fn check_unique_ids(rendered: &Rendered) -> Result<BTreeSet<String>, String> {
    let mut seen = BTreeSet::new();
    for finding in &rendered.output.findings {
        if !seen.insert(finding.id.clone()) {
            return Err(format!("duplicate finding id `{}`", finding.id));
        }
    }
    let value: Value = serde_json::from_str(rendered_text(rendered, "json")?)
        .map_err(|error| error.to_string())?;
    let json_ids: Vec<&str> = value["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|finding| finding["id"].as_str())
        .collect();
    let json_set: BTreeSet<&str> = json_ids.iter().copied().collect();
    if json_set.len() != json_ids.len() {
        return Err(format!("duplicate id in JSON findings: {json_ids:?}"));
    }
    Ok(seen)
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn validate(value_text: &str, schema: &exact_schema_json::Value) -> Result<(), String> {
    let value = exact_schema_json::parse(value_text.as_bytes())?;
    let mut violations = Vec::new();
    validate_exact_value_against_schema(&value, schema, schema, "$".to_string(), &mut violations);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

/// The SARIF 2.1.0 envelope contract GitHub code scanning consumes. This is
/// a subset, not the OASIS schema; the same repository validator checks it.
const SARIF_ENVELOPE_SCHEMA: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["version", "runs"],
  "properties": {
    "version": { "const": "2.1.0" },
    "runs": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "object",
        "required": ["tool", "results"],
        "properties": {
          "tool": {
            "type": "object",
            "required": ["driver"],
            "properties": {
              "driver": {
                "type": "object",
                "required": ["name"],
                "properties": { "name": { "type": "string", "minLength": 1 } }
              }
            }
          },
          "results": {
            "type": "array",
            "items": {
              "type": "object",
              "required": ["ruleId", "message"],
              "properties": {
                "ruleId": { "type": "string", "minLength": 1 },
                "level": { "enum": ["none", "note", "warning", "error"] },
                "message": {
                  "type": "object",
                  "required": ["text"],
                  "properties": { "text": { "type": "string", "minLength": 1 } }
                },
                "locations": { "type": "array" }
              }
            }
          }
        }
      }
    }
  }
}"##;

/// Property 3: committed JSON schema plus the SARIF envelope.
fn check_schemas(rendered: &Rendered) -> Result<(), String> {
    let path = repo_path("schemas/ripr/check.schema.json");
    let bytes =
        std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let check_schema = exact_schema_json::parse(&bytes)?;
    validate(rendered_text(rendered, "json")?, &check_schema)
        .map_err(|error| format!("JSON does not match check.schema.json:\n{error}"))?;
    let sarif_schema = exact_schema_json::parse(SARIF_ENVELOPE_SCHEMA.as_bytes())?;
    validate(rendered_text(rendered, "sarif")?, &sarif_schema)
        .map_err(|error| format!("SARIF does not match the 2.1.0 envelope:\n{error}"))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, max_shrink_iters: 24, .. ProptestConfig::default() })]

    /// Properties 1 and 3 over generated analysis results.
    #[test]
    fn renderers_use_only_static_vocabulary_and_validate_against_schemas(
        specs in workspace_spec(),
    ) {
        let workspace = build_workspace(&specs, 0);
        let order: Vec<usize> = (0..workspace.sections.len()).collect();
        let rendered = analyze(&workspace, &order).map_err(TestCaseError::fail)?;
        prop_assert!(!rendered.output.findings.is_empty(), "generator produced no findings");
        check_vocabulary(&rendered, &crate::forbidden_static_terms())
            .map_err(TestCaseError::fail)?;
        check_schemas(&rendered).map_err(TestCaseError::fail)?;
    }

    /// Property 2: ids are unique and stable under reordering the diff's
    /// file sections and under unrelated line shifts.
    #[test]
    fn finding_ids_are_unique_and_stable(
        specs in workspace_spec(),
        lead in 1usize..5,
    ) {
        let workspace = build_workspace(&specs, 0);
        let forward: Vec<usize> = (0..workspace.sections.len()).collect();
        let reversed: Vec<usize> = forward.iter().rev().copied().collect();
        let base = check_unique_ids(&analyze(&workspace, &forward).map_err(TestCaseError::fail)?)
            .map_err(TestCaseError::fail)?;
        prop_assert!(!base.is_empty(), "generator produced no findings");
        let reordered = check_unique_ids(&analyze(&workspace, &reversed).map_err(TestCaseError::fail)?)
            .map_err(TestCaseError::fail)?;
        prop_assert_eq!(&base, &reordered, "diff section order changed finding ids");
        let shifted_workspace = build_workspace(&specs, lead);
        let shifted = check_unique_ids(&analyze(&shifted_workspace, &forward).map_err(TestCaseError::fail)?)
            .map_err(TestCaseError::fail)?;
        prop_assert_eq!(&base, &shifted, "unrelated line shift changed finding ids");
    }
}

/// Negative experiments: the checks must fail on injected faults, otherwise
/// the properties above could pass vacuously.
mod negative {
    use super::*;

    fn one_fn_sample() -> Result<Rendered, String> {
        let specs = vec![FileSpec {
            fns: vec![FnSpec {
                threshold: 5,
                then_value: 7,
                else_value: 60,
                pinned: false,
            }],
        }];
        analyze(&build_workspace(&specs, 0), &[0])
    }

    #[test]
    fn injected_forbidden_word_fails_the_vocabulary_check() -> Result<(), String> {
        let mut rendered = one_fn_sample()?;
        let forbidden = crate::forbidden_static_terms();
        check_vocabulary(&rendered, &forbidden)?;
        let word = forbidden.first().ok_or("no forbidden terms")?.clone();
        for index in 0..rendered.by_format.len() {
            let original = rendered.by_format[index].1.clone();
            rendered.by_format[index].1 = format!("{original}\nmutant {word}\n");
            if check_vocabulary(&rendered, &forbidden).is_ok() {
                return Err(format!(
                    "{} render accepted forbidden word `{word}`",
                    rendered.by_format[index].0
                ));
            }
            rendered.by_format[index].1 = original;
        }
        Ok(())
    }

    #[test]
    fn injected_foreign_class_fails_the_vocabulary_check() -> Result<(), String> {
        let mut rendered = one_fn_sample()?;
        let mut changed = false;
        for (name, text) in &mut rendered.by_format {
            if *name == "json" {
                let edited = text.replace("\"classification\": \"", "\"classification\": \"sound_");
                changed = edited != *text;
                *text = edited;
            }
        }
        if !changed {
            return Err("fixture did not carry a classification to corrupt".to_string());
        }
        if check_vocabulary(&rendered, &crate::forbidden_static_terms()).is_ok() {
            return Err("foreign class token was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn injected_duplicate_id_fails_the_uniqueness_check() -> Result<(), String> {
        let specs = vec![FileSpec {
            fns: vec![
                FnSpec {
                    threshold: 5,
                    then_value: 7,
                    else_value: 60,
                    pinned: false,
                },
                FnSpec {
                    threshold: 9,
                    then_value: 8,
                    else_value: 70,
                    pinned: true,
                },
            ],
        }];
        let mut rendered = analyze(&build_workspace(&specs, 0), &[0])?;
        check_unique_ids(&rendered)?;
        if rendered.output.findings.len() < 2 {
            return Err("sample did not produce two findings".to_string());
        }
        let first = rendered.output.findings[0].id.clone();
        rendered.output.findings[1].id = first;
        if check_unique_ids(&rendered).is_ok() {
            return Err("duplicate id was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn schema_check_rejects_drifted_json_and_sarif() -> Result<(), String> {
        let mut rendered = one_fn_sample()?;
        check_schemas(&rendered)?;
        for (name, text) in &mut rendered.by_format {
            if *name == "json" {
                *text = text.replace("\"tool\": \"ripr\"", "\"tool\": \"other\"");
            }
        }
        if check_schemas(&rendered).is_ok() {
            return Err("drifted JSON was accepted".to_string());
        }
        let mut rendered = one_fn_sample()?;
        for (name, text) in &mut rendered.by_format {
            if *name == "sarif" {
                *text = text.replace("\"2.1.0\"", "\"2.0.0\"");
            }
        }
        if check_schemas(&rendered).is_ok() {
            return Err("drifted SARIF was accepted".to_string());
        }
        Ok(())
    }

    /// The generator must reach more than one class, or the vocabulary
    /// property would only ever see a single token.
    #[test]
    fn generator_reaches_more_than_one_class() -> Result<(), String> {
        let specs = vec![FileSpec {
            fns: vec![
                FnSpec {
                    threshold: 5,
                    then_value: 7,
                    else_value: 60,
                    pinned: false,
                },
                FnSpec {
                    threshold: 9,
                    then_value: 8,
                    else_value: 70,
                    pinned: true,
                },
            ],
        }];
        let rendered = analyze(&build_workspace(&specs, 0), &[0])?;
        let classes: BTreeSet<&str> = rendered
            .output
            .findings
            .iter()
            .map(|finding| finding.class.as_str())
            .collect();
        if classes.len() < 2 {
            return Err(format!("only reached {classes:?}"));
        }
        Ok(())
    }
}
