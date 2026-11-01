//! #11584: refusals and failures are machine-readable for agents.
//!
//! A JSON request must never leave a script with an empty stdout or a
//! refusal that reads like a clean result: every refusal carries
//! `status: "refused"`, a closed reason `code`, the human `message`, and the
//! one `next` command, while the exit code and the stderr prose stay as before.
use super::*;
use serde_json::Value;

fn stdout_json(output: &Output) -> Result<Value, String> {
    serde_json::from_slice(&output.stdout).map_err(|err| {
        format!(
            "stdout is not one JSON document ({err}):\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn pointer_str<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string at {pointer} in {value}"))
}

fn assert_refusal(
    output: &Output,
    exit: i32,
    code: &str,
    next_contains: &str,
) -> Result<Value, String> {
    if output.status.code() != Some(exit) {
        return Err(format!(
            "expected exit {exit}, got {:?}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let value = stdout_json(output)?;
    for (pointer, expected) in [("/status", "refused"), ("/error/code", code)] {
        let actual = pointer_str(&value, pointer)?;
        if actual != expected {
            return Err(format!("{pointer}: expected {expected}, got {actual}"));
        }
    }
    // `next` is always present; it is null when no single honest command
    // exists, which the empty `next_contains` accepts.
    match value.pointer("/error/next") {
        Some(Value::Null) if next_contains.is_empty() => {}
        Some(Value::Null) => {
            return Err(format!("next is null, expected {next_contains:?}"));
        }
        Some(_) => {
            let next = pointer_str(&value, "/error/next/command")?;
            if !next.contains(next_contains) {
                return Err(format!("next command {next:?} lacks {next_contains:?}"));
            }
        }
        None => return Err("error.next key must always be present".to_string()),
    }
    if pointer_str(&value, "/error/message")?.is_empty() {
        return Err("error.message must carry the one-sentence why".to_string());
    }
    // The human prose stays on stderr, unchanged.
    if output.stderr.is_empty() {
        return Err("the stderr prose must remain".to_string());
    }
    Ok(value)
}

#[test]
fn usage_errors_under_json_write_the_refusal_document() -> Result<(), String> {
    let unknown_flag = run_ripr(&["check", "--json", "--bogus"]);
    let value = assert_refusal(
        &unknown_flag,
        2,
        "unknown_argument",
        "ripr check --help --all",
    )?;
    assert_eq!(pointer_str(&value, "/kind")?, "cli_refusal");
    assert_eq!(pointer_str(&value, "/command")?, "check");
    assert_eq!(value.pointer("/exit_code"), Some(&Value::from(2)));

    let bad_value = run_ripr(&["check", "--json", "--mode", "turbo"]);
    assert_refusal(&bad_value, 2, "unknown_value", "")?;

    let unknown_command = run_ripr(&["nonsense", "--format", "json"]);
    assert_refusal(&unknown_command, 2, "unknown_command", "ripr --help")?;

    // A command that never accepted `--json` still answers it with the
    // document, and the facade commands now accept it.
    let explain = run_ripr(&["explain", "--json", "probe:nope:1"]);
    assert_refusal(&explain, 2, "unknown_argument", "ripr explain --help")?;
    Ok(())
}

#[test]
fn usage_errors_without_a_json_request_keep_stdout_empty() {
    // Negative control: the document is a response to a JSON request, not a
    // new default. Human callers see exactly the prose they saw before.
    for args in [
        &["check", "--bogus"][..],
        &["check", "--format", "xml"][..],
        &["nonsense"][..],
        &["check", "--format", "human", "--bogus"][..],
    ] {
        let output = run_ripr(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            output.stdout.is_empty(),
            "{args:?} wrote stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn check_refusal_is_discriminated_from_a_clean_result() -> Result<(), String> {
    let root = unique_temp_workspace("refusal-envelope-check");
    std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let root_arg = root.to_string_lossy().into_owned();

    let refused = run_ripr(&[
        "check",
        "--root",
        &root_arg,
        "--json",
        "--quiet",
        "--diff",
        "nonexistent.diff",
    ]);
    let value = assert_refusal(&refused, 2, "analysis_failed", "ripr doctor")?;
    // The document is still the check shape, so existing consumers keep
    // parsing it; the discriminator is what is new.
    assert_eq!(value.pointer("/findings"), Some(&Value::Array(Vec::new())));
    assert_eq!(
        pointer_str(&value, "/analysis_scope/run_status")?,
        "analysis_failed"
    );

    // Control: a completed run has neither discriminator. The control needs
    // a real workspace, so the run completes instead of refusing.
    std::fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"refusal-envelope-check\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(root.join("src/lib.rs"), "pub fn one() -> i32 {\n    1\n}\n")
        .map_err(|err| err.to_string())?;
    let patch = root.join("empty.patch");
    std::fs::write(&patch, "").map_err(|err| err.to_string())?;
    let patch_arg = patch.to_string_lossy().into_owned();
    let clean = run_ripr(&[
        "check", "--root", &root_arg, "--json", "--quiet", "--diff", &patch_arg,
    ]);
    assert_eq!(
        clean.status.code(),
        Some(0),
        "the control must complete:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&clean.stdout),
        String::from_utf8_lossy(&clean.stderr)
    );
    let clean = stdout_json(&clean)?;
    assert!(
        clean.get("status").is_none() && clean.get("error").is_none(),
        "a completed run must not carry a refusal discriminator: {clean}"
    );
    Ok(())
}

fn repair_fixture() -> Result<PathBuf, String> {
    let root = unique_temp_workspace("refusal-envelope-repair");
    std::fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
    run_git(&root, &["init"])?;
    run_git(&root, &["config", "user.email", "test@test.com"])?;
    run_git(&root, &["config", "user.name", "Test"])?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"refusal-envelope-repair\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn limit(x: i32, max: i32) -> i32 {\n    if x >= max { max } else { x }\n}\n",
    )
    .map_err(|err| err.to_string())?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-m", "base"])?;
    Ok(root)
}

#[test]
fn continue_without_an_attempt_refuses_with_a_status_next_command() -> Result<(), String> {
    let root = repair_fixture()?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let root_arg = root.to_string_lossy().into_owned();
    let output = run_ripr(&["continue", "--json", "--root", &root_arg]);
    let value = assert_refusal(&output, 3, "continue_declined", "ripr status --root")?;
    assert_eq!(pointer_str(&value, "/command")?, "continue");
    assert_eq!(value.pointer("/exit_code"), Some(&Value::from(3)));
    // Without `--json` the same refusal keeps stdout empty.
    let human = run_ripr(&["continue", "--root", &root_arg]);
    assert_eq!(human.status.code(), Some(3));
    assert!(human.stdout.is_empty());
    Ok(())
}

#[test]
fn repair_without_an_eligible_seam_refuses_with_the_shared_document() -> Result<(), String> {
    let root = repair_fixture()?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let root_arg = root.to_string_lossy().into_owned();
    let output = run_ripr(&["repair", "--json", "--root", &root_arg]);
    let value = assert_refusal(&output, 3, "repair_not_started", "ripr pilot")?;
    assert_eq!(pointer_str(&value, "/command")?, "repair");
    let human = run_ripr(&["repair", "--root", &root_arg]);
    assert_eq!(human.status.code(), Some(3));
    assert!(human.stdout.is_empty());
    Ok(())
}

#[test]
fn context_selector_miss_writes_the_refusal_document() -> Result<(), String> {
    let root = repair_fixture()?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let root_arg = root.to_string_lossy().into_owned();
    let output = run_ripr(&[
        "context",
        "--quiet",
        "--root",
        &root_arg,
        "--at",
        "probe:nope:1",
    ]);
    let value = assert_refusal(&output, 2, "selector_miss", "ripr check")?;
    assert_eq!(pointer_str(&value, "/command")?, "context");
    Ok(())
}

fn read_schema(name: &str) -> Result<Value, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/ripr")
        .join(name);
    let text = std::fs::read_to_string(&path).map_err(|err| format!("{name}: {err}"))?;
    serde_json::from_str(&text).map_err(|err| format!("{name}: {err}"))
}

/// The recovery command is optional by design: a refusal with no single
/// honest command carries `next: null`. Both published schemas must accept
/// that shape, and the emitted document must be the shape they describe.
#[test]
fn schemas_accept_a_null_recovery_command_and_pair_status_with_error() -> Result<(), String> {
    let refusal = read_schema("cli-refusal.schema.json")?;
    let check = read_schema("check.schema.json")?;
    for (label, next) in [
        (
            "cli-refusal",
            refusal.pointer("/properties/error/properties/next"),
        ),
        (
            "check",
            check.pointer("/$defs/refusal_error/properties/next"),
        ),
    ] {
        let next = next.ok_or_else(|| format!("{label}: next is undeclared"))?;
        let types: Vec<&str> = next
            .get("type")
            .and_then(Value::as_array)
            .map(|types| types.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !(types.contains(&"null") && types.contains(&"object")) {
            return Err(format!(
                "{label}: next must allow null or an object: {next}"
            ));
        }
        if next.pointer("/required") != Some(&serde_json::json!(["purpose", "command"])) {
            return Err(format!("{label}: a next object needs purpose and command"));
        }
    }
    // The `error` block always carries `next`, even when it is null.
    for (label, error) in [
        ("cli-refusal", refusal.pointer("/properties/error")),
        ("check", check.pointer("/$defs/refusal_error")),
    ] {
        let required = error
            .and_then(|error| error.get("required"))
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{label}: error.required is undeclared"))?;
        for key in ["code", "message", "next"] {
            if !required.iter().any(|entry| entry == key) {
                return Err(format!("{label}: error.{key} must be required"));
            }
        }
    }
    // `status` and `error` appear together or not at all on a check document.
    assert_eq!(
        check.pointer("/dependentRequired"),
        Some(&serde_json::json!({"status": ["error"], "error": ["status"]}))
    );

    // The emitted standalone document has exactly the declared keys, with
    // `next` present and null for a usage error that has no single command.
    let emitted = run_ripr(&["check", "--json", "--mode", "turbo"]);
    let value = stdout_json(&emitted)?;
    assert_eq!(value.pointer("/error/next"), Some(&Value::Null));
    let declared: Vec<&String> = refusal
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().collect())
        .unwrap_or_default();
    for key in value
        .as_object()
        .map(|object| object.keys())
        .into_iter()
        .flatten()
    {
        if !declared.contains(&key) {
            return Err(format!("emitted key {key} is not declared by the schema"));
        }
    }
    let declared_codes = refusal
        .pointer("/properties/error/properties/code/enum")
        .and_then(Value::as_array)
        .ok_or("code enum is undeclared")?;
    let code = pointer_str(&value, "/error/code")?;
    if !declared_codes.iter().any(|entry| entry == code) {
        return Err(format!("emitted code {code} is not in the schema enum"));
    }
    Ok(())
}

/// `repair` accepts `--json` but writes nothing to stdout that is not one
/// document: a refusal after the command parsed leaves the success path
/// untouched, so the stdout of a refused run is exactly the refusal document.
#[test]
fn a_refused_json_run_writes_exactly_one_document() -> Result<(), String> {
    let root = repair_fixture()?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let root_arg = root.to_string_lossy().into_owned();
    let output = run_ripr(&["repair", "--json", "--root", &root_arg]);
    let text = String::from_utf8_lossy(&output.stdout);
    let mut documents = serde_json::Deserializer::from_str(&text).into_iter::<Value>();
    let first = documents
        .next()
        .ok_or("a refusal under --json must write a document")?
        .map_err(|err| err.to_string())?;
    assert_eq!(first.pointer("/status"), Some(&Value::from("refused")));
    if documents.next().is_some() {
        return Err(format!("stdout carried more than one document:\n{text}"));
    }
    Ok(())
}

/// When the refusal document cannot reach stdout, the operational failure is
/// reported instead of the usage class: a closed stdout must not look like
/// a delivered machine answer. Unix only: it needs `/dev/full`.
#[cfg(unix)]
#[test]
fn a_refusal_that_cannot_be_delivered_reports_the_write_failure() -> Result<(), String> {
    let Ok(full) = std::fs::OpenOptions::new().write(true).open("/dev/full") else {
        return Ok(());
    };
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--json", "--bogus-delivery-11584"])
        .stdout(full)
        .output()
        .map_err(|err| err.to_string())?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("write to stdout failed") && !stderr.contains("flush stdout failed") {
        return Err(format!("the delivery failure must be named, got: {stderr}"));
    }
    Ok(())
}
