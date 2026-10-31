//! Exercise the real command with a fake gh, not a settings-capable account.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

const FAKE_GH: &str = r##"#!/bin/sh
n=0
[ ! -f "$FAKE_DIR/count" ] || n=$(cat "$FAKE_DIR/count")
n=$((n + 1))
echo "$n" > "$FAKE_DIR/count"
printf '%s\n' "$*" >> "$FAKE_DIR/calls"
# Inspect the entire attempted request, including GraphQL query arguments.
case "$*" in
  *protection*|*ruleset*|*branchProtection*|*repositoryRuleset*|*credential*|*secret*|*'auth token'*)
    echo 'privileged request rejected' >&2; exit 91 ;;
esac
if [ "$FAKE_FAIL_AT" = "$n" ]; then echo 'ordinary PR read: HTTP 401' >&2; exit 1; fi
case "$1 $2" in
  'api graphql') cat "$FAKE_DIR/identity.json" ;;
  'pr view') cat "$FAKE_DIR/pr.json" ;;
  *) echo 'unexpected non-PR or settings request' >&2; exit 91 ;;
esac
"##;

fn test_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

fn run(case: &str, fail_at: &str) -> Result<(std::process::Output, String, PathBuf), String> {
    let dir = std::env::temp_dir().join(format!(
        "pr-status-no-settings-{}-{case}",
        std::process::id()
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(dir.join("bin")).map_err(|e| e.to_string())?;
    let script = dir.join("bin/gh");
    fs::write(&script, FAKE_GH).map_err(|e| e.to_string())?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    let head = "1111111111111111111111111111111111111111";
    let base = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let identity =
        serde_json::json!({"number":42,"headRefOid":head,"baseRefOid":base,"baseRefName":"main"});
    let pr = serde_json::json!({"number":42,"title":"fixture","headRefOid":head,"baseRefName":"main","headRefName":"candidate","isDraft":false,"mergeStateStatus":"CLEAN","reviewDecision":"APPROVED", "latestReviews":[{"author":{"login":"reviewer"},"state":"APPROVED","submittedAt":"2026-10-15T01:00:00Z"}], "reviews":[{"author":{"login":"reviewer"},"state":"APPROVED","submittedAt":"2026-10-15T01:00:00Z","commit":{"oid":head}}],"comments":[],"statusCheckRollup":[{"__typename":"CheckRun","name":"rust","status":"COMPLETED","conclusion":"SUCCESS"}]});
    fs::write(
        dir.join("identity.json"),
        serde_json::json!({"data":{"repository":{"pullRequest":identity}}}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    fs::write(dir.join("pr.json"), pr.to_string()).map_err(|e| e.to_string())?;
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = test_command(env!("CARGO_BIN_EXE_xtask"))
        .args(["gh-pr-status", "--pr", "42"])
        .current_dir(&dir)
        .env("PATH", path)
        .env("FAKE_DIR", &dir)
        .env("FAKE_FAIL_AT", fail_at)
        .output()
        .map_err(|e| e.to_string())?;
    let calls = fs::read_to_string(dir.join("calls")).map_err(|e| e.to_string())?;
    Ok((output, calls, dir))
}

#[test]
fn routine_command_uses_only_pr_reads_and_keeps_requirements_unknown() -> Result<(), String> {
    let (output, calls, dir) = run("ok", "")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<_> = calls.lines().collect();
    assert_eq!(lines.len(), 6, "{calls}");
    assert!(
        lines
            .iter()
            .enumerate()
            .all(|(index, line)| if index % 3 == 1 {
                line.starts_with("pr view 42 --json ")
            } else {
                line.starts_with("api graphql ")
            }),
        "{calls}"
    );
    assert!(
        !calls.contains("protection") && !calls.contains("rulesets") && !calls.contains("auth"),
        "{calls}"
    );
    let report = fs::read_to_string(dir.join("target/ripr/reports/gh-pr-status.json"))
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&report).map_err(|e| e.to_string())?;
    assert_eq!(json["required_contexts_available"], false);
    assert!(json["warnings"].to_string().contains("unknown"));
    fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn ordinary_pr_read_401_fails_without_publishing_readiness() -> Result<(), String> {
    for (case, fail_at) in [("first", "1"), ("final", "5")] {
        let (output, calls, dir) = run(case, fail_at)?;
        assert!(
            !output.status.success(),
            "ordinary read failure must not pass"
        );
        assert!(!dir.join("target/ripr/reports/gh-pr-status.json").exists());
        assert!(
            !calls.contains("protection") && !calls.contains("rulesets") && !calls.contains("auth"),
            "{calls}"
        );
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[test]
fn fake_gh_rejects_privileged_rest_and_graphql_requests() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pr-status-deny-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join("gh");
    fs::write(&script, FAKE_GH).map_err(|e| e.to_string())?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    for args in [
        vec!["api", "repos/x/y/branches/main/protection"],
        vec!["api", "repos/x/y/rulesets"],
        vec![
            "api",
            "graphql",
            "-f",
            "query={repository{branchProtectionRules{totalCount}}}",
        ],
    ] {
        let result = test_command(&script)
            .args(args)
            .env("FAKE_DIR", &dir)
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(result.status.code(), Some(91));
    }
    let calls = fs::read_to_string(dir.join("calls")).map_err(|e| e.to_string())?;
    assert_eq!(calls.lines().count(), 3);
    fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn routed_guidance_rejects_routine_settings_reads() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing repository root")?;
    for (path, required) in [
        ("AGENTS.md", "do not read branch-protection or ruleset"),
        ("CLAUDE.md", "not protection or ruleset"),
        (
            ".agents/skills/finish-pr/SKILL.md",
            "Do not read protection or ruleset settings",
        ),
        (
            ".claude/skills/finish-pr/SKILL.md",
            "Do not read protection or ruleset settings",
        ),
        (
            ".factory/skills/review-guidelines/SKILL.md",
            "rather than reading",
        ),
        (
            "docs/AGENT_WORKFLOWS.md",
            "not branch-protection or ruleset settings",
        ),
        ("docs/agent-context/validation.md", "does not need to"),
    ] {
        let guidance = fs::read_to_string(root.join(path)).map_err(|e| e.to_string())?;
        assert!(
            guidance.contains(required),
            "{path} lost settings-free guidance"
        );
        assert!(
            !guidance.contains("required_pull_request_reviews"),
            "{path} contains a privileged endpoint"
        );
        assert!(
            !guidance.contains("Query both authorities"),
            "{path} mandates settings reads"
        );
    }
    Ok(())
}

#[test]
fn request_guidance_preserves_inflight_runs_and_denied_source_boundaries() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing repository root")?;
    for path in [
        ".agents/skills/finish-pr/SKILL.md",
        ".claude/skills/finish-pr/SKILL.md",
    ] {
        let guidance = fs::read_to_string(root.join(path)).map_err(|e| e.to_string())?;
        assert!(
            guidance.contains("inspect the existing request and run state"),
            "{path}"
        );
        assert!(
            guidance
                .contains("never reapply the label over a queued or running current-subject run"),
            "{path}"
        );
        assert!(
            !guidance.contains("remove and re-add it for a new head"),
            "{path}"
        );
    }
    let handoff =
        fs::read_to_string(root.join("docs/PR_AUTOMATION.md")).map_err(|e| e.to_string())?;
    for phrase in [
        "conflict notice with no admitted run",
        "terminal infrastructure failure",
        "success on a stale head",
        "Unknown required-check membership remains unknown",
    ] {
        assert!(handoff.contains(phrase), "missing {phrase}");
    }
    let scenarios =
        fs::read_to_string(root.join("docs/agent-context/delivery-continuation-scenarios.md"))
            .map_err(|e| e.to_string())?;
    for phrase in [
        "independently authorized exact-object source fetch",
        "Stop a separately denied source fetch without rerouting",
        "frozen-source proof cannot establish live PR readiness",
    ] {
        assert!(scenarios.contains(phrase), "missing {phrase}");
    }
    Ok(())
}

#[test]
fn tool_discovery_guidance_keeps_absence_and_denial_distinct() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing repository root")?;
    let validation = fs::read_to_string(root.join("docs/agent-context/validation.md"))
        .map_err(|e| e.to_string())?;
    for phrase in [
        "PATH lookup miss alone",
        "permitted configured and local toolchain locations",
        "process-local Cargo and rustup homes",
        "present but unusable",
        "access denied",
        "Do not read a denied",
        "native-platform proof",
    ] {
        assert!(validation.contains(phrase), "missing {phrase}");
    }
    let scenarios =
        fs::read_to_string(root.join("docs/agent-context/delivery-continuation-scenarios.md"))
            .map_err(|e| e.to_string())?;
    for phrase in [
        "PATH miss, usable tool",
        "Genuine absence",
        "Unusable tool",
        "Denied tool read",
    ] {
        assert!(scenarios.contains(phrase), "missing {phrase}");
    }
    Ok(())
}

#[test]
fn denial_guidance_distinguishes_independent_writes_from_denied_operations() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing repository root")?;
    let protocol = fs::read_to_string(root.join("docs/agent-context/remote-wait-discipline.md"))
        .map_err(|e| e.to_string())?;
    for phrase in [
        "retain the tool, exact action,",
        "does not by itself establish a denial of a distinct write",
        "authorization, dependencies and actual holds separately",
        "write to obtain data from a denied read",
        "Do not retry an unchanged denial",
    ] {
        assert!(protocol.contains(phrase), "missing {phrase}");
    }
    let scenarios =
        fs::read_to_string(root.join("docs/agent-context/delivery-continuation-scenarios.md"))
            .map_err(|e| e.to_string())?;
    for phrase in [
        "Denied read, distinct prepared write",
        "separate review-comment hold",
        "Denied body write",
        "Preserve the denied body-write hold",
    ] {
        assert!(scenarios.contains(phrase), "missing {phrase}");
    }
    Ok(())
}
