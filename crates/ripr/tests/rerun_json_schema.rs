//! The published `ripr rerun --json` report schema, checked against bytes the
//! real command wrote.
//!
//! `schemas/ripr/rerun.schema.json` describes the targeted rerun report. The
//! documents here come from one fixture package run through every branch the
//! schema names: both selectors, `--before` movement (unchanged, improved,
//! regressed, closed, indeterminate, and an unusable before artifact),
//! `--check-parity` (matched and capped), a gap ledger whose records conflict, a ledger carrying the typed
//! command specs `ripr agent packet` produced, and selectors that resolve to
//! nothing. Each document is also asserted to be the case it claims to be, so
//! a schema branch is never "covered" by a run that skipped it.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;
#[path = "common/published_schema.rs"]
mod published_schema;

use common::fixture_git::fixture_git_ok;

const RERUN_SCHEMA: &str = "schemas/ripr/rerun.schema.json";

/// The boundary-gap seam the fixture's `>=` comparison produces.
const SEAM: &str = "67fc764ba37d77bd";

const WEAK_TEST: &str = "use boundary_gap_fixture::discounted_total;\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n";

const BOUNDARY_TEST: &str = "use boundary_gap_fixture::discounted_total;\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n    assert_eq!(discounted_total(100, 100), 90);\n    assert_eq!(discounted_total(99, 100), 99);\n}\n";

const DEFAULT_BOUNDARY: &str =
    "static evidence only; no before snapshot was supplied, so gap movement is not inferred";
const MOVEMENT_BOUNDARY: &str = "static before/after evidence only; movement does not establish runtime mutation behavior, correctness, coverage adequacy, or complete test quality";

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ripr-rerun-schema-{label}-{}-{stamp}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

/// A committed one-function package whose test misses the `>=` boundary.
fn fixture_repo(root: &Path) -> Result<(), String> {
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|error| format!("create {dir}: {error}"))?;
    }
    fixture_git_ok(root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(root, &["config", "user.email", "rerun-schema@t.st"])?;
    fixture_git_ok(root, &["config", "user.name", "rerun-schema"])?;
    fixture_git_ok(root, &["config", "core.autocrlf", "false"])?;
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"boundary_gap_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"boundary_gap_fixture\"\npath = \"src/lib.rs\"\n",
    )?;
    write(&root.join(".gitignore"), "/target/\n")?;
    write(
        &root.join("src/lib.rs"),
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
    )?;
    write(&root.join("tests/pricing.rs"), WEAK_TEST)?;
    fixture_git_ok(root, &["add", "-A"])?;
    fixture_git_ok(root, &["commit", "-q", "-m", "base"])?;
    Ok(())
}

fn ripr(args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    // An ambient seam cap would turn every parity run into the capped case.
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .env_remove("RIPR_REPO_EXPOSURE_SEAM_LIMIT")
        .envs(env.iter().copied())
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

/// Run a command that must succeed and print one JSON document.
fn json_of(args: &[&str], env: &[(&str, &str)]) -> Result<(Value, Vec<u8>), String> {
    let output = ripr(args, env)?;
    if !output.status.success() {
        return Err(format!(
            "ripr {args:?}: exit {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "ripr {args:?} JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    Ok((value, output.stdout))
}

/// The first value named `key` anywhere in `value`, depth first.
fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|nested| find_key(nested, key))),
        Value::Array(items) => items.iter().find_map(|nested| find_key(nested, key)),
        _ => None,
    }
}

fn rerun_documents() -> Result<Vec<(&'static str, Value)>, String> {
    let root = unique("root");
    let side = unique("side");
    let documents = (|| {
        fixture_repo(&root)?;
        std::fs::create_dir_all(&side).map_err(|error| format!("create side dir: {error}"))?;
        let root_arg = root.display().to_string();
        let changed = |extra: &[&str], env: &[(&str, &str)]| {
            let mut args = vec![
                "rerun",
                "--root",
                root_arg.as_str(),
                "--changed-test",
                "tests/pricing.rs",
                "--json",
            ];
            args.extend_from_slice(extra);
            json_of(&args, env)
        };
        let keep = |name: &str, bytes: &[u8]| -> Result<String, String> {
            let path = side.join(name);
            std::fs::write(&path, bytes).map_err(|error| format!("write {name}: {error}"))?;
            Ok(path.display().to_string())
        };

        let (current, current_bytes) = changed(&[], &[])?;
        let current_path = keep("current.json", &current_bytes)?;
        let (named_test, _) = json_of(
            &[
                "rerun",
                "--root",
                &root_arg,
                "--changed-test",
                "tests/pricing.rs::far_above_threshold_discounts",
                "--json",
            ],
            &[],
        )?;
        let (unchanged, _) = changed(&["--before", &current_path], &[])?;
        let (parity_matched, _) = changed(&["--check-parity"], &[])?;
        let (parity_capped, _) = changed(
            &["--check-parity"],
            &[("RIPR_REPO_EXPOSURE_SEAM_LIMIT", "1")],
        )?;
        let missing = side.join("missing.json").display().to_string();
        let (before_unavailable, _) = changed(&["--before", &missing], &[])?;
        let incompatible = keep("incompatible.json", br#"{"schema_version":"other"}"#)?;
        let (before_incompatible, _) = changed(&["--before", &incompatible], &[])?;
        let (selector_unresolved, _) = json_of(
            &[
                "rerun",
                "--root",
                &root_arg,
                "--changed-test",
                "tests/absent.rs",
                "--json",
            ],
            &[],
        )?;

        let gap = current["seams"]
            .as_array()
            .and_then(|seams| seams.iter().find(|seam| seam["seam_id"] == SEAM))
            .and_then(|seam| seam["canonical_gap_id"].as_str())
            .ok_or("the boundary seam has no canonical gap id")?
            .to_string();
        let (packet, _) = json_of(
            &[
                "agent",
                "packet",
                "--root",
                &root_arg,
                "--seam-id",
                SEAM,
                "--json",
            ],
            &[],
        )?;
        let specs = find_key(&packet, "command_specs")
            .cloned()
            .ok_or("the agent packet carries no command specs")?;
        let anchored = json!({"file": "src/lib.rs", "owner": "discounted_total"});
        let conflict_ledger = keep(
            "conflict-ledger.json",
            json!({
                "kind": "gap_decision_ledger",
                "root": root_arg,
                "records": [
                    {"canonical_gap_id": gap, "anchor": anchored,
                     "verification_commands": ["cargo test far", ""],
                     "receipt_command": "ripr receipt write --gap a"},
                    {"canonical_gap_id": gap,
                     "verification_commands": ["cargo test far"],
                     "receipt_command": "ripr receipt write --gap b"}
                ]
            })
            .to_string()
            .as_bytes(),
        )?;
        let typed_ledger = keep(
            "typed-ledger.json",
            json!({
                "kind": "gap_decision_ledger",
                "root": root_arg,
                "records": [{
                    "canonical_gap_id": gap, "anchor": anchored,
                    "verification_commands": ["cargo test far"],
                    "receipt_command": "ripr agent receipt --json",
                    "command_specs": {"verify": specs["verify"], "receipt": specs["receipt"]}
                }]
            })
            .to_string()
            .as_bytes(),
        )?;
        let gap_run = |gap: &str, ledger: &str| {
            json_of(
                &[
                    "rerun",
                    "--root",
                    &root_arg,
                    "--gap",
                    gap,
                    "--gap-ledger",
                    ledger,
                    "--json",
                ],
                &[],
            )
        };
        let (gap_conflict, _) = gap_run(&gap, &conflict_ledger)?;
        let (gap_typed, _) = gap_run(&gap, &typed_ledger)?;
        let (gap_unresolved, _) = gap_run("gap:absent", &conflict_ledger)?;

        // Pin the boundary, then drop the pin again: improved, then regressed.
        write(&root.join("tests/pricing.rs"), BOUNDARY_TEST)?;
        let (improved, improved_bytes) = changed(&["--before", &current_path], &[])?;
        let improved_path = keep("improved.json", &improved_bytes)?;
        // With every seam now strongly gripped, a before artifact that lists
        // the same seams as weakly gripped closes them all; one that lists
        // them as `intentional` (the same rank, another class) moves them
        // without an order, which is `movement_indeterminate`. Only the
        // before stimulus is derived; the reports are the command's bytes.
        let restated = |class: &str| -> Result<Vec<u8>, String> {
            let mut before = improved.clone();
            for seam in before["seams"].as_array_mut().into_iter().flatten() {
                seam["static_class"] = json!(class);
            }
            serde_json::to_vec(&before).map_err(|error| format!("encode before: {error}"))
        };
        let weak_path = keep("all-weak.json", &restated("weakly_gripped")?)?;
        let (closed, _) = changed(&["--before", &weak_path], &[])?;
        let intentional_path = keep("all-intentional.json", &restated("intentional")?)?;
        let (indeterminate, _) = changed(&["--before", &intentional_path], &[])?;
        write(&root.join("tests/pricing.rs"), WEAK_TEST)?;
        let (regressed, _) = changed(&["--before", &improved_path], &[])?;

        let mut documents = vec![
            ("current", current),
            ("named_test", named_test),
            ("unchanged", unchanged),
            ("improved", improved),
            ("regressed", regressed),
            ("closed", closed),
            ("indeterminate", indeterminate),
            ("parity_matched", parity_matched),
            ("parity_capped", parity_capped),
            ("before_unavailable", before_unavailable),
            ("before_incompatible", before_incompatible),
            ("selector_unresolved", selector_unresolved),
            ("gap_conflict", gap_conflict),
            ("gap_typed", gap_typed),
            ("gap_unresolved", gap_unresolved),
        ];
        #[cfg(feature = "lang-typescript")]
        documents.push(("typescript_gap", typescript_gap_rerun(&side)?));
        Ok(documents)
    })();
    // A failing run keeps the fixture and the before artifacts (their names
    // are unique per run) so they can be inspected.
    if documents.is_ok() {
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&side);
    }
    documents
}

/// A gap rerun over the TypeScript preview fixture: its related test has no
/// oracle text and no relation, which the producer writes as empty strings.
/// The ledger names the finding by the evidence id the fixture's golden
/// `check` report records, so the selected seam is the one the golden pins.
#[cfg(feature = "lang-typescript")]
fn typescript_gap_rerun(side: &Path) -> Result<Value, String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typescript_effect_probe_shapes");
    let root = side.join("typescript");
    for path in ["ripr.toml", "src/profile.ts", "tests/profile.test.ts"] {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::copy(fixture.join("input").join(path), &target)
            .map_err(|error| format!("copy {path}: {error}"))?;
    }
    let golden = std::fs::read_to_string(fixture.join("expected/check.json"))
        .map_err(|error| format!("read the TypeScript golden: {error}"))?;
    let golden: Value = serde_json::from_str(&golden)
        .map_err(|error| format!("parse the TypeScript golden: {error}"))?;
    let finding = golden["findings"][0]["id"]
        .as_str()
        .ok_or("the TypeScript golden has no finding id")?
        .to_string();
    let root_arg = root.display().to_string();
    let ledger = side.join("typescript-ledger.json");
    write(
        &ledger,
        &json!({
            "kind": "gap_decision_ledger",
            "root": root_arg,
            "records": [{
                "canonical_gap_id": "gap:typescript",
                "evidence_ids": [finding],
                "anchor": {"file": "src/profile.ts", "owner": "updateProfile", "line": 4},
                "verification_commands": ["npm test", ""]
            }]
        })
        .to_string(),
    )?;
    let ledger_arg = ledger.display().to_string();
    json_of(
        &[
            "rerun",
            "--root",
            &root_arg,
            "--gap",
            "gap:typescript",
            "--gap-ledger",
            &ledger_arg,
            "--json",
        ],
        &[],
    )
    .map(|(value, _)| value)
}

fn conforms(document: &Value, schema: &Value, label: &str) -> Result<(), String> {
    published_schema::check(document, schema, schema, &format!("rerun[{label}]"))
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys = value
        .as_object()
        .map(|map| map.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

/// What each run must be, so a branch of the schema is only counted as
/// covered by a document that really took it: the report state, the
/// limitation kind, the movement and parity states, and the route fields.
fn observed(document: &Value) -> Value {
    json!({
        "selector": document["selector"]["kind"],
        "state": document["state"],
        "limitation": document["limitation"]["kind"],
        "movement": document["movement"]["state"],
        "parity": document["parity"]["state"],
        "route": sorted_keys(&document["route"]),
        "scope_limitations": document["scope_limitations"]
            .as_array()
            .map(|rows| rows.iter().map(|row| row["kind"].clone()).collect::<Vec<_>>()),
    })
}

fn expected(label: &str) -> Value {
    let changed = |state: &str, limitation: Value, movement: Value, parity: Value| {
        json!({
            "selector": "changed_test", "state": state, "limitation": limitation,
            "movement": movement, "parity": parity, "route": [], "scope_limitations": null,
        })
    };
    match label {
        "current" | "named_test" => {
            changed("current_state_only", Value::Null, Value::Null, Value::Null)
        }
        "indeterminate" => changed(
            "limited",
            json!("movement_indeterminate"),
            json!("limited"),
            Value::Null,
        ),
        "unchanged" | "improved" | "regressed" | "closed" => {
            changed(label, Value::Null, json!(label), Value::Null)
        }
        "parity_matched" => changed(
            "current_state_only",
            Value::Null,
            Value::Null,
            json!("matched"),
        ),
        "parity_capped" => changed(
            "limited",
            json!("full_pipeline_parity_incomplete"),
            Value::Null,
            json!("limited"),
        ),
        "before_unavailable" => changed(
            "limited",
            json!("before_artifact_unavailable"),
            Value::Null,
            Value::Null,
        ),
        "before_incompatible" => changed(
            "limited",
            json!("before_artifact_incompatible"),
            Value::Null,
            Value::Null,
        ),
        "selector_unresolved" => changed(
            "limited",
            json!("changed_test_unresolved"),
            Value::Null,
            Value::Null,
        ),
        "gap_conflict" => json!({
            "selector": "canonical_gap", "state": "current_state_only", "limitation": null,
            "movement": null, "parity": null,
            "route": ["receipt_command", "receipt_command_conflict", "verify_commands"],
            "scope_limitations": ["gap_anchor_unresolved"],
        }),
        "typescript_gap" => json!({
            "selector": "canonical_gap", "state": "current_state_only", "limitation": null,
            "movement": null, "parity": null,
            "route": ["receipt_command", "verify_commands"],
            "scope_limitations": null,
        }),
        "gap_typed" => json!({
            "selector": "canonical_gap", "state": "current_state_only", "limitation": null,
            "movement": null, "parity": null,
            "route": ["receipt_command", "receipt_command_spec", "verify_command_specs", "verify_commands"],
            "scope_limitations": null,
        }),
        _ => json!({
            "selector": "canonical_gap", "state": "limited",
            "limitation": "canonical_gap_unresolved", "movement": null, "parity": null,
            "route": [], "scope_limitations": null,
        }),
    }
}

#[test]
fn the_checker_covers_every_keyword_the_rerun_schema_uses() -> Result<(), String> {
    let schema = published_schema::read(RERUN_SCHEMA)?;
    let ignored = published_schema::unsupported_keywords(&schema);
    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the conformance checker ignores these keywords of {RERUN_SCHEMA}, so it would under-check the schema: {ignored:?}"
        ))
    }
}

/// Every run conforms and is the case it claims to be. The ready boundary
/// seam carries an admitted existing test target, so the readiness branch is
/// read from real output too.
#[test]
fn rerun_json_conforms_for_every_selector_and_option() -> Result<(), String> {
    let schema = published_schema::read(RERUN_SCHEMA)?;
    for (label, document) in rerun_documents()? {
        conforms(&document, &schema, label)?;
        let observed = observed(&document);
        let expected = expected(label);
        if observed != expected {
            return Err(format!(
                "{label}: observed {observed}, expected {expected}\n{document:#}"
            ));
        }
        // The empty strings the producer writes are part of the real
        // contract: a blank legacy verify display survives deduplication,
        // and a TypeScript related test with no oracle or relation is "".
        let empty_verify = document["route"]["verify_commands"]
            .as_array()
            .is_some_and(|commands| commands.iter().any(|command| command == ""));
        let empty_related = document["seams"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|seam| {
                seam["related_tests"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .any(|test| {
                ["evidence_summary", "relation_reason", "relation_confidence"]
                    .iter()
                    .all(|field| test[*field] == "")
            });
        let wanted = match label {
            "gap_conflict" => Some((true, false)),
            "typescript_gap" => Some((true, true)),
            _ => None,
        };
        if let Some(wanted) = wanted
            && (empty_verify, empty_related) != wanted
        {
            return Err(format!(
                "{label}: empty verify display and empty related-test text {:?}, expected {wanted:?}\n{document:#}",
                (empty_verify, empty_related)
            ));
        }
    }
    Ok(())
}

/// The negative experiment: the schema rejects a real document once one
/// contract term is broken.
#[test]
fn published_rerun_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut Value, &Value);
    let schema = published_schema::read(RERUN_SCHEMA)?;
    let documents = rerun_documents()?;
    let document = |label: &str| {
        documents
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, document)| document.clone())
            .ok_or(format!("no {label} document"))
    };
    let conflict_route = document("gap_conflict")?["route"].clone();
    let cases: [(&str, &str, Mutation); 24] = [
        ("current", "an unknown top-level field", |doc, _| {
            doc["ok"] = json!(true)
        }),
        ("current", "another version", |doc, _| {
            doc["schema_version"] = json!("ripr-targeted-rerun-v2")
        }),
        ("current", "an unknown state", |doc, _| {
            doc["state"] = json!("done")
        }),
        ("current", "a limitation on a current report", |doc, _| {
            doc["limitation"] = json!({"kind": "changed_test_unresolved", "message": "m"})
        }),
        (
            "selector_unresolved",
            "a limited report with no limitation",
            |doc, _| {
                if let Some(map) = doc.as_object_mut() {
                    map.remove("limitation");
                }
            },
        ),
        (
            "unchanged",
            "a movement claim on a current-state report",
            |doc, _| doc["state"] = json!("current_state_only"),
        ),
        (
            "improved",
            "a movement state the report does not carry",
            |doc, _| doc["movement"]["state"] = json!("closed"),
        ),
        (
            "improved",
            "movement under the no-movement boundary",
            |doc, _| doc["authority_boundary"] = json!(DEFAULT_BOUNDARY),
        ),
        (
            "unchanged",
            "a limited movement that is not indeterminate",
            |doc, _| {
                doc["state"] = json!("limited");
                doc["movement"]["state"] = json!("limited");
                doc["limitation"] = json!({"kind": "changed_test_unresolved", "message": "m"});
            },
        ),
        (
            "before_unavailable",
            "an unusable before artifact under the movement boundary",
            |doc, _| doc["authority_boundary"] = json!(MOVEMENT_BOUNDARY),
        ),
        (
            "parity_matched",
            "matched parity with an input mismatch",
            |doc, _| doc["parity"]["input_mismatches"] = json!(["lockfile_hash"]),
        ),
        (
            "parity_capped",
            "limited parity on a report that is not limited",
            |doc, _| {
                doc["state"] = json!("current_state_only");
                if let Some(map) = doc.as_object_mut() {
                    map.remove("limitation");
                }
            },
        ),
        ("parity_capped", "an unknown input mismatch", |doc, _| {
            doc["parity"]["input_mismatches"] = json!(["weather"])
        }),
        (
            "current",
            "a route on a changed-test selector",
            |doc, route| doc["route"] = route.clone(),
        ),
        (
            "gap_conflict",
            "a gap selector naming a changed test",
            |doc, _| doc["selector"]["changed_test"] = json!("tests/pricing.rs"),
        ),
        (
            "gap_conflict",
            "a receipt command beside a receipt conflict",
            |doc, _| doc["route"]["receipt_command"] = json!("ripr receipt write --gap a"),
        ),
        (
            "gap_typed",
            "a verify spec in the receipt role",
            |doc, _| doc["route"]["verify_command_specs"][0]["role"] = json!("receipt"),
        ),
        ("gap_conflict", "an unknown scope limitation", |doc, _| {
            doc["scope_limitations"][0]["kind"] = json!("ledger_corrupt")
        }),
        ("current", "an unknown repair route state", |doc, _| {
            doc["seams"][0]["repair_route_readiness"]["state"] = json!("maybe")
        }),
        ("current", "an unknown target selection", |doc, _| {
            doc["seams"][0]["repair_route_readiness"]["target_selection"] = json!({"guessed": {}})
        }),
        ("current", "an unknown cache reuse state", |doc, _| {
            doc["cache"]["reuse_state"] = json!("warm")
        }),
        ("current", "a negative cache hit count", |doc, _| {
            doc["cache"]["hits"] = json!(-1)
        }),
        ("current", "a seam on line 0", |doc, _| {
            doc["seams"][0]["line"] = json!(0)
        }),
        ("current", "an empty static class", |doc, _| {
            doc["seams"][0]["static_class"] = json!("")
        }),
    ];
    for (base, label, mutate) in cases {
        let original = document(base)?;
        conforms(&original, &schema, base)?;
        let mut mutated = original.clone();
        mutate(&mut mutated, &conflict_route);
        if mutated == original {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&mutated, &schema, label) {
            Ok(()) => {
                return Err(format!(
                    "the published rerun schema accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => println!("rejected as expected ({label}): {error}"),
        }
    }
    Ok(())
}
