//! Execute the shipped stdlib-Python source verifier over real Git compositions.
//! No product build, native enqueue, protected publisher or hosted claim.
#![cfg(target_os = "linux")]

#[path = "../test_support.rs"]
mod test_support;

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const ACTION: &str = include_str!("../../../.github/actions/verify-merge-group-subject/action.yml");

fn shipped_script() -> TestResult<String> {
    let (_, body) = ACTION
        .split_once("      run: |\n")
        .ok_or("shipped run block")?;
    body.lines()
        .map(|line| {
            line.strip_prefix("        ")
                .or_else(|| line.is_empty().then_some(""))
        })
        .collect::<Option<Vec<_>>>()
        .map(|lines| lines.join("\n"))
        .ok_or_else(|| "action script indentation changed".into())
}

struct Fixture {
    root: PathBuf,
    base: String,
    head: String,
    group: String,
    tree: String,
}

impl Fixture {
    fn command(
        &self,
        program: &str,
        args: &[&str],
        env: &BTreeMap<String, String>,
    ) -> TestResult<Output> {
        Ok(Command::new("timeout")
            .args(["--kill-after=1s", "45s", program])
            .args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").ok_or("PATH")?)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .envs(env)
            .current_dir(&self.root)
            .output()?)
    }

    fn git(&self, args: &[&str]) -> TestResult<String> {
        let output = self.command("git", args, &BTreeMap::new())?;
        assert!(output.status.success(), "git {args:?}: {output:?}");
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    fn new() -> TestResult<Self> {
        let mut fixture = Self {
            root: test_support::scratch("native-group-subject")?,
            base: String::new(),
            head: String::new(),
            group: String::new(),
            tree: String::new(),
        };
        fixture.git(&["init", "-b", "main"])?;
        fixture.git(&["config", "user.name", "Fixture"])?;
        fixture.git(&["config", "user.email", "fixture@example.invalid"])?;
        fixture.git(&["config", "commit.gpgsign", "false"])?;
        fs::write(fixture.root.join("base.txt"), "initial base\n")?;
        fixture.git(&["add", "."])?;
        fixture.git(&["commit", "-m", "initial"])?;
        fixture.git(&["checkout", "-b", "candidate"])?;
        fs::write(fixture.root.join("candidate.txt"), "selected H\n")?;
        fixture.git(&["add", "."])?;
        fixture.git(&["commit", "-m", "candidate"])?;
        fixture.head = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.git(&["checkout", "main"])?;
        fs::write(
            fixture.root.join("new-base.txt"),
            "base moved independently\n",
        )?;
        fixture.git(&["add", "."])?;
        fixture.git(&["commit", "-m", "base movement"])?;
        fixture.base = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.git(&["checkout", "-b", "queue"])?;
        fixture.git(&["merge", "--squash", "candidate"])?;
        fixture.git(&["commit", "-m", "native-shaped singleton (#42)"])?;
        fixture.group = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.tree = fixture.git(&["rev-parse", "HEAD^{tree}"])?;
        Ok(fixture)
    }

    fn event(&self) -> Value {
        json!({"action":"checks_requested", "repository":{
            "full_name":"EffortlessMetrics/ripr-swarm", "node_id":"R_fixture"},
            "merge_group":{"head_sha":self.group,"base_sha":self.base,
                "head_ref":format!("refs/heads/gh-readonly-queue/main/pr-42-{}", self.base),
                "base_ref":"refs/heads/main"}})
    }

    fn env(&self) -> BTreeMap<String, String> {
        [
            ("ADMITTED_HEAD", self.head.clone()),
            ("ADMITTED_BASE", self.base.clone()),
            ("ADMITTED_SUBJECT", self.group.clone()),
            ("ADMITTED_TREE", self.tree.clone()),
            ("ADMITTED_GENERATION", self.group.clone()),
            ("ADMITTED_REQUEST", "19-2".into()),
            ("GITHUB_EVENT_NAME", "merge_group".into()),
            ("GITHUB_REPOSITORY", "EffortlessMetrics/ripr-swarm".into()),
            ("GITHUB_SHA", self.group.clone()),
            ("GITHUB_WORKFLOW_SHA", self.group.clone()),
            (
                "GITHUB_REF",
                format!("refs/heads/gh-readonly-queue/main/pr-42-{}", self.base),
            ),
            ("GITHUB_RUN_ID", "19".into()),
            ("GITHUB_RUN_ATTEMPT", "2".into()),
            (
                "GITHUB_EVENT_PATH",
                self.root.join("event.json").to_string_lossy().into_owned(),
            ),
            (
                "GITHUB_OUTPUT",
                self.root.join("step-output").to_string_lossy().into_owned(),
            ),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
    }

    fn run(&self, event: &Value, env: &BTreeMap<String, String>) -> TestResult<(Output, String)> {
        fs::write(self.root.join("event.json"), serde_json::to_vec(event)?)?;
        let output_path = self.root.join("step-output");
        if output_path.exists() {
            fs::remove_file(&output_path)?;
        }
        let output = self.command("python3", &["-c", &shipped_script()?], env)?;
        let receipt = if output_path.exists() {
            fs::read_to_string(output_path)?
        } else {
            String::new()
        };
        Ok((output, receipt))
    }

    fn refused(&self, event: &Value, env: &BTreeMap<String, String>) -> TestResult {
        let (output, receipt) = self.run(event, env)?;
        assert!(
            !output.status.success(),
            "wrong subject accepted: {event}/{env:?}"
        );
        assert!(receipt.is_empty(), "failure emitted a subject receipt");
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn actual_squash_group_passes_without_imaginary_pr_merge_parents() -> TestResult {
    let fixture = Fixture::new()?;
    assert_eq!(
        fixture.git(&["show", "-s", "--format=%P", "HEAD"])?,
        fixture.base
    );
    assert_ne!(
        fixture.tree,
        fixture.git(&["rev-parse", "candidate^{tree}"])?
    );
    let (output, receipt) = fixture.run(&fixture.event(), &fixture.env())?;
    assert!(output.status.success(), "{output:?}");
    let json = receipt
        .lines()
        .find_map(|line| line.strip_prefix("subject_json="))
        .ok_or("subject output")?;
    let value: Value = serde_json::from_str(json)?;
    assert_eq!(value["version"], 3);
    assert_eq!(value["kind"], "merge-group");
    assert_eq!(value["merge"], fixture.group);
    assert_eq!(value["head"], fixture.head);
    assert_eq!(value["parents"], json!([fixture.base]));
    assert_eq!(value["request_id"], "19-2");
    Ok(())
}

#[test]
fn evidence_diff_uses_the_composed_group_after_base_movement() -> TestResult {
    let fixture = Fixture::new()?;
    let integration_diff =
        fixture.git(&["diff", "--name-status", &fixture.base, &fixture.group])?;
    let head_diff = fixture.git(&["diff", "--name-status", &fixture.base, &fixture.head])?;
    assert_eq!(integration_diff, "A\tcandidate.txt");
    assert_eq!(head_diff, "A\tcandidate.txt\nD\tnew-base.txt");
    let binding = "HEAD_SHA: ${{ steps.admitted-group.outputs.merge || steps.admitted.outputs.head || github.event.pull_request.head.sha || github.event.merge_group.head_sha }}";
    let workflow = include_str!("../../../.github/workflows/rust-gates.yml");
    assert!(workflow.contains(binding));
    let wrong = workflow.replace(
        "HEAD_SHA: ${{ steps.admitted-group.outputs.merge",
        "HEAD_SHA: ${{ steps.admitted-group.outputs.head",
    );
    assert_ne!(wrong, workflow);
    assert!(!wrong.contains(binding));
    Ok(())
}

#[test]
fn preserved_pr_profile_rejects_squash_g_while_new_profile_reaches_native_verifier() -> TestResult {
    let fixture = Fixture::new()?;
    let workflow = include_str!("../../../.github/workflows/ci.yml");
    let (_, tail) = workflow
        .split_once("      - name: Verify exact admitted source\n")
        .ok_or("CI source guard")?;
    let (_, tail) = tail
        .split_once("        run: |\n")
        .ok_or("CI source run block")?;
    let script = tail
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let mut env = fixture.env();
    for (name, value) in [
        ("PROFILE", "qualification"),
        ("REQUEST", "19-2"),
        ("HEAD_SHA", fixture.head.as_str()),
        ("BASE_SHA", fixture.base.as_str()),
        ("SUBJECT", fixture.group.as_str()),
        ("TREE", fixture.tree.as_str()),
        ("GENERATION", fixture.group.as_str()),
        ("SELECT_RUST", "true"),
        ("SELECT_RELEASE", "true"),
        ("SELECT_MSRV", "true"),
        ("SELECT_VSCODE", "true"),
    ] {
        env.insert(name.into(), value.into());
    }
    let old_profile = fixture.command("bash", &["-c", &script], &env)?;
    assert!(!old_profile.status.success());
    assert!(String::from_utf8(old_profile.stderr)?.contains("Wrong integration parents"));
    env.insert("PROFILE".into(), "merge-group".into());
    assert!(
        fixture
            .command("bash", &["-c", &script], &env)?
            .status
            .success()
    );
    assert!(
        fixture
            .run(&fixture.event(), &fixture.env())?
            .0
            .status
            .success()
    );
    Ok(())
}

#[test]
fn native_composition_is_observed_instead_of_mandating_a_parent_shape() -> TestResult {
    let mut fixture = Fixture::new()?;
    fixture.git(&["checkout", "-b", "merge-shaped-queue", &fixture.base])?;
    fixture.git(&["merge", "--no-ff", "--no-edit", "candidate"])?;
    fixture.group = fixture.git(&["rev-parse", "HEAD"])?;
    let (output, receipt) = fixture.run(&fixture.event(), &fixture.env())?;
    assert!(output.status.success(), "{output:?}");
    assert!(receipt.contains(&fixture.group));
    // This is source capability only; it does not change the native queue method.
    Ok(())
}

#[test]
fn stale_wrong_missing_or_foreign_event_and_run_identity_refuse() -> TestResult {
    let fixture = Fixture::new()?;
    for (key, wrong) in [
        ("GITHUB_EVENT_NAME", "pull_request"),
        ("GITHUB_EVENT_NAME", "workflow_dispatch"),
        ("GITHUB_REPOSITORY", "foreign/repo"),
        ("GITHUB_REF", "refs/heads/main"),
        ("GITHUB_SHA", fixture.head.as_str()),
        ("ADMITTED_SUBJECT", fixture.head.as_str()),
        ("ADMITTED_TREE", fixture.head.as_str()),
        ("ADMITTED_GENERATION", fixture.head.as_str()),
        ("ADMITTED_REQUEST", "19-1"),
        ("ADMITTED_HEAD", ""),
        ("GITHUB_RUN_ATTEMPT", ""),
    ] {
        let mut env = fixture.env();
        env.insert(key.into(), wrong.into());
        fixture.refused(&fixture.event(), &env)?;
    }
    for key in ["merge_group", "repository", "action"] {
        let mut event = fixture.event();
        event[key] = Value::Null;
        fixture.refused(&event, &fixture.env())?;
    }
    let mut event = fixture.event();
    event["merge_group"]["base_ref"] = json!("refs/heads/other");
    fixture.refused(&event, &fixture.env())?;
    Ok(())
}

#[test]
fn bare_head_dirty_source_or_unadmitted_composition_cannot_stand_in_for_g() -> TestResult {
    let mut fixture = Fixture::new()?;
    fixture.git(&["checkout", "candidate"])?;
    fixture.refused(&fixture.event(), &fixture.env())?;
    fixture.git(&["checkout", "queue"])?;
    fs::write(fixture.root.join("candidate.txt"), "uncommitted\n")?;
    fixture.refused(&fixture.event(), &fixture.env())?;
    fixture.git(&["checkout", "--", "candidate.txt"])?;
    fs::write(
        fixture.root.join("extra.txt"),
        "another queued constituent\n",
    )?;
    fixture.git(&["add", "extra.txt"])?;
    fixture.git(&["commit", "-m", "unadmitted composition"])?;
    fixture.group = fixture.git(&["rev-parse", "HEAD"])?;
    fixture.tree = fixture.git(&["rev-parse", "HEAD^{tree}"])?;
    fixture.refused(&fixture.event(), &fixture.env())?;
    Ok(())
}

#[test]
fn caller_receipt_must_match_observed_native_subject_and_complete_digest() -> TestResult {
    let fixture = Fixture::new()?;
    let (output, receipt) = fixture.run(&fixture.event(), &fixture.env())?;
    assert!(output.status.success(), "{output:?}");
    let subject = receipt
        .lines()
        .find_map(|line| line.strip_prefix("subject_json="))
        .ok_or("subject")?;
    let digest = receipt
        .lines()
        .find_map(|line| line.strip_prefix("subject_identity="))
        .ok_or("identity")?;
    let mut env = fixture.env();
    env.insert("EXPECTED_SUBJECT_JSON".into(), subject.into());
    env.insert("EXPECTED_SUBJECT_IDENTITY".into(), digest.into());
    assert!(fixture.run(&fixture.event(), &env)?.0.status.success());
    let mut wrong: Value = serde_json::from_str(subject)?;
    wrong["parents"] = json!([fixture.base, fixture.head]);
    env.insert(
        "EXPECTED_SUBJECT_JSON".into(),
        serde_json::to_string(&wrong)?,
    );
    fixture.refused(&fixture.event(), &env)?;
    env.insert("EXPECTED_SUBJECT_JSON".into(), subject.into());
    env.insert("EXPECTED_SUBJECT_IDENTITY".into(), "0".repeat(64));
    fixture.refused(&fixture.event(), &env)?;
    env.remove("EXPECTED_SUBJECT_IDENTITY");
    fixture.refused(&fixture.event(), &env)?;
    Ok(())
}

#[test]
fn required_caller_receipt_keeps_the_frozen_constituent_binding() -> TestResult {
    let fixture = Fixture::new()?;
    let mut env = fixture.env();
    env.insert("CONSTITUENT_PR_NUMBER".into(), "42".into());
    env.insert("CONSTITUENT_PR_NODE_ID".into(), "PR_fixture".into());
    env.insert("CONSTITUENT_HEAD".into(), fixture.head.clone());
    let (output, receipt) = fixture.run(&fixture.event(), &env)?;
    assert!(output.status.success(), "{output:?}");
    env.insert("REQUIRE_CALLER_RECEIPT".into(), "true".into());
    fixture.refused(&fixture.event(), &env)?;
    for (prefix, key) in [
        ("subject_json=", "EXPECTED_SUBJECT_JSON"),
        ("subject_identity=", "EXPECTED_SUBJECT_IDENTITY"),
    ] {
        let value = receipt
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .ok_or("complete receipt")?;
        env.insert(key.into(), value.into());
    }
    assert!(fixture.run(&fixture.event(), &env)?.0.status.success());
    for (key, value) in [
        ("CONSTITUENT_PR_NUMBER", "43"),
        ("CONSTITUENT_PR_NODE_ID", ""),
        ("CONSTITUENT_HEAD", fixture.base.as_str()),
    ] {
        let mut wrong = env.clone();
        wrong.insert(key.into(), value.into());
        fixture.refused(&fixture.event(), &wrong)?;
    }
    Ok(())
}

fn leaf_group_binding(source: &str, hyphens: bool) -> bool {
    let marker = "      - name: Verify actual native merge-group subject\n";
    let Some((_, tail)) = source.split_once(marker) else {
        return false;
    };
    let step = tail.split("\n      - ").next().unwrap_or("");
    let profile = if hyphens {
        "proof-profile"
    } else {
        "proof_profile"
    };
    step.contains(&format!("        if: inputs.{profile} == 'merge-group'\n"))
        && step.contains("        uses: ./.github/actions/verify-merge-group-subject\n")
        && !step.contains("continue-on-error:")
        && [
            "head-sha",
            "base-sha",
            "subject-sha",
            "subject-tree",
            "workflow-generation",
        ]
        .iter()
        .all(|key| {
            let input = if hyphens {
                key.to_string()
            } else {
                key.replace('-', "_")
            };
            step.contains(&format!("          {key}: ${{{{ inputs.{input} }}}}\n"))
        })
        && (if hyphens {
            step.contains("request-id: ${{ format('{0}-{1}', github.run_id, github.run_attempt) }}")
                && step.contains("expected-subject-json: ${{ inputs.subject-json }}")
                && step.contains("expected-subject-identity: ${{ inputs.subject-identity }}")
                && step.contains("require-caller-receipt: 'true'")
                && step.contains("constituent-pr-number: ${{ inputs.constituent-pr-number }}")
                && step.contains("constituent-pr-node-id: ${{ inputs.constituent-pr-node-id }}")
                && step.contains("constituent-head-sha: ${{ inputs.constituent-head-sha }}")
        } else {
            step.contains("request-id: ${{ inputs.request_id }}")
        })
}

#[test]
fn every_callable_leaf_requires_the_same_g_guard_before_product_work() -> TestResult {
    let root = test_support::workspace_root()?;
    for file in [
        "ci.yml",
        "printed-command-paste.yml",
        "python-wheel-qualification.yml",
        "windows-subset-qualification.yml",
        "rust-gates.yml",
    ] {
        let source = fs::read_to_string(root.join(".github/workflows").join(file))?;
        let hyphens = file == "rust-gates.yml";
        assert!(leaf_group_binding(&source, hyphens), "{file}");
        for changed in [
            source.replace(
                "uses: ./.github/actions/verify-merge-group-subject",
                "uses: ./.github/actions/other",
            ),
            source.replace(" == 'merge-group'", " == 'never'"),
            source
                .replace(
                    "subject-sha: ${{ inputs.subject_sha }}",
                    "subject-sha: ${{ inputs.head_sha }}",
                )
                .replace(
                    "subject-sha: ${{ inputs.subject-sha }}",
                    "subject-sha: ${{ inputs.head-sha }}",
                ),
        ] {
            assert_ne!(changed, source, "control changes {file}");
            assert!(!leaf_group_binding(&changed, hyphens), "{file}");
        }
        let guard = source
            .find("- name: Verify actual native merge-group subject")
            .ok_or("guard")?;
        let product_setup = source
            .find("uses: dtolnay/rust-toolchain@")
            .ok_or("toolchain")?;
        assert!(
            guard < product_setup,
            "{file} must verify before product setup"
        );
    }
    Ok(())
}

const CALLER: &str = include_str!("../../../.github/workflows/routed-rust.yml");

fn caller_step(name: &str) -> TestResult<&'static str> {
    let marker = format!("      - name: {name}\n");
    let (_, tail) = CALLER.split_once(&marker).ok_or("caller step")?;
    Ok(tail.split("\n      - ").next().ok_or("step body")?)
}

fn caller_shell(name: &str) -> TestResult<String> {
    let (_, body) = caller_step(name)?
        .split_once("        run: |\n")
        .ok_or("caller script")?;
    Ok(body
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn rust_step(name: &str) -> TestResult<&'static str> {
    let source = include_str!("../../../.github/workflows/rust-gates.yml");
    let marker = format!("      - name: {name}\n");
    let (_, tail) = source.split_once(&marker).ok_or("Rust step")?;
    Ok(tail.split("\n      - ").next().ok_or("step body")?)
}

#[test]
fn owned_profile_requires_exact_native_group_heavy_route_and_private_execution() -> TestResult {
    let fixture = Fixture::new()?;
    let (_, body) = rust_step("Validate execution profile")?
        .split_once("        run: |\n")
        .ok_or("profile script")?;
    let script = body
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let mut env = fixture.env();
    for (key, value) in [
        ("EXECUTION_PROFILE", "owned-jit"),
        ("PROOF_PROFILE", "merge-group"),
        ("USE_SCRATCH", "false"),
        ("TEST_SHARDS", "1"),
        (
            "RUNNER_CONFIG",
            r#"{"group":"em-ci-build","labels":["self-hosted","linux","x64","rust-heavy"]}"#,
        ),
    ] {
        env.insert(key.into(), value.into());
    }
    assert!(
        fixture
            .command("bash", &["-c", &script], &env)?
            .status
            .success()
    );
    for (key, value) in [
        ("EXECUTION_PROFILE", "unknown"),
        ("PROOF_PROFILE", "legacy"),
        ("PROOF_PROFILE", "qualification"),
        ("USE_SCRATCH", "true"),
        ("TEST_SHARDS", "4"),
        ("RUNNER_CONFIG", "\"ubuntu-latest\""),
    ] {
        let mut wrong = env.clone();
        wrong.insert(key.into(), value.into());
        assert!(
            !fixture
                .command("bash", &["-c", &script], &wrong)?
                .status
                .success(),
            "{key}"
        );
    }
    Ok(())
}

#[test]
fn owned_profile_cannot_touch_legacy_mounts_or_the_hosted_cache_writer() -> TestResult {
    for name in [
        "Acquire scratch lease",
        "Reclaim terminal scratch",
        "Prepare toolchain temp",
        "Configure scratch environment",
        "Prepare scratch",
        "Clean scratch",
        "Release scratch lease",
    ] {
        let step = rust_step(name)?;
        assert!(
            step.contains("inputs.execution-profile == 'legacy' && inputs.use-scratch"),
            "{name}"
        );
        let wrong = step.replace("inputs.execution-profile == 'legacy' && ", "");
        assert_ne!(wrong, step);
        assert!(!wrong.contains("inputs.execution-profile == 'legacy' && inputs.use-scratch"));
    }
    assert!(
        rust_step("Restore hosted Rust cache")?
            .contains("inputs.execution-profile == 'legacy' && !inputs.use-scratch")
    );
    let environment = rust_step("Configure owned JIT environment")?;
    for required in [
        "\"$CARGO_HOME\" == /cache/cargo",
        "\"$SCCACHE_DIR\" == /cache/sccache",
        "\"$CARGO_INCREMENTAL\" == 0",
        "\"$CARGO_BUILD_JOBS\" == 8",
        "\"$NEXTEST_TEST_THREADS\" == 8",
        "\"$RUST_TEST_THREADS\" == 8",
        "CARGO_TARGET_DIR=$GITHUB_WORKSPACE/target",
        "TMPDIR=$GITHUB_WORKSPACE/target/tmp",
        "RUSTUP_TOOLCHAIN=1.95.0",
    ] {
        assert!(environment.contains(required), "{required}");
    }
    assert!(
        !environment.contains("/mnt/")
            && !environment.contains("rm -rf")
            && !environment.contains("--install")
    );
    Ok(())
}

fn caller_portfolio_binding(source: &str) -> bool {
    let Some((_, result)) = source.split_once("\n  result:\n") else {
        return false;
    };
    let needs = result.lines().find(|line| line.starts_with("    needs: "));
    let Some(needs) = needs else { return false };
    for job in [
        "windows-subset",
        "python-wheel",
        "ci-feature",
        "printed-command",
    ] {
        if !needs.contains(job) {
            return false;
        }
        let marker = format!("\n  {job}:\n");
        let Some((_, body)) = source.split_once(&marker) else {
            return false;
        };
        let body = body.split("\n\n  ").next().unwrap_or("");
        if !body.contains("    needs: [admission]\n")
            || !body.contains("    if: github.event_name == 'merge_group' && needs.admission.result == 'success' && needs.admission.outputs.status == 'admitted'\n")
            || !body.contains("      proof_profile: merge-group\n")
            || !body.contains("      subject_sha: ${{ needs.admission.outputs.merge }}\n")
            || !body.contains("      head_sha: ${{ needs.admission.outputs.head }}\n")
            || !body.contains("      base_sha: ${{ needs.admission.outputs.base }}\n")
            || !body.contains("      subject_tree: ${{ needs.admission.outputs.tree }}\n")
            || !body.contains("      request_id: ${{ needs.admission.outputs.request }}\n")
            || !body.lines().any(|line| line == "      workflow_generation: ${{ github.workflow_sha }}")
        { return false; }
    }
    source.matches("      proof-profile: merge-group").count() == 1
        && source.contains("      execution-profile: owned-jit")
        && result.contains("if: always() && github.event_name == 'merge_group'")
        && result.contains("GROUP_PROOF_RESULT: ${{ steps.group_proof.outcome }}")
        && result.contains("\"$GROUP_PROOF_RESULT\" == success")
        && [
            "run_rust_only",
            "run_release_proof",
            "run_msrv",
            "run_vscode",
        ]
        .iter()
        .all(|input| source.contains(&format!("      {input}: true\n")))
}

#[test]
fn caller_requires_every_frozen_native_group_leaf_on_the_owned_route() {
    assert!(caller_portfolio_binding(CALLER));
    for wrong in [
        CALLER.replace(
            ", windows-subset, python-wheel, ci-feature, printed-command",
            "",
        ),
        CALLER.replace(
            "      subject_sha: ${{ needs.admission.outputs.merge }}",
            "      subject_sha: ${{ needs.admission.outputs.head }}",
        ),
        CALLER.replace("proof_profile: merge-group", "proof_profile: qualification"),
        CALLER.replace("      run_rust_only: true", "      run_rust_only: false"),
        CALLER.replace(
            "GROUP_PROOF_RESULT: ${{ steps.group_proof.outcome }}",
            "GROUP_PROOF_RESULT: success",
        ),
    ] {
        assert_ne!(wrong, CALLER);
        assert!(!caller_portfolio_binding(&wrong));
    }
}

#[test]
fn production_aggregate_refuses_missing_failed_skipped_cancelled_or_wrong_subject_leaf()
-> TestResult {
    let fixture = Fixture::new()?;
    let (output, receipt) = fixture.run(&fixture.event(), &fixture.env())?;
    assert!(output.status.success());
    let mut env = fixture.env();
    for (prefix, key) in [
        ("subject_json=", "SUBJECT"),
        ("subject_identity=", "IDENTITY"),
    ] {
        let value = receipt
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .ok_or("receipt")?;
        env.insert(key.into(), value.into());
    }
    for (key, value) in [
        ("ADMISSION_RESULT", "success"),
        ("HEAD", &fixture.head),
        ("BASE", &fixture.base),
        ("GROUP", &fixture.group),
        ("GENERATION", &fixture.group),
        ("OWNED_RESULT", "success"),
        ("WINDOWS_RESULT", "success"),
        ("WHEEL_RESULT", "success"),
        ("CI_RESULT", "success"),
        ("PRINTED_RESULT", "success"),
    ] {
        env.insert(key.into(), value.into());
    }
    let script = caller_shell("Require complete native group portfolio")?;
    assert!(
        fixture
            .command("bash", &["-c", &script], &env)?
            .status
            .success()
    );
    for name in [
        "OWNED_RESULT",
        "WINDOWS_RESULT",
        "WHEEL_RESULT",
        "CI_RESULT",
        "PRINTED_RESULT",
        "ADMISSION_RESULT",
    ] {
        for result in ["", "failure", "skipped", "cancelled"] {
            let mut wrong = env.clone();
            wrong.insert(name.into(), result.into());
            assert!(
                !fixture
                    .command("bash", &["-c", &script], &wrong)?
                    .status
                    .success(),
                "{name}={result}"
            );
        }
    }
    for (name, value) in [
        ("GROUP", fixture.head.as_str()),
        ("GITHUB_RUN_ATTEMPT", "3"),
        ("IDENTITY", ""),
    ] {
        let mut wrong = env.clone();
        wrong.insert(name.into(), value.into());
        assert!(
            !fixture
                .command("bash", &["-c", &script], &wrong)?
                .status
                .success(),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn docs_only_group_cannot_bypass_full_portfolio_or_the_rust_result() -> TestResult {
    let fixture = Fixture::new()?;
    let mut env = fixture.env();
    for (key, value) in [
        ("EVENT_NAME", "merge_group"),
        ("ADMISSION_RESULT", "success"),
        ("ADMISSION_STATUS", "admitted"),
        ("ELIGIBILITY_RESULT", "success"),
        ("DOCS_DETECT_RESULT", "success"),
        ("DOCS_ONLY", "true"),
        ("DOCS_GATE_RESULT", "success"),
        ("ROUTE_RESULT", "success"),
        ("TARGET", "github"),
        ("OWNED_RESULT", "success"),
        ("CUT_STATUS", "passed"),
        ("GROUP_PROOF_RESULT", "success"),
    ] {
        env.insert(key.into(), value.into());
    }
    env.insert(
        "GITHUB_STEP_SUMMARY".into(),
        fixture.root.join("summary").to_string_lossy().into_owned(),
    );
    let script = caller_shell("Summarize routed result")?;
    assert!(
        fixture
            .command("bash", &["-c", &script], &env)?
            .status
            .success()
    );
    for name in ["OWNED_RESULT", "GROUP_PROOF_RESULT", "ELIGIBILITY_RESULT"] {
        for value in ["", "skipped", "failure", "cancelled"] {
            let mut wrong = env.clone();
            wrong.insert(name.into(), value.into());
            assert!(
                !fixture
                    .command("bash", &["-c", &script], &wrong)?
                    .status
                    .success(),
                "{name}={value}"
            );
        }
    }
    Ok(())
}
