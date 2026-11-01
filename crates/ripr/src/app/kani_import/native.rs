//! Non-executing parser for the pinned Kani RFC 0015 export profile (schema 0.1.0).
//!
//! Producer: model-checking/kani@744b3e84d6eb68fd5456826997568666791bd355.
//! Successful checks lack a complete exported property inventory. Native success
//! stays at harness scope, and COMPLETE describes accounting, not a proof verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};

use super::{MAX_NATIVE_BYTES, NativeDocument};
use crate::domain::external_verification_native::{
    NativeFailureKind, NativeHarness, NativeOutcome, NativeRun, NativeRunState, NativeVerdict,
};

const CHECK_LISTS: &[&str] = &["failure", "unreachable", "undetermined", "error", "unknown"];
const COVER_LISTS: &[&str] = &[
    "satisfied",
    "unsatisfiable",
    "unreachable",
    "undetermined",
    "error",
    "unknown",
];
const STATUSES: &[&str] = &[
    "SUCCESS",
    "FAILURE",
    "SATISFIED",
    "UNSATISFIABLE",
    "UNREACHABLE",
    "UNDETERMINED",
    "ERROR",
    "UNKNOWN",
    "COVERED",
    "UNCOVERED",
];

pub(super) fn parse_native(json: &str) -> Result<NativeDocument, String> {
    require(
        json.len() <= MAX_NATIVE_BYTES,
        "native export exceeds 16 MiB",
    )?;
    let mut decoder = serde_json::Deserializer::from_str(json);
    let document = UniqueValue::deserialize(&mut decoder)
        .map_err(|error| format!("invalid native JSON: {error}; see `ripr help`"))?
        .0;
    decoder
        .end()
        .map_err(|error| format!("invalid native JSON: {error}"))?;
    let root = object(&document, "native")?;
    let version = string(field(root, "schema_version")?, "schema_version")?;
    require(
        version == "0.1.0",
        "unsupported Kani schema_version (expected 0.1.0)",
    )?;
    let run_state: NativeRunState = typed(field(root, "run_state")?, "run_state")?;
    validate_run_fields(root)?;
    let selection = object(field(root, "harness_selection")?, "harness_selection")?;
    let requested = strings(field(selection, "requested_filters")?, "requested_filters")?;
    let unmatched = strings(field(selection, "unmatched_filters")?, "unmatched_filters")?;
    let exact = boolean(field(selection, "exact")?, "exact")?;
    let matched_count = count(field(selection, "matched_count")?, "matched_count")?;
    // metadata::determine_targets rejects unmatched exact filters and zero matches
    // before main::verify_project reaches write_export_json in the pinned producer.
    require(
        !exact || unmatched.is_empty(),
        "exact selection has unmatched_filters",
    )?;
    require(
        unmatched.iter().all(|item| requested.contains(item)),
        "unmatched filter was not requested",
    )?;
    require(
        requested.is_empty() || requested.iter().any(|item| !unmatched.contains(item)),
        "all requested filters are unmatched",
    )?;
    let mut harnesses = Vec::new();
    let mut keys = BTreeSet::new();
    for (index, entry) in array(field(root, "harnesses")?, "harnesses")?
        .iter()
        .enumerate()
    {
        let harness =
            validate_harness(entry).map_err(|error| format!("harnesses[{index}]: {error}"))?;
        require(
            keys.insert((harness.crate_name.clone(), harness.name.clone())),
            "duplicate (crate_name, name) harness identity",
        )?;
        harnesses.push(harness);
    }
    let imported = length(harnesses.len())?;
    match run_state {
        NativeRunState::Complete => require(
            matched_count > 0 && imported == matched_count,
            "COMPLETE requires nonempty matched_count equal to imported harnesses",
        )?,
        NativeRunState::Partial => require(
            matched_count > imported,
            "PARTIAL requires fewer imported than matched harnesses",
        )?,
        NativeRunState::NoHarnessesSelected => require(
            matched_count == 0 && imported == 0 && requested.is_empty() && unmatched.is_empty(),
            "NO_HARNESSES_SELECTED requires empty filters and subjects",
        )?,
    }
    if harnesses
        .iter()
        .any(|h| matches!(h.outcome, NativeOutcome::Timeout))
    {
        require(
            !field(root, "harness_timeout_s")?.is_null(),
            "TIMEOUT requires harness_timeout_s",
        )?;
    }
    validate_summary(root, &harnesses)?;
    Ok(NativeDocument {
        run: NativeRun {
            schema_version: version.to_owned(),
            run_state,
            target: string(field(root, "target")?, "target")?.to_owned(),
            matched_count,
            missing_count: matched_count
                .checked_sub(imported)
                .ok_or("invalid selected count; see `ripr help`")?,
            harnesses,
        },
        document,
    })
}

fn validate_run_fields(root: &Map<String, Value>) -> Result<(), String> {
    let commit = nullable_string(field(root, "kani_commit")?, "kani_commit")?;
    let dirty = field(root, "kani_commit_dirty")?;
    if !dirty.is_null() {
        boolean(dirty, "kani_commit_dirty")?;
    }
    require(
        commit.is_none() == dirty.is_null(),
        "kani_commit and dirty flag must be null together",
    )?;
    let tools = object(field(root, "tools")?, "tools")?;
    string(field(tools, "kani")?, "tools.kani")?;
    for key in ["rustc", "cbmc"] {
        nullable_string(field(tools, key)?, key)?;
    }
    strings(
        field(root, "enabled_unstable_features")?,
        "enabled_unstable_features",
    )?;
    duration(field(root, "harness_timeout_s")?, true, "harness_timeout_s")?;
    duration(field(root, "wall_time_s")?, false, "wall_time_s")?;
    validate_timestamp(string(field(root, "started_at")?, "started_at")?)?;
    let config = object(field(root, "configuration")?, "configuration")?;
    boolean(field(config, "coverage_enabled")?, "coverage_enabled")?;
    strings(field(config, "cbmc_args")?, "cbmc_args")?;
    let checks = object(field(config, "checks")?, "configuration.checks")?;
    for key in [
        "memory_safety",
        "overflow",
        "unwinding",
        "undefined_function",
        "assertion_reach_checks",
        "ignore_global_asm",
        "extra_pointer_checks",
        "assert_contracts",
        "prove_safety_only",
    ] {
        boolean(field(checks, key)?, key)?;
    }
    let outcome = object(field(root, "outcome")?, "outcome")?;
    require(
        string(field(outcome, "kind")?, "outcome.kind")? == "COMPLETED",
        "run outcome.kind must be COMPLETED",
    )?;
    for key in ["verdict", "code", "message"] {
        absent(outcome, key)?;
    }
    Ok(())
}

fn validate_harness(value: &Value) -> Result<NativeHarness, String> {
    let h = object(value, "harness")?;
    let name = string(field(h, "name")?, "name")?;
    let crate_name = string(field(h, "crate_name")?, "crate_name")?;
    let file = string(field(h, "file")?, "file")?;
    require(
        !name.is_empty() && !crate_name.is_empty() && !file.is_empty(),
        "empty harness identity",
    )?;
    let line = count(field(h, "line")?, "line")?;
    require(line > 0, "harness line must be positive")?;
    let generated = boolean(
        field(h, "is_automatically_generated")?,
        "is_automatically_generated",
    )?;
    let bounded = boolean(field(h, "is_bounded")?, "is_bounded")?;
    require(
        !bounded || generated,
        "is_bounded requires an automatically generated harness",
    )?;
    for key in ["has_loop_contracts", "generated_concrete_test"] {
        boolean(field(h, key)?, key)?;
    }
    nullable_string(field(h, "resolved_solver")?, "resolved_solver")?;
    nullable_count(field(h, "resolved_unwind")?, "resolved_unwind")?;
    let contract = field(h, "contract")?;
    if !contract.is_null() {
        let contract = object(contract, "contract")?;
        string(
            field(contract, "contracted_function_name")?,
            "contracted_function_name",
        )?;
        nullable_string(field(contract, "recursion_tracker")?, "recursion_tracker")?;
    }
    let attributes = object(field(h, "attributes")?, "attributes")?;
    validate_attributes(attributes)?;
    let should_panic = boolean(field(attributes, "should_panic")?, "should_panic")?;
    let resources = object(field(h, "resources")?, "resources")?;
    duration(
        field(resources, "verification_time_s")?,
        false,
        "verification_time_s",
    )?;
    validate_warnings(h)?;
    let outcome = object(field(h, "outcome")?, "outcome")?;
    let kind = string(field(outcome, "kind")?, "outcome.kind")?;
    require(
        matches!(kind, "COMPLETED" | "TIMEOUT" | "OUT_OF_MEMORY" | "CRASHED"),
        "unsupported native outcome.kind",
    )?;
    let completed = kind == "COMPLETED";
    let checks = validate_bucket(field(h, "checks")?, false, completed)?;
    let covers = validate_bucket(field(h, "covers")?, true, completed)?;
    require(
        checks.ids.keys().all(|id| !covers.ids.contains_key(id)),
        "property identity occurs in checks and covers",
    )?;
    let properties = nullable_count(field(h, "n_properties")?, "n_properties")?;
    let failed = nullable_count(field(h, "n_failed")?, "n_failed")?;
    let failure_records = array(field(h, "failed_properties")?, "failed_properties")?;
    let unsupported = array(
        field(h, "unsupported_constructs")?,
        "unsupported_constructs",
    )?;
    let native_outcome = match kind {
        "COMPLETED" => {
            absent(outcome, "code")?;
            absent(outcome, "message")?;
            let verdict: NativeVerdict = typed(field(outcome, "verdict")?, "verdict")?;
            let failure_kind: NativeFailureKind = typed(field(h, "failure_kind")?, "failure_kind")?;
            let passing = if should_panic {
                NativeFailureKind::PanicsOnly
            } else {
                NativeFailureKind::None
            };
            require(
                (verdict == NativeVerdict::Success) == (failure_kind == passing),
                "verdict contradicts failure_kind and should_panic",
            )?;
            let total = sum([
                checks
                    .total
                    .ok_or("missing checks.total; see `ripr help`")?,
                covers
                    .total
                    .ok_or("missing covers.total; see `ripr help`")?,
            ])?;
            require(
                properties == Some(total),
                "n_properties does not match checks plus covers",
            )?;
            let failed_ids: BTreeSet<&str> = checks
                .ids
                .iter()
                .filter(|(_, status)| status.as_str() == "FAILURE")
                .map(|(id, _)| id.as_str())
                .collect();
            require(
                failed == Some(length(failed_ids.len())?),
                "n_failed disagrees with checks.failure",
            )?;
            validate_properties(failure_records, unsupported, &checks, &covers, &failed_ids)?;
            let errors = checks
                .ids
                .values()
                .chain(covers.ids.values())
                .any(|s| s == "ERROR");
            let failures = checks
                .ids
                .values()
                .chain(covers.ids.values())
                .any(|s| s == "FAILURE");
            require(
                failure_kind != NativeFailureKind::None || (!errors && !failures),
                "failure_kind NONE contradicts failed or errored properties",
            )?;
            // call_cbmc::determine_failed_properties requires at least one failure,
            // all in the assertion class, for PANICS_ONLY. Assertion failures are
            // exported in failed_properties, unlike code_coverage properties.
            require(
                failure_kind != NativeFailureKind::PanicsOnly
                    || (!errors
                        && !failure_records.is_empty()
                        && failure_records
                            .iter()
                            .all(|p| p.get("class").and_then(Value::as_str) == Some("assertion"))
                        && covers.ids.values().all(|status| status != "FAILURE")),
                "PANICS_ONLY requires nonempty assertion failures without other failures or errors",
            )?;
            // ERROR may be an upstream override for ignored quantifiers without an ERROR property.
            NativeOutcome::Completed {
                verdict,
                failure_kind,
            }
        }
        "TIMEOUT" | "OUT_OF_MEMORY" | "CRASHED" => {
            absent(outcome, "verdict")?;
            absent(h, "failure_kind")?;
            require(
                properties.is_none()
                    && failed.is_none()
                    && failure_records.is_empty()
                    && unsupported.is_empty(),
                "non-completed harness carries property results",
            )?;
            if kind == "CRASHED" {
                let code = field(outcome, "code")?;
                let code = if code.is_null() {
                    None
                } else {
                    Some(
                        i32::try_from(
                            code.as_i64()
                                .ok_or("crash code must be an integer or null; see `ripr help`")?,
                        )
                        .map_err(|error| format!("crash code is outside i32: {error}"))?,
                    )
                };
                NativeOutcome::Crashed {
                    code,
                    message: nullable_string(field(outcome, "message")?, "message")?
                        .map(str::to_owned),
                }
            } else {
                absent(outcome, "code")?;
                absent(outcome, "message")?;
                if kind == "TIMEOUT" {
                    NativeOutcome::Timeout
                } else {
                    NativeOutcome::OutOfMemory
                }
            }
        }
        _ => {
            return Err(format!(
                "unsupported native outcome.kind: {kind:?}; see `ripr help`"
            ));
        }
    };
    Ok(NativeHarness {
        crate_name: crate_name.to_owned(),
        name: name.to_owned(),
        file: file.to_owned(),
        line,
        should_panic,
        outcome: native_outcome,
        checks_total: checks.total,
        checks_success: checks.success,
        covers_total: covers.total,
    })
}

fn validate_attributes(a: &Map<String, Value>) -> Result<(), String> {
    let kind = field(a, "kind")?;
    if let Some(kind) = kind.as_str() {
        require(
            matches!(kind, "Proof" | "Test"),
            "unsupported attributes.kind",
        )?;
    } else {
        let kind = object(kind, "attributes.kind")?;
        let contract = object(field(kind, "ProofForContract")?, "ProofForContract")?;
        string(field(contract, "target_fn")?, "target_fn")?;
    }
    nullable_count(field(a, "unwind_value")?, "unwind_value")?;
    let solver = field(a, "solver")?;
    if !solver.is_null() && !solver.is_string() {
        string(field(object(solver, "solver")?, "Binary")?, "solver.Binary")?;
    }
    for stub in array(field(a, "stubs")?, "stubs")? {
        let stub = object(stub, "stub")?;
        string(field(stub, "original")?, "stub.original")?;
        string(field(stub, "replacement")?, "stub.replacement")?;
    }
    strings(field(a, "verified_stubs")?, "verified_stubs")?;
    Ok(())
}

fn validate_warnings(h: &Map<String, Value>) -> Result<(), String> {
    count(field(h, "warnings_truncated")?, "warnings_truncated")?;
    for warning in array(field(h, "warnings")?, "warnings")? {
        let warning = object(warning, "warning")?;
        let message = string(field(warning, "message")?, "warning.message")?;
        let truncated = boolean(field(warning, "truncated")?, "warning.truncated")?;
        let original = nullable_count(field(warning, "original_chars")?, "warning.original_chars")?;
        require(
            truncated == original.is_some(),
            "warning truncation fields disagree",
        )?;
        if let Some(original) = original {
            require(
                original > length(message.chars().count())?,
                "truncated warning did not lose characters",
            )?;
        }
    }
    Ok(())
}

struct Bucket {
    total: Option<u64>,
    success: Option<u64>,
    ids: BTreeMap<String, String>,
}

fn validate_bucket(value: &Value, cover: bool, completed: bool) -> Result<Bucket, String> {
    let b = object(value, if cover { "covers" } else { "checks" })?;
    let total = nullable_count(field(b, "total")?, "bucket.total")?;
    let success = if cover {
        None
    } else {
        nullable_count(field(b, "success")?, "checks.success")?
    };
    require(
        total.is_some() == completed && (cover || success.is_some() == completed),
        "bucket counts must be integers on COMPLETED and null otherwise",
    )?;
    let lists = if cover { COVER_LISTS } else { CHECK_LISTS };
    let mut ids = BTreeMap::new();
    for key in lists {
        for id in strings(field(b, key)?, key)? {
            require(
                !id.is_empty()
                    && ids
                        .insert(id.to_owned(), key.to_ascii_uppercase())
                        .is_none(),
                "empty or duplicate property identity in bucket",
            )?;
        }
    }
    for other in array(field(b, "other")?, "bucket.other")? {
        let other = object(other, "other property")?;
        let id = string(field(other, "id")?, "other.id")?;
        let status = string(field(other, "status")?, "other.status")?;
        require(
            STATUSES.contains(&status),
            "unsupported native property status",
        )?;
        require(
            !lists.iter().any(|key| key.eq_ignore_ascii_case(status))
                && (cover || status != "SUCCESS"),
            "other status belongs in a named bucket",
        )?;
        require(
            !id.is_empty() && ids.insert(id.to_owned(), status.to_owned()).is_none(),
            "empty or duplicate property identity in bucket",
        )?;
    }
    if completed {
        require(
            total == Some(sum([success.unwrap_or(0), length(ids.len())?])?),
            "bucket total does not match its parts",
        )?;
    } else {
        require(
            ids.is_empty(),
            "non-completed harness has property bucket entries",
        )?;
    }
    Ok(Bucket {
        total,
        success,
        ids,
    })
}

fn validate_properties(
    failed: &[Value],
    unsupported: &[Value],
    checks: &Bucket,
    covers: &Bucket,
    failed_ids: &BTreeSet<&str>,
) -> Result<(), String> {
    let mut records = BTreeMap::new();
    for record in failed {
        let (id, status, class) = property(record)?;
        require(
            status == "FAILURE" && class != "cover" && class != "code_coverage",
            "failed_properties contains a non-failing check or cover",
        )?;
        require(
            records.insert(id, record).is_none(),
            "duplicate failed_properties identity",
        )?;
    }
    require(
        records.keys().copied().collect::<BTreeSet<_>>() == *failed_ids,
        "failed_properties identities differ from checks.failure",
    )?;
    let mut unsupported_ids = BTreeSet::new();
    let mut successes = 0_u64;
    for record in unsupported {
        let (id, status, class) = property(record)?;
        require(
            class == "unsupported_construct",
            "unsupported_constructs has the wrong class",
        )?;
        require(
            unsupported_ids.insert(id),
            "duplicate unsupported_construct identity",
        )?;
        require(
            !covers.ids.contains_key(id),
            "unsupported check identity occurs in covers",
        )?;
        if status == "SUCCESS" {
            require(
                !checks.ids.contains_key(id),
                "successful unsupported check has a conflicting status",
            )?;
            successes = sum([successes, 1])?;
        } else {
            require(
                checks.ids.get(id).map(String::as_str) == Some(status),
                "unsupported construct disagrees with its check status",
            )?;
            if status == "FAILURE" {
                let failed_record = records
                    .get(id)
                    .ok_or("unsupported failure missing from failed_properties; see `ripr help`")?;
                for key in [
                    "id",
                    "status",
                    "class",
                    "description",
                    "file",
                    "line",
                    "trace_available",
                ] {
                    require(
                        failed_record.get(key) == record.get(key),
                        "unsupported failure records disagree",
                    )?;
                }
            }
        }
    }
    require(
        successes
            <= checks
                .success
                .ok_or("missing successful check count; see `ripr help`")?,
        "unsupported successes exceed checks.success",
    )?;
    require(
        records.iter().all(|(id, record)| {
            record.get("class").and_then(Value::as_str) != Some("unsupported_construct")
                || unsupported_ids.contains(id)
        }),
        "unsupported failure missing from unsupported_constructs",
    )?;
    Ok(())
}

fn property(value: &Value) -> Result<(&str, &str, &str), String> {
    let p = object(value, "property")?;
    let id = string(field(p, "id")?, "property.id")?;
    require(!id.is_empty(), "empty property identity")?;
    let status = string(field(p, "status")?, "property.status")?;
    require(
        STATUSES.contains(&status),
        "unsupported native property status",
    )?;
    let class = string(field(p, "class")?, "property.class")?;
    string(field(p, "description")?, "property.description")?;
    nullable_string(field(p, "file")?, "property.file")?;
    nullable_string(field(p, "line")?, "property.line")?;
    boolean(field(p, "trace_available")?, "property.trace_available")?;
    Ok((id, status, class))
}

fn validate_summary(root: &Map<String, Value>, harnesses: &[NativeHarness]) -> Result<(), String> {
    let summary = object(field(root, "summary")?, "summary")?;
    let verdict_count = |expected| {
        harnesses
            .iter()
            .filter(|h| {
                matches!(h.outcome, NativeOutcome::Completed { verdict, .. } if verdict == expected)
            })
            .count()
    };
    let covers = array(field(root, "harnesses")?, "harnesses")?;
    let mut satisfied = 0;
    for (raw, h) in covers.iter().zip(harnesses) {
        if h.covers_total.is_some() {
            let h = object(raw, "harness")?;
            let b = object(field(h, "covers")?, "covers")?;
            satisfied = sum([
                satisfied,
                length(array(field(b, "satisfied")?, "covers.satisfied")?.len())?,
            ])?;
        }
    }
    for (key, expected) in [
        ("total", length(harnesses.len())?),
        ("successful", length(verdict_count(NativeVerdict::Success))?),
        ("failed", length(verdict_count(NativeVerdict::Failure))?),
        (
            "checks_total",
            sum(harnesses.iter().filter_map(|h| h.checks_total))?,
        ),
        (
            "checks_success",
            sum(harnesses.iter().filter_map(|h| h.checks_success))?,
        ),
        (
            "covers_total",
            sum(harnesses.iter().filter_map(|h| h.covers_total))?,
        ),
        ("covers_satisfied", satisfied),
    ] {
        require(
            count(field(summary, key)?, key)? == expected,
            &format!("summary.{key} disagrees with harnesses"),
        )?;
    }
    Ok(())
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}
fn object<'a>(v: &'a Value, path: &str) -> Result<&'a Map<String, Value>, String> {
    v.as_object()
        .ok_or_else(|| format!("{path} must be an object; see `ripr help`"))
}
fn field<'a>(v: &'a Map<String, Value>, key: &str) -> Result<&'a Value, String> {
    v.get(key)
        .ok_or_else(|| format!("missing required field {key}; see `ripr help`"))
}
fn absent(v: &Map<String, Value>, key: &str) -> Result<(), String> {
    require(
        !v.contains_key(key),
        &format!("unexpected {key} for this outcome"),
    )
}
fn string<'a>(v: &'a Value, path: &str) -> Result<&'a str, String> {
    v.as_str()
        .ok_or_else(|| format!("{path} must be a string; see `ripr help`"))
}
fn nullable_string<'a>(v: &'a Value, path: &str) -> Result<Option<&'a str>, String> {
    if v.is_null() {
        Ok(None)
    } else {
        string(v, path).map(Some)
    }
}
fn count(v: &Value, path: &str) -> Result<u64, String> {
    v.as_u64()
        .ok_or_else(|| format!("{path} must be a nonnegative integer; see `ripr help`"))
}
fn nullable_count(v: &Value, path: &str) -> Result<Option<u64>, String> {
    if v.is_null() {
        Ok(None)
    } else {
        count(v, path).map(Some)
    }
}
fn boolean(v: &Value, path: &str) -> Result<bool, String> {
    v.as_bool()
        .ok_or_else(|| format!("{path} must be a boolean; see `ripr help`"))
}
fn array<'a>(v: &'a Value, path: &str) -> Result<&'a Vec<Value>, String> {
    v.as_array()
        .ok_or_else(|| format!("{path} must be an array; see `ripr help`"))
}
fn strings<'a>(v: &'a Value, path: &str) -> Result<Vec<&'a str>, String> {
    array(v, path)?.iter().map(|v| string(v, path)).collect()
}
fn typed<T: DeserializeOwned>(v: &Value, path: &str) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|error| format!("{path}: {error}"))
}
fn length(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|error| format!("count overflow: {error}"))
}
fn sum(values: impl IntoIterator<Item = u64>) -> Result<u64, String> {
    values.into_iter().try_fold(0_u64, |a, b| {
        a.checked_add(b).ok_or_else(|| "count overflow".to_owned())
    })
}
fn duration(v: &Value, nullable: bool, path: &str) -> Result<(), String> {
    if nullable && v.is_null() {
        return Ok(());
    }
    let n = v
        .as_f64()
        .ok_or_else(|| format!("{path} must be a number; see `ripr help`"))?;
    require(
        n.is_finite() && n >= 0.0,
        &format!("{path} must be nonnegative and finite"),
    )
}
fn validate_timestamp(value: &str) -> Result<(), String> {
    let b = value.as_bytes();
    require(
        b.len() == 20,
        "started_at must have YYYY-MM-DDTHH:MM:SSZ shape",
    )?;
    for (i, byte) in b.iter().enumerate() {
        let separator = match i {
            4 | 7 => Some(b'-'),
            10 => Some(b'T'),
            13 | 16 => Some(b':'),
            19 => Some(b'Z'),
            _ => None,
        };
        require(
            separator.map_or_else(|| byte.is_ascii_digit(), |s| *byte == s),
            "invalid started_at",
        )?;
    }
    let n = |a: usize, z: usize| {
        value[a..z]
            .parse::<u32>()
            .map_err(|error| format!("invalid started_at: {error}; see `ripr help`"))
    };
    let (year, month, day) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 => 28 + u32::from(leap_year),
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    require(
        year > 0 && day > 0 && day <= days && n(11, 13)? < 24 && n(14, 16)? < 60 && n(17, 19)? < 60,
        "started_at is not a real UTC date and time",
    )
}

// Value normally overwrites duplicate JSON object keys. Reject them before validation.
struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = Value;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
                Ok(Value::Number(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
                Ok(Value::Number(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
                Number::from_f64(v)
                    .map(Value::Number)
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Value, E> {
                Ok(Value::String(v.to_owned()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = access.next_element::<UniqueValue>()? {
                    values.push(v.0);
                }
                Ok(Value::Array(values))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
                let mut values = Map::new();
                while let Some(key) = access.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key {key:?}"
                        )));
                    }
                    let _previous = values.insert(key, access.next_value::<UniqueValue>()?.0);
                }
                Ok(Value::Object(values))
            }
        }
        decoder.deserialize_any(UniqueVisitor).map(Self)
    }
}

#[cfg(test)]
mod tests;
