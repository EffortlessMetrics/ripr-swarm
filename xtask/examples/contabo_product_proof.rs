//! #4988/#7164: execute the existing Rust Gates command owner on a reviewed
//! detached rehearsal subject. This is never admission or native queue proof.
use serde_json::{Value, json};
use std::{
    env, fs,
    process::{Command, ExitCode},
    time::Instant,
};

const OWNER: &str = ".github/workflows/rust-gates.yml";
const PORTFOLIO: &[&str] = &[
    "Formatting preflight",
    "Agent skill contracts",
    "Prepare locked owner-return fixture dependencies",
    "Repository policy preflight",
    "Evidence promotion honesty",
    "Dependency policy",
    "Process policy",
    "Network policy",
    "Workspace and all-target check",
    "All-features ripr target check",
    "Strict workspace Clippy",
    "Required Rust tests",
    "Required Rust doctests",
    "Golden output contracts",
    "Fixture contracts",
    "Verdict corpora",
    "PR evidence",
];

#[derive(Debug)]
struct Step {
    name: String,
    script: String,
    environment: Vec<(String, String)>,
}

fn portfolio(source: &str) -> Result<Vec<Step>, String> {
    let mut result = Vec::new();
    for name in PORTFOLIO {
        let marker = format!("      - name: {name}\n");
        let mut matches = source.match_indices(&marker);
        let start = matches
            .next()
            .ok_or_else(|| format!("missing selected owner step: {name}"))?
            .0;
        if matches.next().is_some() {
            return Err(format!("ambiguous selected owner step: {name}"));
        }
        let rest = &source[start + marker.len()..];
        let block = rest.split("\n      - ").next().ok_or("missing step body")?;
        for line in block
            .lines()
            .filter(|line| line.starts_with("        ") && !line.starts_with("         "))
        {
            let field = line.trim();
            if field.is_empty()
                || field.starts_with('#')
                || field.starts_with("id:")
                || field == "env:"
                || field.starts_with("run:")
            {
                continue;
            }
            if *name == "PR evidence" && field == "if: github.event_name == 'pull_request'" {
                continue;
            }
            return Err(format!(
                "unsupported selected owner metadata: {name}: {field}"
            ));
        }
        let runs = repo_policy::extract_workflow_run_blocks(block);
        if runs.len() != 1 || runs[0].text.trim().is_empty() {
            return Err(format!(
                "selected owner step needs exactly one command: {name}"
            ));
        }
        let mut environment = Vec::new();
        let mut in_env = false;
        for line in block.lines() {
            if line == "        env:" {
                in_env = true;
                continue;
            }
            if !line.trim().is_empty()
                && !line.trim_start().starts_with('#')
                && !line.starts_with("          ")
            {
                in_env = false;
            }
            if !in_env || line.trim_start().starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let (key, value) = line
                .trim()
                .split_once(':')
                .ok_or("malformed step environment")?;
            let value = value.trim().trim_matches('"');
            // Event expressions have exactly these projections for this detached
            // integration rehearsal; every other expression is unsupported.
            if value.contains("${{") {
                let supported = match key {
                    "BASE_SHA" => "${{ github.event.pull_request.base.sha }}",
                    "HEAD_SHA" => "${{ github.event.pull_request.head.sha }}",
                    "GITHUB_PR_LABELS" => {
                        "${{ join(github.event.pull_request.labels.*.name, ',') }}"
                    }
                    _ => "",
                };
                if value != supported || supported.is_empty() {
                    return Err(format!("unsupported event projection: {key}"));
                }
            } else {
                environment.push((key.to_owned(), value.to_owned()));
            }
        }
        if runs[0].text.contains("${{") {
            return Err(format!("unsupported run expression: {name}"));
        }
        result.push(Step {
            name: (*name).to_owned(),
            script: runs[0].text.clone(),
            environment,
        });
    }
    Ok(result)
}

fn subject(
    event: &Value,
    event_name: &str,
    sha: &str,
    repository: &str,
) -> Result<(String, String), String> {
    if event_name != "workflow_dispatch" || repository != "EffortlessMetrics/ripr-swarm" {
        return Err("only reviewed owned workflow_dispatch rehearsal is supported".into());
    }
    let expected = event["inputs"]["expected_sha"]
        .as_str()
        .ok_or("missing expected_sha")?;
    let base = event["inputs"]["base_sha"]
        .as_str()
        .ok_or("missing base_sha")?;
    let full_sha = |s: &str| {
        s.len() == 40
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if !full_sha(expected) || !full_sha(base) || expected != sha || expected == base {
        return Err(
            "subject/base must be distinct full SHAs and subject must equal event SHA".into(),
        );
    }
    Ok((base.into(), expected.into()))
}

fn capture(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "{program} identity probe failed: {}",
            output.status
        ));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().into())
        .map_err(|e| e.to_string())
}

fn execute(step: &Step, base: &str, head: &str) -> Result<i32, String> {
    // Same Bash fail-fast semantics as Actions; preserve the first native exit.
    let status = Command::new("bash")
        .args([
            "--noprofile",
            "--norc",
            "-e",
            "-o",
            "pipefail",
            "-c",
            &step.script,
        ])
        .envs(step.environment.iter().cloned())
        .env("BASE_SHA", base)
        .env("HEAD_SHA", head)
        .env("GITHUB_PR_LABELS", "")
        .status()
        .map_err(|e| e.to_string())?;
    Ok(status.code().unwrap_or(1))
}

fn prepare_matching_llvm(
    required: &str,
    selected: &str,
    mut run_command: impl FnMut(&str, &[&str]) -> Result<String, String>,
) -> Result<String, String> {
    if required != selected
        || required.split('.').count() != 3
        || required
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("LLVM setup requires the exact repository-selected Rust toolchain".into());
    }
    // The active one-job image is writable and ephemeral. Add this component
    // only to its selected toolchain; neither image activation nor a shared
    // host/toolchain mutation is part of this consumer setup.
    run_command(
        "rustup",
        &[
            "component",
            "add",
            "llvm-tools-preview",
            "--toolchain",
            selected,
        ],
    )?;
    let sysroot = run_command("rustc", &["--print", "sysroot"])?;
    let version = run_command("rustc", &["-vV"])?;
    if !version
        .lines()
        .any(|line| line == format!("release: {required}"))
    {
        return Err("selected compiler version differs from LLVM component toolchain".into());
    }
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("compiler host missing")?;
    if host != "x86_64-unknown-linux-gnu" || !std::path::Path::new(&sysroot).is_absolute() {
        return Err("LLVM setup requires the selected Linux x64 compiler sysroot".into());
    }
    let llvm = std::path::Path::new(&sysroot)
        .join("lib/rustlib")
        .join(host)
        .join("bin/llvm-profdata");
    let observed = run_command(
        llvm.to_str().ok_or("non-UTF-8 LLVM tool path")?,
        &["--version"],
    )?;
    if observed.trim().is_empty() {
        return Err("matching llvm-profdata returned no version".into());
    }
    Ok(observed)
}

fn run() -> Result<i32, String> {
    let event: Value = serde_json::from_slice(
        &fs::read(env::var("GITHUB_EVENT_PATH").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let (base, head) = subject(
        &event,
        &env::var("GITHUB_EVENT_NAME").unwrap_or_default(),
        &env::var("GITHUB_SHA").unwrap_or_default(),
        &env::var("GITHUB_REPOSITORY").unwrap_or_default(),
    )?;
    if capture("git", &["rev-parse", "HEAD"])? != head {
        return Err("checkout differs from exact rehearsal subject".into());
    }
    capture("git", &["merge-base", "--is-ancestor", &base, &head])?;
    let target = env::var("CARGO_TARGET_DIR")
        .map_err(|error| format!("missing governed CARGO_TARGET_DIR: {error}"))?;
    let temp = env::var("RUNNER_TEMP")
        .map_err(|error| format!("missing governed RUNNER_TEMP: {error}"))?;
    if target != format!("{temp}/cargo-target") {
        return Err("Cargo target differs from governed disposable path".into());
    }
    // .cargo/config.toml force-redirects linker temps here; nextest's store and
    // RIPR reports also intentionally remain workspace-relative (not binaries).
    fs::create_dir_all("target").map_err(|e| e.to_string())?;
    let pin: toml::Value =
        toml::from_str(&fs::read_to_string("rust-toolchain.toml").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let required = pin["toolchain"]["channel"]
        .as_str()
        .ok_or("repository Rust toolchain pin missing")?;
    let selected = env::var("RUSTUP_TOOLCHAIN")
        .map_err(|error| format!("central selected toolchain missing: {error}"))?;
    let llvm_started = Instant::now();
    let llvm_setup = prepare_matching_llvm(required, &selected, capture);
    println!(
        "{}",
        json!({"phase":"llvm_prerequisite", "toolchain":selected,
        "outcome": if llvm_setup.is_ok() { "pass" } else { "setup_refusal" },
        "duration_ms":llvm_started.elapsed().as_millis()})
    );
    let llvm_version = llvm_setup?;
    println!(
        "{}",
        json!({"phase":"llvm_prerequisite", "toolchain":selected, "version":llvm_version})
    );
    let steps = portfolio(&fs::read_to_string(OWNER).map_err(|e| e.to_string())?)?;
    println!(
        "{}",
        json!({"profile":"consumer_qualification", "subject":head, "base":base,
        "owner_blob":capture("git", &["hash-object", OWNER])?, "tree":capture("git", &["rev-parse", "HEAD^{tree}"])?,
        "run":env::var("GITHUB_RUN_ID").unwrap_or_default(), "attempt":env::var("GITHUB_RUN_ATTEMPT").unwrap_or_default(),
        "cargo_target":target, "steps":steps.len()})
    );
    for step in steps {
        let started = Instant::now();
        let code = execute(&step, &base, &head)?;
        println!(
            "{}",
            json!({"step":step.name, "command":step.script, "exit":code, "duration_ms":started.elapsed().as_millis()})
        );
        if code != 0 {
            return Ok(code);
        }
    }
    Ok(0)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("consumer qualification refused: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_dispatch_subject_refuses_metadata_and_stale_inputs() {
        let h = "a".repeat(40);
        let b = "b".repeat(40);
        let event = json!({"inputs":{"expected_sha":h,"base_sha":b}});
        assert!(
            subject(
                &event,
                "workflow_dispatch",
                &h,
                "EffortlessMetrics/ripr-swarm"
            )
            .is_ok()
        );
        for name in ["pull_request", "ready_for_review", "push", "merge_group"] {
            assert!(subject(&event, name, &h, "EffortlessMetrics/ripr-swarm").is_err());
        }
        assert!(
            subject(
                &event,
                "workflow_dispatch",
                &b,
                "EffortlessMetrics/ripr-swarm"
            )
            .is_err()
        );
        assert!(subject(&event, "workflow_dispatch", &h, "fork/ripr-swarm").is_err());
        assert!(
            subject(
                &json!({}),
                "workflow_dispatch",
                &h,
                "EffortlessMetrics/ripr-swarm"
            )
            .is_err()
        );
    }
    #[test]
    fn actual_command_owner_is_complete_and_missing_proof_refuses() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(root.join(OWNER)).map_err(|e| e.to_string())?;
        let steps = portfolio(&source)?;
        assert_eq!(steps.len(), 17);
        assert!(
            steps
                .iter()
                .any(|s| s.script == "cargo test --workspace --doc")
        );
        assert!(steps.iter().any(|s| {
            s.script
                .contains("cargo nextest run --workspace --profile ci")
        }));
        let evidence = steps
            .iter()
            .find(|s| s.name == "PR evidence")
            .ok_or("missing evidence")?;
        assert!(evidence.script.contains("ripr-pr --base"));
        assert!(
            evidence
                .environment
                .contains(&("RIPR_PR_EVIDENCE_TIMEOUT_SECS".into(), "3600".into()))
        );
        assert!(
            portfolio(&source.replace(
                "      - name: Required Rust doctests\n",
                "      - name: Removed proof\n"
            ))
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn real_shell_preserves_failure_and_prevents_later_command() -> Result<(), String> {
        let step = Step {
            name: "negative".into(),
            script: "bash -c 'exit 37'\nprintf 'wrong success'".into(),
            environment: vec![],
        };
        assert_eq!(execute(&step, "base", "head")?, 37);
        let positive = Step {
            name: "positive".into(),
            script: "test \"$BASE_SHA\" = base && test \"$HEAD_SHA\" = head".into(),
            environment: vec![],
        };
        assert_eq!(execute(&positive, "base", "head")?, 0);
        Ok(())
    }
    #[test]
    fn unsupported_owner_projections_and_metadata_refuse() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(root.join(OWNER)).map_err(|e| e.to_string())?;
        for changed in [
            source.replace(
                "BASE_SHA: ${{ github.event.pull_request.base.sha }}",
                "BASE_SHA: ${{ secrets.UNKNOWN }}",
            ),
            source.replace("id: formatting", "id: formatting\n        shell: pwsh"),
            source.replace(
                "id: formatting",
                "id: formatting\n        working-directory: other",
            ),
            source.replace("id: formatting", "id: formatting\n        if: false"),
            source.replace("if: github.event_name == 'pull_request'", "if: false"),
        ] {
            assert_ne!(source, changed);
            assert!(portfolio(&changed).is_err());
        }
        Ok(())
    }
    #[test]
    fn actual_shim_prepares_cold_workspace_before_cargo() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let shim =
            fs::read_to_string(root.join(".ci/contabo-product.sh")).map_err(|e| e.to_string())?;
        let harness = |body: &str| Step {
            name: "cold shim".into(),
            script: format!(
                "fixture=$(mktemp -d)\ntrap 'rm -rf -- \"$fixture\"' EXIT\ncd \"$fixture\"\ncargo() {{ test -d target || return 29; }}\n{body}"
            ),
            environment: vec![],
        };
        assert_eq!(execute(&harness(&shim), "base", "head")?, 0);
        let wrong = shim.replace("mkdir -p target\n", "");
        assert_ne!(shim, wrong);
        assert_eq!(execute(&harness(&wrong), "base", "head")?, 29);
        Ok(())
    }

    #[test]
    fn reusable_condition_refuses_wrong_sha_before_allocation() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(root.join(".github/workflows/contabo-qualification.yml"))
            .map_err(|e| e.to_string())?;
        let condition = source
            .lines()
            .find_map(|line| line.strip_prefix("    if: "))
            .ok_or("reusable job condition missing")?;
        let evaluate = |condition: &str,
                        event: &str,
                        expected: &str,
                        actual: &str,
                        hosted: &str|
         -> Result<bool, String> {
            let mut admitted = true;
            for term in condition.split(" && ") {
                admitted &= match term {
                    "github.event_name == 'workflow_dispatch'" => event == "workflow_dispatch",
                    "inputs.expected_sha == github.sha" => expected == actual,
                    "vars.EM_CI_GITHUB_HOSTED_ENABLED == 'false'" => hosted == "false",
                    _ => return Err(format!("unsupported workflow condition fixture: {term}")),
                };
            }
            Ok(admitted)
        };
        let head = "a".repeat(40);
        let wrong = "b".repeat(40);
        assert!(evaluate(
            condition,
            "workflow_dispatch",
            &head,
            &head,
            "false"
        )?);
        assert!(!evaluate(
            condition,
            "workflow_dispatch",
            &wrong,
            &head,
            "false"
        )?);
        assert!(!evaluate(condition, "pull_request", &head, &head, "false")?);
        assert!(!evaluate(condition, "workflow_dispatch", &head, &head, "")?);
        let old = condition.replace("inputs.expected_sha == github.sha && ", "");
        assert_ne!(condition, old);
        assert!(
            evaluate(&old, "workflow_dispatch", &wrong, &head, "false")?,
            "removing the production guard must admit the wrong source in this control"
        );
        Ok(())
    }

    #[test]
    fn matching_llvm_is_installed_before_the_native_probe() -> Result<(), String> {
        let mut installed = false;
        let mut commands = Vec::new();
        let version = prepare_matching_llvm("1.95.0", "1.95.0", |program, args| {
            commands.push(format!("{program} {}", args.join(" ")));
            match (program, args) {
                (
                    "rustup",
                    [
                        "component",
                        "add",
                        "llvm-tools-preview",
                        "--toolchain",
                        "1.95.0",
                    ],
                ) => {
                    installed = true;
                    Ok(String::new())
                }
                ("rustc", ["--print", "sysroot"]) => Ok("/job/toolchain".into()),
                ("rustc", ["-vV"]) => Ok("release: 1.95.0\nhost: x86_64-unknown-linux-gnu".into()),
                (
                    "/job/toolchain/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata",
                    ["--version"],
                ) if installed => Ok("LLVM version for selected compiler".into()),
                _ => Err("missing matching LLVM prerequisite".into()),
            }
        })?;
        assert!(!version.is_empty());
        assert_eq!(
            commands.first().map(String::as_str),
            Some("rustup component add llvm-tools-preview --toolchain 1.95.0")
        );
        assert_eq!(commands.len(), 4);
        Ok(())
    }

    #[test]
    fn wrong_toolchain_or_missing_component_is_not_a_test_skip() {
        let mut calls = 0;
        assert!(
            prepare_matching_llvm("1.95.0", "1.98.1", |_, _| {
                calls += 1;
                Ok(String::new())
            })
            .is_err()
        );
        assert_eq!(calls, 0);
        assert!(
            prepare_matching_llvm("1.95.0", "1.95.0", |_, _| {
                calls += 1;
                Err("component unavailable".into())
            })
            .is_err()
        );
        assert_eq!(
            calls, 1,
            "setup failure must stop before any compiler/test probe"
        );
    }
}
