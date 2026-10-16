//! Execute the real lightweight result policy. Native artifact authentication is
//! central-owned; these synthetic imported contexts test only consumer refusal.
#![cfg(target_os = "linux")]

use std::{
    fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

const POLICY: &str = include_str!("../../.ci/contabo-result.sh");
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn policy_exit(policy: &str, context: &str, evidence: &str, setup: &str) -> Result<i32, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-result-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let result = (|| {
        fs::write(root.join("policy.sh"), policy).map_err(|e| e.to_string())?;
        let status = Command::new("bash")
            .args(["--noprofile", "--norc", "-e", "-o", "pipefail", "-c", r#"
export GITHUB_REPOSITORY=EffortlessMetrics/ripr-swarm GITHUB_EVENT_NAME=workflow_dispatch
export GITHUB_SHA=$(git rev-parse HEAD) GITHUB_RUN_ID=101 GITHUB_RUN_ATTEMPT=2
export GITHUB_EVENT_PATH="$FIXTURE/event.json" EM_CI_ARTIFACT_DIR="$FIXTURE/imported"
export EM_CI_LANE=ripr-product EM_CI_SELECTED_PROOF_RESULT=success
mkdir "$EM_CI_ARTIFACT_DIR"
# This policy fixture needs a matching distinct SHA, not a history object.
# Ancestry is authenticated by the central importer, outside this unit test.
base=0000000000000000000000000000000000000001
[[ "$base" != "$GITHUB_SHA" ]] || base=0000000000000000000000000000000000000002
tree=$(git rev-parse 'HEAD^{tree}')
jq -n --arg sha "$GITHUB_SHA" --arg base "$base" '{inputs:{expected_sha:$sha,base_sha:$base}}' > "$GITHUB_EVENT_PATH"
jq -n --arg sha "$GITHUB_SHA" --arg base "$base" --arg tree "$tree" '{
 schema:"em-ci-ripr-product-context/v1",identity:{repository:"EffortlessMetrics/ripr-swarm",subject:$sha,base:$base,tree:$tree,run:"101",attempt:"2",producer_check_run_id:42,defining_workflow_sha:"8ff77e5876b4a7808a58e053aa6248b1020e02d0"},native_outcome:"success",retention_days:14,selection:"source-derived-v1",verdict:{disposition:"product_pass",exit:0}
}' | jq "$CONTEXT_CHANGE" > "$EM_CI_ARTIFACT_DIR/em-ci-ripr-product-context.json"
export EM_CI_EVIDENCE=$(jq -nc --arg sha "$GITHUB_SHA" '{schema:"em-ci-rust-result.v1",repository:"EffortlessMetrics/ripr-swarm",sha:$sha,run_id:"101",run_attempt:"2",profile:"standard",route:"self_hosted",result:"success",proof_outcome:"success"}' | jq -c "$EVIDENCE_CHANGE")
eval "$FIXTURE_SETUP"
bash "$FIXTURE/policy.sh"
"#])
            .env("FIXTURE", &root)
            .env("CONTEXT_CHANGE", context)
            .env("EVIDENCE_CHANGE", evidence)
            .env("FIXTURE_SETUP", setup)
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
            .status().map_err(|e| e.to_string())?;
        status
            .code()
            .ok_or_else(|| "policy terminated without native exit".into())
    })();
    fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    result
}

#[test]
fn accepts_exact_import_and_refuses_stale_missing_or_nonproduct_success() -> Result<(), String> {
    assert_eq!(policy_exit(POLICY, ".", ".", ":")?, 0);
    for context in [
        ".identity.subject = (\"a\" * 40)",
        ".identity.base = (\"b\" * 40)",
        ".identity.tree = (\"c\" * 40)",
        ".identity.attempt = \"1\"",
        ".identity.run = \"100\"",
        ".identity.repository = \"fork/ripr-swarm\"",
        ".identity.defining_workflow_sha = (\"d\" * 40)",
        ".identity.producer_check_run_id = 0",
        ".native_outcome = \"failure\"",
        ".verdict.disposition = \"incomplete\"",
        ".verdict.exit = 37",
        ".retention_days = 7",
        "del(.identity)",
    ] {
        assert_ne!(
            policy_exit(POLICY, context, ".", ":")?,
            0,
            "accepted {context}"
        );
    }
    for evidence in [
        ".sha = (\"a\" * 40)",
        ".run_attempt = \"1\"",
        ".run_id = \"100\"",
        ".result = \"failure\"",
        ".proof_outcome = \"cancelled\"",
        ".route = \"hosted_fallback\"",
        ".profile = \"light\"",
        "{}",
    ] {
        assert_ne!(
            policy_exit(POLICY, ".", evidence, ":")?,
            0,
            "accepted {evidence}"
        );
    }
    for setup in [
        "unset EM_CI_ARTIFACT_DIR",
        "export EM_CI_LANE=ripr",
        "export EM_CI_SELECTED_PROOF_RESULT=failure",
        "export GITHUB_EVENT_NAME=pull_request",
        "export GITHUB_REPOSITORY=fork/ripr-swarm",
        "rm \"$EM_CI_ARTIFACT_DIR/em-ci-ripr-product-context.json\"",
        "mv \"$EM_CI_ARTIFACT_DIR\" \"$FIXTURE/original\"; ln -s \"$FIXTURE/original\" \"$EM_CI_ARTIFACT_DIR\"",
    ] {
        assert_ne!(policy_exit(POLICY, ".", ".", setup)?, 0, "accepted {setup}");
    }
    Ok(())
}

#[test]
fn refuses_malformed_dispatch_base_before_shell_normalization() -> Result<(), String> {
    for change in [
        ".inputs.base_sha += \"\\n\"",
        ".inputs.base_sha += \"\\n\\n\"",
        ".inputs.base_sha += \"0\"",
        ".inputs.base_sha = 1",
        ".inputs.base_sha = null",
        ".inputs.base_sha = []",
    ] {
        let setup = format!(
            "jq '{change}' \"$GITHUB_EVENT_PATH\" > \"$FIXTURE/event-changed.json\"; mv \"$FIXTURE/event-changed.json\" \"$GITHUB_EVENT_PATH\""
        );
        assert_ne!(
            policy_exit(POLICY, ".", ".", &setup)?,
            0,
            "accepted malformed dispatch base: {change}"
        );
    }
    Ok(())
}

#[test]
fn source_and_product_guards_have_discriminating_controls() -> Result<(), String> {
    let source_guard = ".identity.attempt == $attempt and";
    assert!(POLICY.contains(source_guard));
    assert_eq!(
        policy_exit(
            &POLICY.replace(source_guard, "true and"),
            ".identity.attempt = \"1\"",
            ".",
            ":"
        )?,
        0
    );
    let verdict_guard = ".verdict == {\"disposition\":\"product_pass\",\"exit\":0}";
    assert!(POLICY.contains(verdict_guard));
    assert_eq!(
        policy_exit(
            &POLICY.replace(verdict_guard, "true"),
            ".verdict.disposition = \"incomplete\"",
            ".",
            ":"
        )?,
        0
    );
    Ok(())
}

#[test]
fn caller_grants_only_the_four_approved_read_scopes() -> Result<(), String> {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.github/workflows/contabo-qualification.yml"),
    )
    .map_err(|e| e.to_string())?;
    // This is a static caller contract, not proof of effective GitHub access.
    // Reject job overrides as well as missing, extra or write-level grants.
    let approved = |source: &str| {
        let permissions: Vec<_> = source
            .lines()
            .filter(|line| line.trim_start().starts_with("permissions:"))
            .collect();
        let Some((_, rest)) = source.split_once("\npermissions:\n") else {
            return false;
        };
        let mut grants: Vec<_> = rest
            .lines()
            .take_while(|line| line.is_empty() || line.starts_with(' '))
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        grants.sort_unstable();
        permissions == ["permissions:"]
            && grants
                == [
                    "actions: read",
                    "checks: read",
                    "contents: read",
                    "pull-requests: read",
                ]
    };
    assert!(
        approved(&source),
        "caller must grant exactly approved read scopes"
    );
    for scope in ["actions", "checks", "contents", "pull-requests"] {
        let grant = format!("  {scope}: read\n");
        assert!(source.contains(&grant));
        assert!(!approved(&source.replace(&grant, "")), "missing {scope}");
        assert!(
            !approved(&source.replace(&grant, &format!("  {scope}: write\n"))),
            "write-level {scope}"
        );
    }
    assert!(!approved(
        &source.replace("permissions:\n", "permissions:\n  id-token: write\n")
    ));
    assert!(!approved(&source.replace(
        "  qualification:\n",
        "  qualification:\n    permissions: write-all\n"
    )));
    Ok(())
}
