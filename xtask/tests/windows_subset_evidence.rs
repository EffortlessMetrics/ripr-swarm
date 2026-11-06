//! Replay the production evidence guard and capture steps without product builds.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const WORKFLOW: &str = include_str!("../../.github/workflows/windows-subset-qualification.yml");
const LABELS: [&str; 4] = [
    "subset-gap-artifacts",
    "subset-cargo-home",
    "subset-cache-warning",
    "subset-clippy",
];
const PRODUCERS: [&str; 4] = [
    "Run lsp::gap_artifacts subset (advisory, #4921)",
    "Run Cargo-home identity subset (advisory, #7748)",
    "Run cache-warning smoke subset (advisory, #4918)",
    "Run clippy subset (advisory, #4337)",
];
static NEXT: AtomicU64 = AtomicU64::new(0);

fn body(name: &str) -> Result<String, String> {
    let (_, step) = WORKFLOW
        .split_once(&format!("      - name: {name}\n"))
        .ok_or("step missing")?;
    let (_, run) = step.split_once("        run: |\n").ok_or("run missing")?;
    Ok(run
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn bash_path(
    configured: Option<std::ffi::OsString>,
    program_files: Option<std::ffi::OsString>,
    windows: bool,
) -> Result<PathBuf, String> {
    if let Some(path) = configured {
        return Ok(PathBuf::from(path));
    }
    if windows {
        let root = program_files.filter(|path| !path.is_empty()).ok_or(
            "ProgramFiles is unavailable; set RIPR_TEST_BASH to the Git for Windows Bash executable",
        )?;
        return Ok(PathBuf::from(root).join("Git/bin/bash.exe"));
    }
    Ok(PathBuf::from("bash"))
}

#[test]
fn bash_resolution_preserves_override_and_standard_install_root() -> Result<(), String> {
    let configured = PathBuf::from("configured-bash");
    let root = PathBuf::from("relocated-program-files");
    assert_eq!(
        bash_path(Some(configured.clone().into_os_string()), None, true)?,
        configured
    );
    assert_eq!(
        bash_path(None, Some(root.clone().into_os_string()), true)?,
        root.join("Git/bin/bash.exe")
    );
    for missing in [None, Some(std::ffi::OsString::new())] {
        let Err(message) = bash_path(None, missing, true) else {
            return Err("missing Windows installation root was accepted".into());
        };
        assert!(message.contains("ProgramFiles"));
        assert!(message.contains("RIPR_TEST_BASH"));
    }
    assert_eq!(bash_path(None, None, false)?, PathBuf::from("bash"));
    Ok(())
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "ripr-subset-evidence-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(path.join("target")).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
    fn valid(&self) -> Result<(), String> {
        for label in LABELS {
            fs::write(self.0.join(format!("target/{label}.status")), "0\n")
                .map_err(|e| e.to_string())?;
            fs::write(
                self.0.join(format!("target/{label}.log")),
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn run(&self, script: &str) -> Result<bool, String> {
        let path = self.0.join("replay.sh");
        fs::write(&path, script).map_err(|e| e.to_string())?;
        let bash = bash_path(
            std::env::var_os("RIPR_TEST_BASH"),
            std::env::var_os("ProgramFiles"),
            cfg!(windows),
        )?;
        let output = Command::new(bash)
            .args(["--noprofile", "--norc", "-eo", "pipefail", "replay.sh"])
            .current_dir(&self.0)
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .output()
            .map_err(|e| e.to_string())?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            code => Err(format!(
                "Bash replay did not reach a contract verdict: exit {code:?}; stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            )),
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn guard_accepts_successful_executed_tests_and_native_advisory_failures() -> Result<(), String> {
    let scratch = Scratch::new()?;
    scratch.valid()?;
    let guard = body("Fail on missing or empty subset evidence")?;
    assert!(scratch.run(&guard)?, "valid success evidence rejected");
    for status in ["1\n", "101\n", "255\n"] {
        for label in LABELS {
            fs::write(scratch.0.join(format!("target/{label}.status")), status)
                .map_err(|e| e.to_string())?;
            fs::write(
                scratch.0.join(format!("target/{label}.log")),
                "native command failed; advisory verdict\n",
            )
            .map_err(|e| e.to_string())?;
        }
        assert!(
            scratch.run(&guard)?,
            "valid nonzero advisory status {status:?} rejected"
        );
    }
    Ok(())
}

#[test]
fn guard_rejects_all_empty_pairs() -> Result<(), String> {
    let scratch = Scratch::new()?;
    for label in LABELS {
        for suffix in ["status", "log"] {
            fs::write(scratch.0.join(format!("target/{label}.{suffix}")), "")
                .map_err(|e| e.to_string())?;
        }
    }
    assert!(
        !scratch.run(&body("Fail on missing or empty subset evidence")?)?,
        "four empty status/log pairs accepted"
    );
    Ok(())
}

#[test]
fn guard_rejects_each_absent_empty_or_nonfile_component() -> Result<(), String> {
    let guard = body("Fail on missing or empty subset evidence")?;
    for label in LABELS {
        for suffix in ["status", "log"] {
            for defect in ["absent", "empty", "directory"] {
                let scratch = Scratch::new()?;
                scratch.valid()?;
                // A nonzero verdict must not exempt its log from evidence checks.
                fs::write(scratch.0.join(format!("target/{label}.status")), "101\n")
                    .map_err(|e| e.to_string())?;
                let path = scratch.0.join(format!("target/{label}.{suffix}"));
                fs::remove_file(&path).map_err(|e| e.to_string())?;
                if defect == "empty" {
                    fs::write(&path, "").map_err(|e| e.to_string())?;
                }
                if defect == "directory" {
                    fs::create_dir(&path).map_err(|e| e.to_string())?;
                }
                assert!(!scratch.run(&guard)?, "accepted {label}.{suffix}: {defect}");
            }
        }
    }
    Ok(())
}

#[test]
fn guard_rejects_malformed_or_out_of_range_native_statuses() -> Result<(), String> {
    let guard = body("Fail on missing or empty subset evidence")?;
    for label in LABELS {
        for status in [
            "\n",
            " ",
            "oops\n",
            "0\n101\n",
            "-1\n",
            "256\n",
            "4294967295\n",
            "01\n",
            "0 \n",
            "0\0\n",
            "0\n\n",
        ] {
            let scratch = Scratch::new()?;
            scratch.valid()?;
            fs::write(scratch.0.join(format!("target/{label}.status")), status)
                .map_err(|e| e.to_string())?;
            assert!(!scratch.run(&guard)?, "accepted {label} status {status:?}");
        }
    }
    Ok(())
}

#[test]
fn guard_rejects_success_without_executed_tests_but_clippy_is_not_a_test() -> Result<(), String> {
    let guard = body("Fail on missing or empty subset evidence")?;
    for label in LABELS.into_iter().take(3) {
        let scratch = Scratch::new()?;
        scratch.valid()?;
        fs::write(
            scratch.0.join(format!("target/{label}.log")),
            "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 9 filtered out\n",
        )
        .map_err(|e| e.to_string())?;
        assert!(
            !scratch.run(&guard)?,
            "zero-execution success accepted: {label}"
        );
    }
    let scratch = Scratch::new()?;
    scratch.valid()?;
    fs::write(
        scratch.0.join("target/subset-clippy.log"),
        "Finished clippy successfully\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(
        scratch.run(&guard)?,
        "clippy incorrectly requires a test denominator"
    );
    Ok(())
}

#[test]
fn producers_preserve_native_verdict_but_fail_capture_write_errors() -> Result<(), String> {
    for (step, label) in PRODUCERS.into_iter().zip(LABELS) {
        let producer = body(step)?;
        let mock = "cargo() { printf 'native command output\\n'; return 101; }\n";
        let scratch = Scratch::new()?;
        assert!(
            scratch.run(&format!("{mock}{producer}"))?,
            "advisory native failure rejected: {label}"
        );
        assert_eq!(
            fs::read_to_string(scratch.0.join(format!("target/{label}.status")))
                .map_err(|e| e.to_string())?,
            "101\n"
        );
        for suffix in ["status", "log"] {
            let scratch = Scratch::new()?;
            fs::create_dir(scratch.0.join(format!("target/{label}.{suffix}")))
                .map_err(|e| e.to_string())?;
            assert!(
                !scratch.run(&format!("{mock}{producer}"))?,
                "capture write failure hidden: {label}.{suffix}"
            );
        }
        let scratch = Scratch::new()?;
        fs::remove_dir(scratch.0.join("target")).map_err(|e| e.to_string())?;
        fs::write(scratch.0.join("target"), "blocked").map_err(|e| e.to_string())?;
        assert!(
            !scratch.run(&format!("{mock}{producer}"))?,
            "target creation failure hidden: {label}"
        );
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn selector_capture_cmdlets_fail_writes_but_keep_native_verdict_advisory() -> Result<(), String> {
    let producer = body("Produce and replay exact-head selector recovery (advisory)")?;
    let start = producer
        .rfind("\ntry {")
        .ok_or("selector capture missing")?;
    let capture = &producer[start..];
    for defect in ["none", "status", "log"] {
        for status in [0, 101] {
            let scratch = Scratch::new()?;
            if defect != "none" {
                fs::create_dir(
                    scratch
                        .0
                        .join(format!("target/subset-selector-literal.{defect}")),
                )
                .map_err(|e| e.to_string())?;
            }
            let script = format!(
                "function python {{ Write-Output 'mock native consumer diagnostic'; $global:LASTEXITCODE = {status} }}\n$consumer = 'mock input'\n{capture}"
            );
            fs::write(scratch.0.join("capture.ps1"), script).map_err(|e| e.to_string())?;
            let output = Command::new("pwsh")
                .args(["-NoProfile", "-NonInteractive", "-File", "capture.ps1"])
                .current_dir(&scratch.0)
                .output()
                .map_err(|e| e.to_string())?;
            assert_eq!(
                output.status.success(),
                defect == "none",
                "selector capture {defect} / native {status}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if defect == "none" {
                assert_eq!(
                    fs::read_to_string(scratch.0.join("target/subset-selector-literal.status"))
                        .map_err(|e| e.to_string())?
                        .trim(),
                    status.to_string()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn terminal_record_distinguishes_verdicts_and_rejects_unbound_pass() -> Result<(), String> {
    let step = body("Record post-execution Windows observation")?;
    let (_, script) = step
        .split_once("python - <<'PY_TERMINAL'\n")
        .ok_or("terminal script missing")?;
    let (script, _) = script
        .split_once("\nPY_TERMINAL")
        .ok_or("terminal script end missing")?;
    let controls = r#"
import tempfile
from pathlib import Path
with tempfile.TemporaryDirectory() as d:
    root = Path(d)
    (root / 'target/selector-literal').mkdir(parents=True)
    a, b, g, t, w = [c * 40 for c in 'abcef']
    env = dict(EXPECTED_PROFILE='merge-group', EXPECTED_REQUEST='123-2', EXPECTED_HEAD=a,
               EXPECTED_BASE=b, EXPECTED_SUBJECT=g, EXPECTED_TREE=t, EXPECTED_GENERATION=w,
               GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2', GITHUB_WORKFLOW_SHA=w,
               GITHUB_WORKFLOW_REF='EffortlessMetrics/ripr-swarm/.github/workflows/ci.yml@refs/heads/main',
               GITHUB_REPOSITORY='EffortlessMetrics/ripr-swarm', GITHUB_EVENT_NAME='merge_group',
               RUNNER_OS='Windows', GITHUB_JOB='windows-advisory-subset',
               OUTCOME_SOURCE_GUARD='success', OUTCOME_GROUP_GUARD='success',
               OUTCOME_CARGO_GUARD='success', OUTCOME_SELECTOR_GUARD='success')
    for key, label, is_test in PRODUCERS:
        env['OUTCOME_' + key.upper()] = 'success'
        (root / ('target/' + label + '.status')).write_text('0\n')
        (root / ('target/' + label + '.log')).write_text('test result: ok. 1 passed; 0 failed; 0 measured;\n')
    (root / 'target/selector-literal/receipt.json').write_text('{}')
    source = dict(proof_profile='merge-group', request_id='123-2', head=a, base=b, subject=g, tree=t,
                  workflow_generation=w, workflow_sha=w, workflow_ref=env['GITHUB_WORKFLOW_REF'],
                  producer_run='123', producer_attempt='2')
    source_path = root / 'target/windows-subset-source.json'
    source_path.write_text(json.dumps(source))
    def run(): return build(env, root, g, t, 'Windows')
    def verdicts(): return [r['verdict'] for r in run()['producers']]
    assert verdicts() == ['pass'] * 5
    assert run()['authority'] == 'not_established' and run()['candidate_authored']
    # Preexisting files cannot create PASS before steps execute.
    env['OUTCOME_GAP'] = 'skipped'
    assert verdicts()[0] == 'not_run'
    env['OUTCOME_GAP'] = 'failure'
    assert verdicts()[0] == 'unknown'
    env['OUTCOME_GAP'] = 'success'
    path = root / 'target/subset-gap-artifacts.status'
    path.write_text('101\n')
    assert verdicts()[0] == 'fail'
    path.write_text('0\n')
    log = root / 'target/subset-gap-artifacts.log'
    log.write_text('test result: ok. 0 passed; 0 failed; 0 measured;\n')
    assert verdicts()[0] == 'zero_tests'
    log.unlink()
    assert verdicts()[0] == 'incomplete'
    log.write_text('test result: ok. 1 passed; 0 failed; 0 measured;\n')
    env['OUTCOME_CARGO_GUARD'] = 'failure'
    assert verdicts()[0] == 'unknown'
    env['OUTCOME_CARGO_GUARD'] = 'success'
    source['subject'] = 'd' * 40
    source_path.write_text(json.dumps(source))
    assert 'pass' not in verdicts()
    source['subject'] = g
    source_path.write_text(json.dumps(source))
    assert not build(env, root, 'd' * 40, t, 'Windows')['identity_matches']
    assert not build(env, root, g, t, 'Linux')['identity_matches']
    env['OUTCOME_GROUP_GUARD'] = 'skipped'
    assert 'pass' not in verdicts()
    env['OUTCOME_GROUP_GUARD'] = 'success'
    old = run()['producers'][0]['files'][0]['sha256']
    log.write_text('substituted bytes\n')
    assert run()['producers'][0]['files'][0]['sha256'] != old
    # Digests are candidate claims; this does not authenticate an uploaded archive.
    assert run()['authority'] == 'not_established'
"#;
    let mut program = String::from("__name__ = 'terminal_test'\n");
    program.push_str(script);
    program.push_str(controls);
    let python = if cfg!(windows) { "python" } else { "python3" };
    let scratch = Scratch::new()?;
    fs::write(scratch.0.join("terminal_controls.py"), program).map_err(|e| e.to_string())?;
    if !scratch.run(&format!("{python} terminal_controls.py"))? {
        return Err("terminal observation controls failed".into());
    }
    Ok(())
}
