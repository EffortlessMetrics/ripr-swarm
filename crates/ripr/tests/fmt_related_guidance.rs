//! Formatting-trait reach is not established by an unbound `.to_string()`.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
#[path = "common/mod.rs"]
mod common;
use common::fixture_git::fixture_git_ok;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unbound_formatting_guidance_checks_reach_but_direct_calls_stay_direct()
-> Result<(), Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = Fixture(
        std::env::temp_dir().join(format!("ripr-fmt-related-{}-{nonce}", std::process::id())),
    );
    fs::create_dir_all(root.0.join("src"))?;
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = \"fmt_related\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    let source = include_str!("fixtures/fmt_related_test.rs");
    fs::write(root.0.join("src/lib.rs"), source)?;
    fixture_git_ok(&root.0, &["init", "-q"])?;
    fixture_git_ok(&root.0, &["add", "Cargo.toml", "src/lib.rs"])?;
    fixture_git_ok(
        &root.0,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    )?;
    let changed = source.replace("if bytes < threshold {", "if bytes <= threshold {");
    assert_ne!(changed, source);
    fs::write(root.0.join("src/lib.rs"), changed)?;
    let run = |format: &str| {
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&root.0)
            .env_remove("RIPR_CACHE_DIR")
            .args(["check", "--worktree", "--format", format])
            .output()
    };
    let json = run("json")?;
    assert!(
        json.status.success(),
        "{}",
        String::from_utf8_lossy(&json.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&json.stdout)?;
    let findings = output["findings"].as_array().ok_or("missing findings")?;
    let fmt = findings
        .iter()
        .find(|f| f["probe"]["expression"] == "bytes <= threshold")
        .ok_or("missing fmt finding")?;
    let step = fmt["recommended_next_step"]
        .as_str()
        .ok_or("missing next step")?;
    assert!(
        step.contains("check whether it reaches the changed code"),
        "{step}"
    );
    assert!(step.contains("inspect other existing tests"), "{step}");
    let rows = fmt["related_tests"]
        .as_array()
        .ok_or("missing related tests")?;
    for (name, relation) in [
        ("format_boundary", "same_test_file"),
        ("builder_boundary", "same_test_file"),
        ("misleading_format_boundary", "weak_token_substring"),
    ] {
        let row = rows
            .iter()
            .find(|r| r["name"] == name)
            .ok_or("missing expected test")?;
        assert_eq!(row["relation_reason"], relation, "{row}");
    }
    // Without the competing type, SPEC-0297 admits the typed formatting call.
    let (before_shadow, rest) = source
        .split_once("    // SHADOW_START\n")
        .ok_or("missing shadow start")?;
    let (_, after_shadow) = rest
        .split_once("    // SHADOW_END\n")
        .ok_or("missing shadow end")?;
    let unshadowed = format!("{before_shadow}{after_shadow}")
        .replace("if bytes < threshold {", "if bytes <= threshold {");
    fs::write(root.0.join("src/lib.rs"), unshadowed)?;
    let positive_json = run("json")?;
    assert!(positive_json.status.success());
    let positive_output: serde_json::Value = serde_json::from_slice(&positive_json.stdout)?;
    let positive = positive_output["findings"]
        .as_array()
        .ok_or("missing positive findings")?
        .iter()
        .find(|f| f["probe"]["expression"] == "bytes <= threshold")
        .ok_or("missing positive fmt finding")?;
    assert!(
        positive["related_tests"]
            .as_array()
            .ok_or("missing positive rows")?
            .iter()
            .any(|r| r["name"] == "format_boundary" && r["relation_reason"] == "direct_owner_call")
    );
    assert!(
        positive["related_tests"]
            .as_array()
            .ok_or("missing positive rows")?
            .iter()
            .any(|r| r["name"] == "builder_boundary" && r["relation_reason"] == "same_test_file")
    );
    fs::write(
        root.0.join("src/lib.rs"),
        source.replace("x < 10", "x <= 10"),
    )?;
    let direct_json = run("json")?;
    assert!(direct_json.status.success());
    let direct_output: serde_json::Value = serde_json::from_slice(&direct_json.stdout)?;
    let direct = direct_output["findings"]
        .as_array()
        .ok_or("missing direct findings")?
        .iter()
        .find(|f| f["probe"]["expression"] == "x <= 10")
        .ok_or("missing direct finding")?;
    assert!(direct["related_tests"].as_array().ok_or("missing direct rows")?.iter().any(|r| r["name"] == "direct_gate_test" && r["relation_reason"] == "direct_owner_call"));
    fs::write(
        root.0.join("src/lib.rs"),
        source.replace("if bytes < threshold {", "if bytes <= threshold {"),
    )?;
    let human = run("human")?;
    assert!(
        human.status.success(),
        "{}",
        String::from_utf8_lossy(&human.stderr)
    );
    let rendered = String::from_utf8(human.stdout)?;
    let text = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("check whether it reaches the changed code"),
        "{text}"
    );
    assert!(text.contains("inspect other existing tests"), "{text}");
    Ok(())
}
