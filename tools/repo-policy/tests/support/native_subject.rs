//! Literal API identity fixtures for actual production-shell replays.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::process::Command;

pub fn fixture() -> Result<Value, String> {
    serde_json::from_str(include_str!(
        "../../../../fixtures/convergence/native-late-admission/live-base-contract.json"
    ))
    .map_err(|error| format!("invalid literal live-base fixture: {error}"))
}

pub fn program(workflow: &str, name: &str) -> String {
    workflow
        .split_once(&format!("  {name}: |\n"))
        .map(|(_, tail)| {
            tail.lines()
                .take_while(|line| line.is_empty() || line.starts_with("    "))
                .map(|line| line.strip_prefix("    ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

pub fn api_pr(pr: &Value) -> Value {
    let mut pr = pr.clone();
    if let Some(object) = pr.as_object_mut() {
        object.insert("node_id".into(), json!("PR_fixture"));
        object.insert("number".into(), json!(8008));
        if let Some(repo) = pr.pointer_mut("/head/repo").and_then(Value::as_object_mut) {
            repo.insert("node_id".into(), json!("R_fixture"));
        }
        if let Some(base) = pr.get_mut("base").and_then(Value::as_object_mut) {
            base.insert("repo".into(), json!({"node_id":"R_fixture"}));
        }
    }
    pr
}

pub const SHARED_PROGRAMS: &[&str] = &[
    "NATIVE_REQUEST_JQ",
    "NATIVE_COMMIT_JQ",
    "QUALIFICATION_SUBJECT_JQ",
    "FINAL_CURRENTNESS_JQ",
    "NATIVE_REVIEWS_JQ",
    "NATIVE_PRIOR_REQUESTS_JQ",
    "NATIVE_PRIOR_JOBS_JQ",
    "NATIVE_PRIOR_ADMISSION_JQ",
    "READINESS_SUBJECT_DIAGNOSTIC_JQ",
    "READINESS_PR_DIAGNOSTIC_JQ",
    "READINESS_REVIEWS_DIAGNOSTIC_JQ",
    "READINESS_THREADS_DIAGNOSTIC_JQ",
];

pub fn configure(command: &mut Command, workflow: &str) -> Result<(), String> {
    for name in SHARED_PROGRAMS {
        let source = program(workflow, name);
        if source.trim().is_empty() {
            return Err(format!("missing shared workflow program: {name}"));
        }
        command.env(name, source);
    }
    let literal = fixture()?;
    let subject = literal["SUBJECT"].to_string();
    let identity = format!("{:x}", Sha256::digest(subject.as_bytes()));
    command
        .env("GITHUB_RUN_ID", "1000")
        .env("GITHUB_RUN_ATTEMPT", "1")
        .env("EXPECTED_REQUEST_ID", "1000-1")
        .env("EXPECTED_IDENTITY", identity)
        .env("REPOSITORY_ID", "R_fixture")
        .env("PR_ID", "PR_fixture")
        .env("PR_DATABASE_ID", "8008")
        .env("GENERATION", "f".repeat(40))
        .env("GITHUB_WORKFLOW_SHA", "f".repeat(40))
        .env("STORED_BASE_SHA", "b".repeat(40))
        .env(
            "NATIVE_SUBJECT_QUERY",
            program(workflow, "NATIVE_SUBJECT_QUERY"),
        )
        .env("NATIVE_SUBJECT_JQ", program(workflow, "NATIVE_SUBJECT_JQ"))
        .env("MOCK_FRAME", literal["FRAME"].to_string())
        .env("EXPECTED_SUBJECT", subject);
    Ok(())
}
