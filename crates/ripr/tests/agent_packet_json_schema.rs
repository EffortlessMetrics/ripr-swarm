//! The published `ripr agent packet --seam-id --json` schema, checked against
//! bytes the real command wrote.
//!
//! `schemas/ripr/agent-packet.schema.json` describes the success document of
//! the `--seam-id` route: the agent-seam-packets envelope with the selected
//! seam's packet and its embedded evidence record. One fixture package with a
//! boundary gap, an error-returning parser, an unexercised function, a field
//! write, a side-effect-only test and an inline test module is analyzed, and
//! every seam it produces is packeted. The run is asserted to reach both
//! packet tasks, the boundary proof obligation, concrete and non-concrete
//! assertion guidance, an actionable and a static-limitation evidence record,
//! and the loop `next` block, so no branch of the schema is "covered" by a run
//! that skipped it. Refused seams (policy-omitted strongly gripped ones) write
//! the refusal envelope `agent-packet-refusal.schema.json` covers.
//!
//! `schemas/ripr/agent-seam-packets.schema.json` describes the repository-wide
//! `ripr check --format agent-seam-packets-json` document over the same packet
//! definitions. Its runs cover the many-packet and one-packet documents, the
//! diff-selector projection, the canonical-delta fields, and the format's own
//! refusal shape.

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;
#[path = "common/published_schema.rs"]
mod published_schema;

use common::fixture_git::fixture_git_ok;

const PACKET_SCHEMA: &str = "schemas/ripr/agent-packet.schema.json";
const SEAM_PACKETS_SCHEMA: &str = "schemas/ripr/agent-seam-packets.schema.json";

const LIB: &str = r#"pub fn discounted_total(amount: i32, threshold: i32) -> i32 {
    if amount >= threshold { amount - 10 } else { amount }
}

pub fn label(code: u8) -> &'static str {
    match code {
        0 => "zero",
        1 => "one",
        _ => "many",
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError { Empty, TooLong }

pub fn parse(input: &str) -> Result<usize, ParseError> {
    if input.is_empty() { return Err(ParseError::Empty); }
    if input.len() > 10 { return Err(ParseError::TooLong); }
    Ok(input.len())
}

pub fn unexercised(x: i32) -> i32 { x * 3 + 1 }

pub struct Counter { pub count: u32 }
impl Counter {
    pub fn bump(&mut self) { self.count += 1; }
}

pub fn logged(x: i32) -> i32 {
    println!("{x}");
    x
}

mod inner {
    pub fn private_double(x: i32) -> i32 { x * 2 }
    #[cfg(test)]
    mod tests {
        #[test]
        fn doubles() { assert!(super::private_double(2) > 0); }
    }
}
"#;

const TESTS: &str = r#"use packet_fixture::*;

#[test]
fn far_above() { assert_eq!(discounted_total(10_000, 100), 9_990); }

#[test]
fn labels() {
    assert_eq!(label(0), "zero");
    assert_eq!(label(1), "one");
    assert_eq!(label(7), "many");
}

#[test]
fn parse_ok() { assert!(parse("abc").is_ok()); }

#[test]
fn bumps() { let mut c = Counter { count: 0 }; c.bump(); }

#[test]
fn logs() { logged(3); }
"#;

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ripr-agent-packet-schema-{label}-{}-{stamp}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

fn fixture_repo(root: &Path) -> Result<(), String> {
    fixture_repo_with(root, LIB, TESTS)
}

fn fixture_repo_with(root: &Path, lib: &str, tests: &str) -> Result<(), String> {
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|error| format!("create {dir}: {error}"))?;
    }
    fixture_git_ok(root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(root, &["config", "user.email", "packet-schema@t.st"])?;
    fixture_git_ok(root, &["config", "user.name", "packet-schema"])?;
    fixture_git_ok(root, &["config", "core.autocrlf", "false"])?;
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"packet_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"packet_fixture\"\npath = \"src/lib.rs\"\n",
    )?;
    write(&root.join(".gitignore"), "/target/\n")?;
    write(&root.join("src/lib.rs"), lib)?;
    write(&root.join("tests/main.rs"), tests)?;
    fixture_git_ok(root, &["add", "-A"])?;
    fixture_git_ok(root, &["commit", "-q", "-m", "base"])?;
    Ok(())
}

fn ripr(args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .env_remove("RIPR_REPO_EXPOSURE_SEAM_LIMIT")
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn parse(args: &[&str], output: &Output) -> Result<Value, String> {
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "ripr {args:?} JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn conforms(document: &Value, schema: &Value, label: &str) -> Result<(), String> {
    published_schema::check(document, schema, schema, &format!("packet[{label}]"))
}

/// Every seam's success packet from the fixture, keyed by seam id.
fn packets() -> Result<Vec<(String, Value)>, String> {
    let root = unique("seams");
    fixture_repo(&root)?;
    let root_arg = root.display().to_string();
    let inventory_args = [
        "check",
        "--root",
        root_arg.as_str(),
        "--mode",
        "draft",
        "--format",
        "repo-exposure-json",
    ];
    let inventory = ripr(&inventory_args)?;
    if !inventory.status.success() {
        return Err(format!(
            "repo-exposure inventory: exit {:?}; stderr:\n{}",
            inventory.status.code(),
            String::from_utf8_lossy(&inventory.stderr)
        ));
    }
    let inventory = parse(&inventory_args, &inventory)?;
    let ids: Vec<String> = inventory["seams"]
        .as_array()
        .ok_or("the inventory has no seams array")?
        .iter()
        .filter_map(|seam| seam["seam_id"].as_str().map(str::to_string))
        .collect();
    let mut documents = Vec::new();
    let mut refused = 0usize;
    for id in ids {
        let args = [
            "agent",
            "packet",
            "--root",
            root_arg.as_str(),
            "--seam-id",
            id.as_str(),
            "--json",
        ];
        let output = ripr(&args)?;
        match output.status.code() {
            Some(0) => {
                let document = parse(&args, &output)?;
                documents.push((id.clone(), document));
            }
            // A policy-omitted seam is refused; its envelope has its own
            // schema. Exit 3 also means an unknown seam id, which would be a
            // fixture fault, so only the policy refusal is skipped.
            Some(3) => {
                let refusal = parse(&args, &output)?;
                let category = &refusal["run_limitations"][0]["category"];
                if category != "seam_policy_omitted" {
                    return Err(format!(
                        "agent packet --seam-id {id}: exit 3 was not a policy refusal: {category}"
                    ));
                }
                refused += 1;
            }
            other => {
                return Err(format!(
                    "agent packet --seam-id {id}: exit {other:?}; stderr:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&root);
    println!(
        "{} packeted seams, {refused} policy-omitted seams",
        documents.len()
    );
    Ok(documents)
}

#[test]
fn the_checker_covers_every_keyword_the_packet_schema_uses() -> Result<(), String> {
    for path in [PACKET_SCHEMA, SEAM_PACKETS_SCHEMA] {
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

/// `anyOf` (the copied assertion-guidance definition uses it) passes when at
/// least one branch matches and fails when none does.
#[test]
fn the_checker_reads_any_of() -> Result<(), String> {
    let schema = json!({"anyOf": [{"type": "string"}, {"type": "integer"}]});
    for accepted in [json!("a"), json!(3)] {
        published_schema::check(&accepted, &schema, &schema, "anyOf")?;
    }
    match published_schema::check(&json!(true), &schema, &schema, "anyOf") {
        Ok(()) => Err("anyOf accepted a value no branch matches".to_string()),
        Err(error) if error.contains("matched none of 2 anyOf branches") => Ok(()),
        Err(error) => Err(format!("anyOf refused for another reason: {error}")),
    }
}

/// Every success document conforms, and together they reach each branch the
/// schema names.
#[test]
fn agent_packet_json_conforms_for_every_fixture_seam() -> Result<(), String> {
    let schema = published_schema::read(PACKET_SCHEMA)?;
    let documents = packets()?;
    if documents.len() < 8 {
        return Err(format!(
            "expected the fixture to packet at least 8 seams, got {}",
            documents.len()
        ));
    }
    let mut reached = BTreeSet::new();
    for (id, document) in &documents {
        conforms(document, &schema, id)?;
        let packet = &document["packets"][0];
        let record = &packet["evidence_record"];
        reached.insert(format!("task:{}", packet["task"].as_str().unwrap_or("")));
        reached.insert(format!(
            "assertion:{}",
            packet["assertion_shape"]["state"]
                .as_str()
                .unwrap_or("null")
        ));
        reached.insert(format!(
            "gap_state:{}",
            record["canonical_item"]["gap_state"].as_str().unwrap_or("")
        ));
        if packet.get("proof_obligation").is_some() {
            reached.insert("proof_obligation".to_string());
        }
        if document.get("next").is_some() {
            reached.insert("next".to_string());
        }
        if !packet["nearest_strong_test_to_imitate"].is_null() {
            reached.insert("nearest_strong_test".to_string());
        }
        if !record["canonical_item"]["command_specs"]["receipt"].is_null() {
            reached.insert("receipt_spec".to_string());
        }
    }
    let wanted = [
        "task:write_targeted_test",
        "task:inspect_static_limitation",
        "assertion:concrete",
        "assertion:unresolved",
        "assertion:null",
        "gap_state:actionable",
        "gap_state:static_limitation",
        "proof_obligation",
        "next",
        "nearest_strong_test",
        "receipt_spec",
    ];
    let missing: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|branch| !reached.contains(*branch))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the fixture run never reached {missing:?}; reached {reached:?}"
        ))
    }
}

type Mutation = fn(&mut Value, &Value);

/// The schema refuses documents the producer never writes, each at the field
/// it changes and for the constraint it breaks.
#[test]
fn published_packet_schema_rejects_documents_it_forbids() -> Result<(), String> {
    let schema = published_schema::read(PACKET_SCHEMA)?;
    let documents = packets()?;
    let by_task = |task: &str| {
        documents
            .iter()
            .map(|(_, document)| document)
            .find(|document| document["packets"][0]["task"] == task)
            .cloned()
            .ok_or(format!("no {task} packet in the fixture run"))
    };
    let write = by_task("write_targeted_test")?;
    let inspect = by_task("inspect_static_limitation")?;
    // Inspection packets of a seam that is not a boundary carry no proof
    // obligation, and the copy below gives one to such a packet.
    let inspect_non_boundary = documents
        .iter()
        .map(|(_, document)| document)
        .find(|document| {
            document["packets"][0]["task"] == "inspect_static_limitation"
                && document["packets"][0]["seam_kind"] != "predicate_boundary"
        })
        .cloned()
        .ok_or("no non-boundary inspection packet in the fixture run")?;
    let concrete = write.clone();
    let cases: [(&str, (&str, &str), Value, Mutation); 20] = [
        (
            "an undeclared top-level field",
            ("packet[an undeclared top-level field]", "emitted `extra`"),
            write.clone(),
            |doc, _| doc["extra"] = json!(1),
        ),
        (
            "another version",
            (".schema_version", "expected const \"0.5\""),
            write.clone(),
            |doc, _| doc["schema_version"] = json!("0.6"),
        ),
        (
            "a capped run status",
            (".run_status", "expected const \"complete\""),
            write.clone(),
            |doc, _| doc["run_status"] = json!("seam_limit_applied"),
        ),
        (
            "a packet count that disagrees",
            (".packets_total", "expected const 1"),
            write.clone(),
            |doc, _| doc["packets_total"] = json!(0),
        ),
        (
            "a targeted-test packet with no loop commands",
            (
                "packet[a targeted-test packet with no loop commands]",
                "missing required field `next`",
            ),
            write.clone(),
            |doc, _| {
                if let Some(object) = doc.as_object_mut() {
                    object.remove("next");
                }
            },
        ),
        (
            "loop commands on an inspection packet",
            (
                "packet[loop commands on an inspection packet]",
                "matched a `not` schema",
            ),
            inspect.clone(),
            |doc, source| doc["next"] = source["next"].clone(),
        ),
        (
            "a targeted-test packet with no test command",
            (
                ".packets[0]",
                "missing required field `suggested_test_command`",
            ),
            write.clone(),
            |doc, _| {
                if let Some(object) = doc["packets"][0].as_object_mut() {
                    object.remove("suggested_test_command");
                }
            },
        ),
        (
            "an edit surface on an inspection packet",
            (".allowed_edit_surface", "above maxItems 0"),
            inspect.clone(),
            |doc, _| doc["packets"][0]["allowed_edit_surface"] = json!(["tests/main.rs"]),
        ),
        (
            "an unknown task",
            (".task", "is not one of the 2 allowed values"),
            inspect.clone(),
            |doc, _| doc["packets"][0]["task"] = json!("fix_it"),
        ),
        (
            "a proof obligation on a seam that is not a boundary",
            (".seam_kind", "expected const \"predicate_boundary\""),
            inspect_non_boundary.clone(),
            |doc, source| {
                doc["packets"][0]["proof_obligation"] =
                    source["packets"][0]["proof_obligation"].clone()
            },
        ),
        (
            "concrete guidance with no example",
            (".assertion_shape", "matched none of 6 anyOf branches"),
            concrete.clone(),
            |doc, _| doc["packets"][0]["assertion_shape"]["example"] = Value::Null,
        ),
        (
            "suggested assertions without concrete guidance",
            (".suggested_assertions", "above maxItems 0"),
            inspect.clone(),
            |doc, _| doc["packets"][0]["suggested_assertions"] = json!(["assert_eq!(f(1), 2)"]),
        ),
        (
            "another evidence record version",
            (".evidence_record.schema_version", "expected const \"0.1\""),
            write.clone(),
            |doc, _| doc["packets"][0]["evidence_record"]["schema_version"] = json!("0.2"),
        ),
        (
            "concrete guidance that recommends no action",
            (
                ".recommendation.action",
                "expected const \"write_targeted_test\"",
            ),
            write.clone(),
            |doc, _| {
                doc["packets"][0]["evidence_record"]["recommendation"]["action"] =
                    json!("no_action")
            },
        ),
        (
            "an item kind that disagrees with the gap state",
            (".canonical_item_kind", "expected const \"gap\""),
            write.clone(),
            |doc, _| {
                doc["packets"][0]["evidence_record"]["canonical_item"]["canonical_item_kind"] =
                    json!("evidence")
            },
        ),
        (
            "a receipt route on a gap that is not actionable",
            (".command_specs.receipt", "is not of type null"),
            inspect.clone(),
            |doc, source| {
                doc["packets"][0]["evidence_record"]["canonical_item"]["command_specs"]["receipt"] =
                source["packets"][0]["evidence_record"]["canonical_item"]["command_specs"]["receipt"].clone()
            },
        ),
        (
            "imported runtime calibration",
            (
                ".calibration.availability",
                "expected const \"not_imported\"",
            ),
            write.clone(),
            |doc, _| {
                doc["packets"][0]["evidence_record"]["calibration"]["availability"] =
                    json!("imported")
            },
        ),
        (
            "an absolute typed working directory",
            (".working_directory", "does not match"),
            write.clone(),
            |doc, _| {
                doc["packets"][0]["evidence_record"]["canonical_item"]["command_specs"]["verify"]
                    ["working_directory"] = json!("/tmp")
            },
        ),
        (
            "presentation text on the record",
            (".presentation_text", "is not of type null"),
            write.clone(),
            |doc, _| {
                doc["packets"][0]["evidence_record"]["presentation_text"] =
                    json!({"visibility": "visible"})
            },
        ),
        (
            "an unknown packet confidence",
            (".confidence", "is not one of the 4 allowed values"),
            write.clone(),
            |doc, _| doc["packets"][0]["confidence"] = json!("certain"),
        ),
    ];
    for (label, (location, reason), base, mutate) in cases {
        let mut mutated = base.clone();
        mutate(&mut mutated, &write);
        if mutated == base {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&mutated, &schema, label) {
            Ok(()) => {
                return Err(format!(
                    "the published packet schema accepted a document the producer never writes: {label}"
                ));
            }
            Err(error) => {
                let at_field = error
                    .split(": ")
                    .next()
                    .is_some_and(|path| path.ends_with(location));
                if !at_field || !error.contains(reason) {
                    return Err(format!(
                        "{label}: rejected for another reason; expected `{reason}` at `{location}`, got: {error}"
                    ));
                }
                println!("rejected as expected ({label}): {error}");
            }
        }
    }
    Ok(())
}

const SINGLE_LIB: &str = r#"pub fn discounted_total(amount: i32, threshold: i32) -> i32 {
    if amount >= threshold { amount - 10 } else { amount }
}
"#;

const SINGLE_TESTS: &str = r#"use packet_fixture::*;

#[test]
fn far_above() { assert_eq!(discounted_total(10_000, 100), 9_990); }
"#;

/// `ripr check --format agent-seam-packets-json` over a fresh fixture, with
/// any extra arguments, and its exit code.
fn seam_packets(lib: &str, tests: &str, extra: &[&str]) -> Result<(Option<i32>, Value), String> {
    seam_packets_with_delta(lib, tests, extra, None)
}

/// As [`seam_packets`], with an optional canonical delta artifact written to
/// `target/ripr/pr/canonical-delta.json` before the run.
fn seam_packets_with_delta(
    lib: &str,
    tests: &str,
    extra: &[&str],
    delta: Option<&Value>,
) -> Result<(Option<i32>, Value), String> {
    let root = unique("check-format");
    fixture_repo_with(&root, lib, tests)?;
    if let Some(delta) = delta {
        let dir = root.join("target/ripr/pr");
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("create {}: {error}", dir.display()))?;
        write(&dir.join("canonical-delta.json"), &delta.to_string())?;
    }
    let root_arg = root.display().to_string();
    let mut args = vec![
        "check",
        "--root",
        root_arg.as_str(),
        "--format",
        "agent-seam-packets-json",
    ];
    args.extend_from_slice(extra);
    let output = ripr(&args)?;
    let document = parse(&args, &output)?;
    let _ = std::fs::remove_dir_all(&root);
    Ok((output.status.code(), document))
}

/// The five `agent-seam-packets-json` documents a fixture run reaches: many
/// packets, one packet, a diff selector with no AnalysisOutcome, a refusal,
/// and many packets with a canonical delta artifact present.
fn seam_packet_documents() -> Result<[(&'static str, Value); 5], String> {
    let (code, many) = seam_packets(LIB, TESTS, &[])?;
    if code != Some(0) {
        return Err(format!("check-format run exited {code:?}"));
    }
    let gap_id = many["packets"]
        .as_array()
        .ok_or("the many-packet document has no packets")?
        .iter()
        .find(|packet| packet["task"] == "write_targeted_test")
        .and_then(|packet| packet["evidence_record"]["canonical_item"]["canonical_gap_id"].as_str())
        .ok_or("no write packet with a canonical gap id")?
        .to_string();
    let state = json!({
        "canonical_owner": "src/lib.rs::discounted_total",
        "behavior_identity": "amount >= threshold",
        "discriminator_identity": "boundary",
        "gap_state": "actionable",
        // The artifact reader takes the domain enum's own spelling; the
        // packet writes it back in lowercase.
        "oracle_strength": "Weak",
    });
    let delta = json!({
        "schema_version": "0.1",
        "coverage": {"base_items": 1, "head_items": 1, "matched_items": 1, "ambiguous_items": 0, "unknown_items": 0},
        "deltas": [{
            "canonical_gap_id": gap_id,
            "delta_attribution": "weakened_by_change",
            "base_state": state,
            "head_state": state,
            "attribution_basis": ["same_canonical_owner", "oracle_strength_decreased"],
            "comparison_confidence": "fixture_backed",
        }],
    });
    let (code, causal) = seam_packets_with_delta(LIB, TESTS, &[], Some(&delta))?;
    if code != Some(0) {
        return Err(format!(
            "check-format run with a canonical delta exited {code:?}"
        ));
    }
    let (code, one) = seam_packets(SINGLE_LIB, SINGLE_TESTS, &[])?;
    if code != Some(0) {
        return Err(format!("one-seam check-format run exited {code:?}"));
    }
    let (code, diff) = seam_packets(LIB, TESTS, &["--base", "HEAD"])?;
    if code != Some(0) {
        return Err(format!("check-format run with --base exited {code:?}"));
    }
    let missing_root = unique("missing-root");
    let missing_arg = missing_root.display().to_string();
    let args = [
        "check",
        "--root",
        missing_arg.as_str(),
        "--format",
        "agent-seam-packets-json",
    ];
    let output = ripr(&args)?;
    if output.status.code() != Some(2) {
        return Err(format!(
            "refused check-format run exited {:?}",
            output.status.code()
        ));
    }
    let refused = parse(&args, &output)?;
    Ok([
        ("many packets", many),
        ("one packet", one),
        ("diff selector", diff),
        ("refused", refused),
        ("canonical delta", causal),
    ])
}

/// Each check-format document conforms and is the variant it claims to be.
#[test]
fn agent_seam_packets_json_conforms_for_each_document_variant() -> Result<(), String> {
    let schema = published_schema::read(SEAM_PACKETS_SCHEMA)?;
    let [many, one, diff, refused, causal] = seam_packet_documents()?;
    for (label, document) in [&many, &one, &diff, &refused, &causal] {
        conforms(document, &schema, label)?;
    }
    let delta_packets = causal.1["packets"]
        .as_array()
        .ok_or("the canonical-delta document has no packets")?
        .iter()
        .filter(|packet| packet.get("delta_attribution").is_some())
        .count();
    let tasks: BTreeSet<&str> = many.1["packets"]
        .as_array()
        .ok_or("the many-packet document has no packets")?
        .iter()
        .filter_map(|packet| packet["task"].as_str())
        .collect();
    let checks = [
        (
            "the many-packet document holds both tasks",
            tasks.len() == 2 && many.1["packets_total"].as_u64() >= Some(8),
        ),
        (
            "the many-packet document names no receipt",
            many.1["next"]["receipt_after_verify"].is_null(),
        ),
        (
            "the one-packet document names its receipt",
            one.1["packets_total"] == 1 && one.1["next"]["receipt_after_verify"].is_string(),
        ),
        (
            "no selector projects nothing",
            many.1["analysis_outcome_status"] == "not_applicable",
        ),
        (
            "a diff selector reports the missing AnalysisOutcome",
            diff.1["analysis_outcome_status"] == "missing"
                && diff.1["analysis_outcome_error"].is_string(),
        ),
        (
            "a canonical delta projects the comparison and one packet's delta",
            causal.1["causal_comparison"]["available"] == true
                && delta_packets == 1
                && many.1.get("causal_comparison").is_none(),
        ),
        (
            "the refusal is the format's own shape",
            refused.1["run_status"] == "refused"
                && refused.1["limitations"][0]["category"] == "repository_root_unusable",
        ),
    ];
    for (claim, held) in checks {
        if !held {
            return Err(format!("{claim}: not so in this run"));
        }
    }
    Ok(())
}

/// The check-format schema shares the `--seam-id` schema's definitions: only
/// `next` differs, and its packet adds the canonical-delta fields and nothing
/// else, so the two documents cannot drift apart unnoticed.
#[test]
fn the_two_packet_schemas_share_their_definitions() -> Result<(), String> {
    let single = published_schema::read(PACKET_SCHEMA)?;
    let family = published_schema::read(SEAM_PACKETS_SCHEMA)?;
    let single_defs = single["$defs"]
        .as_object()
        .ok_or("agent-packet.schema.json has no $defs")?;
    for (name, definition) in single_defs {
        if name == "packet" || name == "next" {
            continue;
        }
        if family["$defs"][name] != *definition {
            return Err(format!(
                "$defs/{name} differs between the two packet schemas"
            ));
        }
    }
    let causal = [
        "delta_attribution",
        "base_state",
        "head_state",
        "attribution_basis",
        "comparison_confidence",
    ];
    let mut family_packet = family["$defs"]["packet"].clone();
    let properties = family_packet["properties"]
        .as_object_mut()
        .ok_or("the family packet has no properties")?;
    for field in causal {
        if properties.remove(field).is_none() {
            return Err(format!("the family packet lacks `{field}`"));
        }
    }
    let rules = family_packet["allOf"]
        .as_array_mut()
        .ok_or("the family packet has no allOf")?;
    let causal_rule = rules.pop().ok_or("the family packet has no rules")?;
    if causal_rule["then"]["required"] != json!(causal) {
        return Err(format!(
            "the last family packet rule is not the canonical-delta rule: {causal_rule}"
        ));
    }
    if family_packet != single_defs["packet"] {
        return Err(
            "the family packet differs from the --seam-id packet beyond the canonical-delta fields"
                .to_string(),
        );
    }
    Ok(())
}

/// The check-format schema refuses documents the producer never writes, each
/// at the field it changes and for the constraint it breaks.
#[test]
fn published_seam_packets_schema_rejects_documents_it_forbids() -> Result<(), String> {
    let schema = published_schema::read(SEAM_PACKETS_SCHEMA)?;
    let [many, one, diff, refused, _] = seam_packet_documents()?;
    let inspect_only = {
        let mut document = many.1.clone();
        let inspections: Vec<Value> = document["packets"]
            .as_array()
            .ok_or("no packets")?
            .iter()
            .filter(|packet| packet["task"] == "inspect_static_limitation")
            .cloned()
            .collect();
        document["packets_total"] = json!(inspections.len());
        document["packets"] = Value::Array(inspections);
        if let Some(object) = document.as_object_mut() {
            object.remove("next");
        }
        conforms(&document, &schema, "inspection-only document")?;
        document
    };
    let cases: [(&str, (&str, &str), Value, Mutation); 12] = [
        (
            "an unknown run status",
            (".run_status", "is not one of the 2 allowed values"),
            many.1.clone(),
            |doc, _| doc["run_status"] = json!("seam_limit_applied"),
        ),
        (
            "limitations on a completed run",
            (
                "packet[limitations on a completed run]",
                "matched a `not` schema",
            ),
            many.1.clone(),
            |doc, refused| doc["limitations"] = refused["limitations"].clone(),
        ),
        (
            "a refusal with no limitation",
            (
                "packet[a refusal with no limitation]",
                "missing required field `limitations`",
            ),
            refused.1.clone(),
            |doc, _| {
                if let Some(object) = doc.as_object_mut() {
                    object.remove("limitations");
                }
            },
        ),
        (
            "a refusal that counts packets",
            (".packets_total", "expected const 0"),
            refused.1.clone(),
            |doc, _| doc["packets_total"] = json!(1),
        ),
        (
            "a refusal with an analysis outcome status",
            (
                "packet[a refusal with an analysis outcome status]",
                "matched a `not` schema",
            ),
            refused.1.clone(),
            |doc, _| doc["analysis_outcome_status"] = json!("not_applicable"),
        ),
        (
            "packets counted as none",
            (".packets_total", "below minimum 1"),
            many.1.clone(),
            |doc, _| doc["packets_total"] = json!(0),
        ),
        (
            "a receipt for many packets",
            (".receipt_after_verify", "is not of type null"),
            many.1.clone(),
            |doc, one| {
                doc["next"]["receipt_after_verify"] = one["next"]["receipt_after_verify"].clone()
            },
        ),
        (
            "no receipt for one packet",
            (".receipt_after_verify", "is not of type string"),
            one.1.clone(),
            |doc, _| doc["next"]["receipt_after_verify"] = Value::Null,
        ),
        (
            "loop commands with nothing to write",
            (
                "packet[loop commands with nothing to write]",
                "matched a `not` schema",
            ),
            inspect_only.clone(),
            |doc, one| doc["next"] = one["next"].clone(),
        ),
        (
            "an error with no selector",
            (".analysis_outcome_error", "is not of type null"),
            many.1.clone(),
            |doc, _| doc["analysis_outcome_error"] = json!("no outcome"),
        ),
        (
            "a missing outcome with no error",
            (".analysis_outcome_error", "is not of type string"),
            diff.1.clone(),
            |doc, _| doc["analysis_outcome_error"] = Value::Null,
        ),
        (
            "a partial canonical delta on a packet",
            (".packets[0]", "missing required field `base_state`"),
            many.1.clone(),
            |doc, _| doc["packets"][0]["delta_attribution"] = json!("introduced_by_change"),
        ),
    ];
    let sources = json!({
        "limitations": refused.1["limitations"].clone(),
        "next": one.1["next"].clone(),
    });
    for (label, (location, reason), base, mutate) in cases {
        let mut mutated = base.clone();
        mutate(&mut mutated, &sources);
        if mutated == base {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&mutated, &schema, label) {
            Ok(()) => {
                return Err(format!(
                    "the published seam-packets schema accepted a document the producer never writes: {label}"
                ));
            }
            Err(error) => {
                let at_field = error
                    .split(": ")
                    .next()
                    .is_some_and(|path| path.ends_with(location));
                if !at_field || !error.contains(reason) {
                    return Err(format!(
                        "{label}: rejected for another reason; expected `{reason}` at `{location}`, got: {error}"
                    ));
                }
                println!("rejected as expected ({label}): {error}");
            }
        }
    }
    Ok(())
}
