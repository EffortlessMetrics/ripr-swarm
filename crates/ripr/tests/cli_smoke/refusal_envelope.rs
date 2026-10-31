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
    let next = pointer_str(&value, "/error/next/command")?;
    if !next.contains(next_contains) {
        return Err(format!("next command {next:?} lacks {next_contains:?}"));
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

    // Control: a completed run has neither discriminator.
    std::fs::write(root.join("empty.patch"), "").map_err(|err| err.to_string())?;
    let clean = run_ripr(&[
        "check",
        "--root",
        &root_arg,
        "--json",
        "--quiet",
        "--diff",
        "empty.patch",
    ]);
    assert_eq!(clean.status.code(), Some(0));
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
