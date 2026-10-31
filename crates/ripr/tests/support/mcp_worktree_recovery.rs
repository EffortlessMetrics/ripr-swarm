//! Existing #10799 source-preserving failure/retry controls.
//! Native f697 baseline rejected by the independently retained literal retry.

use super::*;

#[cfg(feature = "lang-rust")]
#[test]
fn worktree_failed_base_recovery_preserves_saved_edits_and_last_good() -> Result<(), String> {
    const TIMEOUT: Duration = Duration::from_mins(3);
    let root = install_b4_repair_ready_fixture()?;
    let _cleanup = FixtureGuard { root: root.clone() };
    std::fs::write(root.join(".gitignore"), "/target/\n").map_err(|error| error.to_string())?;
    commit_fixture(&root, "ignore generated analysis artifacts")?;
    fixture_git_ok(&root, &["update-ref", "refs/heads/main", "HEAD"])?;
    fixture_git_ok(&root, &["branch", "-m", "main", "trunk"])?;
    let source = root.join("src/lib.rs");
    let original = std::fs::read_to_string(&source).map_err(|error| error.to_string())?;
    let changed = original.replace("amount - 10", "amount - 15");
    assert_ne!(
        changed, original,
        "the fixture must contain the actual boundary source"
    );
    std::fs::write(&source, &changed).map_err(|error| error.to_string())?;
    let porcelain = common::fixture_git::fixture_git_output(&root, &["status", "--porcelain"])?;
    assert!(porcelain.contains(" M src/lib.rs"));

    let mut session = SequentialStdio::spawn(&root)?;
    session.call("discover", "server/discover", json!({}), TIMEOUT)?;
    let baseline = session.call_tool(
        "committed-base",
        "ripr_refresh",
        json!({"base": "trunk"}),
        TIMEOUT,
    )?;
    let baseline = structured_tool_success(&baseline, "committed explicit base")?;
    assert_eq!(baseline["attempt"]["state"], "completed");
    assert_eq!(baseline["snapshot"]["outcome_kind"], "no_scope");
    let committed_id = baseline["snapshot"]["snapshot_id"]
        .as_str()
        .ok_or("committed baseline has no snapshot")?
        .to_string();

    // The default cannot resolve trunk. The last-good remains committed;
    // the failed request must still disclose that worktree was requested.
    for (id, args) in [
        ("worktree-default", json!({"source": "worktree"})),
        (
            "worktree-bad-base",
            json!({"source": "worktree", "base": "no-such-ref"}),
        ),
    ] {
        let reset = session.call_tool(
            "reset-committed",
            "ripr_refresh",
            json!({"base": "trunk"}),
            TIMEOUT,
        )?;
        let reset = structured_tool_success(&reset, "reset committed last-good")?;
        assert_eq!(reset["snapshot"]["snapshot_id"], committed_id);
        let failed = session.call_tool(id, "ripr_refresh", args, TIMEOUT)?;
        let failed = structured_tool_success(&failed, "failed worktree base")?;
        assert_eq!(failed["attempt"]["state"], "failed");
        assert_eq!(failed["requested_source"], "worktree");
        assert_eq!(failed["captured_source"], "committed");
        assert_eq!(failed["last_known_good"]["snapshot_id"], committed_id);
        let recovery = failed
            .pointer("/attempt/failure/recovery")
            .and_then(Value::as_str)
            .ok_or("failure has no recovery")?;
        // Follow the emitted MCP retry, replacing only its documented ref
        // placeholder. A keyword-only source assertion is not the oracle.
        let begin = recovery
            .find('{')
            .ok_or_else(|| format!("missing JSON retry: {recovery}"))?;
        let end = recovery[begin..]
            .find('}')
            .map(|end| begin + end + 1)
            .ok_or_else(|| format!("unfinished JSON retry: {recovery}"))?;
        let mut retry: Value = serde_json::from_str(&recovery[begin..end])
            .map_err(|error| format!("invalid JSON retry {recovery}: {error}"))?;
        assert_eq!(retry["base"], "<ref>");
        retry["base"] = json!("trunk");
        let refreshed = session.call_tool("literal-retry", "ripr_refresh", retry, TIMEOUT)?;
        let refreshed = structured_tool_success(&refreshed, "literal worktree recovery")?;
        assert_eq!(refreshed["attempt"]["state"], "completed");
        assert_eq!(refreshed["requested_source"], "worktree");
        assert_eq!(refreshed["captured_source"], "worktree");
        assert_eq!(refreshed["snapshot"]["source"], "worktree");
        assert_eq!(refreshed["requested_base"], "trunk");
        assert!(
            refreshed["snapshot"]["total_items"]
                .as_u64()
                .is_some_and(|n| n > 0)
        );
        assert_ne!(refreshed["snapshot"]["snapshot_id"], committed_id);
        assert_eq!(refreshed["scope"]["repair_cards"], "available");
        assert_eq!(
            refreshed["scope"]["consumed_source_commitment"],
            "established"
        );
    }
    assert_eq!(
        std::fs::read_to_string(&source).map_err(|error| error.to_string())?,
        changed
    );
    // SequentialStdio's existing Drop owns process shutdown/reaping.
    Ok(())
}

#[cfg(feature = "lang-rust")]
#[test]
fn config_failure_literal_retry_preserves_requested_worktree_and_nondefault_base()
-> Result<(), String> {
    const TIMEOUT: Duration = Duration::from_mins(3);
    let root = install_b4_repair_ready_fixture()?;
    let _cleanup = FixtureGuard { root: root.clone() };
    std::fs::write(root.join(".gitignore"), "/target/\n").map_err(|error| error.to_string())?;
    commit_fixture(&root, "ignore generated analysis artifacts")?;
    // The requested base is deliberately not the available default branch.
    fixture_git_ok(
        &root,
        &["update-ref", "refs/heads/config-recovery-base", "HEAD"],
    )?;
    let source = root.join("src/lib.rs");
    let original = std::fs::read_to_string(&source).map_err(|error| error.to_string())?;
    let changed = original.replace("amount - 10", "amount - 15");
    assert_ne!(
        changed, original,
        "fixture must contain the changed behavior"
    );
    std::fs::write(&source, &changed).map_err(|error| error.to_string())?;
    let config = root.join("ripr.toml");
    assert!(
        !config.exists(),
        "fixture must begin without project config"
    );
    let mut session = SequentialStdio::spawn(&root)?;
    session.call("discover", "server/discover", json!({}), TIMEOUT)?;
    let baseline = session.call_tool(
        "config-committed-baseline",
        "ripr_refresh",
        json!({"base": "config-recovery-base"}),
        TIMEOUT,
    )?;
    let baseline = structured_tool_success(&baseline, "config committed baseline")?;
    assert_eq!(baseline["snapshot"]["outcome_kind"], "no_scope");
    let committed_id = baseline["snapshot"]["snapshot_id"]
        .as_str()
        .ok_or("no baseline id")?
        .to_string();
    std::fs::write(&config, "invalid = [\n").map_err(|error| error.to_string())?;
    let failed = session.call_tool(
        "config-worktree-failed",
        "ripr_refresh",
        json!({"source": "worktree", "base": "config-recovery-base"}),
        TIMEOUT,
    )?;
    let failed = structured_tool_success(&failed, "failed worktree config")?;
    assert_eq!(failed["attempt"]["state"], "failed");
    assert_eq!(failed["attempt"]["failure"]["code"], "config_invalid");
    assert_eq!(failed["requested_source"], "worktree");
    assert_eq!(failed["captured_source"], "committed");
    assert_eq!(failed["last_known_good"]["snapshot_id"], committed_id);
    let recovery = failed
        .pointer("/attempt/failure/recovery")
        .and_then(Value::as_str)
        .ok_or("missing config recovery")?;
    // Correct only the configuration, then follow the actual returned JSON.
    std::fs::remove_file(&config).map_err(|error| error.to_string())?;
    let begin = recovery
        .find('{')
        .ok_or_else(|| format!("config recovery has no JSON retry: {recovery}"))?;
    let end = recovery[begin..]
        .find('}')
        .map(|end| begin + end + 1)
        .ok_or("unfinished config JSON retry")?;
    let retry: Value =
        serde_json::from_str(&recovery[begin..end]).map_err(|error| error.to_string())?;
    assert_eq!(retry["source"], "worktree");
    assert_eq!(retry["base"], "config-recovery-base");
    let recovered = session.call_tool("config-literal-retry", "ripr_refresh", retry, TIMEOUT)?;
    let recovered = structured_tool_success(&recovered, "literal config recovery")?;
    assert_eq!(recovered["attempt"]["state"], "completed");
    assert_eq!(recovered["requested_base"], "config-recovery-base");
    assert_eq!(recovered["requested_source"], "worktree");
    assert_eq!(recovered["captured_source"], "worktree");
    assert_eq!(recovered["snapshot"]["source"], "worktree");
    assert!(
        recovered["snapshot"]["total_items"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    assert_ne!(recovered["snapshot"]["snapshot_id"], committed_id);
    assert_eq!(recovered["scope"]["repair_cards"], "available");
    assert_eq!(
        recovered["scope"]["consumed_source_commitment"],
        "established"
    );
    assert_eq!(
        std::fs::read_to_string(&source).map_err(|error| error.to_string())?,
        changed
    );
    Ok(())
}
