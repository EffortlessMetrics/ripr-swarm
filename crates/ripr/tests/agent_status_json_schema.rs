//! The published `ripr agent status --json` schema, checked against bytes the
//! real command wrote at each stage of a repair loop.
//!
//! `schemas/ripr/agent-status.schema.json` describes the inventory document.
//! The documents here come from a fresh workspace, an attempt awaiting its
//! test edit, a finished attempt with an issued receipt, and a failed attempt,
//! so the seam, receipt, test-run and warning branches are all read from real
//! output rather than from a hand-written fixture.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;
#[path = "common/published_schema.rs"]
mod published_schema;

use common::fixture_git::fixture_git_ok;

const AGENT_STATUS_SCHEMA: &str = "schemas/ripr/agent-status.schema.json";

/// The boundary-gap seam the fixture's `>=` comparison produces.
const SEAM: &str = "67fc764ba37d77bd";

const WEAK_TEST: &str = "use boundary_gap_fixture::discounted_total;\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n";

const BOUNDARY_TEST: &str = "\n#[test]\nfn at_threshold_discounts() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

/// The seven loop artifacts in the order status reports them.
const ARTIFACT_NAMES: [&str; 7] = [
    "before_snapshot",
    "after_snapshot",
    "analysis_outcome",
    "agent_brief",
    "agent_packet",
    "agent_verify",
    "agent_receipt",
];

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ripr-agent-status-schema-{label}-{}-{stamp}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write(root: &Path, path: &str, text: &str) -> Result<(), String> {
    std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
}

/// A committed one-function package whose tests miss the `>=` boundary, with
/// Cargo's build directory ignored so the repair edit cage admits it.
fn fixture_repo(label: &str) -> Result<PathBuf, String> {
    let root = unique(label);
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|error| format!("create {dir}: {error}"))?;
    }
    fixture_git_ok(&root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(&root, &["config", "user.email", "agent-status@t.st"])?;
    fixture_git_ok(&root, &["config", "user.name", "agent-status"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"boundary_gap_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"boundary_gap_fixture\"\npath = \"src/lib.rs\"\n",
    )?;
    write(&root, ".gitignore", "/target/\n")?;
    write(
        &root,
        "src/lib.rs",
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
    )?;
    write(&root, "tests/pricing.rs", WEAK_TEST)?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    Ok(root)
}

fn ripr(args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn describe(output: &Output) -> String {
    format!(
        "exit {:?}; stderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// `ripr agent status --json` for `root`, parsed.
fn status(root: &Path) -> Result<Value, String> {
    let root_arg = root.display().to_string();
    let output = ripr(&["agent", "status", "--root", &root_arg, "--json"])?;
    if !output.status.success() {
        return Err(format!("ripr agent status {}", describe(&output)));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "agent status JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Run the repair before phase and return the attempt it recorded.
fn repair_before(root: &Path) -> Result<String, String> {
    let root_arg = root.display().to_string();
    let output = ripr(&[
        "agent",
        "repair",
        "--json",
        "--root",
        &root_arg,
        "--seam-id",
        SEAM,
        "--phase",
        "before",
    ])?;
    if !output.status.success() {
        return Err(format!(
            "ripr agent repair --phase before {}",
            describe(&output)
        ));
    }
    let dir = root.join("target/ripr/repair-attempts");
    let mut ids = std::fs::read_dir(&dir)
        .map_err(|error| format!("read {}: {error}", dir.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    // The store also holds its own lock file next to the attempt directories.
    ids.retain(|id| id.starts_with("repair-attempt-"));
    match (ids.pop(), ids.is_empty()) {
        (Some(id), true) => Ok(id),
        (first, _) => Err(format!(
            "expected exactly one attempt under {}, found {first:?} and {ids:?}; before phase {}",
            dir.display(),
            describe(&output)
        )),
    }
}

/// Write the boundary test and run the after phase; `Ok(true)` on success.
fn repair_after(root: &Path, attempt: &str) -> Result<bool, String> {
    write(
        root,
        "tests/pricing.rs",
        &format!("{WEAK_TEST}{BOUNDARY_TEST}"),
    )?;
    let root_arg = root.display().to_string();
    let output = ripr(&[
        "agent",
        "repair",
        "--json",
        "--root",
        &root_arg,
        "--attempt",
        attempt,
        "--phase",
        "after",
    ])?;
    Ok(output.status.success())
}

/// The status documents of one loop, labelled by stage.
fn loop_documents() -> Result<Vec<(&'static str, Value)>, String> {
    let finishing = fixture_repo("finishing")?;
    let failing = fixture_repo("failing")?;
    let documents = (|| {
        let fresh = status(&finishing)?;
        let attempt = repair_before(&finishing)?;
        let awaiting = status(&finishing)?;
        if !repair_after(&finishing, &attempt)? {
            return Err("the after phase refused the boundary test".to_string());
        }
        let finished = status(&finishing)?;

        let attempt = repair_before(&failing)?;
        write(&failing, "stray.txt", "outside the edit cage\n")?;
        if repair_after(&failing, &attempt)? {
            return Err("a stray file must violate the edit cage".to_string());
        }
        let failed = status(&failing)?;
        Ok(vec![
            ("fresh", fresh),
            ("awaiting", awaiting),
            ("finished", finished),
            ("failed", failed),
        ])
    })();
    // A failing run keeps both fixtures (their names are unique per run) so
    // the attempt store and artifacts can be inspected.
    if documents.is_ok() {
        let _ = std::fs::remove_dir_all(&finishing);
        let _ = std::fs::remove_dir_all(&failing);
    }
    documents
}

fn conforms(document: &Value, schema: &Value, label: &str) -> Result<(), String> {
    published_schema::check(document, schema, schema, &format!("agent_status[{label}]"))
}

#[test]
fn the_checker_covers_every_keyword_the_agent_status_schema_uses() -> Result<(), String> {
    let schema = published_schema::read(AGENT_STATUS_SCHEMA)?;
    let ignored = published_schema::unsupported_keywords(&schema);
    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the conformance checker ignores these keywords of {AGENT_STATUS_SCHEMA}, so it would under-check the schema: {ignored:?}"
        ))
    }
}

/// Every stage conforms, and each stage is the one it claims to be, so a
/// branch of the schema is never "covered" by a document that skipped it.
/// Artifacts come in the reported order, and `required` is `false` exactly
/// while an attempt is recorded.
#[test]
fn agent_status_json_conforms_at_every_stage_of_a_repair_loop() -> Result<(), String> {
    let schema = published_schema::read(AGENT_STATUS_SCHEMA)?;
    for (label, document) in loop_documents()? {
        conforms(&document, &schema, label)?;
        let attempts = document["repair_attempts"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let names = document["artifacts"]
            .as_array()
            .map(|artifacts| {
                artifacts
                    .iter()
                    .map(|artifact| artifact["name"].clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // A recorded attempt supersedes every repository-global copy
        // (`REPAIR_ATTEMPT_SUPERSEDED_ARTIFACTS` lists all seven).
        let required_matches = document["artifacts"].as_array().is_some_and(|artifacts| {
            artifacts
                .iter()
                .all(|artifact| artifact["required"] == json!(attempts.is_empty()))
        });
        let first = attempts.first().cloned().unwrap_or(Value::Null);
        let stage = match label {
            "fresh" => json!([
                document["status"] == "incomplete",
                attempts.is_empty(),
                document["next_command"]["step"] == "select_seam",
            ]),
            "awaiting" => json!([
                document["seam"]["seam_id"] == SEAM,
                first["state"] == "awaiting_edit",
                first["disposition"] == "resumable",
            ]),
            "finished" => json!([
                first["state"] == "ready_to_finish",
                first["receipt"]["issued_for_attempt"] == true,
                document["test_run"]["attempt_id"] == first["attempt_id"],
            ]),
            _ => json!([
                first["state"] == "failed",
                first["disposition"] == "ended",
                document["warnings"]
                    .as_array()
                    .is_some_and(|warnings| !warnings.is_empty()),
            ]),
        };
        let observed = json!([names == ARTIFACT_NAMES, required_matches, stage]);
        if observed != json!([true, true, [true, true, true]]) {
            return Err(format!("{label}: {observed}\n{document:#}"));
        }
    }
    Ok(())
}

/// The negative experiment: the schema rejects a real document once one
/// contract term is broken.
#[test]
fn published_agent_status_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut Value);
    let schema = published_schema::read(AGENT_STATUS_SCHEMA)?;
    let documents = loop_documents()?;
    let finished = documents
        .iter()
        .find(|(label, _)| *label == "finished")
        .map(|(_, document)| document.clone())
        .ok_or("no finished document")?;
    conforms(&finished, &schema, "finished")?;
    let cases: [(&str, Mutation); 17] = [
        ("unknown top-level field", |doc| doc["ok"] = json!(true)),
        ("another version", |doc| {
            doc["schema_version"] = json!("0.2")
        }),
        ("another tool", |doc| doc["tool"] = json!("other")),
        ("an unknown status", |doc| doc["status"] = json!("done")),
        ("complete with a next command", |doc| {
            doc["status"] = json!("complete");
            doc["next_command"] = json!({
                "step": "s", "artifact": "a", "reason": "r", "command": "ripr x"
            });
        }),
        ("warning with no warnings", |doc| {
            doc["status"] = json!("warning");
            doc["warnings"] = json!([]);
        }),
        ("six artifacts", |doc| {
            if let Some(artifacts) = doc["artifacts"].as_array_mut() {
                artifacts.pop();
            }
        }),
        ("an artifact path for another name", |doc| {
            doc["artifacts"][0]["path"] = json!("target/ripr/workflow/after.repo-exposure.json")
        }),
        ("the first artifact repeated seven times", |doc| {
            let first = doc["artifacts"][0].clone();
            doc["artifacts"] = json!([
                first.clone(),
                first.clone(),
                first.clone(),
                first.clone(),
                first.clone(),
                first.clone(),
                first
            ]);
        }),
        ("the artifacts in reverse order", |doc| {
            if let Some(artifacts) = doc["artifacts"].as_array_mut() {
                artifacts.reverse();
            }
        }),
        ("a missing artifact with a size", |doc| {
            doc["artifacts"][0]["state"] = json!("missing")
        }),
        ("an unknown attempt state", |doc| {
            doc["repair_attempts"][0]["state"] = json!("done")
        }),
        ("an unknown disposition", |doc| {
            doc["repair_attempts"][0]["disposition"] = json!("closed")
        }),
        ("an unissued receipt that shows the gap closed", |doc| {
            doc["repair_attempts"][0]["receipt"]["issued_for_attempt"] = json!(false)
        }),
        ("a receipt on an attempt awaiting its edit", |doc| {
            doc["repair_attempts"][0]["state"] = json!("awaiting_edit")
        }),
        ("an unknown seam source", |doc| {
            doc["seam"]["source"] = json!("agent_guess")
        }),
        ("an unknown test-run status", |doc| {
            doc["test_run"]["status"] = json!("passed")
        }),
    ];
    for (label, mutate) in cases {
        let mut document = finished.clone();
        mutate(&mut document);
        if document == finished {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&document, &schema, label) {
            Ok(()) => {
                return Err(format!(
                    "the published agent status schema accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => println!("rejected as expected ({label}): {error}"),
        }
    }
    Ok(())
}
