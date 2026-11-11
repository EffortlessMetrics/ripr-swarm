//! The published `ripr diff --json` schema, checked against bytes the real
//! command wrote.
//!
//! `schemas/ripr/diff.schema.json` describes the `ripr_diff` report. A run
//! that cannot produce a report writes the separate refusal envelope instead;
//! that document is not a `ripr_diff` and this schema does not describe it.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;
#[path = "common/published_schema.rs"]
mod published_schema;

use common::fixture_git::fixture_git_ok;

const DIFF_SCHEMA: &str = "schemas/ripr/diff.schema.json";
const CHECK_SCHEMA: &str = "schemas/ripr/check.schema.json";

/// Definitions `diff.schema.json` copies from `check.schema.json`, because the
/// repository validator resolves local references only.
const SHARED_WITH_CHECK: [&str; 7] = [
    "analysis_outcome",
    "analysis_identity",
    "analysis_counts",
    "analysis_limitation",
    "recovery",
    "nullable_string",
    "preview_language",
];

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

const BASE_LIB: &str =
    "pub fn over(amount: i32, threshold: i32) -> bool {\n    amount > threshold\n}\n";
const CHANGED_LIB: &str =
    "pub fn over(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}\n";

fn write(root: &Path, path: &str, text: &str) -> Result<(), String> {
    std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
}

fn commit(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(root, &["add", "-A"])?;
    fixture_git_ok(root, &["commit", "-q", "-m", message])
}

/// A two-commit Rust package: the second commit moves one predicate boundary
/// that the only test does not pin.
fn fixture_repo(label: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-diff-schema-{label}-{}-{stamp}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|error| format!("create {dir}: {error}"))?;
    }
    fixture_git_ok(&root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(&root, &["config", "user.email", "diff-schema@t.st"])?;
    fixture_git_ok(&root, &["config", "user.name", "diff-schema"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"diff_schema_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(&root, "src/lib.rs", BASE_LIB)?;
    write(
        &root,
        "tests/over.rs",
        "use diff_schema_fixture::over;\n#[test]\nfn big() {\n    assert!(over(10, 1));\n}\n",
    )?;
    commit(&root, "base")?;
    write(&root, "src/lib.rs", CHANGED_LIB)?;
    commit(&root, "move the boundary")?;
    Ok(root)
}

fn diff_json(root: &Path, base: &str) -> Result<Value, String> {
    let root_arg = root.display().to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["diff", "--json", "--root", &root_arg, "--base", base])
        .output()
        .map_err(|error| format!("spawn ripr diff: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ripr diff --json --base {base} exited {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "diff JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn conforms(document: &Value, schema: &Value) -> Result<(), String> {
    published_schema::check(document, schema, schema, "diff")
}

#[test]
fn the_checker_covers_every_keyword_the_diff_schema_uses() -> Result<(), String> {
    let schema = published_schema::read(DIFF_SCHEMA)?;
    let ignored = published_schema::unsupported_keywords(&schema);
    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the conformance checker ignores these published keywords, so it would under-check the schema: {ignored:?}"
        ))
    }
}

/// `analysis_outcome` and `preview_languages` are the same objects in check
/// JSON and diff JSON. The diff schema carries copies, so this holds each copy
/// equal to the check schema's definition: a field added to one command's
/// outcome cannot be published for one schema and forgotten in the other.
#[test]
fn definitions_shared_with_the_check_schema_are_identical() -> Result<(), String> {
    let diff = published_schema::read(DIFF_SCHEMA)?;
    let check = published_schema::read(CHECK_SCHEMA)?;
    for name in SHARED_WITH_CHECK {
        let pointer = format!("/$defs/{name}");
        let ours = diff
            .pointer(&pointer)
            .ok_or_else(|| format!("diff schema has no {pointer}"))?;
        let theirs = check
            .pointer(&pointer)
            .ok_or_else(|| format!("check schema has no {pointer}"))?;
        if ours != theirs {
            return Err(format!(
                "{pointer} differs between diff.schema.json and check.schema.json; copy the check schema's definition"
            ));
        }
    }
    if diff.pointer("/properties/mode") != check.pointer("/properties/mode") {
        return Err("`mode` differs between diff.schema.json and check.schema.json".to_string());
    }
    Ok(())
}

/// Three real runs, one per arm of the contract: a complete diff with a
/// finding, a complete empty diff, and an incomplete diff whose outcome and
/// preview-language advisory are both present.
#[test]
fn diff_json_conforms_to_the_published_schema_in_each_completion_state() -> Result<(), String> {
    let schema = published_schema::read(DIFF_SCHEMA)?;
    let root = fixture_repo("states")?;

    let with_finding = diff_json(&root, "HEAD~1")?;
    conforms(&with_finding, &schema).map_err(|error| format!("complete with finding: {error}"))?;

    let empty = diff_json(&root, "HEAD")?;
    conforms(&empty, &schema).map_err(|error| format!("empty diff: {error}"))?;

    write(
        &root,
        "app.ts",
        "export function f(a: number) { return a > 1; }\n",
    )?;
    commit(&root, "add a preview-language file")?;
    let incomplete = diff_json(&root, "HEAD~1")?;
    let _ = std::fs::remove_dir_all(&root);
    conforms(&incomplete, &schema).map_err(|error| format!("incomplete: {error}"))?;

    // The runs are the arms intended, so three conforming documents are not
    // one arm three times.
    let observed = json!({
        "with_finding": [with_finding["run_status"], with_finding["changed_seams"].as_array().is_some_and(|seams| !seams.is_empty())],
        "empty": [empty["run_status"], empty["changed_seams"].as_array().map(Vec::len), empty["analysis_outcome"]["outcome"]["kind"]],
        "incomplete": [incomplete["run_status"], incomplete["analysis_outcome"]["analysis_complete"], incomplete["preview_languages"][0]["language"]],
    });
    let expected = json!({
        "with_finding": ["diff_complete_full_repo_limited", true],
        "empty": ["diff_complete_full_repo_limited", 0, "no_scope"],
        "incomplete": ["diff_incomplete_full_repo_limited", false, "typescript"],
    });
    if observed == expected {
        Ok(())
    } else {
        Err(format!("unexpected runs: {observed}"))
    }
}

/// The negative experiment: each case starts from real emitted bytes and
/// breaks one thing a consumer relies on.
#[test]
fn published_diff_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut Value);
    let schema = published_schema::read(DIFF_SCHEMA)?;
    let root = fixture_repo("negative")?;
    let report = diff_json(&root, "HEAD~1");
    let _ = std::fs::remove_dir_all(&root);
    let report = report?;
    conforms(&report, &schema)?;
    let cases: [(&str, Mutation); 12] = [
        ("unknown top-level field", |doc| {
            doc["verdict"] = json!("clean")
        }),
        ("unknown seam field", |doc| {
            doc["changed_seams"][0]["runtime_outcome"] = json!(true)
        }),
        ("stale schema_version", |doc| {
            doc["schema_version"] = json!("0.2")
        }),
        ("another document kind", |doc| {
            doc["kind"] = json!("ripr_check")
        }),
        ("missing required top-level field", |doc| {
            if let Some(object) = doc.as_object_mut() {
                object.remove("runtime_status");
            }
        }),
        ("missing required seam field", |doc| {
            if let Some(object) = doc["changed_seams"][0].as_object_mut() {
                object.remove("classification");
            }
        }),
        ("unlisted run_status", |doc| {
            doc["run_status"] = json!("clean")
        }),
        ("run_status disagreeing with runtime_status", |doc| {
            doc["runtime_status"]["state"] = json!("diff_incomplete_full_repo_limited")
        }),
        ("incomplete run without an outcome", |doc| {
            doc["run_status"] = json!("diff_incomplete_full_repo_limited")
        }),
        ("full-repository context reported as consumable", |doc| {
            doc["runtime_status"]["full_repo_context"]["downstream_consumable"] = json!(true)
        }),
        ("negative count", |doc| doc["summary"]["probes"] = json!(-1)),
        ("empty preview_languages array", |doc| {
            doc["preview_languages"] = json!([])
        }),
    ];
    for (label, mutate) in cases {
        let mut document = report.clone();
        mutate(&mut document);
        if document == report {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&document, &schema) {
            Ok(()) => {
                return Err(format!(
                    "the published diff schema accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => println!("rejected as expected ({label}): {error}"),
        }
    }
    Ok(())
}
