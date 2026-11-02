//! #5395: dependent-crate tests that reach an owner only through trait
//! dispatch must not produce a confident `no_static_path`. Nested
//! `macro_rules!` `test!` → `test_fn!` generators stay unindexed (cause 2
//! residual; no nested expansion).

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-dependent-crate-reach-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(root: &std::path::Path, path: &str, text: &str) -> Result<(), String> {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(full, text).map_err(|error| error.to_string())
}

const CORE_LIB: &str = "pub fn write_escaped(text: &str) -> String {\n    text.replace('&', \"&amp;\")\n}\n\npub trait Serialize {\n    fn serialize(&self) -> String;\n}\n\npub fn serialize(node: &impl Serialize) -> String {\n    node.serialize()\n}\n";

const CORE_DIFF: &str = "diff --git a/html_core/src/lib.rs b/html_core/src/lib.rs\n--- a/html_core/src/lib.rs\n+++ b/html_core/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn write_escaped(text: &str) -> String {\n-    text.to_string()\n+    text.replace('&', \"&amp;\")\n }\n";

const DOM_LIB: &str = "pub struct Node(pub &'static str);\n\nimpl html_core::Serialize for Node {\n    fn serialize(&self) -> String {\n        html_core::write_escaped(self.0)\n    }\n}\n";

const DOM_TEST: &str = "#[test]\nfn amp_is_escaped() {\n    assert_eq!(html_core::serialize(&html_dom::Node(\"&\")), \"&amp;\");\n}\n";

fn check_json(root: &std::path::Path, diff: &str) -> Result<Value, String> {
    let diff_file = root.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.to_path_buf(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())
}

fn write_escaped_finding(report: &Value) -> Result<&Value, String> {
    let findings = report["findings"]
        .as_array()
        .ok_or("findings is not an array")?;
    findings
        .iter()
        .find(|finding| {
            finding["probe"]["expression"]
                .as_str()
                .is_some_and(|expression| {
                    expression.contains("replace")
                        || expression.contains("write_escaped")
                        || expression.contains("&amp;")
                })
                || finding["probe"]["owner"]
                    .as_str()
                    .is_some_and(|owner| owner.contains("write_escaped"))
        })
        .or_else(|| findings.first())
        .ok_or_else(|| format!("no finding: {report}"))
}

fn two_crate_workspace(dom_test: Option<&str>) -> Result<Scratch, String> {
    let scratch = Scratch::create()?;
    let root = &scratch.0;
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"html_core\", \"html_dom\"]\nresolver = \"2\"\n",
    )?;
    write(
        root,
        "html_core/Cargo.toml",
        "[package]\nname = \"html_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n",
    )?;
    write(root, "html_core/src/lib.rs", CORE_LIB)?;
    write(
        root,
        "html_dom/Cargo.toml",
        "[package]\nname = \"html_dom\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[dependencies]\nhtml_core = { path = \"../html_core\" }\n",
    )?;
    write(root, "html_dom/src/lib.rs", DOM_LIB)?;
    if let Some(test) = dom_test {
        write(root, "html_dom/tests/html-serializer.rs", test)?;
    }
    Ok(scratch)
}

#[test]
fn trait_dispatch_from_a_dependent_crate_withholds_no_static_path() -> Result<(), String> {
    let scratch = two_crate_workspace(Some(DOM_TEST))?;
    let report = check_json(&scratch.0, CORE_DIFF)?;
    let finding = write_escaped_finding(&report)?;
    assert_eq!(finding["classification"], "static_unknown", "{finding}");
    assert_eq!(
        finding["static_limit_kind"], "rust_dependent_crate_reach_unresolved",
        "{finding}"
    );
    let related = finding["related_tests"].as_array();
    assert!(
        related.is_none_or(|tests| tests.is_empty()),
        "trait-dispatch tests must not be related: {finding}"
    );
    let reach = finding["ripr"]["reach"]["state"]
        .as_str()
        .or_else(|| finding["ripr"]["reach"].as_str());
    assert_ne!(
        reach,
        Some("no"),
        "reach must not stay ruled out: {finding}"
    );
    Ok(())
}

#[test]
fn a_workspace_with_no_dependents_keeps_no_static_path() -> Result<(), String> {
    let scratch = Scratch::create()?;
    let root = &scratch.0;
    write(
        root,
        "Cargo.toml",
        "[package]\nname = \"html_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n",
    )?;
    write(root, "src/lib.rs", CORE_LIB)?;
    let diff = CORE_DIFF
        .replace("a/html_core/src/lib.rs", "a/src/lib.rs")
        .replace("b/html_core/src/lib.rs", "b/src/lib.rs");
    let report = check_json(root, &diff)?;
    let finding = write_escaped_finding(&report)?;
    assert_eq!(finding["classification"], "no_static_path", "{finding}");
    assert!(
        finding
            .get("static_limit_kind")
            .is_none_or(|kind| kind.is_null()
                || kind.as_str() == Some("rust_integration_public_api_path_unresolved")
                || kind.as_str() == Some("rust_transitive_reach_unresolved")),
        "{finding}"
    );
    Ok(())
}

#[test]
fn a_dependent_crate_with_no_tests_keeps_no_static_path() -> Result<(), String> {
    let scratch = two_crate_workspace(None)?;
    let report = check_json(&scratch.0, CORE_DIFF)?;
    let finding = write_escaped_finding(&report)?;
    assert_eq!(finding["classification"], "no_static_path", "{finding}");
    Ok(())
}

#[test]
fn a_dependent_crate_that_names_the_owner_still_relates() -> Result<(), String> {
    let scratch = two_crate_workspace(Some(
        "#[test]\nfn amp_is_escaped() {\n    assert_eq!(html_core::write_escaped(\"&\"), \"&amp;\");\n}\n",
    ))?;
    let report = check_json(&scratch.0, CORE_DIFF)?;
    let finding = write_escaped_finding(&report)?;
    let related = finding["related_tests"]
        .as_array()
        .ok_or_else(|| format!("related_tests missing: {finding}"))?;
    assert!(
        related.iter().any(|test| test["name"] == "amp_is_escaped"),
        "direct unique owner call must relate: {finding}"
    );
    assert_ne!(
        finding["classification"], "no_static_path",
        "a related test must not stay a confident no-path: {finding}"
    );
    assert_ne!(
        finding["static_limit_kind"], "rust_dependent_crate_reach_unresolved",
        "related tests must not take the dependent-crate withhold: {finding}"
    );
    Ok(())
}

/// Cause 2 residual: `test!` forwards to a local `test_fn!` whose transcriber
/// emits `#[test] fn $name`. Bounded local-macro expansion requires the
/// invocation to be the generator with `#[test]` in its own transcriber, so
/// this nested chain is not indexed.
#[test]
fn nested_test_macro_chain_is_not_indexed() -> Result<(), String> {
    let scratch = Scratch::create()?;
    let root = &scratch.0;
    write(
        root,
        "Cargo.toml",
        "[package]\nname = \"html_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n",
    )?;
    write(
        root,
        "src/lib.rs",
        "pub fn write_escaped(text: &str) -> String {\n    text.replace('&', \"&amp;\")\n}\n",
    )?;
    write(
        root,
        "tests/html-serializer.rs",
        "macro_rules! test_fn {\n    ($f:ident, $name:ident, $input:expr, $output:expr) => {\n        #[test]\n        fn $name() {\n            assert_eq!($f($input), $output);\n        }\n    };\n}\nmacro_rules! test {\n    ($($t:tt)*) => {\n        test_fn!(write_escaped, $($t)*);\n    };\n}\ntest!(amp_is_escaped, \"&\", \"&amp;\");\n",
    )?;
    let diff = CORE_DIFF
        .replace("a/html_core/src/lib.rs", "a/src/lib.rs")
        .replace("b/html_core/src/lib.rs", "b/src/lib.rs");
    let report = check_json(root, &diff)?;
    let finding = write_escaped_finding(&report)?;
    let related = finding["related_tests"].as_array();
    assert!(
        related.is_none_or(|tests| { !tests.iter().any(|test| test["name"] == "amp_is_escaped") }),
        "nested test! → test_fn! must stay unindexed: {finding}"
    );
    Ok(())
}
