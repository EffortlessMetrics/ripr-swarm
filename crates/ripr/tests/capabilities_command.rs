//! Runs the installed `ripr capabilities` binary end to end (#5531).

use std::process::Command;

fn run(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map(|output| {
            (
                output.status.success(),
                String::from_utf8_lossy(&output.stdout).into_owned(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            )
        });
    match output {
        Ok(result) => result,
        Err(err) => (false, String::new(), format!("spawn failed: {err}")),
    }
}

fn json(args: &[&str]) -> serde_json::Value {
    let (ok, stdout, stderr) = run(args);
    assert!(ok, "ripr {args:?} failed: {stderr}");
    serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null)
}

#[test]
fn json_registry_carries_the_documented_fields() {
    let doc = json(&["capabilities", "--json"]);
    assert_eq!(doc["kind"], "ripr_language_capabilities");
    assert_eq!(doc["schema_version"], "0.1");
    assert_eq!(doc["absent_cell_meaning"], "not_proven");
    assert!(
        doc["semantic_digest"].as_str().is_some_and(
            |digest| digest.len() == "sha256:".len() + 64 && digest.starts_with("sha256:")
        ),
        "{doc}"
    );
    let languages = doc["languages"].as_array().cloned().unwrap_or_default();
    for wire in ["rust", "typescript", "javascript", "python", "perl"] {
        assert!(
            languages.iter().any(|entry| entry["language"] == wire),
            "missing language {wire}"
        );
    }
    let cells = doc["cells"].as_array().cloned().unwrap_or_default();
    assert!(!cells.is_empty());
    for cell in &cells {
        for field in [
            "id",
            "language",
            "framework",
            "mode",
            "family",
            "state",
            "tier_ceiling",
            "proof_refs",
        ] {
            assert!(!cell[field].is_null(), "cell missing {field}: {cell}");
        }
        assert!(
            doc["support_state_vocabulary"]
                .as_array()
                .is_some_and(|vocab| vocab.contains(&cell["state"])),
            "state outside the closed vocabulary: {cell}"
        );
    }
}

#[test]
fn language_filter_narrows_cells_and_keeps_the_digest() {
    let all = json(&["capabilities", "--json"]);
    let python = json(&["capabilities", "--json", "--language", "python"]);
    let cells = python["cells"].as_array().cloned().unwrap_or_default();
    assert!(!cells.is_empty());
    assert!(cells.iter().all(|cell| cell["language"] == "python"));
    assert_eq!(all["semantic_digest"], python["semantic_digest"]);
}

#[test]
fn unknown_language_is_refused_with_a_nonzero_exit() {
    let (ok, _stdout, stderr) = run(&["capabilities", "--language", "cobol"]);
    assert!(!ok);
    assert!(stderr.contains("cobol"), "{stderr}");
}

#[test]
fn language_flag_without_a_value_is_refused() {
    for args in [
        &["capabilities", "--language"][..],
        &["capabilities", "--language", "--json"][..],
    ] {
        let (ok, _stdout, stderr) = run(args);
        assert!(!ok, "{args:?}");
        assert!(stderr.contains("--language"), "{args:?}: {stderr}");
    }
}

#[test]
fn human_output_and_help_route() {
    let (ok, stdout, _) = run(&["capabilities"]);
    assert!(ok);
    assert!(stdout.contains("ripr language support"), "{stdout}");
    assert!(!stdout.contains("not_proven"), "{stdout}");
    assert!(!stdout.contains("RIPR-SPEC"), "{stdout}");
    let (ok, help, _) = run(&["capabilities", "--help"]);
    assert!(ok);
    assert!(help.contains("Usage: ripr capabilities"), "{help}");
}

#[test]
fn markdown_is_the_generated_support_tiers_block_and_matches_the_doc() {
    let (ok, stdout, stderr) = run(&["capabilities", "--markdown"]);
    assert!(ok, "{stderr}");
    assert!(
        stdout.starts_with("<!-- capability-registry:begin"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("<!-- capability-registry:end -->\n"),
        "{stdout}"
    );
    let doc_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/status/SUPPORT_TIERS.md");
    let doc = std::fs::read_to_string(&doc_path).unwrap_or_default();
    assert!(
        doc.contains(stdout.trim_end()),
        "doc block differs from --markdown"
    );
    for combined in [
        &["capabilities", "--markdown", "--json"][..],
        &["capabilities", "--markdown", "--language", "rust"][..],
    ] {
        let (ok, _stdout, stderr) = run(combined);
        assert!(!ok, "{combined:?}");
        assert!(stderr.contains("whole-table"), "{combined:?}: {stderr}");
    }
}

// The published `ripr capabilities --json` schema, checked against bytes the
// real command wrote (#14067). The registry is a static table, so every
// `--language` projection is exercised, not one sample.

#[path = "common/published_schema.rs"]
mod published_schema;

const CAPABILITIES_SCHEMA: &str = "schemas/ripr/capabilities.schema.json";
const LANGUAGES: [&str; 5] = ["rust", "typescript", "javascript", "python", "perl"];

fn conforms(document: &serde_json::Value, schema: &serde_json::Value) -> Result<(), String> {
    published_schema::check(document, schema, schema, "capabilities")
}

#[test]
fn the_checker_covers_every_keyword_the_capabilities_schema_uses() -> Result<(), String> {
    let schema = published_schema::read(CAPABILITIES_SCHEMA)?;
    let ignored = published_schema::unsupported_keywords(&schema);
    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the conformance checker ignores these published keywords, so it would under-check the schema: {ignored:?}"
        ))
    }
}

#[test]
fn capabilities_json_conforms_to_the_published_schema() -> Result<(), String> {
    let schema = published_schema::read(CAPABILITIES_SCHEMA)?;
    let whole = json(&["capabilities", "--json"]);
    conforms(&whole, &schema).map_err(|error| format!("whole table: {error}"))?;
    assert!(whole["filtered_by"].is_null(), "{whole}");
    let mut narrowed_cells = 0usize;
    for language in LANGUAGES {
        let narrowed = json(&["capabilities", "--json", "--language", language]);
        conforms(&narrowed, &schema).map_err(|error| format!("--language {language}: {error}"))?;
        assert_eq!(narrowed["filtered_by"]["language"], language);
        narrowed_cells += narrowed["cells"].as_array().map_or(0, Vec::len);
    }
    // The projections partition the table, so the loop above validated every
    // cell a second time rather than five empty arrays.
    let whole_cells = whole["cells"].as_array().map_or(0, Vec::len);
    assert!(whole_cells > 0, "{whole}");
    assert_eq!(narrowed_cells, whole_cells);
    Ok(())
}

/// The schema's closed vocabularies are the producer's, in the producer's
/// order: a state or language added to the domain cannot ship while the
/// published schema still lists the old set, and the schema cannot list a
/// value the build never emits.
#[test]
fn published_vocabularies_equal_the_emitted_ones() -> Result<(), String> {
    let schema = published_schema::read(CAPABILITIES_SCHEMA)?;
    let whole = json(&["capabilities", "--json"]);
    assert_eq!(
        schema["$defs"]["support_state"]["enum"], whole["support_state_vocabulary"],
        "support_state enum differs from the emitted support_state_vocabulary"
    );
    let emitted_languages: Vec<serde_json::Value> = whole["languages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| entry["language"].clone())
        .collect();
    assert_eq!(
        schema["$defs"]["language"]["enum"],
        serde_json::Value::Array(emitted_languages),
        "language enum differs from the emitted languages"
    );
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        whole["schema_version"]
    );
    Ok(())
}

/// The negative experiment: a checker or schema that accepted everything
/// would still pass the conformance test above. Each case starts from real
/// emitted bytes and breaks one thing a consumer relies on.
#[test]
fn published_capabilities_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut serde_json::Value);
    let schema = published_schema::read(CAPABILITIES_SCHEMA)?;
    let whole = json(&["capabilities", "--json"]);
    conforms(&whole, &schema)?;
    let cases: [(&str, Mutation); 12] = [
        ("unknown top-level field", |doc| {
            doc["confidence"] = serde_json::json!(0.9)
        }),
        ("unknown cell field", |doc| {
            doc["cells"][0]["score"] = serde_json::json!(1)
        }),
        ("unknown language-entry field", |doc| {
            doc["languages"][0]["enabled"] = serde_json::json!(true)
        }),
        ("stale schema_version", |doc| {
            doc["schema_version"] = serde_json::json!("0.0")
        }),
        ("numeric schema_version", |doc| {
            doc["schema_version"] = serde_json::json!(0.1)
        }),
        ("missing required top-level field", |doc| {
            if let Some(object) = doc.as_object_mut() {
                object.remove("non_claims");
            }
        }),
        ("missing required cell field", |doc| {
            if let Some(object) = doc["cells"][0].as_object_mut() {
                object.remove("limit");
            }
        }),
        ("state outside the closed vocabulary", |doc| {
            doc["cells"][0]["state"] = serde_json::json!("supported")
        }),
        ("unlisted language", |doc| {
            doc["cells"][0]["language"] = serde_json::json!("cobol")
        }),
        ("truncated digest", |doc| {
            doc["semantic_digest"] = serde_json::json!("sha256:abc")
        }),
        ("vocabulary with a dropped state", |doc| {
            if let Some(vocabulary) = doc["support_state_vocabulary"].as_array_mut() {
                vocabulary.pop();
            }
        }),
        ("string adapter_compiled", |doc| {
            doc["languages"][0]["adapter_compiled"] = serde_json::json!("true")
        }),
    ];
    for (label, mutate) in cases {
        let mut document = whole.clone();
        mutate(&mut document);
        assert_ne!(document, whole, "{label}: the mutation changed nothing");
        match conforms(&document, &schema) {
            Ok(()) => {
                return Err(format!(
                    "the published capabilities schema accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => println!("rejected as expected ({label}): {error}"),
        }
    }
    Ok(())
}
