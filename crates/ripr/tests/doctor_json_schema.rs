//! The published `ripr doctor --json` schema, checked against bytes the real
//! command wrote (#5214).
//!
//! `schemas/ripr/doctor.schema.json` describes a document no committed
//! fixture can carry: doctor writes per-run bytes to stdout. A hand-written
//! `tests/fixtures/verification/ripr/doctor.valid.json` therefore lets the
//! schema confirm itself while the producer drifts, which is exactly what the
//! repair-attempt contract row records for the same situation. This target is
//! the narrower authority: it reads the published schema and checks the
//! document the production path actually emitted, then runs the negative
//! experiment that proves the checker rejects drift rather than accepting
//! everything.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn published_schema() -> Result<Value, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/ripr/doctor.schema.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn fixture_root(label: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-doctor-schema-{label}-{}-{stamp}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    for dir in ["src", "lib", "t"] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|error| format!("create {dir}: {error}"))?;
    }
    for (path, text) in [
        (
            "Cargo.toml",
            "[package]\nname = \"doctor-schema-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        ("src/lib.rs", "pub fn placeholder() {}\n"),
        // An explicit enabled set keeps Python detected and disabled, so the
        // preview-language gap and the detected/enabled distinction are both
        // exercised by real emitted bytes.
        ("ripr.toml", "[languages]\nenabled = [\"rust\"]\n"),
        (
            "pyproject.toml",
            "[project]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
        ),
        ("Makefile.PL", "use ExtUtils::MakeMaker;\n"),
        ("lib/Pricing.pm", "package Pricing;\n1;\n"),
        ("t/pricing.t", "use Test::More;\ndone_testing();\n"),
        ("tool.go", "package main\n\nfunc main() {}\n"),
    ] {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))?;
    }
    Ok(root)
}

fn doctor_json(root: &Path) -> Result<Value, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["doctor", "--root", &root.display().to_string(), "--json"])
        .output()
        .map_err(|error| format!("spawn doctor: {error}"))?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "doctor JSON did not parse: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// Check one emitted value against one subschema.
///
/// The keyword set mirrors what the doctor schema actually uses. A schema that
/// grows a keyword this checker ignores is reported by
/// `the_checker_covers_every_keyword_the_published_schema_uses`, so the
/// narrower authority cannot silently under-check the schema it claims to
/// enforce.
fn check(value: &Value, schema: &Value, root: &Value, location: &str) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let pointer = reference.strip_prefix('#').unwrap_or(reference);
        let resolved = root
            .pointer(pointer)
            .ok_or_else(|| format!("{location}: unresolved schema reference {reference}"))?;
        check(value, resolved, root, location)?;
        // A `$ref` with no sibling keyword carries nothing else to enforce.
        // Re-running the object walk against it would demand a `properties`
        // map that lives on the referenced subschema instead.
        if schema.as_object().is_some_and(|object| object.len() == 1) {
            return Ok(());
        }
    }
    if let Some(expected) = schema.get("const")
        && value != expected
    {
        return Err(format!(
            "{location}: expected const {expected}, got {value}"
        ));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.iter().any(|candidate| candidate == value)
    {
        return Err(format!(
            "{location}: {value} is not one of the {} allowed values",
            allowed.len()
        ));
    }
    if let Some(expected) = schema.get("type") {
        check_type(value, expected, location)?;
    }
    if let Some(object) = value.as_object() {
        check_object(object, schema, root, location)?;
    }
    if let Some(items) = value.as_array() {
        for (index, item) in items.iter().enumerate() {
            if let Some(item_schema) = schema.get("items") {
                check(item, item_schema, root, &format!("{location}[{index}]"))?;
            }
        }
    }
    if let Some(text) = value.as_str()
        && let Some(minimum) = schema.get("minLength").and_then(Value::as_u64)
        && (text.chars().count() as u64) < minimum
    {
        return Err(format!("{location}: shorter than minLength {minimum}"));
    }
    if let Some(number) = value.as_i64()
        && let Some(minimum) = schema.get("minimum").and_then(Value::as_i64)
        && number < minimum
    {
        return Err(format!("{location}: {number} is below minimum {minimum}"));
    }
    Ok(())
}

fn check_type(value: &Value, expected: &Value, location: &str) -> Result<(), String> {
    let names: Vec<&str> = match expected {
        Value::String(name) => vec![name.as_str()],
        Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
        _ => {
            return Err(format!(
                "{location}: unsupported `type` declaration {expected}"
            ));
        }
    };
    let mut matched = false;
    for name in &names {
        matched = match *name {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            other => {
                return Err(format!("{location}: unsupported `type` value `{other}`"));
            }
        };
        if matched {
            break;
        }
    }
    if matched {
        Ok(())
    } else {
        Err(format!(
            "{location}: {value} is not of type {}",
            names.join(" or ")
        ))
    }
}

fn check_object(
    object: &Map<String, Value>,
    schema: &Value,
    root: &Value,
    location: &str,
) -> Result<(), String> {
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{location}: schema declares no properties"))?;
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{location}: schema declares no required list"))?
    {
        let field = required
            .as_str()
            .ok_or_else(|| format!("{location}: schema required entries must be strings"))?;
        if !object.contains_key(field) {
            return Err(format!("{location}: missing required field `{field}`"));
        }
    }
    if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
        for field in object.keys() {
            if !properties.contains_key(field) {
                return Err(format!(
                    "{location}: emitted `{field}`, which the published schema does not declare"
                ));
            }
        }
    }
    for (field, declared) in properties {
        if let Some(emitted) = object.get(field) {
            check(emitted, declared, root, &format!("{location}.{field}"))?;
        }
    }
    Ok(())
}

/// Every keyword the published doctor schema uses, so a future keyword cannot
/// slip past `check` unnoticed.
const SUPPORTED_KEYWORDS: &[&str] = &[
    "$id",
    "$ref",
    "$schema",
    "additionalProperties",
    "const",
    "description",
    "enum",
    "items",
    "minLength",
    "minimum",
    "properties",
    "required",
    "title",
    "type",
];

fn unsupported_keywords(schema: &Value, found: &mut Vec<String>) {
    // Only schema positions are walked. A `properties` map holds property
    // *names* alongside schemas, so recursing into it blindly would report
    // every field name as an unhandled keyword.
    for keyword in ["properties", "$defs"] {
        if let Some(named) = schema.get(keyword).and_then(Value::as_object) {
            for nested in named.values() {
                unsupported_keywords(nested, found);
            }
        }
    }
    for keyword in ["items", "additionalProperties"] {
        if let Some(nested) = schema.get(keyword) {
            unsupported_keywords(nested, found);
        }
    }
    for keyword in schema
        .as_object()
        .map(|object| object.keys())
        .into_iter()
        .flatten()
    {
        if matches!(
            keyword.as_str(),
            "properties" | "$defs" | "items" | "additionalProperties"
        ) {
            continue;
        }
        if !SUPPORTED_KEYWORDS.contains(&keyword.as_str()) {
            found.push(keyword.clone());
        }
    }
}

#[test]
fn the_checker_covers_every_keyword_the_published_schema_uses() -> Result<(), String> {
    let schema = published_schema()?;
    let mut found = Vec::new();
    unsupported_keywords(&schema, &mut found);
    if !found.is_empty() {
        return Err(format!(
            "the conformance checker ignores these published keywords, so it would under-check the schema: {found:?}"
        ));
    }
    Ok(())
}

/// The production document must satisfy the published contract: every field it
/// emits is declared, every declared requirement it carries, and every pinned
/// value it matches. This is the narrower authority named for
/// `schemas/ripr/doctor.schema.json`.
#[test]
fn doctor_json_conforms_to_the_published_schema() -> Result<(), String> {
    let schema = published_schema()?;
    let root = fixture_root("conforms")?;
    let document = doctor_json(&root);
    let _ = std::fs::remove_dir_all(&root);
    let document = document?;

    check(&document, &schema, &schema, "doctor")?;

    // The conformance helper must not pass vacuously: the emitted document has
    // to actually exercise the newly typed fields, or this proves only that a
    // sparse root conforms.
    for field in [
        "detected_languages",
        "unanalyzed_source_languages",
        "preview_language_gaps",
        "config_defaults",
        "cache",
        "test_surfaces",
        "perl_preview",
        "binary",
    ] {
        if document.get(field).is_none_or(Value::is_null) {
            return Err(format!(
                "the fixture root must exercise `{field}`, or this contract proves nothing: {document}"
            ));
        }
    }
    Ok(())
}

/// Assert the published schema **rejects** `document`. Each caller names the
/// shape it mutated, so a failure says which negative stopped being enforced.
fn assert_rejected(label: &str, document: &Value, schema: &Value) -> Result<(), String> {
    match check(document, schema, schema, "doctor") {
        Ok(()) => Err(format!(
            "the published doctor schema accepted a document the contract forbids: {label}"
        )),
        Err(error) => {
            println!("rejected as expected ({label}): {error}");
            Ok(())
        }
    }
}

/// The negative experiment for the conformance helper. Without it a checker
/// that accepted everything would still report every real document as
/// conformant, and each case below is a shape this change could plausibly have
/// shipped.
#[test]
fn published_doctor_schema_rejects_documents_the_schema_forbids() -> Result<(), String> {
    let schema = published_schema()?;
    let root = fixture_root("negative")?;
    let base = doctor_json(&root);
    let _ = std::fs::remove_dir_all(&root);
    let base = base?;

    // The positive control first: an unmutated emitted document conforms.
    check(&base, &schema, &schema, "doctor")
        .map_err(|error| format!("a conformant document was rejected: {error}"))?;

    // The field schema 0.3 published and 0.4 removed.
    let mut resurrected_sections = base.clone();
    resurrected_sections["sections"] = Value::Array(Vec::new());
    assert_rejected("resurrected sections array", &resurrected_sections, &schema)?;

    let mut unknown_field = base.clone();
    unknown_field["detected"] = Value::Array(Vec::new());
    assert_rejected("undeclared top-level field", &unknown_field, &schema)?;

    let mut unpinned_version = base.clone();
    unpinned_version["schema_version"] = Value::String("0.3".to_string());
    assert_rejected("stale schema_version", &unpinned_version, &schema)?;

    let mut missing_required = base.clone();
    if let Some(object) = missing_required.as_object_mut() {
        object.remove("cache");
    }
    assert_rejected("missing required cache", &missing_required, &schema)?;

    let mut unlisted_check_status = base.clone();
    unlisted_check_status["checks"][0]["status"] = Value::String("maybe".to_string());
    assert_rejected("unlisted check status", &unlisted_check_status, &schema)?;

    let mut unknown_language_tier = base.clone();
    unknown_language_tier["detected_languages"][0]["status"] =
        Value::String("generally_supported".to_string());
    assert_rejected("unlisted language tier", &unknown_language_tier, &schema)?;

    let mut unknown_exporter_state = base.clone();
    unknown_exporter_state["perl_preview"]["exporter"]["state"] =
        Value::String("probably_works".to_string());
    assert_rejected("untyped exporter state", &unknown_exporter_state, &schema)?;

    let mut wrong_nested_type = base.clone();
    wrong_nested_type["cache"]["size_bytes"] = Value::String("0 B".to_string());
    assert_rejected("stringified byte count", &wrong_nested_type, &schema)?;

    let mut undeclared_nested_field = base.clone();
    undeclared_nested_field["cache"]["free_form_note"] = Value::String("hello".to_string());
    assert_rejected("undeclared nested field", &undeclared_nested_field, &schema)?;

    let mut wrong_test_surface_type = base.clone();
    wrong_test_surface_type["test_surfaces"] = Value::String("rust".to_string());
    assert_rejected(
        "stringified test_surfaces",
        &wrong_test_surface_type,
        &schema,
    )?;

    let mut conflated_enabled_language = base.clone();
    // The acceptance-1 trap as a schema question: a detected language entry
    // that omits whether the configuration enables it would leave a consumer
    // unable to tell detected-but-skipped from detected-and-analyzed.
    if let Some(object) = conflated_enabled_language["detected_languages"][0].as_object_mut() {
        object.remove("enabled");
    }
    assert_rejected(
        "detected language without its enabled state",
        &conflated_enabled_language,
        &schema,
    )?;

    // The positive counterpart: `perl_preview: null` stays legal, because "no
    // Perl project here" is a state the schema must admit rather than reject.
    let mut no_perl = base.clone();
    no_perl["perl_preview"] = Value::Null;
    check(&no_perl, &schema, &schema, "doctor")
        .map_err(|error| format!("an explicit null perl_preview must stay legal: {error}"))?;

    Ok(())
}
