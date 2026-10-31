//! #10875: the absorbing-`min` rule through the real classifier call site
//! (`classifier/evidence.rs`), not only its sink constructor.

use super::OutputFormat;
use crate::app::{CheckInput, Mode, check_workspace};
use crate::domain::{ExposureClass, FlowSinkKind};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,4 +1,4 @@\n pub fn f(n: u32) -> u32 {\n-    let r: u32 = if n == 5 { 1 } else { 0 };\n+    let r: u32 = if n == 6 { 1 } else { 0 };\n     r.min(0)\n }\n";
const PARKED_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn f(n: u32) -> u32 {\n     let cap = n / 6;\n-    let r: u32 = if n == 5 { 1 } else { 0 };\n+    let r: u32 = if n == 6 { 1 } else { 0 };\n     r.min(cap)\n }\n";
const TEST: &str = "use demo::*;\n#[test]\nfn pins_f() {\n    assert_eq!(f(5), 0);\n}\n";

fn temp_root(name: &str) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("ripr-absorbing-{name}-{stamp}"));
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("create src: {err}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|err| format!("create tests: {err}"))?;
    Ok(root)
}

/// The class and unknown-sink text of the one finding for `lib` plus any
/// extra workspace files.
fn run(
    name: &str,
    lib: &str,
    diff: &str,
    extra: &[(&str, &str)],
) -> Result<(ExposureClass, Vec<String>), String> {
    let root = temp_root(name)?;
    let write = |rel: &str, text: &str| {
        std::fs::write(root.join(rel), text).map_err(|err| format!("write {rel}: {err}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    write("src/lib.rs", lib)?;
    write("tests/t.rs", TEST)?;
    write("example.diff", diff)?;
    for (rel, text) in extra {
        write(rel, text)?;
    }
    let output = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(root.join("example.diff")),
        mode: Mode::Draft,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let finding = output
        .findings
        .iter()
        .find(|finding| finding.probe.expression == "n == 6")
        .ok_or_else(|| "fixture precondition: a finding for `n == 6`".to_string())?;
    let result = (
        finding.class.clone(),
        finding
            .flow_sinks
            .iter()
            .filter(|sink| sink.kind == FlowSinkKind::Unknown)
            .map(|sink| sink.text.clone())
            .collect(),
    );
    std::fs::remove_dir_all(root).map_err(|err| format!("remove root: {err}"))?;
    Ok(result)
}

const CLAMPED: &str =
    "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 6 { 1 } else { 0 };\n    r.min(0)\n}\n";

/// Positive: the issue's shape reads a propagation limit, never exposed.
#[test]
fn literal_min_reads_a_propagation_limit_through_the_classifier() -> Result<(), String> {
    let (class, sinks) = run("positive", CLAMPED, DIFF, &[])?;
    assert_eq!(class, ExposureClass::PropagationUnknown);
    assert!(
        sinks
            .iter()
            .any(|text| text.contains("pins the returned `u32`")),
        "{sinks:?}"
    );
    Ok(())
}

/// The parked witness: `cap = n / 6` makes the condition observable at
/// `n == 6`, so a non-literal operand keeps the ordinary returned-value sink.
#[test]
fn parked_non_literal_witness_stays_exposed() -> Result<(), String> {
    let lib = "pub fn f(n: u32) -> u32 {\n    let cap = n / 6;\n    let r: u32 = if n == 6 { 1 } else { 0 };\n    r.min(cap)\n}\n";
    let (class, sinks) = run("parked", lib, PARKED_DIFF, &[])?;
    assert_ne!(class, ExposureClass::PropagationUnknown, "{sinks:?}");
    assert!(sinks.is_empty(), "{sinks:?}");
    Ok(())
}

/// Any workspace that could replace std's dispatch keeps the sink: a custom
/// `min`, `#![no_implicit_prelude]`, a macro-generated item, a primitive
/// rename.
#[test]
fn a_workspace_that_can_replace_std_dispatch_keeps_the_sink() -> Result<(), String> {
    for (name, extra) in [
        (
            "custom-min",
            (
                "src/extra.rs",
                "pub trait Floor { fn min(self, o: u32) -> u32; }\n",
            ),
        ),
        ("no-prelude", ("src/extra.rs", "#![no_implicit_prelude]\n")),
        (
            "macro-fn",
            (
                "src/extra.rs",
                "macro_rules! m { ($n:ident) => { fn $n() {} }; }\n",
            ),
        ),
        ("rename", ("src/extra.rs", "use other::Wide as u32;\n")),
        (
            "extern-rename",
            ("src/extra.rs", "extern crate other as u32;\n"),
        ),
    ] {
        let (class, sinks) = run(name, CLAMPED, DIFF, &[extra])?;
        assert_ne!(
            class,
            ExposureClass::PropagationUnknown,
            "{name}: {sinks:?}"
        );
        assert!(sinks.is_empty(), "{name}: {sinks:?}");
    }
    Ok(())
}
