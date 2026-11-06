//! Replay literal workflow Bash with bounded, deliberately hostile stand-ins.
//! No actual product execution, hosted evidence or cryptographic Git custody.
use super::*;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const CARGO: &str = r#"#!/bin/bash
set -euo pipefail
printf '%s\n' "$*" >> "$ROOT/trace"
if [[ "$*" == 'xtask check-user-text --tree '* ]]; then
  [ "$PWD" = "$RUNNER_TEMP/user-text-base" ] || exit 82
  [ "$3" = --tree ] && [ "$4" = "$GITHUB_WORKSPACE" ] || exit 83
  exit "$FAIL_AT"
fi
n=$(wc -l < "$ROOT/trace")
[ "$FAIL_AT" != "$n" ] || exit 71
if [[ "$1 $2" == 'xtask ripr-pr' ]]; then
  [ "$3" = --base ] && [ "$4" = "$BASE_SHA" ] && [ "$5" = --head ] && [ "$6" = "$HEAD_SHA" ] || exit 84
  mkdir -p target/ripr/pr
  if [ "$#" = 6 ]; then
    printf '%s' "$STAMP" > target/ripr/pr/check.json
    printf '%s' "$STAMP" > target/ripr/pr/repo-exposure.json
  else
    [ "$7" = --check ] && [ "$(cat target/ripr/pr/check.json)" = "$STAMP" ] || exit 85
  fi
elif [[ "$1 $2" == 'xtask ripr-review-comments' ]]; then
  [ "$3" = --base ] && [ "$4" = "$BASE_SHA" ] && [ "$5" = --head ] && [ "$6" = "$HEAD_SHA" ] || exit 84
  [ "${@: -2:1}" = --check-output ] && [ "${@: -1}" = target/ripr/pr/check.json ] || exit 85
  [ "$(cat target/ripr/pr/check.json)" = "$STAMP" ] || exit 86
  mkdir -p target/ripr/review
  if [ "$7" = --check ]; then [ "$(cat target/ripr/review/comments.json)" = "$STAMP" ] || exit 87
  else printf '%s' "$STAMP" > target/ripr/review/comments.json; fi
elif [[ "$*" == 'run --quiet -p ripr -- doctor --json' ]]; then
  printf '%s' "$STAMP"
elif [[ "$1 $2" == 'xtask schema-producer-sweep' ]]; then
  shift 2
  expected=2
  if [[ "$EVENT_NAME" == pull_request || "$EVENT_NAME" == merge_group ]]; then expected=6; fi
  [ "$#" = "$expected" ] || exit 88
  [ "$2" = schemas/ripr/doctor.schema.json=target/ripr/sweep/doctor.json ] || exit 88
  if [ "$expected" = 6 ]; then
    [ "$4" = schemas/ripr/pr-evidence.schema.json=target/ripr/pr/repo-exposure.json ] || exit 88
    [ "$6" = schemas/ripr/review-comments.schema.json=target/ripr/review/comments.json ] || exit 88
  fi
  while [ "$#" -gt 0 ]; do
    [ "$1" = --artifact ] || exit 89
    [ "$(cat "${2#*=}")" = "$STAMP" ] || exit 90
    shift 2
  done
fi
"#;

const GIT: &str = r#"#!/bin/bash
set -euo pipefail
if [[ "$*" == 'rev-parse HEAD^1' ]]; then printf '%s\n' "$BASE_SHA"
elif [[ "$1 $2 $3" == 'worktree add --detach' ]]; then
  [ "$4" = "$RUNNER_TEMP/user-text-base" ] && [ "$5" = "$BASE_SHA" ] || exit 92
  cp -R "$ROOT/base" "$4"
elif [[ "$1" == -C && "$2" == "$RUNNER_TEMP/user-text-base" ]]; then
  shift 2
  case "$*" in
    'rev-parse HEAD') printf '%s\n' "$BASE_SHA" ;;
    'rev-parse HEAD:crates/ripr/src/app/test_stub.rs')
      if [ "$RECOVERY" = none ]; then echo 0000000000000000000000000000000000000000
      else echo 41ce81981802365b31e5182b7a7a51540ff8d7be; fi ;;
    'hash-object crates/ripr/src/app/test_stub.rs')
      grep -q '    root: &Path,' "$RUNNER_TEMP/user-text-base/crates/ripr/src/app/test_stub.rs" || exit 93
      grep -q 'source, Some(root)' "$RUNNER_TEMP/user-text-base/crates/ripr/src/app/test_stub.rs" || exit 93
      grep -q 'probe_line(root, &in_file' "$RUNNER_TEMP/user-text-base/crates/ripr/src/app/test_stub.rs" || exit 93
      if [ "$RECOVERY" = bad-hash ]; then echo incorrect; else echo e934320980555de2d431b2aa8482a6059e9cf7f7; fi ;;
    'diff --name-only')
      echo crates/ripr/src/app/test_stub.rs
      [ "$RECOVERY" != extra-file ] || echo xtask/src/policy/user_text.rs ;;
    *) exit 94 ;;
  esac
else exit 95
fi
"#;

impl Fixture {
    fn new(supports_tree: bool) -> TestResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "ripr-composite-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        let fixture = Self(root);
        for dir in [
            "bin",
            "runner",
            "checkout",
            "base/xtask/src/policy",
            "base/crates/ripr/src/app",
        ] {
            fs::create_dir_all(fixture.0.join(dir))?;
        }
        for (name, body) in [("cargo", CARGO), ("git", GIT)] {
            let path = fixture.0.join("bin").join(name);
            fs::write(&path, body)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        }
        fs::write(
            fixture.0.join("base/xtask/src/policy/user_text.rs"),
            if supports_tree {
                "\"--tree\""
            } else {
                "legacy matcher"
            },
        )?;
        fs::write(
            fixture.0.join("base/crates/ripr/src/app/test_stub.rs"),
            "fn return_value_direct_at_probe_line(\nrust_test_stub(&seam, None, source);\nreturn_value_direct_at_probe_line(&in_file, file, line, &source)\n",
        )?;
        Ok(fixture)
    }

    fn run(&self, source: &str, event: &str, fail: usize, recovery: &str) -> TestResult<Output> {
        Ok(Command::new("timeout")
            .args([
                "--kill-after=1s",
                "5s",
                "bash",
                "--noprofile",
                "--norc",
                "-eo",
                "pipefail",
                "-c",
                source,
            ])
            .current_dir(self.0.join("checkout"))
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.0.join("bin").display()),
            )
            .env("HOME", &self.0)
            .env("ROOT", &self.0)
            .env("GITHUB_WORKSPACE", self.0.join("checkout"))
            .env("RUNNER_TEMP", self.0.join("runner"))
            .env("BASE_SHA", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
            .env("HEAD_SHA", "cccccccccccccccccccccccccccccccccccccccc")
            .env("EVENT_NAME", event)
            .env("FAIL_AT", fail.to_string())
            .env("STAMP", "this-run-attempt-and-subject")
            .env("RECOVERY", recovery)
            .output()?)
    }

    fn trace(&self) -> TestResult<String> {
        let path = self.0.join("trace");
        Ok(if path.exists() {
            fs::read_to_string(path)?
        } else {
            String::new()
        })
    }
}

#[test]
fn first_parent_matcher_recovery_and_legacy_fallback_remain_distinct() -> TestResult {
    let contract = CompositeProducer::BaseUserText.contract(Event::MergeGroup);
    let scripts: Vec<_> = contract
        .steps
        .iter()
        .map(|owner| script(step(WORKFLOW, owner.name)?))
        .collect::<TestResult<_>>()?;
    for (support, recovery, fail, expected_exit, executed) in [
        (true, "none", 0, 0, true),
        (true, "exact", 0, 0, true),
        (true, "bad-hash", 0, 1, false),
        (true, "extra-file", 0, 1, false),
        (true, "none", 17, 17, true),
        (false, "none", 0, 0, false),
    ] {
        let fixture = Fixture::new(support)?;
        // Separate shells preserve the real three owning step boundaries.
        let mut terminal = None;
        for source in &scripts {
            let output = fixture.run(source, "merge_group", fail, recovery)?;
            let success = output.status.success();
            terminal = Some(output);
            if !success {
                break;
            }
        }
        let terminal = terminal.ok_or("no steps replayed")?;
        assert_eq!(
            terminal.status.code(),
            Some(expected_exit),
            "support={support} recovery={recovery}: {terminal:?}"
        );
        assert_eq!(!fixture.trace()?.is_empty(), executed);
        if !support {
            assert!(String::from_utf8_lossy(&terminal.stdout).contains("matcher pin only"));
        }
    }
    // Recovery refuses a changed source shape before claiming the pinned repair.
    let fixture = Fixture::new(true)?;
    let stub = fixture.0.join("base/crates/ripr/src/app/test_stub.rs");
    let source = fs::read_to_string(&stub)?;
    fs::write(stub, format!("{source}{source}"))?;
    assert!(
        fixture
            .run(&scripts[0], "merge_group", 0, "exact")?
            .status
            .success()
    );
    assert!(
        !fixture
            .run(&scripts[1], "merge_group", 0, "exact")?
            .status
            .success()
    );
    assert!(fixture.trace()?.is_empty());
    // Head-owned execution is not saved by a successful-looking command string.
    let fixture = Fixture::new(true)?;
    assert!(
        fixture
            .run(&scripts[0], "merge_group", 0, "none")?
            .status
            .success()
    );
    let head_owned = scripts[2].replace("cd \"$base_tree\"", "cd \"$GITHUB_WORKSPACE\"");
    assert_eq!(
        fixture
            .run(&head_owned, "merge_group", 0, "none")?
            .status
            .code(),
        Some(82)
    );
    Ok(())
}

#[test]
fn pr_evidence_keeps_order_exact_comparison_and_native_failure() -> TestResult {
    let source = script(step(WORKFLOW, "PR evidence")?)?;
    for fail_at in 0..=10 {
        let fixture = Fixture::new(true)?;
        let output = fixture.run(&source, "merge_group", fail_at, "none")?;
        assert_eq!(
            output.status.code(),
            Some(if fail_at == 0 { 0 } else { 71 }),
            "{fail_at}: {output:?}"
        );
        let trace = fixture.trace()?;
        let commands: Vec<_> = trace.lines().collect();
        assert_eq!(commands.len(), if fail_at == 0 { 10 } else { fail_at });
        let expected: Vec<_> = PR_COMMANDS
            .iter()
            .map(|line| {
                line.trim_start_matches("cargo ")
                    .replace("\"$BASE_SHA\"", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                    .replace("\"$HEAD_SHA\"", "cccccccccccccccccccccccccccccccccccccccc")
            })
            .collect();
        assert_eq!(commands, expected[..commands.len()]);
    }
    Ok(())
}

#[test]
fn schema_sweep_requires_fresh_doctor_and_event_dependent_same_run_inputs() -> TestResult {
    let evidence = script(step(WORKFLOW, "PR evidence")?)?;
    let sweep = script(step(WORKFLOW, "Schema producer sweep")?)?;
    for event in ["pull_request", "merge_group", "push"] {
        for corrupt in [
            "none",
            "missing-pr",
            "stale-pr",
            "missing-review",
            "stale-review",
        ] {
            let fixture = Fixture::new(true)?;
            if event != "push" {
                assert!(fixture.run(&evidence, event, 0, "none")?.status.success());
            }
            let path = fixture
                .0
                .join("checkout")
                .join(if corrupt.contains("review") {
                    "target/ripr/review/comments.json"
                } else {
                    "target/ripr/pr/repo-exposure.json"
                });
            if event != "push" {
                if corrupt.starts_with("missing") {
                    fs::remove_file(&path)?;
                }
                if corrupt.starts_with("stale") {
                    fs::write(&path, "previous-run")?;
                }
            }
            // An old doctor file must be overwritten even outside PR events.
            let doctor = fixture.0.join("checkout/target/ripr/sweep/doctor.json");
            fs::create_dir_all(doctor.parent().ok_or("doctor parent")?)?;
            fs::write(&doctor, "previous-run")?;
            let output = fixture.run(&sweep, event, 0, "none")?;
            assert_eq!(
                output.status.success(),
                event == "push" || corrupt == "none",
                "{event}/{corrupt}: {output:?}"
            );
            assert_eq!(fs::read_to_string(doctor)?, "this-run-attempt-and-subject");
        }
    }
    for fail_at in [1, 2] {
        let fixture = Fixture::new(true)?;
        assert_eq!(
            fixture.run(&sweep, "push", fail_at, "none")?.status.code(),
            Some(71)
        );
    }
    // Reordering or deleting doctor generation must not pass on stale bytes.
    for changed in [sweep.replace("cargo run --quiet -p ripr -- doctor --json", "true"),
        sweep.replace("if [ \"$EVENT_NAME\" = \"pull_request\" ] || [ \"$EVENT_NAME\" = \"merge_group\" ]; then", "if false; then")] {
        let fixture = Fixture::new(true)?;
        assert!(!fixture.run(&changed, "merge_group", 0, "none")?.status.success());
    }
    Ok(())
}
