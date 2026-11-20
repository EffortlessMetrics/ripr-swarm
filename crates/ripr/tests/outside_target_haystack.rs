//! A Rust file that no Cargo target compiles (a benchmark or corpus haystack
//! such as `benchmarks/haystacks/code/rust-library.rs`) contributes no
//! related test or parser-based assertion refusal (#8999, #14768). The same file
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
const REFUSED_HAY_SOURCE: &str = "use keyed::discount;\n#[test]\nfn corpus_case() {\n    assert_eq!(discount(10), 5);\n}\nmacro_rules! declare { ($name:ident) => { mod $name; } }\nfn broken( {\n";
// A metavariable name can spell a keyword without declaring a module.
const ORDINARY_MACRO: &str = "macro_rules! value { ($mod:ident) => { $mod }; }\n";
const TESTS: &str = "use keyed::discount;\n#[test]\nfn discount_boundary() {\n    assert_eq!(discount(10), 5);\n}\n";
const MANIFEST: &str = "[package]\nname = \"keyed\"\nversion = \"0.0.0\"\nedition = \"2024\"\nexclude = [\"/benchmarks\"]\n";
const LIB: &str = "pub fn discount(q: u32) -> u32 {\n    if q > 9 { 5 } else { 0 }\n}\n";
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
    human: String,
}

fn run(lib_extra: &str, manifest_extra: &str, haystack: Option<&str>) -> Result<Run, String> {
    let scratch = Scratch::create()?;
    scratch.write(
        "Cargo.toml",
        &format!("{MANIFEST}\n[workspace]\n{manifest_extra}"),
    )?;
    scratch.write("src/lib.rs", &format!("{LIB}{lib_extra}"))?;
    scratch.write("tests/api.rs", TESTS)?;
    if let Some(source) = haystack {
        scratch.write(HAY, source)?;
    }
    scratch.write("change.diff", DIFF)?;
    analyze(&scratch, &Mode::Fast)
}

fn analyze(scratch: &Scratch, mode: &Mode) -> Result<Run, String> {
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: mode.clone(),
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
        human: render_check(&report, &OutputFormat::Human)?,
    })
}

fn nested_package_fixture() -> Result<Scratch, String> {
    let scratch = Scratch::create()?;
    scratch.write("a/Cargo.toml", MANIFEST)?;
    scratch.write("a/src/lib.rs", LIB)?;
    scratch.write("a/tests/api.rs", TESTS)?;
    scratch.write(
        "a/benchmarks/hay.rs",
        &HAY_SOURCE.replace("keyed::", "super::"),
    )?;
    scratch.write("change.diff", &DIFF.replace("src/lib.rs", "a/src/lib.rs"))?;
    Ok(scratch)
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
    let baseline = run(ORDINARY_MACRO, "", None)?;
    let run = run(ORDINARY_MACRO, "", Some(REFUSED_HAY_SOURCE))?;
    assert_eq!(run.related_files, ["tests/api.rs"], "{}", run.limitations);
    assert_eq!(run.classification, baseline.classification);
    assert_eq!(
        run.outcome_kind, "complete_with_findings",
        "{}",
        run.limitations
    );
    assert!(!run.limitations.contains(HAY), "{}", run.limitations);
    assert!(!run.human.contains(HAY), "{}", run.human);
    assert!(run.human.contains("discount_boundary"), "{}", run.human);
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

#[test]
fn a_module_building_macro_keeps_the_file_whatever_its_spacing() -> Result<(), String> {
    // Generated module membership is unknown, not shown excluded: newline and
    // comment trivia between `mod` and `$n` must read like the compact form.
    for builder in [
        "mod $n;",
        "mod\n        $n;",
        "mod/* generated */$n;",
        "mod // generated\n        $n;",
        "mod /* outer /* nested */ */ $n;",
    ] {
        let lib = format!(
            "const OPEN: &str = \"/*\";\nmacro_rules! m {{ ($n:ident) => {{ {builder} }} }}\n"
        );
        let run = run(&lib, "", Some(HAY_SOURCE))?;
        assert!(
            run.related_files.contains(&HAY.to_string()),
            "{builder:?}: {:?}",
            run.related_files
        );
    }
    Ok(())
}

#[test]
fn a_custom_test_target_and_path_module_keep_the_same_source() -> Result<(), String> {
    let source =
        format!("{HAY_SOURCE}macro_rules! declare {{ ($name:ident) => {{ mod $name; }} }}\n");
    let manifest = format!("\n[[test]]\nname = \"hay\"\npath = \"{HAY}\"\n");
    let module = format!(
        "extern crate self as keyed;\n#[cfg(test)]\n#[path = \"../{HAY}\"]\nmod haystack;\n"
    );
    for (lib, manifest) in [("", manifest.as_str()), (module.as_str(), "")] {
        let run = run(lib, manifest, Some(&source))?;
        assert_eq!(run.related_files, [HAY, "tests/api.rs"]);
    }
    Ok(())
}

#[test]
fn draft_retains_a_file_mounted_by_an_unselected_workspace_member() -> Result<(), String> {
    let scratch = nested_package_fixture()?;
    scratch.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"3\"\n",
    )?;
    scratch.write("b/Cargo.toml", &MANIFEST.replace("\"keyed\"", "\"b\""))?;
    scratch.write(
        "b/src/lib.rs",
        "#[path = \"../../a/benchmarks/hay.rs\"]\nmod mount;\n#[path = \"../../a/src/lib.rs\"]\nmod implementation;\nuse implementation::discount;\n",
    )?;
    for mode in [Mode::Draft, Mode::Deep] {
        assert_eq!(
            analyze(&scratch, &mode)?.related_files,
            ["a/benchmarks/hay.rs", "a/tests/api.rs"],
            "{mode:?}"
        );
    }
    scratch.write("b/src/lib.rs", "pub fn unrelated() {}\n")?;
    assert_eq!(
        analyze(&scratch, &Mode::Deep)?.related_files,
        ["a/tests/api.rs"]
    );
    Ok(())
}

#[test]
fn a_declared_test_target_can_forward_a_module_keyword() -> Result<(), String> {
    let scratch = Scratch::create()?;
    scratch.write(
        "Cargo.toml",
        &format!("{MANIFEST}\n[[test]]\nname = \"driver\"\npath = \"benchmarks/driver.rs\"\n"),
    )?;
    scratch.write("src/lib.rs", LIB)?;
    scratch.write("tests/api.rs", TESTS)?;
    scratch.write(
        "benchmarks/hay.rs",
        &HAY_SOURCE.replace("keyed::", "super::"),
    )?;
    scratch.write(
        "benchmarks/driver.rs",
        "use keyed::discount;\nmacro_rules! mount { ($kw:tt, $name:ident) => { $kw $name; }; }\nmount!(mod, hay);\n",
    )?;
    scratch.write("change.diff", DIFF)?;
    for mode in [Mode::Draft, Mode::Deep] {
        assert_eq!(
            analyze(&scratch, &mode)?.related_files,
            ["benchmarks/hay.rs", "tests/api.rs"],
            "{mode:?}"
        );
    }
    Ok(())
}

#[test]
fn an_unreadable_or_malformed_root_manifest_keeps_uncertainty() -> Result<(), String> {
    let absent = nested_package_fixture()?;
    let malformed = nested_package_fixture()?;
    malformed.write("Cargo.toml", "[workspace\n")?;
    let unreadable = nested_package_fixture()?;
    std::fs::create_dir(unreadable.0.join("Cargo.toml")).map_err(|error| error.to_string())?;
    for mode in [Mode::Draft, Mode::Deep] {
        assert_eq!(analyze(&absent, &mode)?.related_files, ["a/tests/api.rs"]);
        for scratch in [&malformed, &unreadable] {
            assert_eq!(
                analyze(scratch, &mode)?.related_files,
                ["a/benchmarks/hay.rs", "a/tests/api.rs"],
                "{mode:?}"
            );
        }
    }
    Ok(())
}
