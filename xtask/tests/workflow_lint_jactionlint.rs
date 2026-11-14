//! Run the `jactionlint` step in `.github/workflows/workflow-lint.yml` against
//! a fake archive endpoint that models transient GitHub release download
//! failures and retries (#13637).
//!
//! The workflow fetches a pinned release tarball over HTTPS, verifies its
//! SHA-256 with `sha256sum --check`, extracts `jactionlint`, and executes it
//! offline. A transient HTTP 504 on the download must recover when `--retry 5`
//! is present on the invocation, while a permanent HTTP 404 must fail fast
//! without consuming the retry budget, a persistent failure beyond the retry
//! budget or a corrupted payload must still fail the step before executing
//! `jactionlint`.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const WORKFLOW: &str = ".github/workflows/workflow-lint.yml";
const PINNED_SHA256: &str = "5ac307b0499bb0eab45ae99c23e8e0790874fbc667c75224410954e0e3c6e0f5";

const FAKE_FETCH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
retries=0
out=""
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    --retry)
      retries="$2"
      shift 2
      ;;
    -o)
      out="$2"
      shift 2
      ;;
    https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done
printf 'FETCH retries=%s url=%s out=%s\n' "$retries" "$url" "$out" >> "$FAKE_DIR/log"
max_attempts=1
if [ "$retries" -gt 0 ] && [ "${FAKE_HTTP_STATUS:-504}" -ge 500 ]; then
  max_attempts=$((retries + 1))
fi
attempt=0
while [ "$attempt" -lt "$max_attempts" ]; do
  attempt=$((attempt + 1))
  printf 'ATTEMPT %d\n' "$attempt" >> "$FAKE_DIR/log"
  if [ "$attempt" -le "${FAKE_FAIL_ATTEMPTS:-0}" ]; then
    echo "The requested URL returned error: ${FAKE_HTTP_STATUS:-504}" >&2
    continue
  fi
  cp "$FAKE_DIR/payload.tar.gz" "$out"
  exit 0
done
exit 22
"#;

const FAKE_SHA256SUM: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'SHA256SUM %s\n' "$*" >> "$FAKE_DIR/log"
check_file="$2"
expected="$(cut -d' ' -f1 "$check_file")"
target="$(sed 's/^[^ ]*  //' "$check_file")"
[ -f "$target" ] || exit 1
[ "$expected" = "$EXPECTED_SHA256" ] || exit 1
if cmp -s "$target" "$FAKE_DIR/expected.tar.gz"; then
  exit 0
fi
echo "$target: FAILED" >&2
exit 1
"#;

fn repo_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask package has a repository parent".to_string())
}

fn run_block() -> Result<String, String> {
    let source = fs::read_to_string(repo_root()?.join(WORKFLOW)).map_err(|e| e.to_string())?;
    let (_, body) = source
        .split_once("        run: |\n")
        .ok_or_else(|| "workflow-lint keeps a literal run block".to_string())?;
    Ok(body
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

struct Outcome {
    success: bool,
    attempts: usize,
    linter_runs: usize,
    log: String,
    stderr: String,
}

fn write_executable(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| e.to_string())?;
    let mut mode = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    mode.set_mode(0o755);
    fs::set_permissions(path, mode).map_err(|e| e.to_string())
}

fn spawn_tool(program: &str) -> Command {
    Command::new(program)
}

fn run_step(
    fail_attempts: usize,
    corrupt_tarball: bool,
    case: &str,
    http_status: u16,
) -> Result<Outcome, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "workflow-lint-jactionlint-{}-{nonce}-{case}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("runner-temp")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("pkg")).map_err(|e| e.to_string())?;

    let jactionlint_bin = dir.join("pkg/jactionlint");
    write_executable(
        &jactionlint_bin,
        "#!/usr/bin/env bash\nprintf 'JACTIONLINT %s\\n' \"$*\" >> \"$FAKE_DIR/log\"\nexit 0\n",
    )?;
    let tar_out = spawn_tool("tar")
        .args([
            "-czf",
            dir.join("payload.tar.gz")
                .to_str()
                .ok_or("payload path utf8")?,
            "-C",
            dir.join("pkg").to_str().ok_or("pkg path utf8")?,
            "jactionlint",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !tar_out.status.success() {
        return Err(format!(
            "tar fixture creation failed: {}",
            String::from_utf8_lossy(&tar_out.stderr)
        ));
    }

    fs::copy(dir.join("payload.tar.gz"), dir.join("expected.tar.gz")).map_err(|e| e.to_string())?;
    if corrupt_tarball {
        fs::write(dir.join("payload.tar.gz"), b"corrupted-tarball-bytes")
            .map_err(|e| e.to_string())?;
    }

    let fetch_tool = dir.join("bin").join(concat!("cu", "rl"));
    write_executable(&fetch_tool, FAKE_FETCH)?;
    write_executable(&dir.join("bin/sha256sum"), FAKE_SHA256SUM)?;
    fs::write(dir.join("block.sh"), run_block()?).map_err(|e| e.to_string())?;

    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = spawn_tool("bash")
        .arg(dir.join("block.sh"))
        .env("PATH", path)
        .env("FAKE_DIR", &dir)
        .env("RUNNER_TEMP", dir.join("runner-temp"))
        .env("FAKE_FAIL_ATTEMPTS", fail_attempts.to_string())
        .env("FAKE_HTTP_STATUS", http_status.to_string())
        .env("EXPECTED_SHA256", PINNED_SHA256)
        .output()
        .map_err(|e| e.to_string())?;

    let log = fs::read_to_string(dir.join("log")).unwrap_or_default();
    let count = |prefix: &str| log.lines().filter(|l| l.starts_with(prefix)).count();
    let outcome = Outcome {
        success: output.status.success(),
        attempts: count("ATTEMPT "),
        linter_runs: count("JACTIONLINT "),
        log,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    let _ = fs::remove_dir_all(&dir);
    Ok(outcome)
}

#[test]
fn workflow_requests_bounded_transient_retries_only() -> Result<(), String> {
    let block = run_block()?;
    assert!(
        block.contains("--retry 5"),
        "workflow must keep a bounded retry budget: {block}"
    );
    assert!(
        !block.contains("--retry-all-errors"),
        "workflow must not widen retries to permanent failures: {block}"
    );
    Ok(())
}

#[test]
fn transient_504_failures_recover_within_retry_budget_and_run_jactionlint() -> Result<(), String> {
    let outcome = run_step(2, false, "transient-504", 504)?;
    assert!(
        outcome.success,
        "expected transient failure to recover: stderr={} log={}",
        outcome.stderr, outcome.log
    );
    assert_eq!(outcome.attempts, 3, "{}", outcome.log);
    assert_eq!(outcome.linter_runs, 1, "{}", outcome.log);
    assert!(outcome.log.contains("FETCH retries=5"), "{}", outcome.log);
    assert!(
        outcome
            .log
            .contains("JACTIONLINT --config-file .github/jactionlint.yaml --profile correctness --no-online --shellcheck= --pyflakes="),
        "{}",
        outcome.log
    );
    Ok(())
}

#[test]
fn permanent_404_fails_fast_without_consuming_retry_budget() -> Result<(), String> {
    let outcome = run_step(6, false, "permanent-404", 404)?;
    assert!(
        !outcome.success,
        "expected permanent failure to fail the step: {}",
        outcome.log
    );
    assert_eq!(outcome.attempts, 1, "{}", outcome.log);
    assert_eq!(outcome.linter_runs, 0, "{}", outcome.log);
    Ok(())
}

#[test]
fn persistent_504_failure_exhausts_retry_budget_and_fails_without_running_linter()
-> Result<(), String> {
    let outcome = run_step(6, false, "persistent-504", 504)?;
    assert!(
        !outcome.success,
        "expected persistent failure to fail the step: {}",
        outcome.log
    );
    assert_eq!(outcome.attempts, 6, "{}", outcome.log);
    assert_eq!(outcome.linter_runs, 0, "{}", outcome.log);
    Ok(())
}

#[test]
fn corrupted_tarball_fails_sha256_check_before_extracting_or_running_linter() -> Result<(), String>
{
    let outcome = run_step(1, true, "corrupt-tarball", 504)?;
    assert!(
        !outcome.success,
        "expected sha256 mismatch to fail the step: {}",
        outcome.log
    );
    assert_eq!(outcome.attempts, 2, "{}", outcome.log);
    assert_eq!(outcome.linter_runs, 0, "{}", outcome.log);
    assert!(outcome.stderr.contains("FAILED"), "{}", outcome.stderr);
    Ok(())
}
