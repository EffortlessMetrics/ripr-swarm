//! #4988/#7164: execute the existing Rust Gates command owner on a reviewed
//! detached rehearsal subject. This is never admission or native queue proof.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Write,
    num::NonZeroUsize,
    path::Path,
    process::{Command, ExitCode},
    time::Instant,
};

const OWNER: &str = ".github/workflows/rust-gates.yml";
const PRODUCT_CONTRACT: &str = ".ci/contabo-product-contract.json";
const COMMAND_RECEIPT: &str = "target/ripr/contabo/commands.json";
const CLIPPY_BUILD_JOBS: &str = "${{ inputs.use-scratch && inputs.cargo-build-jobs || '3' }}";
const PORTFOLIO: &[&str] = &[
    "Formatting preflight",
    "Agent skill contracts",
    "Prepare locked owner-return fixture dependencies",
    "Repository policy preflight",
    "Evidence promotion honesty",
    "Strict all-features ripr Clippy",
    "Strict workspace Clippy",
    "Golden and fixture contracts",
    "Verdict corpora",
    "Required Rust tests",
    "Required Rust doctests",
    "PR evidence",
];

#[derive(Debug)]
struct Step {
    name: String,
    script: String,
    environment: Vec<(String, String)>,
}

fn self_hosted_build_jobs(route: &str, value: &str) -> Result<NonZeroUsize, String> {
    // The immutable central workflow marks owned execution, and its slot owner
    // supplies CARGO_BUILD_JOBS from the selected resource profile. This
    // rehearsal models that self-hosted branch of the Rust Gates owner only.
    if route != "true" || value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(
            "consumer requires the owned route and a positive Cargo build-job bound".into(),
        );
    }
    value
        .parse()
        .map_err(|_| "consumer Cargo build-job bound must be a positive integer".into())
}

fn portfolio(source: &str, build_jobs: NonZeroUsize) -> Result<Vec<Step>, String> {
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
            if *name == "PR evidence"
                && field
                    == "if: github.event_name == 'pull_request' || github.event_name == 'merge_group'"
            {
                continue;
            }
            // The importer rehearses the unpartitioned form: `test-shards` defaults
            // to 1 on the self-hosted routes it models, so this one condition (#10507)
            // is true there. Any other condition, or this one elsewhere, refuses.
            if *name == "Required Rust tests" && field == "if: inputs.test-shards == 1" {
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
            if key == "CARGO_BUILD_JOBS" {
                if *name != "Strict workspace Clippy" || value != CLIPPY_BUILD_JOBS {
                    return Err(format!("unsupported build-job projection: {name}"));
                }
                // Preserve the selected slot's capacity. The hosted '3' branch
                // is not the consumer's route, and unknown expressions refuse.
                environment.push((key.to_owned(), build_jobs.to_string()));
                continue;
            }
            // The detached rehearsal uses its reviewed subject, not a queue's
            // admission record. Explicit empty projections prevent ambient
            // history-sync inputs from suppressing ordinary evidence producers.
            let projection = match key {
                "BASE_SHA" => Some((
                    "${{ steps.admitted.outputs.base || github.event.pull_request.base.sha || github.event.merge_group.base_sha }}",
                    None,
                )),
                "HEAD_SHA" => Some((
                    "${{ steps.admitted.outputs.head || github.event.pull_request.head.sha || github.event.merge_group.head_sha }}",
                    None,
                )),
                "GITHUB_PR_LABELS" => Some((
                    "${{ inputs.pr-labels || join(github.event.pull_request.labels.*.name, ',') }}",
                    None,
                )),
                "EVENT_NAME" => Some(("${{ github.event_name }}", Some("workflow_dispatch"))),
                "ADMISSION_REASON" => Some(("${{ inputs.admission-reason }}", Some(""))),
                "ADMISSION_HEAD" => Some(("${{ inputs.admission-head }}", Some(""))),
                "SYNC_SOURCE" => Some(("${{ inputs.history-sync-source }}", Some(""))),
                "SYNC_BASE" => Some(("${{ inputs.history-sync-base }}", Some(""))),
                _ => None,
            };
            if let Some((expected, projected)) = projection {
                if *name != "PR evidence" || value != expected {
                    return Err(format!("unsupported event projection: {key}"));
                }
                if let Some(projected) = projected {
                    environment.push((key.to_owned(), projected.to_owned()));
                }
            } else if value.contains("${{") {
                return Err(format!("unsupported event projection: {key}"));
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

fn command_row(step: &Step, code: i32, duration_ms: u128) -> Value {
    json!({"step":step.name, "command":step.script,
        "command_sha256":format!("{:x}", Sha256::digest(step.script.as_bytes())),
        "outcome":if code == 0 { "pass" } else { "failure" },
        "exit":code, "duration_ms":duration_ms})
}

fn retain_terminal_commands(
    context: &Value,
    rows: &[Value],
    code: i32,
    path: &Path,
) -> Result<(), String> {
    let ordered = rows.len() <= PORTFOLIO.len()
        && rows
            .iter()
            .enumerate()
            .all(|(index, row)| row["step"] == PORTFOLIO[index] && row["exit"].as_i64().is_some());
    let terminal = if code == 0 {
        rows.len() == PORTFOLIO.len() && rows.iter().all(|row| row["exit"] == 0)
    } else {
        rows.last().is_some_and(|row| row["exit"] == code)
            && rows
                .iter()
                .take(rows.len().saturating_sub(1))
                .all(|row| row["exit"] == 0)
    };
    if !ordered || !terminal {
        return Err(
            "terminal command receipt cannot promote incomplete or discordant execution".into(),
        );
    }
    let mut receipt = context.clone();
    receipt["schema"] = json!("ripr-swarm-product-commands/v1");
    receipt["disposition"] = json!(if code == 0 {
        "product_pass"
    } else {
        "product_failure"
    });
    receipt["exit"] = json!(code);
    receipt["steps"] = json!(
        PORTFOLIO
            .iter()
            .enumerate()
            .map(|(index, name)| {
                rows.get(index)
                    .cloned()
                    .unwrap_or_else(|| json!({"step":name,"outcome":"not_executed"}))
            })
            .collect::<Vec<_>>()
    );
    fs::create_dir_all(path.parent().ok_or("command receipt has no parent")?)
        .map_err(|error| error.to_string())?;
    // Never overwrite an earlier attempt or follow a pre-existing receipt link.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("fresh terminal command receipt unavailable: {error}"))?;
    file.write_all(&serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
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
    let build_jobs = self_hosted_build_jobs(
        &env::var("EM_CI_EXECUTION_SELF_HOSTED").unwrap_or_default(),
        &env::var("CARGO_BUILD_JOBS").unwrap_or_default(),
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
    let steps = portfolio(
        &fs::read_to_string(OWNER).map_err(|e| e.to_string())?,
        build_jobs,
    )?;
    let context = json!({"profile":"consumer_qualification", "repository":"EffortlessMetrics/ripr-swarm",
        "subject":head, "base":base,
        "owner_blob":capture("git", &["hash-object", OWNER])?, "tree":capture("git", &["rev-parse", "HEAD^{tree}"])?,
        "artifact_contract_blob":capture("git", &["hash-object", PRODUCT_CONTRACT])?,
        "run":env::var("GITHUB_RUN_ID").unwrap_or_default(), "attempt":env::var("GITHUB_RUN_ATTEMPT").unwrap_or_default(),
        "cargo_target":target, "selected_steps":steps.len()});
    println!("{context}");
    let mut rows = Vec::new();
    for step in steps {
        let started = Instant::now();
        let code = execute(&step, &base, &head)?;
        let row = command_row(&step, code, started.elapsed().as_millis());
        println!("{row}");
        rows.push(row);
        if code != 0 {
            // Receipt failure cannot replace the original product failure.
            if let Err(error) =
                retain_terminal_commands(&context, &rows, code, Path::new(COMMAND_RECEIPT))
            {
                eprintln!("terminal receipt incomplete: {error}");
            }
            return Ok(code);
        }
    }
    retain_terminal_commands(&context, &rows, 0, Path::new(COMMAND_RECEIPT))?;
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

// This consumer executes the Linux/Bash contract of the owned Contabo route.
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn terminal_receipt_retains_native_failure_and_refuses_prior_attempt() -> Result<(), String> {
        let step = Step {
            name: PORTFOLIO[0].into(),
            script: "exit 37".into(),
            environment: vec![],
        };
        let code = execute(&step, "base", "head")?;
        let row = command_row(&step, code, 1);
        let root = env::temp_dir().join(format!("contabo-terminal-{}", std::process::id()));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let path = root.join("commands.json");
        let result = (|| {
            retain_terminal_commands(
                &json!({"subject":"fixture"}),
                std::slice::from_ref(&row),
                code,
                &path,
            )?;
            let bytes = fs::read(&path).map_err(|error| error.to_string())?;
            let receipt: Value =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            assert_eq!(receipt["disposition"], "product_failure");
            assert_eq!(receipt["exit"], 37);
            assert_eq!(receipt["steps"][0]["exit"], 37);
            assert_eq!(receipt["steps"][1]["outcome"], "not_executed");
            assert_eq!(
                receipt["steps"].as_array().map(Vec::len),
                Some(PORTFOLIO.len())
            );
            assert!(
                retain_terminal_commands(&json!({}), &[], 0, &root.join("missing.json")).is_err()
            );
            assert!(!root.join("missing.json").exists());
            assert!(
                retain_terminal_commands(
                    &json!({"subject":"different-attempt"}),
                    &[row],
                    code,
                    &path
                )
                .is_err()
            );
            assert_eq!(fs::read(&path).map_err(|error| error.to_string())?, bytes);
            Ok(())
        })();
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        result
    }
    #[test]
    fn terminal_success_requires_every_native_command_row() -> Result<(), String> {
        let root = env::temp_dir().join(format!("contabo-complete-{}", std::process::id()));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let result = (|| {
            let mut rows = Vec::new();
            for name in PORTFOLIO {
                let step = Step {
                    name: (*name).into(),
                    script: "test \"$BASE_SHA\" = base && test \"$HEAD_SHA\" = head".into(),
                    environment: vec![],
                };
                let code = execute(&step, "base", "head")?;
                assert_eq!(code, 0);
                rows.push(command_row(&step, code, 1));
            }
            let incomplete = root.join("incomplete.json");
            assert!(
                retain_terminal_commands(&json!({}), &rows[..PORTFOLIO.len() - 1], 0, &incomplete)
                    .is_err()
            );
            assert!(!incomplete.exists());
            let complete = root.join("complete.json");
            retain_terminal_commands(&json!({"subject":"fixture"}), &rows, 0, &complete)?;
            let receipt: Value =
                serde_json::from_slice(&fs::read(&complete).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            assert_eq!(receipt["disposition"], "product_pass");
            assert_eq!(receipt["steps"].as_array().map(Vec::len), Some(12));
            assert!(
                receipt["steps"]
                    .as_array()
                    .ok_or("missing rows")?
                    .iter()
                    .all(|row| row["outcome"] == "pass" && row["exit"] == 0)
            );
            Ok(())
        })();
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        result
    }
    #[test]
    fn product_members_are_bound_to_actual_producer_paths() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let contract: Value = serde_json::from_slice(
            &fs::read(root.join(PRODUCT_CONTRACT)).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(contract["schema"], "ripr-swarm-product-members/v1");
        assert_eq!(contract["command_receipt"], COMMAND_RECEIPT);
        let mut paths = std::collections::BTreeSet::new();
        for group in contract["required_on_complete_success"]
            .as_array()
            .ok_or("missing groups")?
        {
            let source = fs::read_to_string(
                root.join(group["producer_source"].as_str().ok_or("missing owner")?),
            )
            .map_err(|e| e.to_string())?;
            for member in group["members"].as_array().ok_or("missing members")? {
                let path = member["path"].as_str().ok_or("missing member path")?;
                assert!(paths.insert(path.to_owned()), "duplicate member {path}");
                assert!(
                    source.contains(path),
                    "declared output is absent from its real producer: {path}"
                );
            }
        }
        assert_eq!(paths.len(), 19);
        assert!(paths.contains("target/nextest/ci/junit.xml"));
        assert!(paths.contains("target/ripr/review/annotations.txt"));
        assert!(!paths.contains("target/receipts/quality/ripr-plus.json"));
        Ok(())
    }
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
        let jobs = self_hosted_build_jobs("true", "6")?;
        let steps = portfolio(&source, jobs)?;
        assert_eq!(steps.len(), 12);
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
            portfolio(
                &source.replace(
                    "      - name: Required Rust doctests\n",
                    "      - name: Removed proof\n"
                ),
                jobs
            )
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
        let jobs = self_hosted_build_jobs("true", "6")?;
        for changed in [
            source.replace(
                "BASE_SHA: ${{ steps.admitted.outputs.base || github.event.pull_request.base.sha || github.event.merge_group.base_sha }}",
                "BASE_SHA: ${{ secrets.UNKNOWN }}",
            ),
            source.replace("id: formatting", "id: formatting\n        shell: pwsh"),
            source.replace(
                "id: formatting",
                "id: formatting\n        working-directory: other",
            ),
            source.replace("id: formatting", "id: formatting\n        if: false"),
            source.replace(
                "if: inputs.test-shards == 1",
                "if: inputs.test-shards == 4",
            ),
            source.replace(
                "id: formatting",
                "id: formatting\n        if: inputs.test-shards == 1",
            ),
            source.replace(
                "if: github.event_name == 'pull_request' || github.event_name == 'merge_group'",
                "if: false",
            ),
        ] {
            assert_ne!(source, changed);
            assert!(portfolio(&changed, jobs).is_err());
        }
        Ok(())
    }

    #[test]
    fn build_jobs_require_the_owned_route_and_a_positive_profile_bound() {
        for value in ["1", "6", "8", "16"] {
            assert!(self_hosted_build_jobs("true", value).is_ok());
        }
        for route in ["", "false", "True", "unknown"] {
            assert!(self_hosted_build_jobs(route, "6").is_err());
        }
        for value in [
            "",
            "0",
            "-1",
            "+1",
            "6.0",
            " 6",
            "six",
            "999999999999999999999999999",
        ] {
            assert!(self_hosted_build_jobs("true", value).is_err());
        }
    }

    #[test]
    fn workspace_clippy_keeps_the_slot_bound_and_native_exit() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(root.join(OWNER)).map_err(|error| error.to_string())?;
        for (bound, native_exit) in [("6", 0), ("8", 23)] {
            let jobs = self_hosted_build_jobs("true", bound)?;
            let mut clippy = portfolio(&source, jobs)?
                .into_iter()
                .find(|step| step.name == "Strict workspace Clippy")
                .ok_or("missing workspace Clippy")?;
            assert!(
                clippy
                    .environment
                    .contains(&("CARGO_BUILD_JOBS".into(), bound.into()))
            );
            // Execute the imported command with Cargo stood in, retaining the
            // actual step environment and shell/native exit propagation.
            clippy.script = format!(
                "cargo() {{ test \"$CARGO_BUILD_JOBS\" = {bound} || return 97; test \"$*\" = 'clippy --workspace --all-targets -- -D warnings' || return 98; return {native_exit}; }}\n{}",
                clippy.script
            );
            assert_eq!(execute(&clippy, "base", "head")?, native_exit);
        }
        Ok(())
    }

    #[test]
    fn changed_or_misplaced_build_job_projections_refuse() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(root.join(OWNER)).map_err(|error| error.to_string())?;
        let jobs = self_hosted_build_jobs("true", "6")?;
        for value in [
            "${{ inputs.cargo-build-jobs }}",
            "${{ inputs.use-scratch && inputs.cargo-build-jobs || '4' }}",
            "${{ secrets.UNKNOWN }}",
            "3",
        ] {
            let changed = source.replace(CLIPPY_BUILD_JOBS, value);
            assert_ne!(changed, source);
            assert!(portfolio(&changed, jobs).is_err());
        }
        for changed in [
            source.replace(
                "id: formatting",
                &format!(
                    "id: formatting\n        env:\n          CARGO_BUILD_JOBS: {CLIPPY_BUILD_JOBS}"
                ),
            ),
            source.replace("CARGO_BUILD_JOBS:", "RUSTFLAGS:"),
        ] {
            assert_ne!(changed, source);
            assert!(portfolio(&changed, jobs).is_err());
        }
        Ok(())
    }

    #[test]
    fn detached_evidence_does_not_inherit_admission_or_change_subject_projection()
    -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(root.join(OWNER)).map_err(|error| error.to_string())?;
        let jobs = self_hosted_build_jobs("true", "6")?;
        for expression in [
            "${{ steps.admitted.outputs.base || github.event.pull_request.base.sha || github.event.merge_group.base_sha }}",
            "${{ steps.admitted.outputs.head || github.event.pull_request.head.sha || github.event.merge_group.head_sha }}",
            "${{ github.event_name }}",
            "${{ inputs.admission-reason }}",
            "${{ inputs.admission-head }}",
            "${{ inputs.history-sync-source }}",
            "${{ inputs.history-sync-base }}",
        ] {
            for replacement in ["${{ inputs.unknown }}", "history_sync_admitted"] {
                let changed = source.replace(expression, replacement);
                assert_ne!(changed, source);
                assert!(portfolio(&changed, jobs).is_err());
            }
        }
        Ok(())
    }

    #[test]
    fn detached_evidence_runs_ordinary_producers_and_keeps_native_failure() -> Result<(), String> {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(repo.join(OWNER)).map_err(|error| error.to_string())?;
        let root = env::temp_dir().join(format!("contabo-evidence-{}", std::process::id()));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let result = (|| {
            for native_exit in [0, 37] {
                let mut evidence = portfolio(&source, self_hosted_build_jobs("true", "6")?)?
                    .into_iter()
                    .find(|step| step.name == "PR evidence")
                    .ok_or("missing PR evidence")?;
                for (key, value) in [
                    ("EVENT_NAME", "workflow_dispatch"),
                    ("ADMISSION_REASON", ""),
                    ("ADMISSION_HEAD", ""),
                    ("SYNC_SOURCE", ""),
                    ("SYNC_BASE", ""),
                ] {
                    assert!(evidence.environment.contains(&(key.into(), value.into())));
                }
                let trace = root.join(format!("commands-{native_exit}"));
                let output = root.join(format!("output-{native_exit}"));
                evidence.environment.extend([
                    ("COMMAND_LOG".into(), trace.display().to_string()),
                    ("GITHUB_OUTPUT".into(), output.display().to_string()),
                    ("NATIVE_EXIT".into(), native_exit.to_string()),
                ]);
                evidence.script = format!(
                    "cargo() {{ printf '%s\\n' \"$*\" >> \"$COMMAND_LOG\"; return \"$NATIVE_EXIT\"; }}\n{}",
                    evidence.script
                );
                assert_eq!(
                    execute(&evidence, "reviewed-base", "reviewed-head")?,
                    native_exit
                );
                let commands = fs::read_to_string(trace).map_err(|error| error.to_string())?;
                assert_eq!(
                    commands.lines().next(),
                    Some("xtask ripr-pr --base reviewed-base --head reviewed-head")
                );
                if native_exit == 0 {
                    assert_eq!(commands.lines().count(), 10);
                    assert_eq!(
                        commands.lines().last(),
                        Some("xtask ripr-annotations --check")
                    );
                    assert_eq!(
                        fs::read_to_string(output).map_err(|error| error.to_string())?,
                        "evidence=produced\n"
                    );
                } else {
                    assert_eq!(commands.lines().count(), 1);
                    assert!(!output.exists());
                }
            }
            Ok(())
        })();
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        result
    }

    #[test]
    fn the_shard_condition_on_the_required_tests_step_still_imports() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = fs::read_to_string(root.join(OWNER)).map_err(|e| e.to_string())?;
        assert!(source.contains("if: inputs.test-shards == 1"));
        let steps = portfolio(&source, self_hosted_build_jobs("true", "6")?)?;
        let tests = steps
            .iter()
            .find(|step| step.name == "Required Rust tests")
            .ok_or("Required Rust tests step missing")?;
        assert!(
            tests
                .script
                .contains("cargo nextest run --workspace --profile ci")
        );
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
