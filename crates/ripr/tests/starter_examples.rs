//! The starter examples under the repository's `examples/` directory show
//! what their READMEs say (#14151).
//!
//! Each example is one small project whose `change.diff` moves a boundary
//! from exclusive to inclusive while the tests only probe either side of it.
//! ripr must report that one changed predicate as `weakly_exposed` with the
//! tests as first written, and as `exposed` once the example's strengthened
//! test file (one added assertion at the boundary) replaces them. Both are
//! static evidence about the tests' assertions; no test or mutant is run.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Starter {
    /// Directory under `examples/`.
    name: &'static str,
    /// The changed source file, relative to the project root.
    source: &'static str,
    /// The line of the changed predicate in `source`.
    line: u64,
    /// The boundary the first check must name as the missing test input.
    boundary: &'static str,
    /// The test file the strengthened copy replaces, relative to the project
    /// root; the strengthened copy has the same file name.
    test: &'static str,
}

fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("create {}: {error}", to.display()))?;
    let entries =
        std::fs::read_dir(from).map_err(|error| format!("read {}: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", from.display()))?;
        let name = entry.file_name();
        // Build output a contributor may have left behind is not part of the example.
        if name == "target" || name == "node_modules" {
            continue;
        }
        let target = to.join(name);
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .map_err(|error| format!("copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Runs the command each example's README gives, from the example directory,
/// and returns the findings after checking that the analysis is complete.
fn findings(example: &Path) -> Result<Vec<serde_json::Value>, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(example)
        .args([
            "check",
            "--root",
            "project",
            "--diff",
            "change.diff",
            "--json",
        ])
        .output()
        .map_err(|error| format!("run ripr: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let document: serde_json::Value = serde_json::from_str(&stdout).map_err(|error| {
        format!(
            "invalid JSON ({error}); status {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let outcome = &document["analysis_outcome"];
    if outcome["analysis_complete"] != true
        || outcome["outcome"]["limitations"] != serde_json::json!([])
    {
        return Err(format!("analysis is not complete: {outcome}"));
    }
    document["findings"]
        .as_array()
        .cloned()
        .ok_or_else(|| "findings is not an array".to_string())
}

/// The class of the example's single finding, which must be the changed
/// predicate on the documented line of the documented file.
fn only_class(starter: &Starter, findings: &[serde_json::Value]) -> Result<String, String> {
    let [finding] = findings else {
        return Err(format!(
            "{}: expected one finding, got {}",
            starter.name,
            findings.len()
        ));
    };
    let probe = &finding["probe"];
    let file = probe["file"].as_str().unwrap_or_default();
    if !file.ends_with(starter.source) || probe["line"] != starter.line {
        return Err(format!(
            "{}: finding is at {file}:{}, expected {}:{}",
            starter.name, probe["line"], starter.source, starter.line
        ));
    }
    finding["classification"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{}: finding has no classification", starter.name))
}

fn weak_then_exposed(starter: &Starter) -> Result<(), String> {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-starter-example-{}-{}",
        starter.name,
        std::process::id()
    )));
    copy_tree(&examples_root().join(starter.name), &scratch.0)?;
    let example = &scratch.0;

    let before = findings(example)?;
    assert_eq!(
        only_class(starter, &before)?,
        "weakly_exposed",
        "{}: tests as first written",
        starter.name
    );
    let missing = before[0]["missing"].to_string();
    assert!(
        missing.contains(starter.boundary),
        "{}: missing check does not name `{}`: {missing}",
        starter.name,
        starter.boundary
    );

    let test_name = Path::new(starter.test)
        .file_name()
        .ok_or_else(|| format!("{}: test path has no file name", starter.name))?;
    std::fs::copy(
        example.join("strengthened").join(test_name),
        example.join("project").join(starter.test),
    )
    .map_err(|error| format!("{}: copy strengthened test: {error}", starter.name))?;

    let after = findings(example)?;
    assert_eq!(
        only_class(starter, &after)?,
        "exposed",
        "{}: strengthened tests",
        starter.name
    );
    Ok(())
}

#[test]
fn rust_starter_example_goes_from_weak_to_exposed() -> Result<(), String> {
    weak_then_exposed(&Starter {
        name: "rust-discount-threshold",
        source: "src/lib.rs",
        line: 5,
        test: "tests/pricing.rs",
        boundary: "subtotal == DISCOUNT_THRESHOLD",
    })
}

#[cfg(feature = "lang-python")]
#[test]
fn python_starter_example_goes_from_weak_to_exposed() -> Result<(), String> {
    weak_then_exposed(&Starter {
        name: "python-bulk-discount",
        source: "pricing/core.py",
        line: 3,
        test: "tests/test_pricing.py",
        boundary: "units == 20",
    })
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_starter_example_goes_from_weak_to_exposed() -> Result<(), String> {
    weak_then_exposed(&Starter {
        name: "typescript-discount-threshold",
        source: "src/pricing.ts",
        line: 5,
        test: "test/pricing.test.ts",
        boundary: "subtotal == DISCOUNT_THRESHOLD",
    })
}
