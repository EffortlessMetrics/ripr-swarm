use super::{
    FixtureGuard, SequentialStdio, commit_fixture, common, fixture_git_ok,
    install_b4_repair_ready_fixture, structured_tool_success,
};
use serde_json::{Value, json};
use std::time::Duration;

/// tools/list must declare the nullable scope that both real tools emit (#7766).
#[test]
fn status_and_refresh_advertise_committed_scope_variants() -> Result<(), String> {
    let root = install_b4_repair_ready_fixture()?;
    let _cleanup = FixtureGuard { root: root.clone() };
    std::fs::write(root.join(".gitignore"), "/target/\n/.ripr/\n")
        .map_err(|error| error.to_string())?;
    commit_fixture(&root, "ignore analysis artifacts")?;
    fixture_git_ok(&root, &["update-ref", "refs/heads/main", "HEAD"])?;
    let mut session = SequentialStdio::spawn(&root)?;
    let timeout = Duration::from_mins(1);
    session.call("discover", "server/discover", json!({}), timeout)?;
    let listed = session.call("tools", "tools/list", json!({}), timeout)?;
    let tools = listed
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("tools/list omitted tools: {listed}"))?;
    // Independent literal contract, not generated from the production helper.
    let expected = json!({
        "type": ["object", "null"],
        "properties": {
            "analyzed": { "type": "string", "const": "committed default-branch diff" },
            "uncommitted_edits": { "type": "string", "const": "outside this analysis" },
            "note": { "type": "string" },
            "untracked_edits": {
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 3 },
                    "total": { "type": "integer", "minimum": 1 },
                    "more": { "type": "integer", "minimum": 0 },
                    "listing_example": { "type": "string" },
                    "remedy": { "type": "string" }
                },
                "required": ["files", "total", "more", "listing_example", "remedy"],
                "additionalProperties": false
            }
        },
        "required": ["analyzed", "uncommitted_edits", "note"],
        "additionalProperties": false
    });
    for (name, path) in [
        (
            "ripr_workspace_status",
            "/outputSchema/oneOf/0/properties/session",
        ),
        ("ripr_refresh", "/outputSchema/oneOf/0"),
    ] {
        let descriptor = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .ok_or_else(|| format!("missing descriptor {name}"))?;
        let owner = descriptor.pointer(path).ok_or("missing success schema")?;
        if owner.pointer("/properties/scope") != Some(&expected)
            || !owner["required"]
                .as_array()
                .is_some_and(|fields| fields.contains(&json!("scope")))
        {
            return Err(format!(
                "{name} must declare and require precise nullable scope: {owner}"
            ));
        }
    }
    let status = session.call_tool("before", "ripr_workspace_status", json!({}), timeout)?;
    let status = structured_tool_success(&status, "status before refresh")?;
    if status.pointer("/session/scope") != Some(&Value::Null)
        || status.pointer("/session/attempt_state") != Some(&json!("no_snapshot"))
    {
        return Err(format!("no snapshot must emit null scope: {status}"));
    }
    for state in ["clean", "tracked", "untracked", "failed"] {
        if state == "tracked" {
            let source = root.join("src/lib.rs");
            let text = std::fs::read_to_string(&source).map_err(|error| error.to_string())?;
            std::fs::write(&source, format!("{text}\n// staged edit\n"))
                .map_err(|error| error.to_string())?;
            fixture_git_ok(&root, &["add", "src/lib.rs"])?;
            std::fs::write(&source, format!("{text}\n// unstaged edit\n"))
                .map_err(|error| error.to_string())?;
            let dirty = common::fixture_git::fixture_git_output(&root, &["status", "--porcelain"])?;
            if !dirty.contains("MM src/lib.rs") {
                return Err(format!("missing dirty stimulus: {dirty}"));
            }
        } else if state == "untracked" {
            for index in 0..4 {
                std::fs::write(
                    root.join(format!("src/untracked{index}.rs")),
                    "pub fn extra() {}\n",
                )
                .map_err(|error| error.to_string())?;
            }
        } else if state == "failed" {
            std::fs::write(root.join("ripr.toml"), "[broken\n")
                .map_err(|error| error.to_string())?;
        }
        let refresh = session.call_tool("refresh", "ripr_refresh", json!({}), timeout)?;
        let refresh = structured_tool_success(&refresh, "refresh")?;
        let status = session.call_tool("status", "ripr_workspace_status", json!({}), timeout)?;
        let status = structured_tool_success(&status, "status")?;
        if status.pointer("/session/scope") != refresh.get("scope") {
            return Err(format!("scope parity lost in {state}"));
        }
        let scope = refresh.get("scope").ok_or("scope omitted")?;
        if state == "clean" {
            if !scope.is_null() {
                return Err(format!("clean scope must be null: {scope}"));
            }
        } else {
            if scope["analyzed"] != "committed default-branch diff"
                || scope["uncommitted_edits"] != "outside this analysis"
                || !scope["note"].is_string()
            {
                return Err(format!("dirty disclosure lost in {state}: {scope}"));
            }
            if state == "tracked" {
                if scope.get("untracked_edits").is_some() {
                    return Err(format!(
                        "tracked-only scope invented untracked files: {scope}"
                    ));
                }
            } else if scope.pointer("/untracked_edits/total") != Some(&json!(4))
                || scope.pointer("/untracked_edits/more") != Some(&json!(1))
                || scope
                    .pointer("/untracked_edits/files")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    != Some(3)
            {
                return Err(format!(
                    "bounded untracked disclosure lost in {state}: {scope}"
                ));
            }
        }
        let attempt = if state == "failed" {
            "failed"
        } else {
            "completed"
        };
        if refresh.pointer("/attempt/state") != Some(&json!(attempt))
            || status.pointer("/session/attempt_state") != Some(&json!(attempt))
        {
            return Err(format!("wrong attempt state for {state}: {refresh}"));
        }
    }
    Ok(())
}
