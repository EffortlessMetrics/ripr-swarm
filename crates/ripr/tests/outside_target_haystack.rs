//! A Rust file that no Cargo target compiles (a benchmark or corpus haystack
//! such as `benchmarks/haystacks/code/rust-library.rs`) contributes no
//! related test and no lexical-fallback limitation (#8999). The same file
//! compiled through a `#[path]` module is the positive control: its test is
//! related and its parser refusal is disclosed. Each case reads the rendered
//! finding classification, related test files and analysis outcome.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const HAY: &str = "benchmarks/haystacks/code/rust-library.rs";
const HAY_SOURCE: &str =
    "use keyed::discount;\n#[test]\nfn corpus_case() {\n    assert_eq!(discount(10), 5);\n}\n";
// The same text with a token the reference parser refuses, so it is indexed
// lexically.
const REFUSED_HAY_SOURCE: &str = "use keyed::discount;\n#[test]\nfn corpus_case() {\n    assert_eq!(discount(10), 5);\n}\nfn broken( {\n";
const TESTS: &str = "use keyed::discount;\n#[test]\nfn discount_boundary() {\n    assert_eq!(discount(10), 5);\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,1 +2,1 @@
-    if q > 10 { 5 } else { 0 }
+    if q > 9 { 5 } else { 0 }
";

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-outside-target-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let full = self.0.join(relative);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&full, content).map_err(|error| format!("write {relative}: {error}"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Rendered facts of one run.
struct Run {
    classification: String,
    related_files: Vec<String>,
    outcome_kind: String,
    limitations: String,
}

fn run(lib_extra: &str, manifest_extra: &str, haystack: Option<&str>) -> Result<Run, String> {
    let scratch = Scratch::create()?;
    scratch.write(
        "Cargo.toml",
        &format!(
            "[package]\nname = \"keyed\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n{manifest_extra}"
        ),
    )?;
    let lib = format!(
        "pub fn discount(q: u32) -> u32 {{\n    if q > 9 {{ 5 }} else {{ 0 }}\n}}\n{lib_extra}"
    );
    scratch.write("src/lib.rs", &lib)?;
    scratch.write("tests/api.rs", TESTS)?;
    if let Some(source) = haystack {
        scratch.write(HAY, source)?;
    }
    scratch.write("change.diff", DIFF)?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let finding = json["findings"]
        .as_array()
        .and_then(|findings| {
            findings
                .iter()
                .find(|finding| finding["probe"]["family"] == "predicate")
        })
        .cloned()
        .ok_or_else(|| format!("premise: a predicate finding: {json}"))?;
    let mut related_files: Vec<String> = finding["related_tests"]
        .as_array()
        .map(|tests| {
            tests
                .iter()
                .filter_map(|test| test["file"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    related_files.sort();
    related_files.dedup();
    let outcome = &json["analysis_outcome"]["outcome"];
    Ok(Run {
        classification: finding["classification"]
            .as_str()
            .ok_or("missing classification")?
            .to_string(),
        related_files,
        outcome_kind: outcome["kind"].as_str().unwrap_or_default().to_string(),
        limitations: outcome["limitations"].to_string(),
    })
}

#[test]
fn an_uncompiled_haystack_is_not_a_related_test() -> Result<(), String> {
    let baseline = run("", "", None)?;
    assert_eq!(baseline.related_files, ["tests/api.rs"]);
    let run = run("", "", Some(HAY_SOURCE))?;
    assert_eq!(run.related_files, ["tests/api.rs"], "{}", run.limitations);
    assert_eq!(run.classification, baseline.classification);
    assert_eq!(
        run.outcome_kind, "complete_with_findings",
        "{}",
        run.limitations
    );
    Ok(())
}

#[test]
fn an_uncompiled_unparsed_haystack_is_not_a_lexical_limitation() -> Result<(), String> {
    // The workspace-wide macro-binding scan still reads this unparsed file, so
    // the class may stay `static_unknown`; the claim here is only that the file
    // is neither related evidence nor a lexical-fallback disclosure.
    let run = run("", "", Some(REFUSED_HAY_SOURCE))?;
    assert_eq!(run.related_files, ["tests/api.rs"], "{}", run.limitations);
    assert_eq!(
        run.outcome_kind, "complete_with_findings",
        "{}",
        run.limitations
    );
    assert!(!run.limitations.contains(HAY), "{}", run.limitations);
    Ok(())
}

#[test]
fn the_same_file_compiled_through_a_path_module_is_related_and_disclosed() -> Result<(), String> {
    let lib =
        "#[cfg(test)]\n#[path = \"../benchmarks/haystacks/code/rust-library.rs\"]\nmod haystack;\n";
    let run = run(lib, "", Some(REFUSED_HAY_SOURCE))?;
    assert_eq!(run.related_files, [HAY, "tests/api.rs"]);
    assert_eq!(run.outcome_kind, "partial_with_limitations");
    assert!(run.limitations.contains(HAY), "{}", run.limitations);
    Ok(())
}

#[test]
fn a_bench_target_naming_the_file_keeps_it_compiled() -> Result<(), String> {
    let manifest = format!("\n[[bench]]\nname = \"hay\"\npath = \"{HAY}\"\nharness = false\n");
    let run = run("", &manifest, Some(HAY_SOURCE))?;
    assert!(
        run.related_files.contains(&HAY.to_string()),
        "{:?}",
        run.related_files
    );
    Ok(())
}
