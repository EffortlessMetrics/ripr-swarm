//! The published refusal-envelope schemas of `ripr rerun --json`,
//! `ripr agent brief --json`, `ripr agent packet --json` and `ripr context`,
//! checked against bytes the real commands wrote.
//!
//! Each command answers a post-argv-parse failure with the check JSON refusal
//! envelope keyed with its own `schema_version` (#7331). The four schemas are
//! `schemas/ripr/diff-refusal.schema.json` with a different version, scope and
//! set of optional fields, and a test holds them to exactly that difference so
//! a change to the shared envelope cannot reach one schema and miss another.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;
#[path = "common/published_schema.rs"]
mod published_schema;

use common::fixture_git::fixture_git_ok;

const DIFF_REFUSAL_SCHEMA: &str = "schemas/ripr/diff-refusal.schema.json";
const RERUN_REFUSAL_SCHEMA: &str = "schemas/ripr/rerun-refusal.schema.json";
const AGENT_BRIEF_REFUSAL_SCHEMA: &str = "schemas/ripr/agent-brief-refusal.schema.json";
const AGENT_PACKET_REFUSAL_SCHEMA: &str = "schemas/ripr/agent-packet-refusal.schema.json";
const CONTEXT_REFUSAL_SCHEMA: &str = "schemas/ripr/context-refusal.schema.json";

/// One schema derived from the diff refusal schema: its pinned version, its
/// `analysis_scope.scope`, whether it may echo `base`, and whether it carries
/// the run's `mode`. Only `context` accepts `--mode`, so only its schema keeps
/// `mode`.
struct Derived {
    path: &'static str,
    version: &'static str,
    scope: &'static str,
    echoes_base: bool,
    has_mode: bool,
}

const DERIVED: [Derived; 4] = [
    Derived {
        path: RERUN_REFUSAL_SCHEMA,
        version: "ripr-targeted-rerun-v1",
        scope: "diff",
        echoes_base: false,
        has_mode: false,
    },
    Derived {
        path: AGENT_BRIEF_REFUSAL_SCHEMA,
        version: "0.2",
        scope: "repo",
        echoes_base: true,
        has_mode: false,
    },
    Derived {
        path: AGENT_PACKET_REFUSAL_SCHEMA,
        version: "0.5",
        scope: "repo",
        echoes_base: false,
        has_mode: false,
    },
    Derived {
        path: CONTEXT_REFUSAL_SCHEMA,
        version: "1.1",
        scope: "diff",
        echoes_base: true,
        has_mode: true,
    },
];

/// A `ripr context` run that echoes `--base`; `--mode` is left at its default.
const CONTEXT_ARGS: &[&str] = &[
    "context",
    "--json",
    "--base",
    "main",
    "--at",
    "src/lib.rs:2",
];

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn write(root: &Path, path: &str, text: &str) -> Result<(), String> {
    std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
}

/// A one-commit Rust package. `broken_config` adds a `ripr.toml` that does
/// not parse.
fn fixture_repo(label: &str, broken_config: bool) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-refusal-schema-{label}-{}-{stamp}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("create src: {error}"))?;
    fixture_git_ok(&root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(&root, &["config", "user.email", "refusal-schema@t.st"])?;
    fixture_git_ok(&root, &["config", "user.name", "refusal-schema"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"refusal_schema_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(
        &root,
        "src/lib.rs",
        "pub fn over(amount: i32, threshold: i32) -> bool {\n    amount > threshold\n}\n",
    )?;
    if broken_config {
        write(&root, "ripr.toml", "[analysis\nmode = \n")?;
    }
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    Ok(root)
}

/// Stdout bytes of a `ripr` run that must exit with `exit`.
fn ripr_stdout(args: &[&str], root: &Path, exit: i32) -> Result<Vec<u8>, String> {
    let root_arg = root.display().to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .args(["--root", &root_arg])
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))?;
    if output.status.code() != Some(exit) {
        return Err(format!(
            "ripr {args:?} should exit {exit}, got {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

/// Stdout of a `ripr` run that must exit with `exit` and write a refusal.
fn refusal_json(args: &[&str], root: &Path, exit: i32) -> Result<Value, String> {
    let stdout = ripr_stdout(args, root, exit)?;
    serde_json::from_slice(&stdout).map_err(|error| {
        format!(
            "ripr {args:?} refusal JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&stdout)
        )
    })
}

/// The refusals one command writes for an unusable root and an unreadable
/// config, each labelled with its expected identity.
fn root_and_config_refusals(
    label: &str,
    args: &[&str],
) -> Result<Vec<(&'static str, Value)>, String> {
    let missing = std::env::temp_dir().join(format!(
        "ripr-refusal-schema-{label}-missing-{}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let unusable_root = refusal_json(args, &missing, 2)?;
    let broken = fixture_repo(label, true)?;
    let bad_config = refusal_json(args, &broken, 2);
    let _ = std::fs::remove_dir_all(&broken);
    Ok(vec![
        ("repository_root_unusable", unusable_root),
        ("config_invalid", bad_config?),
    ])
}

fn conforms(document: &Value, schema: &Value) -> Result<(), String> {
    published_schema::check(document, schema, schema, "refusal")
}

/// Validate each labelled refusal against `path` and require the identity it
/// was produced for.
fn check_refusals(path: &str, refusals: &[(&str, Value)]) -> Result<(), String> {
    let schema = published_schema::read(path)?;
    for (identity, refusal) in refusals {
        conforms(refusal, &schema).map_err(|error| format!("{path}, {identity}: {error}"))?;
        if refusal["analysis_scope"]["run_status"] != *identity {
            return Err(format!(
                "{path}: expected a {identity} refusal, got {}",
                refusal["analysis_scope"]["run_status"]
            ));
        }
    }
    Ok(())
}

#[test]
fn the_checker_covers_every_keyword_the_refusal_schemas_use() -> Result<(), String> {
    for derived in &DERIVED {
        let schema = published_schema::read(derived.path)?;
        let ignored = published_schema::unsupported_keywords(&schema);
        if !ignored.is_empty() {
            return Err(format!(
                "the conformance checker ignores these keywords of {}, so it would under-check the schema: {ignored:?}",
                derived.path
            ));
        }
    }
    Ok(())
}

/// The four schemas copy the diff refusal schema. Apart from the identity
/// text, the pinned version, the scope and the optional fields a command never
/// writes, each must be identical to it, so the envelope's shared definitions
/// (`summary`, `run_limitation`, `refusal_remedy`, themselves held equal to
/// the check schema) cannot drift in one copy.
#[test]
fn refusal_schemas_differ_from_the_diff_refusal_schema_only_where_stated() -> Result<(), String> {
    let diff = published_schema::read(DIFF_REFUSAL_SCHEMA)?;
    for derived in &DERIVED {
        let mut ours = published_schema::read(derived.path)?;
        let mut expected = diff.clone();
        for document in [&mut ours, &mut expected] {
            if let Some(object) = document.as_object_mut() {
                for key in ["$id", "title", "description"] {
                    object.remove(key);
                }
            }
            if let Some(base) = document
                .pointer_mut("/properties/base")
                .and_then(Value::as_object_mut)
            {
                base.remove("description");
            }
        }
        expected["properties"]["schema_version"]["const"] = json!(derived.version);
        expected["properties"]["analysis_scope"]["properties"]["scope"]["const"] =
            json!(derived.scope);
        if let Some(properties) = expected["properties"].as_object_mut() {
            if !derived.has_mode {
                properties.remove("mode");
            }
            if !derived.echoes_base {
                properties.remove("base");
            }
        }
        if ours != expected {
            return Err(format!(
                "{} differs from {DIFF_REFUSAL_SCHEMA} beyond its version, scope and omitted fields; make the change in every refusal schema",
                derived.path
            ));
        }
    }
    Ok(())
}

#[test]
fn rerun_json_refusals_conform_to_the_published_schema() -> Result<(), String> {
    let refusals = root_and_config_refusals(
        "rerun",
        &["rerun", "--json", "--changed-test", "tests/over.rs"],
    )?;
    check_refusals(RERUN_REFUSAL_SCHEMA, &refusals)?;
    let observed: Vec<Value> = refusals
        .iter()
        .map(|(_, refusal)| {
            json!([
                refusal["schema_version"],
                refusal["analysis_scope"]["scope"],
                refusal.get("base"),
                refusal.get("mode"),
                refusal.get("kind"),
            ])
        })
        .collect();
    let expected = json!(["ripr-targeted-rerun-v1", "diff", null, null, null]);
    if observed.iter().all(|row| *row == expected) {
        Ok(())
    } else {
        Err(format!("unexpected rerun refusals: {observed:?}"))
    }
}

/// `agent brief` echoes `--base` when that selected the working set, and
/// omits it for another selector.
#[test]
fn agent_brief_json_refusals_conform_to_the_published_schema() -> Result<(), String> {
    let mut refusals = root_and_config_refusals(
        "agent-brief",
        &["agent", "brief", "--json", "--base", "main"],
    )?;
    let broken = fixture_repo("agent-brief-seam", true)?;
    let by_seam = refusal_json(
        &["agent", "brief", "--json", "--seam-id", "seam:none"],
        &broken,
        2,
    );
    let _ = std::fs::remove_dir_all(&broken);
    refusals.push(("config_invalid", by_seam?));
    check_refusals(AGENT_BRIEF_REFUSAL_SCHEMA, &refusals)?;
    let observed: Vec<Value> = refusals
        .iter()
        .map(|(_, refusal)| {
            json!([
                refusal["schema_version"],
                refusal["analysis_scope"]["scope"],
                refusal.get("base"),
                refusal.get("mode"),
            ])
        })
        .collect();
    let expected = vec![
        json!(["0.2", "repo", "main", null]),
        json!(["0.2", "repo", "main", null]),
        json!(["0.2", "repo", null, null]),
    ];
    if observed == expected {
        Ok(())
    } else {
        Err(format!("unexpected agent brief refusals: {observed:?}"))
    }
}

/// `agent packet` refuses an unknown seam as a decision (exit 3) through the
/// same envelope as its operational refusals (exit 2).
#[test]
fn agent_packet_json_refusals_conform_to_the_published_schema() -> Result<(), String> {
    let args = ["agent", "packet", "--json", "--seam-id", "seam:none"];
    let mut refusals = root_and_config_refusals("agent-packet", &args)?;
    let root = fixture_repo("agent-packet-unknown", false)?;
    let unknown = refusal_json(&args, &root, 3);
    let _ = std::fs::remove_dir_all(&root);
    refusals.push(("seam_id_unknown", unknown?));
    check_refusals(AGENT_PACKET_REFUSAL_SCHEMA, &refusals)?;
    let observed: Vec<Value> = refusals
        .iter()
        .map(|(_, refusal)| {
            json!([
                refusal["schema_version"],
                refusal["analysis_scope"]["scope"],
                refusal.get("base"),
                refusal.get("mode"),
            ])
        })
        .collect();
    if observed
        .iter()
        .all(|row| *row == json!(["0.5", "repo", null, null]))
    {
        Ok(())
    } else {
        Err(format!("unexpected agent packet refusals: {observed:?}"))
    }
}

/// `ripr context` refuses through the same envelope for an unusable root, an
/// unreadable config, an unresolvable base and an unreadable `--from` check
/// artifact, echoing `--mode` always and `--base` when given;
/// `--format json-compact` writes the same document without whitespace.
#[test]
fn context_json_refusals_conform_to_the_published_schema() -> Result<(), String> {
    let mut refusals = root_and_config_refusals("context", CONTEXT_ARGS)?;
    let root = fixture_repo("context-base", false)?;
    let produced = (|| {
        let unresolvable = refusal_json(
            &[
                "context",
                "--json",
                "--mode",
                "deep",
                "--base",
                "nosuchrev",
                "--at",
                "src/lib.rs:2",
            ],
            &root,
            2,
        )?;
        let artifact = refusal_json(
            &[
                "context",
                "--json",
                "--from",
                "missing-check-artifact.json",
                "--at",
                "src/lib.rs:2",
            ],
            &root,
            2,
        )?;
        let compact_args = [
            "context",
            "--format",
            "json-compact",
            "--mode",
            "deep",
            "--base",
            "nosuchrev",
            "--at",
            "src/lib.rs:2",
        ];
        let compact_bytes = ripr_stdout(&compact_args, &root, 2)?;
        let compact: Value = serde_json::from_slice(&compact_bytes)
            .map_err(|error| format!("json-compact refusal did not parse: {error}"))?;
        if compact_bytes.trim_ascii_end().contains(&b'\n') || compact != unresolvable {
            return Err(format!(
                "--format json-compact must write the --json refusal without whitespace:\n{}",
                String::from_utf8_lossy(&compact_bytes)
            ));
        }
        Ok::<_, String>([unresolvable, artifact])
    })();
    let _ = std::fs::remove_dir_all(&root);
    let [unresolvable, artifact] = produced?;
    refusals.push(("base_unresolvable", unresolvable));
    refusals.push(("check_artifact_unusable", artifact));
    check_refusals(CONTEXT_REFUSAL_SCHEMA, &refusals)?;
    let observed: Vec<Value> = refusals
        .iter()
        .map(|(_, refusal)| {
            json!([
                refusal["schema_version"],
                refusal["analysis_scope"]["scope"],
                refusal.get("base"),
                refusal.get("mode"),
            ])
        })
        .collect();
    let expected = vec![
        json!(["1.1", "diff", "main", "draft"]),
        json!(["1.1", "diff", "main", "draft"]),
        json!(["1.1", "diff", "nosuchrev", "deep"]),
        json!(["1.1", "diff", null, "draft"]),
    ];
    if observed == expected {
        Ok(())
    } else {
        Err(format!("unexpected context refusals: {observed:?}"))
    }
}

/// The negative experiment: each schema rejects a real refusal of its own
/// command once one contract term is broken, including another command's
/// version or scope and a field the command never writes.
#[test]
fn published_refusal_schemas_reject_documents_they_forbid() -> Result<(), String> {
    type Mutation = fn(&mut Value);
    let shared: [(&str, Mutation); 9] = [
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
        (
            "a mode that is not a run mode, or on a command without --mode",
            |doc| doc["mode"] = json!("turbo"),
        ),
        ("the diff report's version", |doc| {
            doc["schema_version"] = json!("0.1")
        }),
        (
            "a base that is not a string, or on a command that never echoes one",
            |doc| doc["base"] = json!(7),
        ),
    ];
    let missing = std::env::temp_dir().join(format!(
        "ripr-refusal-schema-negative-missing-{}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let commands: [&[&str]; 4] = [
        &["rerun", "--json", "--changed-test", "tests/over.rs"],
        &["agent", "brief", "--json", "--base", "main"],
        &["agent", "packet", "--json", "--seam-id", "seam:none"],
        CONTEXT_ARGS,
    ];
    let mut rejected = 0;
    for (derived, args) in DERIVED.iter().zip(commands) {
        let schema = published_schema::read(derived.path)?;
        let refusal = refusal_json(args, &missing, 2)?;
        conforms(&refusal, &schema)?;
        let other_scope = if derived.scope == "diff" {
            "repo"
        } else {
            "diff"
        };
        let mut cases: Vec<(String, Value)> = shared
            .iter()
            .map(|(label, mutate)| {
                let mut document = refusal.clone();
                mutate(&mut document);
                ((*label).to_string(), document)
            })
            .collect();
        let mut scoped = refusal.clone();
        scoped["analysis_scope"]["scope"] = json!(other_scope);
        cases.push(("another command's scope".to_string(), scoped));
        for (label, document) in cases {
            if document == refusal {
                return Err(format!(
                    "{}: {label}: the mutation changed nothing",
                    derived.path
                ));
            }
            match conforms(&document, &schema) {
                Ok(()) => {
                    return Err(format!(
                        "{} accepted a document the contract forbids: {label}",
                        derived.path
                    ));
                }
                Err(error) => {
                    rejected += 1;
                    println!("rejected as expected ({}, {label}): {error}", derived.path);
                }
            }
        }
    }
    if rejected == DERIVED.len() * (shared.len() + 1) {
        Ok(())
    } else {
        Err(format!("only {rejected} mutations were rejected"))
    }
}
