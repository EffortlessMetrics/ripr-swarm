//! Native TLC pilot for the existing repair-attempt finish boundary.
//! Model evidence is never promoted to production-code evidence.

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::run;

const MODEL_DIR: &str = "formal/repair-attempt";
const JAR_SHA256: &str = "936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88";
const INPUTS: &[&str] = &[
    "RepairAttemptFinish.tla",
    "RepairAttemptFinish.cfg",
    "IgnoreCurrentness.cfg",
    "IgnoreCageVerdict.cfg",
    "BypassAwaitingGuard.cfg",
    "assumptions.json",
    "properties.json",
];
const SOURCE_INPUTS: &[&str] = &[
    "crates/ripr/src/app/repair_attempt/mod.rs",
    "crates/ripr/src/app/repair_attempt/store.rs",
    "crates/ripr/src/edit_cage/mod.rs",
    "xtask/src/repair_lifecycle_model.rs",
];
const NATIVE_ADAPTER_INPUTS: &[&str] = &[
    "xtask/src/repair_lifecycle_model.rs",
    "xtask/src/run.rs",
    "xtask/src/main.rs",
    "xtask/src/command.rs",
    "xtask/src/dispatch.rs",
    "xtask/Cargo.toml",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
];

pub(crate) fn run_lane(args: &[String]) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no workspace parent")?;
    match args {
        [mode, base] if mode == "select" => {
            println!("{}", affected(root, base));
            Ok(())
        }
        [mode, jar_flag, jar, out_flag, out]
            if mode == "check" && jar_flag == "--tlc-jar" && out_flag == "--out" =>
        {
            check(root, Path::new(jar), Path::new(out))
        }
        _ => Err("usage: cargo xtask repair-lifecycle-model select <base-sha-or-empty> | check --tlc-jar <file> --out <fresh-directory>".to_owned()),
    }
}

/// A conservative projection for this one pilot, not another CI planner.
/// Unknown bases, invalid output and Git failures select the check.
fn affected(root: &Path, base: &str) -> bool {
    if base.len() != 40 || !base.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return true;
    }
    let args = ["diff", "--no-renames", "--name-only", "-z", base, "HEAD", "--"].map(str::to_owned);
    let output = run::capture_bytes_in_dir_with_env_budget(
        Path::new("git"),
        &args,
        root,
        (&[], &[]),
        run::ByteCaptureBudget {
            timeout: Duration::from_secs(10),
            stdout_bytes: 1024 * 1024,
            stderr_bytes: 64 * 1024,
        },
        "repair lifecycle affected inputs",
    );
    let Ok(output) = output else {
        return true;
    };
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return true;
    }
    let Ok(paths) = std::str::from_utf8(&output.stdout) else {
        return true;
    };
    paths.split('\0').any(path_is_affected)
}

fn path_is_affected(path: &str) -> bool {
    [
        "formal/repair-attempt/",
        "crates/ripr/src/app/",
        "crates/ripr/src/edit_cage/",
        "xtask/",
        ".github/workflows/",
        ".cargo/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
        || matches!(path, "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml")
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:v1:{:x}", Sha256::digest(bytes))
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create fresh {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write {}: {error}", path.display()))
}

fn json_bytes(value: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(value).map_err(|error| format!("encode report: {error}"))
}

fn source_bindings(root: &Path) -> Result<Value, String> {
    Ok(json!({
        "claim": "source correspondence obligations only; not test execution or Rust refinement",
        "inputs": file_commitments(root, SOURCE_INPUTS)?
    }))
}

fn file_commitments(root: &Path, paths: &[&str]) -> Result<Value, String> {
    let mut entries = Vec::new();
    for path in paths {
        entries.push(json!({"path": path, "digest": digest(&read(&root.join(path))?)}));
    }
    Ok(json!(entries))
}

fn native_toolchain(jar: &[u8], stdout: &[u8], stderr: &[u8], adapter_inputs: &Value) -> Value {
    json!({
        "tlc_version": "1.7.4",
        "jar_digest": digest(jar),
        "jar_bytes": jar.len(),
        "java_stdout_digest": digest(stdout),
        "java_stderr_digest": digest(stderr),
        "native_adapter_and_build_inputs": adapter_inputs,
        "environment": "trusted host JVM; version observation, not execution attestation",
        "removed_environment": ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS", "CLASSPATH"]
    })
}

fn check(root: &Path, jar: &Path, out: &Path) -> Result<(), String> {
    let jar_bytes = read(jar)?;
    if jar_bytes.len() != 2_274_532 || digest(&jar_bytes) != format!("sha256:v1:{JAR_SHA256}") {
        return Err(
            "TLC jar is not the pinned v1.7.4 official asset (size and SHA-256 required)".to_owned(),
        );
    }
    // Reject reuse, including partial prior runs. A fresh path is evidence
    // hygiene, not a cleanup action on the caller's existing files.
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create output parent: {error}"))?;
    }
    fs::create_dir(out).map_err(|error| format!("output must be fresh: {error}"))?;
    let out = fs::canonicalize(out).map_err(|error| format!("resolve output: {error}"))?;
    write(&out.join("tla2tools.jar"), &jar_bytes)?;
    let before = source_bindings(root)?;
    let adapter_inputs = file_commitments(root, NATIVE_ADAPTER_INPUTS)?;
    write(&out.join("source-bindings.json"), &json_bytes(&before)?)?;
    for input in INPUTS {
        let bytes = read(&root.join(MODEL_DIR).join(input))?;
        if input.ends_with(".cfg") {
            validate_config(input, &bytes)?;
        }
        write(&out.join(input), &bytes)?;
    }
    let model_bytes = read(&out.join("RepairAttemptFinish.tla"))?;
    validate_model_imports(&model_bytes)?;
    let model_closure = json!({
        "schema_id": "ripr.native-tla-model-closure.v1",
        "sources": [{
            "path": "RepairAttemptFinish.tla",
            "digest": digest(&model_bytes)
        }],
        "standard_imports": ["Naturals", "FiniteSets"],
        "standard_library": {
            "container": "tla2tools.jar",
            "digest": digest(&jar_bytes),
            "scope": "whole pinned jar, including transitive standard modules"
        }
    });
    let model_closure_bytes = json_bytes(&model_closure)?;
    write(&out.join("model-closure.json"), &model_closure_bytes)?;
    // Version output belongs to the same native process owner. It is retained
    // separately and is not treated as authenticated execution provenance.
    let java = run::capture_bytes_in_dir_with_env_budget(
        Path::new("java"),
        &["-version".to_owned()],
        &out,
        (
            &[],
            &["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS", "CLASSPATH"],
        ),
        run::ByteCaptureBudget {
            timeout: Duration::from_secs(10),
            stdout_bytes: 64 * 1024,
            stderr_bytes: 64 * 1024,
        },
        "repair lifecycle Java version",
    )?;
    write(&out.join("java.stdout"), &java.stdout)?;
    write(&out.join("java.stderr"), &java.stderr)?;
    if java.timed_out || !java.status.is_some_and(|status| status.success()) {
        return Err("Java version observation failed; no model evidence established".to_owned());
    }
    let toolchain = native_toolchain(&jar_bytes, &java.stdout, &java.stderr, &adapter_inputs);
    write(&out.join("toolchain.json"), &json_bytes(&toolchain)?)?;
    let checks = [
        ("RepairAttemptFinish.cfg", None),
        ("IgnoreCurrentness.cfg", Some("FinishRequiresCurrentSnapshot")),
        ("IgnoreCageVerdict.cfg", Some("FinishRequiresCompliantCage")),
        ("BypassAwaitingGuard.cfg", Some("AtMostOneFinish")),
    ];
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for (config, negative) in checks {
        let stem = config.trim_end_matches(".cfg");
        let args = vec![
            "-Xmx256m".to_owned(),
            "-Duser.language=en".to_owned(),
            "-Duser.country=US".to_owned(),
            "-Dfile.encoding=UTF-8".to_owned(),
            "-cp".to_owned(),
            "tla2tools.jar".to_owned(),
            "tlc2.TLC".to_owned(),
            "-tool".to_owned(),
            "-workers".to_owned(),
            "1".to_owned(),
            "-config".to_owned(),
            config.to_owned(),
            "-metadir".to_owned(),
            format!("{stem}.states"),
            "RepairAttemptFinish.tla".to_owned(),
        ];
        let scope = json!({
            "config": config,
            "config_digest": digest(&read(&out.join(config))?),
            "argv": args,
            "timeout_seconds": 60,
            "stdout_byte_limit": 4 * 1024 * 1024,
            "stderr_byte_limit": 1024 * 1024,
            "model_finishers": 2,
            "tlc_workers": 1,
            "scheduler": "TLC breadth-first reachable-state exploration, one TLC worker",
            "fairness": "none",
            "claim": "finite safety only; no liveness or code proof"
        });
        write(&out.join(format!("{stem}.scope.json")), &json_bytes(&scope)?)?;
        let result = run::capture_bytes_in_dir_with_env_budget(
            Path::new("java"),
            &args,
            &out,
            (
                &[],
                &["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS", "CLASSPATH"],
            ),
            run::ByteCaptureBudget {
                timeout: Duration::from_secs(60),
                stdout_bytes: 4 * 1024 * 1024,
                stderr_bytes: 1024 * 1024,
            },
            "repair lifecycle TLC",
        );
        let row = match result {
            Ok(output) => {
                write(&out.join(format!("{stem}.stdout")), &output.stdout)?;
                write(&out.join(format!("{stem}.stderr")), &output.stderr)?;
                let exit = output.status.and_then(|status| status.code());
                let verdict = validate_native(&output.stdout, exit, output.timed_out, negative);
                let accepted = verdict.is_ok();
                if let Err(reason) = &verdict {
                    errors.push(format!("{config}: {reason}"));
                }
                json!({
                    "config": config, "expected_counterexample": negative,
                    "accepted": accepted, "exit_code": exit,
                    "timed_out": output.timed_out,
                    "duration_ms": output.duration.as_millis(),
                    "stdout_digest": digest(&output.stdout),
                    "stderr_digest": digest(&output.stderr),
                    "scope_digest": digest(&json_bytes(&scope)?),
                    "reason": verdict.err()
                })
            }
            Err(reason) => {
                errors.push(format!("{config}: {reason}"));
                json!({"config":config, "accepted":false, "reason":reason})
            }
        };
        results.push(row);
    }
    // Refuse mixed source snapshots even though source correspondence is a
    // separate, weaker record than implementation-conformance evidence.
    if before != source_bindings(root)? {
        errors.push("implementation/adapter inputs changed during the run".to_owned());
    }
    if adapter_inputs != file_commitments(root, NATIVE_ADAPTER_INPUTS)? {
        errors.push("native adapter/process owner/build inputs changed during the run".to_owned());
    }
    for input in INPUTS {
        if read(&out.join(input))? != read(&root.join(MODEL_DIR).join(input))? {
            errors.push(format!("model input changed during the run: {input}"));
        }
    }
    let properties: Value = serde_json::from_slice(&read(&out.join("properties.json"))?)
        .map_err(|error| format!("read property manifest: {error}"))?;
    let rows = validate_properties(&properties)?;
    let mut evidence = Vec::new();
    if errors.is_empty() {
        for property in rows {
            evidence.push(json!({
                "state_machine": {
                    "schema_id": "proof.state-machine-evidence-binding.v1",
                    "machine_id": "ripr.repair-attempt",
                    "property_id": property["property_id"],
                    "evidence_kind": "model_check",
                    "model_digest": digest(&model_closure_bytes),
                    "property_digest": digest(&json_bytes(property)?),
                    "assumptions_digest": digest(&read(&out.join("assumptions.json"))?),
                    "scope_digest": results[0]["scope_digest"],
                    "toolchain_digest": digest(&json_bytes(&toolchain)?)
                },
                "claim": "finite model checked; not implementation conformance or proof"
            }));
        }
    }
    let report = json!({
        "schema_id": "ripr.repair-lifecycle-native-run.v1",
        "accepted": errors.is_empty(),
        "checks": results,
        "errors": errors,
        "model_evidence": evidence,
        "implementation_evidence": {
            "status": "not_established_by_this_command",
            "owner": "existing required Rust nextest/JUnit, including sharded execution",
            "obligations": properties["properties"]
        }
    });
    write(&out.join("report.json"), &json_bytes(&report)?)?;
    if errors.is_empty() {
        println!("repair lifecycle: finite model and three known-wrong controls accepted");
        Ok(())
    } else {
        Err(format!("repair lifecycle NOT-ESTABLISHED: {}", errors.join("; ")))
    }
}

/// Fixed-pilot import guard. All standard-module dependencies are committed
/// by the complete verified jar, not merely by their names. Extending this
/// pilot with another module/INSTANCE requires an explicit closure change.
fn validate_model_imports(bytes: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let tokens = text
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.iter().filter(|token| **token == "EXTENDS").count() != 1
        || tokens.iter().filter(|token| **token == "MODULE").count() != 1
        || tokens.contains(&"INSTANCE")
        || !text.lines().any(|line| line.trim() == "EXTENDS Naturals, FiniteSets")
    {
        return Err("unexpected model/import closure; this pilot admits only its single module and pinned Naturals/FiniteSets standard imports".to_owned());
    }
    Ok(())
}

/// A fixed pilot contract, deliberately not a general TLC config parser.
/// Removing an invariant or adding constraints/overrides cannot silently
/// narrow the explored space while retaining the same property claim.
fn validate_config(name: &str, bytes: &[u8]) -> Result<(), String> {
    let (current, cage, guard) = match name {
        "RepairAttemptFinish.cfg" => ("FALSE", "FALSE", "FALSE"),
        "IgnoreCurrentness.cfg" => ("TRUE", "FALSE", "FALSE"),
        "IgnoreCageVerdict.cfg" => ("FALSE", "TRUE", "FALSE"),
        "BypassAwaitingGuard.cfg" => ("FALSE", "FALSE", "TRUE"),
        _ => return Err(format!("unsupported pilot config {name}")),
    };
    let expected = format!(
        "CONSTANTS IgnoreCurrentness = {current} IgnoreCageVerdict = {cage} \
         BypassAwaitingGuard = {guard} INIT Init NEXT Next INVARIANTS TypeOK \
         FinishRequiresCurrentSnapshot FinishRequiresCompliantCage AtMostOneFinish"
    );
    let actual = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    if !actual.split_whitespace().eq(expected.split_whitespace()) {
        return Err(format!("{name}: changed invariants, constants, bounds or directives require reviewed pilot-contract changes"));
    }
    Ok(())
}

fn validate_properties(properties: &Value) -> Result<&[Value], String> {
    if properties["schema_id"].as_str() != Some("ripr.repair-attempt-properties.v1")
        || properties["machine_id"].as_str() != Some("ripr.repair-attempt")
    {
        return Err("unexpected repair-attempt property contract".to_owned());
    }
    let rows = properties["properties"]
        .as_array()
        .ok_or("property manifest has no properties")?;
    let expected = [
        (
            "finish-requires-current-snapshot",
            "FinishRequiresCurrentSnapshot",
        ),
        (
            "finish-requires-compliant-cage",
            "FinishRequiresCompliantCage",
        ),
        ("at-most-one-finish", "AtMostOneFinish"),
    ];
    if rows.len() != expected.len() {
        return Err("incomplete or extra property inventory".to_owned());
    }
    for (id, invariant) in expected {
        if rows
            .iter()
            .filter(|row| {
                row["property_id"].as_str() == Some(id)
                    && row["invariant"].as_str() == Some(invariant)
            })
            .count()
            != 1
        {
            return Err(format!("missing/duplicate/mismatched property {id}"));
        }
    }
    Ok(rows.as_slice())
}

#[derive(Debug)]
struct Message {
    code: u32,
    severity: u32,
    body: String,
}

/// Parse the pinned TLC -tool framing. Incomplete, nested, mismatched frames
/// refuse; English-looking success outside a native frame cannot pass.
fn messages(bytes: &[u8]) -> Result<Vec<Message>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| format!("non-UTF8 TLC output: {error}"))?;
    let mut result = Vec::new();
    let mut current: Option<Message> = None;
    for line in text.lines() {
        if let Some(header) = line.strip_prefix("@!@!@STARTMSG ") {
            if current.is_some() {
                return Err("nested TLC message".to_owned());
            }
            let header = header.strip_suffix(" @!@!@").ok_or("malformed TLC header")?;
            let (code, severity) = header.split_once(':').ok_or("missing TLC severity")?;
            let severity = severity.parse::<u32>().map_err(|_| "invalid TLC severity")?;
            if severity > 4 {
                return Err("unknown TLC severity".to_owned());
            }
            current = Some(Message {
                code: code.parse().map_err(|_| "invalid TLC code")?,
                severity,
                body: String::new(),
            });
        } else if let Some(end) = line.strip_prefix("@!@!@ENDMSG ") {
            let end = end.strip_suffix(" @!@!@").ok_or("malformed TLC end")?;
            let message = current.take().ok_or("TLC end without start")?;
            if end.parse::<u32>().map_err(|_| "invalid TLC end code")? != message.code {
                return Err("mismatched TLC message code".to_owned());
            }
            result.push(message);
        } else if line.contains("@!@!@") {
            return Err("malformed TLC delimiter".to_owned());
        } else if let Some(message) = &mut current {
            message.body.push_str(line);
            message.body.push('\n');
        }
    }
    if current.is_some() {
        return Err("truncated TLC message".to_owned());
    }
    Ok(result)
}

fn validate_native(
    stdout: &[u8],
    exit: Option<i32>,
    timed_out: bool,
    negative: Option<&str>,
) -> Result<(), String> {
    if timed_out {
        return Err("native TLC timed out".to_owned());
    }
    let messages = messages(stdout)?;
    let has = |code| messages.iter().any(|message| message.code == code);
    if messages.iter().filter(|message| message.code == 2185).count() != 1
        || messages.iter().filter(|message| message.code == 2186).count() != 1
    {
        return Err("missing native start or completion".to_owned());
    }
    if let Some(invariant) = negative {
        let expected = format!("Invariant {invariant} is violated.");
        if exit != Some(12)
            || has(2193)
            || !messages.iter().any(|message| {
                message.code == 2110 && message.body.trim() == expected
            })
            || !has(2121)
            || messages.iter().filter(|message| message.code == 2217).count() < 2
            || messages.iter().filter(|message| message.code == 2110).count() != 1
            || messages.iter().any(|message| {
                message.severity == 2
                    || (message.severity == 1 && !matches!(message.code, 2110 | 2121))
            })
        {
            return Err(format!("expected native safety counterexample for {invariant}"));
        }
        return Ok(());
    }
    if exit != Some(0)
        || !has(2193)
        || messages.iter().any(|message| matches!(message.severity, 1 | 2))
    {
        return Err("no successful native safety verdict".to_owned());
    }
    let stats = messages.iter().filter(|message| message.code == 2199).collect::<Vec<_>>();
    if stats.len() != 1 {
        return Err("expected one final native state count".to_owned());
    }
    let text = stats[0].body.trim();
    let (generated, rest) = text.split_once(" states generated, ").ok_or("invalid TLC statistics")?;
    let (distinct, remaining) = rest.split_once(" distinct states found, ").ok_or("invalid TLC statistics")?;
    let remaining = remaining.strip_suffix(" states left on queue.").ok_or("invalid TLC statistics")?;
    let number = |text: &str| -> Result<u64, String> {
        text.replace(',', "").parse::<u64>().map_err(|_| "invalid state count".to_owned())
    };
    if number(generated)? < 2 || number(distinct)? < 2 || number(remaining)? != 0 {
        return Err("empty/incomplete native state exploration".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(code: u32, severity: u32, body: &str) -> String {
        format!("@!@!@STARTMSG {code}:{severity} @!@!@\n{body}\n@!@!@ENDMSG {code} @!@!@\n")
    }

    fn positive(stats: &str) -> Vec<u8> {
        [
            message(2185, 0, "Starting"),
            message(2193, 0, "Model checking completed."),
            message(2199, 0, stats),
            message(2186, 0, "Finished"),
        ].concat().into_bytes()
    }

    #[test]
    fn native_parser_refuses_empty_incomplete_or_forged_success() {
        let valid = positive("12 states generated, 8 distinct states found, 0 states left on queue.");
        assert!(validate_native(&valid, Some(0), false, None).is_ok());
        assert!(validate_native(&valid, Some(0), true, None).is_err());
        assert!(validate_native(&valid, Some(1), false, None).is_err());
        for stats in [
            "0 states generated, 0 distinct states found, 0 states left on queue.",
            "12 states generated, 8 distinct states found, 1 states left on queue.",
        ] {
            assert!(validate_native(&positive(stats), Some(0), false, None).is_err());
        }
        assert!(validate_native(b"Model checking completed. No error has been found.", Some(0), false, None).is_err());
        assert!(messages(b"@!@!@STARTMSG 2193:0 @!@!@\nincomplete").is_err());
    }

    #[test]
    fn negative_control_requires_the_named_native_counterexample() {
        let trace = [
            message(2185, 0, "Starting"),
            message(2110, 1, "Invariant FinishRequiresCurrentSnapshot is violated."),
            message(2121, 1, "The behavior up to this point is:"),
            message(2217, 4, "1: <Initial predicate>"),
            message(2217, 4, "2: <Capture>"),
            message(2217, 4, "3: <Commit>"),
            message(2186, 0, "Finished"),
        ].concat().into_bytes();
        assert!(validate_native(&trace, Some(12), false, Some("FinishRequiresCurrentSnapshot")).is_ok());
        assert!(validate_native(&trace, Some(12), false, Some("AtMostOneFinish")).is_err());
        assert!(validate_native(&trace, Some(0), false, Some("FinishRequiresCurrentSnapshot")).is_err());
        assert!(validate_native(&trace, Some(12), false, None).is_err());
    }

    #[test]
    fn affected_scope_includes_owner_adapter_models_and_toolchain() {
        for path in [
            "formal/repair-attempt/RepairAttemptFinish.cfg",
            "crates/ripr/src/app/repair_attempt/mod.rs",
            "crates/ripr/src/edit_cage/mod.rs",
            "xtask/src/run.rs",
            ".github/workflows/rust-gates.yml",
            "Cargo.lock",
        ] {
            assert!(path_is_affected(path), "missed {path}");
        }
        assert!(!path_is_affected("docs/README.md"));
        assert!(!path_is_affected(""));
    }

    #[test]
    fn binding_manifest_does_not_replace_the_real_production_obligations() -> Result<(), String> {
        let properties: Value = serde_json::from_str(include_str!(
            "../../formal/repair-attempt/properties.json"
        )).map_err(|error| error.to_string())?;
        let rows = validate_properties(&properties)?;
        let source = include_str!("../../crates/ripr/src/app/repair_attempt/mod.rs");
        for row in rows {
            let tests = row["tests"].as_array().ok_or("missing production tests")?;
            if tests.is_empty() {
                return Err("empty production obligation inventory".to_owned());
            }
            for test in tests {
                let name = test.as_str().ok_or("non-string test binding")?;
                if !source.contains(&format!("fn {name}(")) {
                    return Err(format!("bound production test was removed: {name}"));
                }
            }
        }
        let mut changed = properties.clone();
        changed["properties"][0]["invariant"] = json!("TypeOK");
        assert!(validate_properties(&changed).is_err());
        Ok(())
    }

    #[test]
    fn config_contract_rejects_removed_invariants_and_extra_constraints() -> Result<(), String> {
        let valid = include_str!("../../formal/repair-attempt/RepairAttemptFinish.cfg");
        validate_config("RepairAttemptFinish.cfg", valid.as_bytes())?;
        for changed in [
            valid.replace("    FinishRequiresCompliantCage\n", ""),
            format!("{valid}CONSTRAINT finishCount < 1\n"),
            valid.replace("IgnoreCurrentness = FALSE", "IgnoreCurrentness = TRUE"),
            valid.replace("NEXT Next", "NEXT Quiesce"),
        ] {
            assert!(validate_config("RepairAttemptFinish.cfg", changed.as_bytes()).is_err());
        }
        for (name, config) in [
            ("IgnoreCurrentness.cfg", include_str!("../../formal/repair-attempt/IgnoreCurrentness.cfg")),
            ("IgnoreCageVerdict.cfg", include_str!("../../formal/repair-attempt/IgnoreCageVerdict.cfg")),
            ("BypassAwaitingGuard.cfg", include_str!("../../formal/repair-attempt/BypassAwaitingGuard.cfg")),
        ] {
            validate_config(name, config.as_bytes())?;
        }
        Ok(())
    }

    #[test]
    fn model_closure_rejects_uncommitted_imports() -> Result<(), String> {
        let valid = include_str!("../../formal/repair-attempt/RepairAttemptFinish.tla");
        validate_model_imports(valid.as_bytes())?;
        for changed in [
            valid.replace("EXTENDS Naturals, FiniteSets", "EXTENDS Naturals, FiniteSets, Hidden"),
            valid.replace("Workers == 1..2", "Workers == 1..2\nHidden == INSTANCE Other"),
            valid.replace("Workers == 1..2", "Workers == 1..2\nHidden==INSTANCE Other"),
        ] {
            assert!(validate_model_imports(changed.as_bytes()).is_err());
        }
        Ok(())
    }

    #[test]
    fn adapter_only_change_invalidates_the_model_toolchain_binding() -> Result<(), String> {
        let original = json!([{
            "path": "xtask/src/repair_lifecycle_model.rs",
            "digest": digest(b"original parser")
        }]);
        let changed = json!([{
            "path": "xtask/src/repair_lifecycle_model.rs",
            "digest": digest(b"changed parser")
        }]);
        let first = native_toolchain(b"same jar", b"", b"same JVM", &original);
        let second = native_toolchain(b"same jar", b"", b"same JVM", &changed);
        assert_ne!(digest(&json_bytes(&first)?), digest(&json_bytes(&second)?));
        assert!(NATIVE_ADAPTER_INPUTS.contains(&"xtask/src/run.rs"));
        assert!(NATIVE_ADAPTER_INPUTS.contains(&"Cargo.lock"));
        Ok(())
    }
}
