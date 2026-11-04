//! Behavioural controls for the `ripr-pr` dogfood workflow's refusal reporting
//! (#13054). The workflow's `run:` blocks are extracted from the YAML and run
//! under bash (with `pipefail`, stricter than the default step shell) against a
//! stand-in `ripr` and `gh`, so what the PR comment can say is exercised without
//! the network or a runner.
#![cfg(unix)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const WORKFLOW: &str = include_str!("../../../.github/workflows/ripr-pr.yml");

const REAL_REFUSAL: &str = "ripr: diff_scope_oversized: 2556 changed Rust lines across 3 Rust files exceed the RIPR_MAX_DIFF_CHANGED_RUST_LINES limit (2000); analysis was not run to protect runner memory. Repair route: pass `--path <glob>`.\n";

/// The body of a step's `run: |` block (de-indented), found by step name.
fn run_block(step: &str) -> TestResult<String> {
    let lines: Vec<&str> = WORKFLOW.lines().collect();
    let header = format!("- name: {step}");
    let start = lines
        .iter()
        .position(|line| line.trim() == header)
        .ok_or_else(|| format!("missing step {step}"))?;
    let run = lines[start..]
        .iter()
        .position(|line| line.trim() == "run: |")
        .ok_or_else(|| format!("step {step} has no run block"))?
        + start;
    let indent = lines[run + 1].len() - lines[run + 1].trim_start().len();
    let mut body = Vec::new();
    for line in &lines[run + 1..] {
        if line.trim().is_empty() {
            body.push("");
        } else if line.len() - line.trim_start().len() >= indent {
            body.push(&line[indent..]);
        } else {
            break;
        }
    }
    Ok(body.join("\n"))
}

fn scratch(label: &str) -> TestResult<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "ripr-pr-workflow-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn stand_in(dir: &Path, name: &str, script: &str) -> TestResult<()> {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Host tools the blocks need, passed through; `gh` is a stand-in that succeeds.
fn tool_dir(root: &Path) -> TestResult<PathBuf> {
    let bin = root.join("bin");
    fs::create_dir_all(&bin)?;
    for tool in [
        "mkdir", "sed", "grep", "head", "tr", "od", "cat", "wc", "sleep", "rm",
    ] {
        let real = ["/usr/bin", "/bin", "/usr/local/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(tool))
            .find(|path| path.exists());
        if let Some(real) = real {
            stand_in(&bin, tool, &format!("exec {} \"$@\"", real.display()))?;
        }
    }
    stand_in(&bin, "gh", "exit 0")?;
    Ok(bin)
}

struct Case {
    root: PathBuf,
    bin: PathBuf,
}

struct Outcome {
    log: String,
    body: String,
    artifact: String,
}

impl Case {
    fn new(label: &str) -> TestResult<Self> {
        let root = scratch(label)?;
        let bin = tool_dir(&root)?;
        Ok(Self { root, bin })
    }

    fn bash(&self, script: &str, dir: &Path, env: &[(&str, &str)]) -> TestResult<String> {
        let mut command = Command::new("/bin/bash");
        command
            .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", script])
            .current_dir(dir)
            .env_clear()
            .env("PATH", &self.bin)
            .env("RUNNER_TEMP", self.root.join("runner-temp"));
        for (key, value) in env {
            command.env(key, value);
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "run block failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Runs the analyze block with a stand-in ripr that writes `stderr` to
    /// stderr, `report` to stdout and exits `code`; then the comment block over
    /// the artifact directory. `tamper` overwrites `reason.txt` first.
    fn run(
        &self,
        stderr: &[u8],
        report: &str,
        code: i32,
        tamper: Option<&str>,
    ) -> TestResult<Outcome> {
        let head = self.root.join("head");
        fs::create_dir_all(&head)?;
        fs::create_dir_all(self.root.join("runner-temp"))?;
        fs::write(self.root.join("stderr.fixture"), stderr)?;
        fs::write(self.root.join("report.fixture"), report)?;
        stand_in(
            &self.bin,
            "ripr-stand-in",
            &format!(
                "cat \"{root}/report.fixture\"\ncat \"{root}/stderr.fixture\" >&2\nexit {code}",
                root = self.root.display()
            ),
        )?;
        let ripr = self.bin.join("ripr-stand-in");
        let log = self.bash(
            &run_block("Run ripr on the pull request diff")?,
            &head,
            &[
                ("BASE_SHA", "0000000000000000000000000000000000000000"),
                ("HEAD_SHA", "1111111111111111111111111111111111111111"),
                ("RIPR_BIN", ripr.to_str().ok_or("path")?),
            ],
        )?;
        let out = self.root.join("runner-temp/out");
        if let Some(reason) = tamper {
            fs::write(out.join("reason.txt"), reason)?;
        }
        let comment = self.root.join("comment");
        fs::create_dir_all(&comment)?;
        let _ = fs::remove_dir_all(comment.join("out"));
        copy_dir(&out, &comment.join("out"))?;
        self.bash(
            &run_block("Upsert the ripr comment")?,
            &comment,
            &[
                ("REPO", "o/r"),
                ("PR", "1"),
                ("HEAD_SHA", "1111111111111111111111111111111111111111"),
                ("RUN_URL", "https://example.invalid/run"),
            ],
        )?;
        let mut artifact = String::new();
        for entry in fs::read_dir(&out)? {
            let entry = entry?;
            artifact.push_str(&entry.file_name().to_string_lossy());
            artifact.push('\n');
            artifact.push_str(&String::from_utf8_lossy(&fs::read(entry.path())?));
        }
        Ok(Outcome {
            log,
            body: fs::read_to_string(comment.join("body.md"))?,
            artifact,
        })
    }
}

fn copy_dir(from: &Path, to: &Path) -> TestResult {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        fs::copy(entry.path(), to.join(entry.file_name()))?;
    }
    Ok(())
}

const NUMERIC_REASON: &str = "diff_scope_oversized: 2556 changed Rust lines across 3 Rust files exceed the RIPR_MAX_DIFF_CHANGED_RUST_LINES limit (2000)";

#[test]
fn a_real_size_refusal_is_reconstructed_from_its_numbers() -> TestResult {
    let outcome = Case::new("real")?.run(REAL_REFUSAL.as_bytes(), "", 2, None)?;
    assert!(outcome.body.contains(NUMERIC_REASON), "{}", outcome.body);
    assert!(outcome.body.contains("Nothing here is a finding."));
    assert!(
        !outcome.body.contains("--path"),
        "raw tail must not be copied"
    );
    Ok(())
}

#[test]
fn raw_diagnostics_never_reach_the_comment_or_the_artifact() -> TestResult {
    let cases: [(&str, Vec<u8>); 9] = [
        ("first line secret", b"SECRET_FIRST\nripr: diff_scope_oversized: 1 changed Rust lines exceed the RIPR_X limit (1)\n".to_vec()),
        ("second line secret", b"ripr: diff_scope_oversized: 5 changed Rust lines exceed the RIPR_X limit (1)\nSECRET_SECOND\n".to_vec()),
        ("token after allowed code", b"ripr: analysis_timeout: SECRET_TOKEN ghp_abc123\n".to_vec()),
        ("hostile markup", b"ripr: diff_scope_oversized: ```\n@user ![x](http://e) <script>SECRET_HTML</script>\n".to_vec()),
        ("numbers with junk", b"ripr: diff_scope_oversized: 5 changed Rust lines exceed the RIPR_X limit (1) SECRET_TAIL\n".to_vec()),
        ("unknown env name", b"ripr: diff_scope_oversized: 5 changed Rust lines exceed the RIPR_ASECRET limit (1)\n".to_vec()),
        ("oversized number", format!("ripr: diff_scope_oversized: {} changed Rust lines exceed the RIPR_MAX_DIFF_INDEX_FILES limit (1) SECRET\n", "9".repeat(150)).into_bytes()),
        ("unknown code", b"ripr: SECRET_CODE: 1 changed Rust lines exceed the RIPR_X limit (1)\n".to_vec()),
        ("oversized first line", {
            let mut big = b"ripr: analysis_timeout: ".to_vec();
            big.extend(std::iter::repeat_n(b'a', 2_000_000));
            big.extend_from_slice(b" SECRET_BIG");
            big
        }),
    ];
    for (label, stderr) in cases {
        let outcome = Case::new("raw")?.run(&stderr, "", 2, None)?;
        for text in [&outcome.body, &outcome.artifact] {
            assert!(!text.contains("SECRET"), "{label}: leaked into {text:.300}");
            assert!(!text.contains("ghp_"), "{label}: leaked a token shape");
            assert!(!text.contains('@'), "{label}: mention reached output");
            let without_marker = text.replace("<!-- ripr-pr-comment -->", "");
            assert!(
                !without_marker.contains('<'),
                "{label}: markup reached output"
            );
        }
        assert!(
            !outcome.artifact.contains("stderr.txt"),
            "{label}: raw stderr must not be uploaded"
        );
        assert_eq!(
            outcome.body.matches("```").count() % 2,
            0,
            "{label}: unbalanced fence"
        );
    }
    Ok(())
}

#[test]
fn a_tampered_artifact_reason_is_rejected_by_the_comment_job() -> TestResult {
    for forged in [
        "foo: SECRET_FIRST\n",
        "diff_scope_oversized: SECRET_FIRST\n",
        "diff_scope_oversized: 5 changed Rust lines exceed the RIPR_X limit (1) SECRET_TAIL\n",
        "diff_scope_oversized: 1\n```\n@all\n",
    ] {
        let outcome = Case::new("forged")?.run(REAL_REFUSAL.as_bytes(), "", 2, Some(forged))?;
        assert!(
            !outcome.body.contains("SECRET"),
            "{forged:?} reached the body"
        );
        assert!(
            !outcome.body.contains("@all"),
            "{forged:?} reached the body"
        );
        assert_eq!(outcome.body.matches("```").count() % 2, 0);
    }
    Ok(())
}

#[test]
fn stderr_without_a_trailing_newline_cannot_swallow_the_resume_token() -> TestResult {
    let outcome = Case::new("resume")?.run(b"ripr: analysis_timeout: no newline", "", 2, None)?;
    let lines: Vec<&str> = outcome.log.lines().collect();
    let stop = lines
        .iter()
        .position(|line| line.starts_with("::stop-commands::"))
        .ok_or("missing stop-commands")?;
    let token = lines[stop].trim_start_matches("::stop-commands::");
    assert!(!token.is_empty());
    assert!(
        lines.iter().any(|line| *line == format!("::{token}::")),
        "resume token must start its own line: {lines:?}"
    );
    assert!(lines.contains(&"ripr: analysis_timeout: no newline"));
    Ok(())
}

#[test]
fn a_successful_report_is_unchanged() -> TestResult {
    let report = "#ripr agent static-advisory findings=0 gaps=0\ndone=no changed behavior found\n";
    let outcome =
        Case::new("success")?.run(b"ripr: 3 limitation(s) SECRET_NOTE\n", report, 0, None)?;
    assert!(
        outcome
            .body
            .contains("```text\n#ripr agent static-advisory")
    );
    assert!(outcome.body.contains("Took "));
    assert!(!outcome.body.contains("did not produce a report"));
    assert!(
        !outcome.body.contains("SECRET_NOTE"),
        "stderr notes stay in the log"
    );
    Ok(())
}
