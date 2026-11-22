//! The published `ripr context` packet schema, checked against bytes the real
//! command wrote.
//!
//! `schemas/ripr/context.schema.json` describes the one-finding packet
//! `ripr context --at <finding-id>` writes (exit 0). The packets come from
//! every finding of eight committed fixtures that together reach every
//! exposure class, both source-currentness states the fixtures produce, a
//! `null` witness and an exposed finding that keeps one, preview-language
//! confidence caps and each missing-discriminator shape, plus a git fixture
//! whose worktree edit reaches `probe.owner_signature` and
//! `witness.before_condition`. The refusal envelope has its own schema,
//! checked in `refusal_envelope_schemas.rs`.

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;
#[path = "common/published_schema.rs"]
mod published_schema;

use common::fixture_git::fixture_git_ok;

const CONTEXT_SCHEMA: &str = "schemas/ripr/context.schema.json";
const CHECK_SCHEMA: &str = "schemas/ripr/check.schema.json";

/// Committed fixtures whose findings together reach every variant the
/// coverage test names, chosen from a survey of every fixture's packets.
const COVERING_FIXTURES: [&str; 8] = [
    "python_ranking_noise_control",
    "rust_constant_tail_change",
    "rust_path_keyword_affinity",
    "typescript_oracle_helper_gated",
    "error_variant_wrapper_wrong_receiver_pin",
    "returned_closure_arithmetic_masking",
    "guarded_result_match_positive",
    "collection_argument_guarded",
];

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The one spawn of this file: `ripr` with `args`.
fn ripr(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

/// Stdout of a `ripr` run that must exit 0, parsed as JSON.
fn ripr_json(args: &[&str]) -> Result<Value, String> {
    let output = ripr(args)?;
    if output.status.code() != Some(0) {
        return Err(format!(
            "ripr {args:?} exited {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "ripr {args:?} JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// The ids of every finding `ripr check --json` reports for `scope`.
fn finding_ids(scope: &[&str]) -> Result<Vec<String>, String> {
    let mut args = vec!["check", "--json"];
    args.extend_from_slice(scope);
    let report = ripr_json(&args)?;
    let findings = report["findings"]
        .as_array()
        .ok_or_else(|| format!("ripr {args:?} wrote no findings array"))?;
    findings
        .iter()
        .map(|finding| {
            finding["id"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("ripr {args:?}: a finding has no id"))
        })
        .collect()
}

/// The context packet of every finding in `scope`, labelled by fixture and id.
fn packets_for(label: &str, scope: &[&str]) -> Result<Vec<(String, Value)>, String> {
    let ids = finding_ids(scope)?;
    if ids.is_empty() {
        return Err(format!("{label}: the check run reported no finding"));
    }
    let mut packets = Vec::new();
    for id in ids {
        let mut args = vec!["context", "--json", "--at", id.as_str()];
        args.extend_from_slice(scope);
        packets.push((format!("{label} {id}"), ripr_json(&args)?));
    }
    Ok(packets)
}

fn fixture_packets() -> Result<Vec<(String, Value)>, String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut packets = Vec::new();
    for name in COVERING_FIXTURES {
        let root = fixtures.join(name).join("input").display().to_string();
        let diff = fixtures.join(name).join("diff.patch").display().to_string();
        packets.extend(packets_for(
            name,
            &["--root", root.as_str(), "--diff", diff.as_str()],
        )?);
    }
    Ok(packets)
}

fn write(root: &Path, path: &str, text: &str) -> Result<(), String> {
    std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
}

const BASE_LIB: &str = "pub fn over(amount: i32, threshold: i32) -> bool {\n    amount > threshold\n}\n\npub fn price(total: i32) -> i32 {\n    if total >= 100 { total - 10 } else { total }\n}\n";
const CHANGED_LIB: &str = "pub fn over(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}\n\npub fn price(total: i32) -> i32 {\n    if total > 200 { total - 25 } else { total }\n}\n";

/// A committed Rust package with an uncommitted edit: a moved boundary in a
/// free function (named by `owner_signature`) and a guard whose line also
/// changed outside its condition (paired by `before_condition`).
fn git_fixture() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-context-schema-{}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|error| format!("create {dir}: {error}"))?;
    }
    fixture_git_ok(&root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(&root, &["config", "user.email", "context-schema@t.st"])?;
    fixture_git_ok(&root, &["config", "user.name", "context-schema"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"context_schema_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(&root, "src/lib.rs", BASE_LIB)?;
    write(
        &root,
        "tests/lib.rs",
        "use context_schema_fixture::{over, price};\n#[test]\nfn big() {\n    assert!(over(10, 1));\n}\n#[test]\nfn priced() {\n    assert_eq!(price(500), 475);\n}\n",
    )?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    write(&root, "src/lib.rs", CHANGED_LIB)?;
    Ok(root)
}

/// The git fixture's packets, each also checked to be written byte-for-byte
/// minified under `--format json-compact`.
fn git_packets() -> Result<Vec<(String, Value)>, String> {
    let root = git_fixture()?;
    let root_arg = root.display().to_string();
    let produced = (|| {
        let scope = ["--root", root_arg.as_str()];
        let packets = packets_for("git fixture", &scope)?;
        for (label, packet) in &packets {
            let id = packet["probe"]["id"].as_str().unwrap_or_default();
            let output = ripr(&[
                "context",
                "--format",
                "json-compact",
                "--at",
                id,
                "--root",
                root_arg.as_str(),
            ])?;
            let compact: Value = serde_json::from_slice(&output.stdout)
                .map_err(|error| format!("{label}: json-compact did not parse: {error}"))?;
            if output.status.code() != Some(0)
                || output.stdout.trim_ascii_end().contains(&b'\n')
                || compact != *packet
            {
                return Err(format!(
                    "{label}: --format json-compact must write the --json packet without whitespace"
                ));
            }
        }
        Ok::<_, String>(packets)
    })();
    // A failed run keeps the fixture for inspection.
    if produced.is_ok() {
        let _ = std::fs::remove_dir_all(&root);
    }
    produced
}

fn conforms(document: &Value, schema: &Value, label: &str) -> Result<(), String> {
    published_schema::check(document, schema, schema, label)
}

/// The variants a packet reaches, named for the coverage check.
fn variants(packet: &Value) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for key in ["classification", "source_currentness", "severity"] {
        if let Some(value) = packet[key].as_str() {
            found.insert(format!("{key}={value}"));
        }
    }
    for key in ["confidence_basis", "canonical_gap_id"] {
        if packet.get(key).is_some() {
            found.insert(key.to_string());
        }
    }
    if packet["probe"].get("owner_signature").is_some() {
        found.insert("owner_signature".to_string());
    }
    let witness = &packet["witness"];
    if witness.is_null() {
        found.insert("witness=null".to_string());
        return found;
    }
    if packet["classification"] == "exposed" {
        found.insert("exposed witness".to_string());
    }
    for key in ["before_condition", "fix_site", "limitations"] {
        if witness.get(key).is_some() {
            found.insert(format!("witness.{key}"));
        }
    }
    if witness.get("fix_site").is_none() {
        found.insert("no fix_site".to_string());
    }
    for discriminator in witness["missing_discriminators"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(shape) = discriminator["shape"].as_str() {
            found.insert(format!("shape={shape}"));
        }
        if discriminator["flow_sink"].is_null() {
            found.insert("flow_sink=null".to_string());
        }
    }
    found
}

/// Every packet of every covering fixture and of the git fixture conforms to
/// the published schema, and together they reach each named variant, so a
/// schema branch the corpus never exercises shows up as a coverage failure
/// rather than as silent acceptance.
#[test]
fn context_json_conforms_for_every_finding_of_the_covering_fixtures() -> Result<(), String> {
    let schema = published_schema::read(CONTEXT_SCHEMA)?;
    let mut packets = fixture_packets()?;
    packets.extend(git_packets()?);
    let mut reached = BTreeSet::new();
    for (label, packet) in &packets {
        conforms(packet, &schema, label)?;
        reached.extend(variants(packet));
    }
    let mut expected: BTreeSet<String> = [
        "source_currentness=candidate_current",
        "source_currentness=base_deleted",
        "severity=info",
        "severity=note",
        "severity=warning",
        "confidence_basis",
        "canonical_gap_id",
        "owner_signature",
        "witness=null",
        "exposed witness",
        "witness.before_condition",
        "witness.fix_site",
        "witness.limitations",
        "no fix_site",
        "shape=Boundary",
        "shape=GuardDomain",
        "shape=Other",
        "flow_sink=null",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for class in [
        "exposed",
        "weakly_exposed",
        "reachable_unrevealed",
        "no_static_path",
        "infection_unknown",
        "propagation_unknown",
        "static_unknown",
    ] {
        expected.insert(format!("classification={class}"));
    }
    let missed: Vec<&String> = expected.difference(&reached).collect();
    if missed.is_empty() {
        println!("{} packets conform", packets.len());
        Ok(())
    } else {
        Err(format!(
            "the corpus no longer reaches these variants: {missed:?}"
        ))
    }
}

#[test]
fn the_checker_covers_every_keyword_the_context_schema_uses() -> Result<(), String> {
    let schema = published_schema::read(CONTEXT_SCHEMA)?;
    let ignored = published_schema::unsupported_keywords(&schema);
    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the conformance checker ignores these keywords of {CONTEXT_SCHEMA}, so it would under-check the schema: {ignored:?}"
        ))
    }
}

/// `confidence_basis` is wire-identical to the check finding's field, so the
/// two schemas hold the same definition.
#[test]
fn confidence_basis_is_the_check_finding_definition() -> Result<(), String> {
    let context = published_schema::read(CONTEXT_SCHEMA)?;
    let check = published_schema::read(CHECK_SCHEMA)?;
    let ours = &context["properties"]["confidence_basis"];
    let theirs = &check["$defs"]["finding"]["properties"]["confidence_basis"];
    if ours == theirs && !ours.is_null() {
        Ok(())
    } else {
        Err(format!(
            "{CONTEXT_SCHEMA} confidence_basis differs from the check finding's: {ours} vs {theirs}"
        ))
    }
}

/// The negative experiment: the schema rejects a real packet once one
/// contract term is broken.
#[test]
fn published_context_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut Value);
    let schema = published_schema::read(CONTEXT_SCHEMA)?;
    let packets = fixture_packets()?;
    let with_witness = packets
        .iter()
        .find(|(_, packet)| {
            packet["witness"]["missing_discriminators"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
                && packet["related_tests"]
                    .as_array()
                    .is_some_and(|items| !items.is_empty())
        })
        .map(|(_, packet)| packet.clone())
        .ok_or("no packet carries a witness discriminator and a related test")?;
    conforms(&with_witness, &schema, "base packet")?;
    let mutations: [(&str, Mutation); 16] = [
        ("an undeclared field", |doc| doc["status"] = json!("ok")),
        ("the check report's schema_version key", |doc| {
            doc["schema_version"] = json!("1.1")
        }),
        ("another version", |doc| doc["version"] = json!("1.0")),
        ("an unknown classification", |doc| {
            doc["classification"] = json!("covered")
        }),
        ("a severity a finding cannot carry", |doc| {
            doc["severity"] = json!("off")
        }),
        ("a confidence above one", |doc| {
            doc["confidence"] = json!(1.5)
        }),
        ("a capped confidence with no why line", |doc| {
            doc["confidence_basis"] = json!([{"code": "c", "summary": "s"}]);
            if let Some(object) = doc.as_object_mut() {
                object.remove("confidence_why_capped");
            }
        }),
        ("an unknown stage state", |doc| {
            doc["ripr"]["reach"] = json!("maybe")
        }),
        ("a probe on line 0", |doc| doc["probe"]["line"] = json!(0)),
        ("a null witness on a finding that is not exposed", |doc| {
            doc["classification"] = json!("weakly_exposed");
            doc["witness"] = Value::Null
        }),
        ("a why with no miss", |doc| {
            doc["related_tests"][0]["why"] = json!("no input reaches it");
            if let Some(object) = doc["related_tests"][0].as_object_mut() {
                object.remove("miss");
            }
        }),
        ("an unknown relation confidence", |doc| {
            doc["related_tests"][0]["relation_confidence"] = json!("certain")
        }),
        (
            "a top-level discriminator with the witness's flow sink",
            |doc| {
                doc["missing_discriminators"] =
                    json!([{"value": "a == b", "reason": "r", "flow_sink": null}])
            },
        ),
        ("an unknown discriminator shape", |doc| {
            doc["witness"]["missing_discriminators"][0]["shape"] = json!("boundary")
        }),
        ("an empty witness limitation list", |doc| {
            doc["witness"]["limitations"] = json!([])
        }),
        ("a witness with no explain command", |doc| {
            if let Some(object) = doc["witness"].as_object_mut() {
                object.remove("explain_command");
            }
        }),
    ];
    for (label, mutate) in mutations {
        let mut document = with_witness.clone();
        mutate(&mut document);
        if document == with_witness {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&document, &schema, label) {
            Ok(()) => {
                return Err(format!(
                    "{CONTEXT_SCHEMA} accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => println!("rejected as expected ({label}): {error}"),
        }
    }
    Ok(())
}
