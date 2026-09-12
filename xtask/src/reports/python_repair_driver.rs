//! Driver binding validation for the governed Python repair-trust corpus —
//! `cargo xtask python-repair-trust check-driver` (RIPR-SPEC-0176, #3569).
//!
//! The two-phase external-edit driver (#2443) binds one durable repair
//! attempt (#2927) to one accepted selection row (#3568) by digests and
//! retains a typed binding record inside the attempt. This validator is the
//! offline promotion-side counterpart: given the accepted selection manifest
//! and a set of retained driver binding records, it re-checks every digest
//! anchor and claim boundary the records assert, so a promotion pass can
//! trust the retained evidence without re-running the driver.
//!
//! What the check enforces, fail closed:
//!
//! - the record binds to the ACCEPTED selection manifest by exact-bytes
//!   sha256 — a changed manifest makes every retained record stale;
//! - the record binds one selection row by its recomputed canonical
//!   `selection_digest` over the retained preimage — a replaced or edited row
//!   makes the record stale;
//! - the record's target identity agrees with the row (`target_path`,
//!   `target_state`); an attempt is never fabricated or substituted by name;
//! - the driver issued the binding only under explicit operator/agent
//!   authorization (`status: granted`, named authority, the
//!   explicit-operator-flags method) — an inferred or absent authorization
//!   fails;
//! - the standing non-claims ride on every record: the driver claims no
//!   verification result, no static movement, and no closure. Records carry
//!   no lifecycle, movement, or execution fields at all (deny-unknown), so a
//!   driver record can never appear completed;
//! - apply-phase records additionally carry the durable attempt identity
//!   (#2927 shape), the prepare-record digest chain, the patch digest, the
//!   changed-file set, the edit-cage decision, and the resulting head;
//! - production/generated/vendor/environment edit surfaces fail: a record
//!   whose target falls under a denied surface prefix is rejected even when
//!   the row declared it `unsafe`, because the driver binds only test-only,
//!   existing, unambiguous targets.
//!
//! Verdict vocabulary: `valid`/`inconsistent`/`not_run` (no records
//! supplied). None of the three is a support-tier, repair-correctness, gate,
//! badge, or promotion claim; #3570 owns the verification phase.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};

use super::python_repair_trust::{
    KNOWN_SPEC, SelectionManifest, canonical_selection_digest, check_git_sha, check_portable_path,
    check_sha256_digest, known_value_or_fail, load_strict_json, opt_string, reject_secret_tokens,
    reject_unknown_keys, require_string, validate_selection_manifest,
};

const RERUN_COMMAND: &str = "cargo xtask python-repair-trust check-driver";
const CHECK_REPORT_JSON: &str = "python-repair-driver-check.json";
const CHECK_REPORT_MD: &str = "python-repair-driver-check.md";

/// The record schema mirrors the driver's binding record exactly. The crate
/// and xtask share the schema by construction, not by a shared dependency:
/// the digest anchors are the contract.
const BINDING_KIND: &str = "python_repair_driver_binding";
const BINDING_SPEC: &str = KNOWN_SPEC;
const BINDING_SCHEMA_VERSION: &str = "0.1";

const RECORD_KEYS_PREPARE: [&str; 14] = [
    "schema_version",
    "kind",
    "spec",
    "phase",
    "seam_id",
    "repository_head",
    "selection_manifest_path",
    "driver",
    "config",
    "input",
    "trust",
    "edit_surface",
    "authorization",
    "non_claims",
];

const RECORD_KEYS_APPLY: [&str; 17] = [
    "schema_version",
    "kind",
    "spec",
    "phase",
    "seam_id",
    "repository_head",
    "selection_manifest_path",
    "durable_attempt_id",
    "binding_artifact_sha256",
    "driver",
    "config",
    "input",
    "trust",
    "edit_surface",
    "authorization",
    "apply",
    "non_claims",
];

const TRUST_KEYS: [&str; 18] = [
    "attempt_id",
    "case_id",
    "subject_id",
    "repository",
    "base",
    "head",
    "tree",
    "source_currentness",
    "selection_manifest_sha256",
    "selection_digest",
    "target_path",
    "target_state",
    "family",
    "owner",
    "discriminator",
    "relation",
    "oracle",
    "limitation",
];

const DRIVER_KEYS: [&str; 2] = ["binary_sha256", "version"];
const CONFIG_KEYS: [&str; 1] = ["profile"];
const INPUT_KEYS: [&str; 2] = ["packet_sha256", "before_snapshot_sha256"];
const EDIT_SURFACE_KEYS: [&str; 2] = ["allowed", "forbidden"];
const AUTHORIZATION_KEYS: [&str; 3] = ["status", "authority", "method"];
const APPLY_KEYS: [&str; 5] = [
    "patch_sha256",
    "changed_paths",
    "cage_status",
    "repository_head_after",
    "current",
];

const CAGE_STATUSES: [&str; 3] = ["compliant", "violated", "incomparable"];
const AUTHORIZATION_STATUSES: [&str; 1] = ["granted"];
const AUTHORIZATION_METHODS: [&str; 1] = ["explicit-operator-flags"];

/// The standing non-claims every driver record must carry verbatim.
const BINDING_NON_CLAIMS: [&str; 3] = [
    "no verification result is claimed by the driver",
    "no static movement is claimed by the driver",
    "no closure is claimed by the driver",
];

/// Denied edit-surface prefixes, mirroring the corpus vocabulary in
/// `python_repair_trust` (shared invariant, local matcher by design).
const DENIED_SURFACE_PREFIXES: [&str; 14] = [
    "target/",
    "dist/",
    "build/",
    "vendor/",
    "vendored/",
    "node_modules/",
    "generated/",
    "__pycache__/",
    "site-packages/",
    ".venv/",
    "venv/",
    "env/",
    ".tox/",
    ".eggs/",
];

/// Fail-closed error text for the driver check: names subject/field/reason
/// plus the driver rerun command (the shared `fail` names the corpus check).
fn driver_fail(subject: &str, field: &str, reason: impl std::fmt::Display) -> String {
    format!(
        "python-repair-trust check-driver failed: subject=`{subject}` field=`{field}`: {reason}
rerun: {RERUN_COMMAND}"
    )
}

fn is_denied_edit_surface(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    DENIED_SURFACE_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
        || lowered
            .split('/')
            .any(|component| component.contains(".generated."))
}

/// The durable attempt identity shape (#2927): `repair-attempt-` plus 24
/// lowercase hexadecimal characters.
fn check_durable_attempt_id(subject: &str, field: &str, value: &str) -> Result<(), String> {
    let suffix = value.strip_prefix("repair-attempt-");
    let shaped = match suffix {
        Some(suffix) => {
            suffix.len() == 24
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        }
        None => false,
    };
    if shaped {
        Ok(())
    } else {
        Err(driver_fail(
            subject,
            field,
            "durable attempt id must be `repair-attempt-` plus 24 lowercase hexadecimal characters",
        ))
    }
}

/// One validated record, reduced to what the report needs.
struct DriverBindingRecord {
    display: String,
    phase: String,
    trust_attempt_id: String,
    target_path: String,
}

struct DriverCheckOutcome {
    manifest_path: String,
    manifest: Option<SelectionManifest>,
    bindings_input: Option<String>,
    records: Vec<DriverBindingRecord>,
    violations: Vec<String>,
}

impl DriverCheckOutcome {
    fn verdict(&self) -> &'static str {
        if self.records.is_empty() {
            return "not_run";
        }
        if self.violations.is_empty() {
            "valid"
        } else {
            "inconsistent"
        }
    }
}

fn parse_check_driver_args(args: &[String]) -> Result<(String, String), String> {
    let mut manifest = super::python_repair_trust::DEFAULT_MANIFEST.to_string();
    let mut bindings: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--manifest" => {
                index += 1;
                manifest = args.get(index).cloned().ok_or_else(|| {
                    format!(
                        "python-repair-trust check-driver --manifest requires a value\nrerun: {RERUN_COMMAND}"
                    )
                })?;
            }
            "--bindings" => {
                index += 1;
                bindings = Some(args.get(index).cloned().ok_or_else(|| {
                    format!(
                        "python-repair-trust check-driver --bindings requires a value\nrerun: {RERUN_COMMAND}"
                    )
                })?);
            }
            other => {
                return Err(format!(
                    "unknown python-repair-trust check-driver argument: {other}\nusage: cargo xtask python-repair-trust check-driver [--manifest <path>] --bindings <dir-or-file>\nrerun: {RERUN_COMMAND}"
                ));
            }
        }
        index += 1;
    }
    let bindings = bindings.ok_or_else(|| {
        format!(
            "python-repair-trust check-driver requires --bindings <dir-or-file>\nrerun: {RERUN_COMMAND}"
        )
    })?;
    Ok((manifest, bindings))
}

/// The command entry: `python-repair-trust check-driver`, dispatched from the
/// `python-repair-trust` subcommand router.
pub(crate) fn run_check_driver(args: &[String]) -> Result<(), String> {
    let (manifest_path, bindings_input) = parse_check_driver_args(args)?;
    let outcome = check_driver_artifacts(&manifest_path, &bindings_input)?;
    let verdict = outcome.verdict();

    match &outcome.manifest {
        None => println!(
            "python-repair-trust check-driver: manifest={manifest_path} selections=<none> (no accepted selection manifest; binding records cannot bind without one)"
        ),
        Some(manifest) => println!(
            "python-repair-trust check-driver: manifest={manifest_path} selections={} sha256={}",
            manifest.selections.len(),
            manifest.sha256
        ),
    }
    match &outcome.bindings_input {
        None => println!(
            "python-repair-trust check-driver: bindings=<none> verdict={verdict} (no records supplied; not_run is not a pass)"
        ),
        Some(input) => println!(
            "python-repair-trust check-driver: bindings={input} records={} violations={} verdict={verdict}",
            outcome.records.len(),
            outcome.violations.len()
        ),
    }
    for violation in &outcome.violations {
        println!("  violation: {violation}");
    }
    println!(
        "python-repair-trust check-driver verdict: {verdict} — binding structural validation only; no completed or correct repair is established and no verification phase is claimed"
    );
    println!("rerun: {RERUN_COMMAND}");

    let report = render_check_driver_json(&outcome)?;
    crate::write_report(CHECK_REPORT_JSON, &format!("{report}\n"))?;
    let markdown = render_check_driver_markdown(&outcome);
    crate::write_report(CHECK_REPORT_MD, &markdown)?;
    if verdict == "inconsistent" {
        return Err(format!(
            "python-repair-trust check-driver found {} violation(s); see target/ripr/reports/{CHECK_REPORT_JSON}",
            outcome.violations.len()
        ));
    }
    Ok(())
}

fn check_driver_artifacts(
    manifest_path: &str,
    bindings_input: &str,
) -> Result<DriverCheckOutcome, String> {
    if !Path::new(manifest_path).exists() {
        return Err(driver_fail(
            manifest_path,
            "manifest",
            "driver binding records bind to an accepted selection manifest; no manifest exists at this path",
        ));
    }
    let (value, manifest_sha256) = load_strict_json(manifest_path)?;
    let manifest = validate_selection_manifest(&value, manifest_sha256.clone())?;

    let path = Path::new(bindings_input);
    if !path.exists() {
        return Err(driver_fail(
            bindings_input,
            "bindings",
            "bindings input path does not exist",
        ));
    }
    let files = if path.is_dir() {
        let mut entries = Vec::new();
        let read = std::fs::read_dir(path).map_err(|error| {
            driver_fail(
                bindings_input,
                "bindings",
                format!("failed to read directory: {error}"),
            )
        })?;
        for entry in read {
            let entry = entry.map_err(|error| {
                driver_fail(
                    bindings_input,
                    "bindings",
                    format!("failed to read directory entry: {error}"),
                )
            })?;
            let name = entry.file_name().to_string_lossy().to_string();
            let full = entry.path();
            if full.is_file() && name.ends_with(".json") {
                entries.push(full.to_string_lossy().to_string());
            }
        }
        entries.sort();
        entries
    } else {
        vec![bindings_input.to_string()]
    };

    let mut records = Vec::new();
    let mut violations = Vec::new();
    for display in &files {
        match load_strict_json(display) {
            Ok((record, _sha)) => {
                match validate_binding_record(display, &record, &manifest, &manifest_sha256) {
                    Ok(record) => records.push(record),
                    Err(violation) => violations.push(violation),
                }
            }
            Err(error) => violations.push(error),
        }
    }
    Ok(DriverCheckOutcome {
        manifest_path: manifest_path.to_string(),
        manifest: Some(manifest),
        bindings_input: Some(bindings_input.to_string()),
        records,
        violations,
    })
}

/// Validates one driver binding record against the accepted selection
/// manifest. Every digest anchor is recomputed, every closed field set is
/// deny-unknown, and every claim the record could overstate is pinned by the
/// non-claims list.
fn validate_binding_record(
    display: &str,
    record: &Value,
    manifest: &SelectionManifest,
    manifest_sha256: &str,
) -> Result<DriverBindingRecord, String> {
    let top = record.as_object().ok_or_else(|| {
        driver_fail(
            display,
            "record",
            "driver binding record must be a JSON object",
        )
    })?;
    let phase_value = require_string(display, top, "phase")?;
    let apply_phase = match phase_value.as_str() {
        "prepare" => false,
        "apply" => true,
        other => {
            return Err(driver_fail(
                display,
                "phase",
                format!("unknown phase `{other}`; a driver record is `prepare` or `apply`"),
            ));
        }
    };
    let allowed_keys: &[&str] = if apply_phase {
        &RECORD_KEYS_APPLY
    } else {
        &RECORD_KEYS_PREPARE
    };
    reject_unknown_keys(top, allowed_keys, display, "driver binding record")?;
    for (field, expected) in [
        ("schema_version", BINDING_SCHEMA_VERSION),
        ("kind", BINDING_KIND),
        ("spec", BINDING_SPEC),
    ] {
        let actual = require_string(display, top, field)?;
        if actual != expected {
            return Err(driver_fail(
                display,
                field,
                format!("expected `{expected}`, got `{actual}`"),
            ));
        }
    }
    require_string(display, top, "seam_id")?;
    let repository_head = require_string(display, top, "repository_head")?;
    check_git_sha(display, "repository_head", &repository_head)?;
    require_string(display, top, "selection_manifest_path")?;

    if apply_phase {
        let durable = require_string(display, top, "durable_attempt_id")?;
        check_durable_attempt_id(display, "durable_attempt_id", &durable)?;
        let binding_digest = require_string(display, top, "binding_artifact_sha256")?;
        check_sha256_digest(display, "binding_artifact_sha256", &binding_digest)?;
    }

    // Driver identity: the running binary digest and version, recorded from
    // the real producer, never a fabricated pin.
    let driver = match top.get("driver") {
        Some(Value::Object(map)) => map,
        _ => {
            return Err(driver_fail(
                display,
                "driver",
                "`driver` identity block must be an object",
            ));
        }
    };
    reject_unknown_keys(driver, &DRIVER_KEYS, display, "`driver` identity block")?;
    let driver_binary = require_string(display, driver, "binary_sha256")?;
    check_sha256_digest(display, "driver.binary_sha256", &driver_binary)?;
    require_string(display, driver, "version")?;

    let config = match top.get("config") {
        Some(Value::Object(map)) => map,
        _ => {
            return Err(driver_fail(
                display,
                "config",
                "`config` identity block must be an object",
            ));
        }
    };
    reject_unknown_keys(config, &CONFIG_KEYS, display, "`config` identity block")?;
    require_string(display, config, "profile")?;

    let input = match top.get("input") {
        Some(Value::Object(map)) => map,
        _ => {
            return Err(driver_fail(
                display,
                "input",
                "`input` identity block must be an object",
            ));
        }
    };
    reject_unknown_keys(input, &INPUT_KEYS, display, "`input` identity block")?;
    let packet_digest = require_string(display, input, "packet_sha256")?;
    check_sha256_digest(display, "input.packet_sha256", &packet_digest)?;
    let before_digest = require_string(display, input, "before_snapshot_sha256")?;
    check_sha256_digest(display, "input.before_snapshot_sha256", &before_digest)?;

    // Trust block: the digest anchors and the identity agreement with the
    // accepted selection row.
    let trust = match top.get("trust") {
        Some(Value::Object(map)) => map,
        _ => {
            return Err(driver_fail(
                display,
                "trust",
                "`trust` identity block must be an object",
            ));
        }
    };
    reject_unknown_keys(trust, &TRUST_KEYS, display, "`trust` identity block")?;
    let trust_attempt_id = require_string(display, trust, "attempt_id")?;
    let selection = manifest
        .selections
        .iter()
        .find(|selection| selection.attempt_id == trust_attempt_id)
        .ok_or_else(|| {
            driver_fail(
                &trust_attempt_id,
                "trust.attempt_id",
                "names an attempt identity outside the accepted selection denominator; selected rows cannot be substituted by name",
            )
        })?;
    for field in [
        "case_id",
        "subject_id",
        "repository",
        "family",
        "owner",
        "discriminator",
        "relation",
        "oracle",
    ] {
        require_string(display, trust, field)?;
    }
    let base = require_string(display, trust, "base")?;
    check_git_sha(display, "trust.base", &base)?;
    let head = require_string(display, trust, "head")?;
    check_git_sha(display, "trust.head", &head)?;
    if let Some(tree) = opt_string(display, trust, "tree")? {
        check_sha256_digest(display, "trust.tree", &tree)?;
    }
    if let Some(currentness) = opt_string(display, trust, "source_currentness")? {
        known_value_or_fail(
            display,
            "trust.source_currentness",
            &currentness,
            &super::python_repair_trust::SOURCE_CURRENTNESS,
            "source-currentness disposition",
        )?;
    }
    if let Some(limitation) = opt_string(display, trust, "limitation")?
        && limitation.trim().is_empty()
    {
        return Err(driver_fail(
            display,
            "trust.limitation",
            "must be non-empty when present",
        ));
    }
    let recorded_manifest_digest = require_string(display, trust, "selection_manifest_sha256")?;
    check_sha256_digest(
        display,
        "trust.selection_manifest_sha256",
        &recorded_manifest_digest,
    )?;
    if recorded_manifest_digest != manifest_sha256 {
        return Err(driver_fail(
            &trust_attempt_id,
            "trust.selection_manifest_sha256",
            format!(
                "stale digest: the record binds to manifest {recorded_manifest_digest} but the accepted selection manifest is {manifest_sha256}"
            ),
        ));
    }
    let recorded_selection_digest = require_string(display, trust, "selection_digest")?;
    check_sha256_digest(
        display,
        "trust.selection_digest",
        &recorded_selection_digest,
    )?;
    let target_path = require_string(display, trust, "target_path")?;
    check_portable_path(display, "trust.target_path", &target_path)?;
    let target_state = require_string(display, trust, "target_state")?;
    known_value_or_fail(
        display,
        "trust.target_state",
        &target_state,
        &super::python_repair_trust::TARGET_STATES,
        "target state",
    )?;
    if target_path != selection.target_path || target_state != selection.target_state {
        return Err(driver_fail(
            &trust_attempt_id,
            "trust.target_path",
            format!(
                "target identity disagreement: the record names `{target_path}` (`{target_state}`) but the accepted selection row names `{}` (`{}`)",
                selection.target_path, selection.target_state
            ),
        ));
    }
    if is_denied_edit_surface(&target_path) {
        return Err(driver_fail(
            &trust_attempt_id,
            "trust.target_path",
            format!(
                "target `{target_path}` falls under a production/generated/vendor/environment edit surface; the driver binds only test-only targets"
            ),
        ));
    }
    // The row digest anchor: recompute the canonical content digest of the
    // retained preimage and require equality, so a replaced or edited
    // selected row fails the record that trusted it.
    let row_preimage = manifest
        .row_preimages
        .get(&trust_attempt_id)
        .ok_or_else(|| {
            driver_fail(
                &trust_attempt_id,
                "trust.selection_digest",
                "the accepted selection manifest retained no canonical preimage for this row",
            )
        })?;
    let recomputed = canonical_selection_digest(row_preimage, &trust_attempt_id)?;
    if recorded_selection_digest != recomputed {
        return Err(driver_fail(
            &trust_attempt_id,
            "trust.selection_digest",
            format!(
                "selection row digest mismatch: the record pins `{recorded_selection_digest}` but the accepted row now digests to `{recomputed}`; a replaced or edited selected row makes the binding stale"
            ),
        ));
    }

    // Edit surface: the declared cage, portable paths only.
    let edit_surface = match top.get("edit_surface") {
        Some(Value::Object(map)) => map,
        _ => {
            return Err(driver_fail(
                display,
                "edit_surface",
                "`edit_surface` identity block must be an object",
            ));
        }
    };
    reject_unknown_keys(
        edit_surface,
        &EDIT_SURFACE_KEYS,
        display,
        "`edit_surface` block",
    )?;
    let allowed_paths = match edit_surface.get("allowed") {
        Some(Value::Array(values)) => values,
        _ => {
            return Err(driver_fail(
                display,
                "edit_surface.allowed",
                "must be an array of paths",
            ));
        }
    };
    if allowed_paths.is_empty() {
        return Err(driver_fail(
            display,
            "edit_surface.allowed",
            "must name at least one allowed path",
        ));
    }
    for (index, value) in allowed_paths.iter().enumerate() {
        let path = value.as_str().ok_or_else(|| {
            driver_fail(
                display,
                &format!("edit_surface.allowed[{index}]"),
                "path must be a string",
            )
        })?;
        check_portable_path(display, &format!("edit_surface.allowed[{index}]"), path)?;
    }
    let forbidden_paths = match edit_surface.get("forbidden") {
        Some(Value::Array(values)) => values,
        _ => {
            return Err(driver_fail(
                display,
                "edit_surface.forbidden",
                "must be an array of paths",
            ));
        }
    };
    for (index, value) in forbidden_paths.iter().enumerate() {
        let path = value.as_str().ok_or_else(|| {
            driver_fail(
                display,
                &format!("edit_surface.forbidden[{index}]"),
                "path must be a string",
            )
        })?;
        check_portable_path(display, &format!("edit_surface.forbidden[{index}]"), path)?;
    }

    // Authorization: the driver applies no edit without the explicit pair,
    // and it never infers one — anything but the granted explicit record
    // fails.
    let authorization = match top.get("authorization") {
        Some(Value::Object(map)) => map,
        _ => {
            return Err(driver_fail(
                display,
                "authorization",
                "`authorization` block must be an object",
            ));
        }
    };
    reject_unknown_keys(
        authorization,
        &AUTHORIZATION_KEYS,
        display,
        "`authorization` block",
    )?;
    let status = require_string(display, authorization, "status")?;
    known_value_or_fail(
        display,
        "authorization.status",
        &status,
        &AUTHORIZATION_STATUSES,
        "authorization status",
    )?;
    require_string(display, authorization, "authority")?;
    let method = require_string(display, authorization, "method")?;
    known_value_or_fail(
        display,
        "authorization.method",
        &method,
        &AUTHORIZATION_METHODS,
        "authorization method",
    )?;

    // Non-claims: the standing claim boundary must ride on the record
    // verbatim.
    let non_claims = match top.get("non_claims") {
        Some(Value::Array(values)) => values,
        _ => {
            return Err(driver_fail(
                display,
                "non_claims",
                "must be an array of non-claim strings",
            ));
        }
    };
    let recorded_non_claims = non_claims
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    for expected in BINDING_NON_CLAIMS {
        if !recorded_non_claims.contains(expected) {
            return Err(driver_fail(
                display,
                "non_claims",
                format!("dropped the standing non-claim `{expected}`"),
            ));
        }
    }

    // Apply block: present exactly on apply-phase records, carrying the
    // applied-edit evidence and nothing beyond it — no verification verdict,
    // no movement, no closure.
    if apply_phase {
        let apply = match top.get("apply") {
            Some(Value::Object(map)) => map,
            _ => {
                return Err(driver_fail(
                    display,
                    "apply",
                    "apply records must carry the `apply` block",
                ));
            }
        };
        reject_unknown_keys(apply, &APPLY_KEYS, display, "`apply` block")?;
        let patch = require_string(display, apply, "patch_sha256")?;
        check_sha256_digest(display, "apply.patch_sha256", &patch)?;
        let changed_paths = match apply.get("changed_paths") {
            Some(Value::Array(values)) => values,
            _ => {
                return Err(driver_fail(
                    display,
                    "apply.changed_paths",
                    "must be an array of paths",
                ));
            }
        };
        for (index, value) in changed_paths.iter().enumerate() {
            let path = value.as_str().ok_or_else(|| {
                driver_fail(
                    display,
                    &format!("apply.changed_paths[{index}]"),
                    "path must be a string",
                )
            })?;
            check_portable_path(display, &format!("apply.changed_paths[{index}]"), path)?;
        }
        let cage_status = require_string(display, apply, "cage_status")?;
        known_value_or_fail(
            display,
            "apply.cage_status",
            &cage_status,
            &CAGE_STATUSES,
            "edit-cage decision",
        )?;
        let head_after = require_string(display, apply, "repository_head_after")?;
        check_git_sha(display, "apply.repository_head_after", &head_after)?;
        if !matches!(apply.get("current"), Some(Value::Bool(_))) {
            return Err(driver_fail(display, "apply.current", "must be a boolean"));
        }
    }

    let record_value = Value::Object(top.clone());
    reject_secret_tokens(&record_value, display, "binding record")?;

    Ok(DriverBindingRecord {
        display: display.to_string(),
        phase: phase_value,
        trust_attempt_id,
        target_path,
    })
}

fn render_check_driver_json(outcome: &DriverCheckOutcome) -> Result<String, String> {
    let verdict = outcome.verdict();
    let manifest_json = match &outcome.manifest {
        None => json!(null),
        Some(manifest) => json!({
            "path": outcome.manifest_path,
            "sha256": manifest.sha256,
            "selections": manifest.selections.len(),
        }),
    };
    let records: Vec<Value> = outcome
        .records
        .iter()
        .map(|record| {
            json!({
                "file": record.display,
                "phase": record.phase,
                "trust_attempt_id": record.trust_attempt_id,
                "target_path": record.target_path,
            })
        })
        .collect();
    let violations: Vec<Value> = outcome
        .violations
        .iter()
        .map(|violation| json!(violation))
        .collect();
    let document = json!({
        "schema_version": "0.1",
        "kind": "python_repair_driver_check_report",
        "spec": BINDING_SPEC,
        "verdict": verdict,
        "manifest": manifest_json,
        "bindings": {
            "input": outcome.bindings_input,
            "records": records,
            "violations": violations,
        },
        "offline": true,
        "claim_boundary": "binding structural validation only; no completed or correct repair is established and no verification phase is claimed",
        "rerun": RERUN_COMMAND,
    });
    serde_json::to_string_pretty(&document)
        .map_err(|error| format!("failed to render python-repair-trust check-driver JSON: {error}"))
}

fn render_check_driver_markdown(outcome: &DriverCheckOutcome) -> String {
    let verdict = outcome.verdict();
    let mut out = String::new();
    out.push_str("# Python Repair Driver Binding Check\n\n");
    out.push_str(&format!("Verdict: **{verdict}**\n\n"));
    out.push_str(
        "Binding structural validation only — no completed or correct repair is established and no verification phase is claimed.\n\n",
    );
    match &outcome.manifest {
        None => {
            out.push_str("- manifest: none (records cannot bind without an accepted manifest)\n")
        }
        Some(manifest) => out.push_str(&format!(
            "- manifest: {} ({} selections, sha256 `{}`)\n",
            outcome.manifest_path,
            manifest.selections.len(),
            manifest.sha256
        )),
    }
    match &outcome.bindings_input {
        None => out.push_str("- bindings: none supplied (not_run; not a pass)\n"),
        Some(input) => {
            out.push_str(&format!(
                "- bindings: {input} ({} record(s), {} violation(s))\n",
                outcome.records.len(),
                outcome.violations.len()
            ));
            for record in &outcome.records {
                out.push_str(&format!(
                    "  - {} phase={} trust attempt `{}` target `{}`\n",
                    record.display, record.phase, record.trust_attempt_id, record.target_path
                ));
            }
        }
    }
    if outcome.violations.is_empty() {
        out.push_str("\n## Violations\n\nNone.\n");
    } else {
        out.push_str("\n## Violations\n\n");
        for violation in &outcome.violations {
            out.push_str(&format!("- {violation}\n"));
        }
    }
    out.push_str(&format!("\nrerun: `{RERUN_COMMAND}`\n"));
    out
}

// ---------------------------------------------------------------------------
// Tests (module named `python_repair_driver_binding` under the file module
// `python_repair_driver`, so `cargo test -p xtask python_repair_driver`
// selects exactly these tests; no unwrap/expect — Result returns only)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod python_repair_driver_binding {
    use super::super::python_repair_trust::{MANIFEST_KIND, SCHEMA_VERSION};
    use super::*;
    use crate::python_judged_panel_replay::sha256_hex;

    const GIT_SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const GIT_SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const DIGEST_ONE: &str = "1010101010101010101010101010101010101010101010101010101010101010";
    const DIGEST_TWO: &str = "2020202020202020202020202020202020202020202020202020202020202020";

    /// A complete, well-formed selection row whose recorded `selection_digest`
    /// is recomputed from the content.
    fn selection_row(attempt_id: &str, target_path: &str, target_state: &str) -> Value {
        let mut row = json!({
            "attempt_id": attempt_id,
            "case_id": format!("case-{attempt_id}"),
            "subject_id": format!("subj-{attempt_id}"),
            "repository": "https://example.com/repo",
            "base": GIT_SHA_B,
            "head": GIT_SHA_C,
            "tree": DIGEST_ONE,
            "source_currentness": "candidate_current",
            "selection_reason": "behavior changed in the diff and the case discriminates it",
            "diversity_stratum": "pytest_library",
            "family": "error_path_gating",
            "owner": "module.handler",
            "discriminator": "raises ValueError on empty payload",
            "relation": "case calls owner directly",
            "oracle": "pytest.raises exact message pin",
            "expected_direction": "should_gap",
            "claim_boundary": "static exposure evidence only",
            "target_path": target_path,
            "target_state": target_state,
            "selected_at": "2026-09-10T00:00:00Z",
            "selector": "campaign-selector",
            "authority_snapshot_digest": DIGEST_TWO,
        });
        if let Some(entry) = row.as_object_mut() {
            let digest = canonical_selection_digest(entry, attempt_id).unwrap_or_default();
            entry.insert("selection_digest".to_string(), json!(digest));
        }
        row
    }

    struct Fixture {
        text: String,
        sha256: String,
        value: Value,
    }

    /// The accepted manifest for one row, with its exact-bytes digest.
    fn build_fixture(row: Value) -> Result<Fixture, String> {
        let manifest = json!({
            "schema_version": SCHEMA_VERSION,
            "kind": MANIFEST_KIND,
            "spec": KNOWN_SPEC,
            "description": "driver binding fixture",
            "selections": [row],
        });
        let text = serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("serialize fixture manifest: {error}"))?;
        let sha256 = sha256_hex(text.as_bytes());
        let value =
            serde_json::from_str(&text).map_err(|error| format!("reparse fixture: {error}"))?;
        Ok(Fixture {
            text,
            sha256,
            value,
        })
    }

    fn row_digest(fixture: &Fixture, attempt_id: &str) -> Result<String, String> {
        let row = fixture
            .value
            .get("selections")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .cloned()
            .ok_or("fixture manifest carries no row")?;
        let canonical = row.as_object().cloned().ok_or("row is not an object")?;
        canonical_selection_digest(&canonical, attempt_id)
    }

    fn prepare_record(
        fixture: &Fixture,
        attempt_id: &str,
        target_path: &str,
    ) -> Result<Value, String> {
        let record = json!({
            "schema_version": BINDING_SCHEMA_VERSION,
            "kind": BINDING_KIND,
            "spec": BINDING_SPEC,
            "phase": "prepare",
            "seam_id": "seam-under-test",
            "repository_head": GIT_SHA_C,
            "selection_manifest_path": "target/ripr/trust-manifest.json",
            "driver": {
                "binary_sha256": DIGEST_ONE,
                "version": "0.11.0",
            },
            "config": {
                "profile": "subject-ripr-toml",
            },
            "input": {
                "packet_sha256": DIGEST_ONE,
                "before_snapshot_sha256": DIGEST_TWO,
            },
            "trust": {
                "attempt_id": attempt_id,
                "case_id": format!("case-{attempt_id}"),
                "subject_id": format!("subj-{attempt_id}"),
                "repository": "https://example.com/repo",
                "base": GIT_SHA_B,
                "head": GIT_SHA_C,
                "selection_manifest_sha256": fixture.sha256,
                "selection_digest": row_digest(fixture, attempt_id)?,
                "target_path": target_path,
                "target_state": "existing",
                "family": "error_path_gating",
                "owner": "module.handler",
                "discriminator": "raises ValueError on empty payload",
                "relation": "case calls owner directly",
                "oracle": "pytest.raises exact message pin",
            },
            "edit_surface": {
                "allowed": [target_path],
                "forbidden": [],
            },
            "authorization": {
                "status": "granted",
                "authority": "operator-a",
                "method": "explicit-operator-flags",
            },
            "non_claims": BINDING_NON_CLAIMS,
        });
        Ok(record)
    }

    fn apply_record(
        fixture: &Fixture,
        attempt_id: &str,
        target_path: &str,
    ) -> Result<Value, String> {
        let mut record = prepare_record(fixture, attempt_id, target_path)?;
        let object = record
            .as_object_mut()
            .ok_or_else(|| "apply record is not an object".to_string())?;
        object.insert("phase".to_string(), json!("apply"));
        object.insert(
            "durable_attempt_id".to_string(),
            json!("repair-attempt-0123456789abcdef01234567"),
        );
        object.insert("binding_artifact_sha256".to_string(), json!(DIGEST_ONE));
        object.insert(
            "apply".to_string(),
            json!({
                "patch_sha256": DIGEST_TWO,
                "changed_paths": [target_path],
                "cage_status": "compliant",
                "repository_head_after": GIT_SHA_C,
                "current": true,
            }),
        );
        Ok(record)
    }

    /// Sets one field of the record's trust block.
    fn set_trust_field(record: &mut Value, field: &str, value: Value) {
        if let Some(object) = record.as_object_mut()
            && let Some(trust) = object.get_mut("trust").and_then(Value::as_object_mut)
        {
            trust.insert(field.to_string(), value);
        }
    }

    /// Sets (or removes) one field of the record's authorization block.
    fn set_authorization_field(record: &mut Value, field: &str, value: Option<Value>) {
        if let Some(object) = record.as_object_mut()
            && let Some(authorization) = object
                .get_mut("authorization")
                .and_then(Value::as_object_mut)
        {
            match value {
                Some(value) => {
                    authorization.insert(field.to_string(), value);
                }
                None => {
                    authorization.remove(field);
                }
            }
        }
    }

    fn checked(record: &Value, fixture: &Fixture) -> Result<DriverBindingRecord, String> {
        let manifest = validate_selection_manifest(&fixture.value, fixture.sha256.clone())?;
        validate_binding_record("record.json", record, &manifest, &fixture.sha256)
    }

    fn expect_rejection(record: &Value, fixture: &Fixture, needle: &str) -> Result<(), String> {
        match checked(record, fixture) {
            Err(error) if error.contains(needle) => Ok(()),
            Err(error) => Err(format!("rejection did not name `{needle}`: {error}")),
            Ok(_) => Err(format!("expected rejection containing `{needle}`, got Ok")),
        }
    }

    #[test]
    fn accepts_valid_prepare_record() -> Result<(), String> {
        let fixture = build_fixture(selection_row("att-ok", "tests/test_handler.py", "existing"))?;
        let record = prepare_record(&fixture, "att-ok", "tests/test_handler.py")?;
        checked(&record, &fixture)?;
        Ok(())
    }

    #[test]
    fn accepts_valid_apply_record() -> Result<(), String> {
        let fixture = build_fixture(selection_row("att-ok", "tests/test_handler.py", "existing"))?;
        let record = apply_record(&fixture, "att-ok", "tests/test_handler.py")?;
        checked(&record, &fixture)?;
        Ok(())
    }

    #[test]
    fn rejects_stale_manifest_digest() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-stale",
            "tests/test_handler.py",
            "existing",
        ))?;
        let mut record = apply_record(&fixture, "att-stale", "tests/test_handler.py")?;
        if let Some(object) = record.as_object_mut()
            && let Some(trust) = object.get_mut("trust").and_then(Value::as_object_mut)
        {
            trust.insert("selection_manifest_sha256".to_string(), json!(DIGEST_ONE));
        }
        expect_rejection(&record, &fixture, "stale digest")
    }

    #[test]
    fn rejects_unknown_trust_attempt() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-known",
            "tests/test_handler.py",
            "existing",
        ))?;
        let mut record = prepare_record(&fixture, "att-unknown", "tests/test_handler.py")?;
        if let Some(object) = record.as_object_mut()
            && let Some(trust) = object.get_mut("trust").and_then(Value::as_object_mut)
        {
            trust.insert("attempt_id".to_string(), json!("att-unknown"));
        }
        expect_rejection(
            &record,
            &fixture,
            "outside the accepted selection denominator",
        )
    }

    #[test]
    fn rejects_replaced_selection_row_digest() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-row",
            "tests/test_handler.py",
            "existing",
        ))?;
        let mut record = prepare_record(&fixture, "att-row", "tests/test_handler.py")?;
        if let Some(object) = record.as_object_mut()
            && let Some(trust) = object.get_mut("trust").and_then(Value::as_object_mut)
        {
            trust.insert("selection_digest".to_string(), json!(DIGEST_TWO));
        }
        expect_rejection(&record, &fixture, "selection row digest mismatch")
    }

    #[test]
    fn rejects_target_disagreement_with_the_row() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-target",
            "tests/test_handler.py",
            "existing",
        ))?;
        let mut record = prepare_record(&fixture, "att-target", "tests/other_test.py")?;
        set_trust_field(&mut record, "target_path", json!("tests/other_test.py"));
        expect_rejection(&record, &fixture, "target identity disagreement")
    }

    #[test]
    fn rejects_denied_edit_surface_target() -> Result<(), String> {
        // The row itself declares the denied surface `unsafe` (accepted by the
        // corpus), but the driver record that names it must still fail: the
        // driver binds only test-only targets.
        let fixture = build_fixture(selection_row("att-unsafe", "vendor/lib/ext.py", "unsafe"))?;
        let mut record = prepare_record(&fixture, "att-unsafe", "vendor/lib/ext.py")?;
        set_trust_field(&mut record, "target_state", json!("unsafe"));
        expect_rejection(
            &record,
            &fixture,
            "production/generated/vendor/environment edit surface",
        )
    }

    #[test]
    fn rejects_inferred_or_incomplete_authorization() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-auth",
            "tests/test_handler.py",
            "existing",
        ))?;
        for (field, value, needle) in [
            ("status", json!("inferred"), "unknown authorization status"),
            ("authority", json!(None::<String>), "required field"),
            ("method", json!("automatic"), "unknown authorization method"),
        ] {
            let mut record = prepare_record(&fixture, "att-auth", "tests/test_handler.py")?;
            let replacement = if value.is_null() {
                None
            } else {
                Some(value.clone())
            };
            set_authorization_field(&mut record, field, replacement);
            expect_rejection(&record, &fixture, needle)?;
        }
        Ok(())
    }

    #[test]
    fn rejects_missing_apply_block_on_apply_phase() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-apply",
            "tests/test_handler.py",
            "existing",
        ))?;
        let mut record = apply_record(&fixture, "att-apply", "tests/test_handler.py")?;
        if let Some(object) = record.as_object_mut() {
            object.remove("apply");
        }
        expect_rejection(&record, &fixture, "must carry the `apply` block")
    }

    #[test]
    fn rejects_malformed_durable_attempt_identity() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-durable",
            "tests/test_handler.py",
            "existing",
        ))?;
        let mut record = apply_record(&fixture, "att-durable", "tests/test_handler.py")?;
        if let Some(object) = record.as_object_mut() {
            object.insert("durable_attempt_id".to_string(), json!("my-attempt-1"));
        }
        expect_rejection(&record, &fixture, "durable attempt id must be")
    }

    #[test]
    fn rejects_malformed_apply_block() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-cage",
            "tests/test_handler.py",
            "existing",
        ))?;
        for (mutation, needle) in [
            ("cage", "unknown edit-cage decision"),
            ("patch", "must be bare lowercase sha256 hex"),
            ("head", "must be a bare lowercase 40-character"),
        ] {
            let mut record = apply_record(&fixture, "att-cage", "tests/test_handler.py")?;
            if let Some(object) = record.as_object_mut()
                && let Some(apply) = object.get_mut("apply").and_then(Value::as_object_mut)
            {
                match mutation {
                    "cage" => {
                        apply.insert("cage_status".to_string(), json!("looks_fine"));
                    }
                    "patch" => {
                        apply.insert("patch_sha256".to_string(), json!("sha256:deadbeef"));
                    }
                    _ => {
                        apply.insert("repository_head_after".to_string(), json!("deadbeef"));
                    }
                }
            }
            expect_rejection(&record, &fixture, needle)?;
        }
        Ok(())
    }

    #[test]
    fn rejects_dropped_non_claims() -> Result<(), String> {
        let fixture = build_fixture(selection_row(
            "att-claims",
            "tests/test_handler.py",
            "existing",
        ))?;
        let mut record = prepare_record(&fixture, "att-claims", "tests/test_handler.py")?;
        if let Some(object) = record.as_object_mut() {
            object.insert(
                "non_claims".to_string(),
                json!(["no verification result is claimed by the driver"]),
            );
        }
        expect_rejection(&record, &fixture, "dropped the standing non-claim")
    }

    #[test]
    fn rejects_lifecycle_vocabulary_on_driver_records() -> Result<(), String> {
        // A driver record can never appear completed: lifecycle, movement, and
        // execution fields are unknown fields and fail.
        let fixture = build_fixture(selection_row(
            "att-vocab",
            "tests/test_handler.py",
            "existing",
        ))?;
        for field in ["states", "movement", "execution"] {
            let mut record = prepare_record(&fixture, "att-vocab", "tests/test_handler.py")?;
            if let Some(object) = record.as_object_mut() {
                object.insert(field.to_string(), json!(["selected"]));
            }
            expect_rejection(&record, &fixture, "unknown field")?;
        }
        Ok(())
    }

    #[test]
    fn end_to_end_check_driver_over_files_and_directory() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("clock before epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-python-repair-driver-check-{}-{stamp}",
            std::process::id()
        ));
        let manifest_dir = root.join("corpus");
        let bindings_dir = root.join("bindings");
        std::fs::create_dir_all(&manifest_dir)
            .map_err(|error| format!("create corpus: {error}"))?;
        std::fs::create_dir_all(&bindings_dir)
            .map_err(|error| format!("create bindings: {error}"))?;

        let fixture = build_fixture(selection_row(
            "att-e2e",
            "tests/test_handler.py",
            "existing",
        ))?;
        let manifest_path = manifest_dir.join("manifest.json");
        std::fs::write(&manifest_path, &fixture.text)
            .map_err(|error| format!("write manifest: {error}"))?;
        let apply = apply_record(&fixture, "att-e2e", "tests/test_handler.py")?;
        let apply_text = serde_json::to_string_pretty(&apply)
            .map_err(|error| format!("serialize apply record: {error}"))?;
        std::fs::write(bindings_dir.join("apply.json"), &apply_text)
            .map_err(|error| format!("write apply record: {error}"))?;

        // The directory input validates every record and exits clean.
        run_check_driver(&args_of(
            manifest_path.to_string_lossy().as_ref(),
            bindings_dir.to_string_lossy().as_ref(),
        ))?;

        // A record that no longer matches the accepted manifest fails.
        let stale = json!({
            "schema_version": BINDING_SCHEMA_VERSION,
            "kind": BINDING_KIND,
            "spec": BINDING_SPEC,
            "phase": "prepare",
            "seam_id": "seam-under-test",
            "repository_head": GIT_SHA_C,
            "selection_manifest_path": "target/ripr/trust-manifest.json",
            "driver": {"binary_sha256": DIGEST_ONE, "version": "0.11.0"},
            "config": {"profile": "default"},
            "input": {"packet_sha256": DIGEST_ONE, "before_snapshot_sha256": DIGEST_TWO},
            "trust": {
                "attempt_id": "att-e2e",
                "case_id": "case-att-e2e",
                "subject_id": "subj-att-e2e",
                "repository": "https://example.com/repo",
                "base": GIT_SHA_B,
                "head": GIT_SHA_C,
                "selection_manifest_sha256": DIGEST_ONE,
                "selection_digest": DIGEST_TWO,
                "target_path": "tests/test_handler.py",
                "target_state": "existing",
                "family": "error_path_gating",
                "owner": "module.handler",
                "discriminator": "raises ValueError on empty payload",
                "relation": "case calls owner directly",
                "oracle": "pytest.raises exact message pin",
            },
            "edit_surface": {"allowed": ["tests/test_handler.py"], "forbidden": []},
            "authorization": {
                "status": "granted",
                "authority": "operator-a",
                "method": "explicit-operator-flags",
            },
            "non_claims": BINDING_NON_CLAIMS,
        });
        let stale_text = serde_json::to_string_pretty(&stale)
            .map_err(|error| format!("serialize stale record: {error}"))?;
        std::fs::write(bindings_dir.join("stale.json"), &stale_text)
            .map_err(|error| format!("write stale record: {error}"))?;
        let failure = run_check_driver(&args_of(
            manifest_path.to_string_lossy().as_ref(),
            bindings_dir.to_string_lossy().as_ref(),
        ));
        std::fs::remove_file(bindings_dir.join("stale.json"))
            .map_err(|error| format!("remove stale record: {error}"))?;
        let error = match failure {
            Err(error) => error,
            Ok(()) => return Err("a stale binding record passed the end-to-end check".to_string()),
        };
        if !error.contains("1 violation(s)") {
            return Err(format!("unexpected end-to-end failure: {error}"));
        }

        std::fs::remove_dir_all(&root).map_err(|error| format!("remove temp root: {error}"))?;
        Ok(())
    }

    fn args_of(manifest: &str, bindings: &str) -> Vec<String> {
        vec![
            "--manifest".to_string(),
            manifest.to_string(),
            "--bindings".to_string(),
            bindings.to_string(),
        ]
    }
}
