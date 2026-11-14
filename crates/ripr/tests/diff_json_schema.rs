//! The published `ripr diff --json` schema, checked against bytes the real
//! command wrote.
//!
//! The command writes exactly one of two documents to stdout:
//! `schemas/ripr/diff.schema.json` describes the `ripr_diff` report, and
//! `schemas/ripr/diff-refusal.schema.json` the refusal envelope a run writes
//! with exit code 2 when it cannot produce a report.

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
const DIFF_REFUSAL_SCHEMA: &str = "schemas/ripr/diff-refusal.schema.json";
const CHECK_SCHEMA: &str = "schemas/ripr/check.schema.json";

/// Definitions each schema copies from `check.schema.json`, because the
/// repository validator resolves local references only.
const SHARED_WITH_CHECK: [(&str, &[&str]); 2] = [
    (
        DIFF_SCHEMA,
        &[
            "analysis_outcome",
            "analysis_identity",
            "analysis_counts",
            "analysis_limitation",
            "recovery",
            "nullable_string",
            "preview_language",
        ],
    ),
    (
        DIFF_REFUSAL_SCHEMA,
        &["summary", "run_limitation", "refusal_remedy"],
    ),
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

/// Stdout of a `ripr diff --json` run that must exit 2 with a refusal.
fn diff_refusal_json(root: &Path, base: &str) -> Result<Value, String> {
    let root_arg = root.display().to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["diff", "--json", "--root", &root_arg, "--base", base])
        .output()
        .map_err(|error| format!("spawn ripr diff: {error}"))?;
    if output.status.code() != Some(2) {
        return Err(format!(
            "ripr diff --json --base {base} should exit 2, got {:?}",
            output.status.code()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "refusal JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn conforms(document: &Value, schema: &Value) -> Result<(), String> {
    published_schema::check(document, schema, schema, "diff")
}

#[test]
fn the_checker_covers_every_keyword_the_diff_schemas_use() -> Result<(), String> {
    for path in [DIFF_SCHEMA, DIFF_REFUSAL_SCHEMA] {
        let schema = published_schema::read(path)?;
        let ignored = published_schema::unsupported_keywords(&schema);
        if !ignored.is_empty() {
            return Err(format!(
                "the conformance checker ignores these keywords of {path}, so it would under-check the schema: {ignored:?}"
            ));
        }
    }
    Ok(())
}

/// `analysis_outcome` and `preview_languages` are the same objects in check
/// JSON and diff JSON. The diff schema carries copies, so this holds each copy
/// equal to the check schema's definition: a field added to one command's
/// outcome cannot be published for one schema and forgotten in the other.
#[test]
fn definitions_shared_with_the_check_schema_are_identical() -> Result<(), String> {
    let check = published_schema::read(CHECK_SCHEMA)?;
    for (path, names) in SHARED_WITH_CHECK {
        let schema = published_schema::read(path)?;
        let mut pointers: Vec<String> = names.iter().map(|name| format!("/$defs/{name}")).collect();
        pointers.push("/properties/mode".to_string());
        for pointer in pointers {
            let ours = schema
                .pointer(&pointer)
                .ok_or_else(|| format!("{path} has no {pointer}"))?;
            let theirs = check
                .pointer(&pointer)
                .ok_or_else(|| format!("check schema has no {pointer}"))?;
            if ours != theirs {
                return Err(format!(
                    "{pointer} differs between {path} and check.schema.json; copy the check schema's definition"
                ));
            }
        }
        // Every definition the schema carries is either shared or its own:
        // a copied definition that is not listed here would not be held equal.
        let carried = schema["$defs"].as_object().map_or(0, serde_json::Map::len);
        let own = usize::from(path == DIFF_SCHEMA) * 3;
        if carried != names.len() + own {
            return Err(format!(
                "{path} carries {carried} definitions; expected {} shared and {own} of its own",
                names.len()
            ));
        }
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
    // The incomplete arm comes from a preview-language advisory, which only
    // a build with the TypeScript adapter raises. A Rust-only build reports
    // the same range as complete; it is still checked for conformance above,
    // and the incomplete branch is then covered by the committed fixture.
    let incomplete_arm = if cfg!(feature = "lang-typescript") {
        json!(["diff_incomplete_full_repo_limited", false, "typescript"])
    } else {
        json!(["diff_complete_full_repo_limited", true, null])
    };
    let expected = json!({
        "with_finding": ["diff_complete_full_repo_limited", true],
        "empty": ["diff_complete_full_repo_limited", 0, "no_scope"],
        "incomplete": incomplete_arm,
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
    // The completeness mutations below edit `analysis_outcome` in place, so
    // the base document must carry one or they would fail for another reason.
    if report["analysis_outcome"]["analysis_complete"] != json!(true) {
        return Err(format!(
            "expected a complete typed outcome in the base document, got {}",
            report["analysis_outcome"]
        ));
    }
    let cases: [(&str, Mutation); 15] = [
        ("complete run with an incomplete outcome flag", |doc| {
            doc["analysis_outcome"]["analysis_complete"] = json!(false)
        }),
        ("complete run with an incomplete outcome kind", |doc| {
            doc["analysis_outcome"]["outcome"]["kind"] = json!("partial_with_limitations")
        }),
        ("unlisted outcome kind", |doc| {
            doc["analysis_outcome"]["outcome"]["kind"] = json!("clean")
        }),
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

/// A run that cannot produce a report still writes one schema-conforming
/// document: the refusal envelope, for a base that does not resolve and for
/// a config that does not parse.
#[test]
fn diff_json_refusals_conform_to_the_published_schema() -> Result<(), String> {
    let schema = published_schema::read(DIFF_REFUSAL_SCHEMA)?;
    let root = fixture_repo("refusal")?;
    let bad_base = diff_refusal_json(&root, "no-such-ref-14067");
    let _ = std::fs::remove_dir_all(&root);
    let bad_base = bad_base?;
    conforms(&bad_base, &schema).map_err(|error| format!("unresolvable base: {error}"))?;

    // A second refusal identity through the same envelope: a config that
    // does not parse refuses before any base is resolved.
    let broken = fixture_repo("refusal-config")?;
    write(&broken, "ripr.toml", "[analysis\nmode = \n")?;
    let bad_config = diff_refusal_json(&broken, "HEAD~1");
    let _ = std::fs::remove_dir_all(&broken);
    let bad_config = bad_config?;
    conforms(&bad_config, &schema).map_err(|error| format!("unreadable config: {error}"))?;

    let observed = json!([
        bad_base["analysis_scope"]["run_status"],
        bad_base["base"],
        bad_base.get("kind"),
        bad_config["analysis_scope"]["run_status"] != "base_unresolvable",
        bad_config["analysis_scope"]["run_status"].is_string(),
    ]);
    let expected = json!(["base_unresolvable", "no-such-ref-14067", null, true, true]);
    if observed == expected {
        Ok(())
    } else {
        Err(format!("unexpected refusals: {observed}"))
    }
}

/// The negative experiment for the refusal arm.
#[test]
fn published_diff_refusal_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut Value);
    let schema = published_schema::read(DIFF_REFUSAL_SCHEMA)?;
    let root = fixture_repo("refusal-negative")?;
    let refusal = diff_refusal_json(&root, "no-such-ref-14067");
    let _ = std::fs::remove_dir_all(&root);
    let refusal = refusal?;
    conforms(&refusal, &schema)?;
    let cases: [(&str, Mutation); 8] = [
        ("unknown field", |doc| doc["status"] = json!("refused")),
        ("a refusal that carries a finding", |doc| {
            doc["findings"] = json!([{}])
        }),
        ("a refusal with a non-zero summary", |doc| {
            doc["summary"]["findings"] = json!(1)
        }),
        ("a refusal reported as consumable", |doc| {
            doc["analysis_scope"]["downstream_consumable"] = json!(true)
        }),
        ("no run_limitations entry", |doc| {
            doc["run_limitations"] = json!([])
        }),
        ("a limitation with no remedy", |doc| {
            if let Some(object) = doc["run_limitations"][0].as_object_mut() {
                object.remove("remedy");
            }
        }),
        ("a remedy flag that is not a long flag", |doc| {
            doc["run_limitations"][0]["remedy"]["flags"] = json!(["base"])
        }),
        ("another command's scope", |doc| {
            doc["analysis_scope"]["scope"] = json!("repo")
        }),
    ];
    for (label, mutate) in cases {
        let mut document = refusal.clone();
        mutate(&mut document);
        if document == refusal {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&document, &schema) {
            Ok(()) => {
                return Err(format!(
                    "the published diff refusal schema accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => println!("rejected as expected ({label}): {error}"),
        }
    }
    Ok(())
}
