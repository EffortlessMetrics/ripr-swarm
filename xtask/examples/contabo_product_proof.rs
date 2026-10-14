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
                if !["BASE_SHA", "HEAD_SHA", "GITHUB_PR_LABELS"].contains(&key) {
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
    let target = env::var("CARGO_TARGET_DIR").map_err(|_| "missing governed CARGO_TARGET_DIR")?;
    let temp = env::var("RUNNER_TEMP").map_err(|_| "missing governed RUNNER_TEMP")?;
    if target != format!("{temp}/cargo-target") {
        return Err("Cargo target differs from governed disposable path".into());
    }
    // .cargo/config.toml force-redirects linker temps here; nextest's store and
    // RIPR reports also intentionally remain workspace-relative (not binaries).
    fs::create_dir_all("target").map_err(|e| e.to_string())?;
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
            script: "exit 37\nprintf 'wrong success'".into(),
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
}
