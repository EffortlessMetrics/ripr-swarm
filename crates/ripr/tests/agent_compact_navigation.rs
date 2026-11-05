//! Follow compact check -> explain -> context commands literally (#13157).
//! These Unix tests exercise the built binary from a foreign working directory;
//! renderer-only tests separately cover all platforms and reach-limited rows.
#![cfg(all(unix, feature = "lang-rust"))]

use ripr::process_owner::OwnedProcess;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "common/mod.rs"]
mod common;
use common::fixture_git::fixture_git_ok;

const MAX_CAPTURE_BYTES: u64 = 1_048_576;

struct Fixture {
    scratch: PathBuf,
    root: PathBuf,
    foreign: PathBuf,
    diff: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("fixture clock: {error}"))?
            .as_nanos();
        let scratch = std::env::temp_dir().join(format!(
            "ripr-compact-navigation-{}-{stamp}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&scratch).map_err(|error| format!("create scratch: {error}"))?;
        let fixture = Self {
            root: scratch.join("repo with 'quote'"),
            foreign: scratch.join("foreign cwd"),
            diff: scratch.join("change with 'quote'.diff"),
            scratch,
        };
        fs::create_dir_all(fixture.root.join("src")).map_err(|error| error.to_string())?;
        fs::create_dir_all(fixture.root.join("tests")).map_err(|error| error.to_string())?;
        fs::create_dir(&fixture.foreign).map_err(|error| error.to_string())?;
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/boundary_gap");
        for relative in ["Cargo.toml", "src/lib.rs", "tests/pricing.rs"] {
            fs::copy(
                source.join("input").join(relative),
                fixture.root.join(relative),
            )
            .map_err(|error| format!("copy {relative}: {error}"))?;
        }
        let manifest_path = fixture.root.join("Cargo.toml");
        let mut manifest = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
        if !manifest.contains("[workspace]") {
            manifest.push_str("\n[workspace]\n");
            fs::write(&manifest_path, manifest).map_err(|error| error.to_string())?;
        }
        fs::copy(source.join("diff.patch"), &fixture.diff)
            .map_err(|error| format!("copy diff: {error}"))?;
        let source_path = fixture.root.join("src/lib.rs");
        let changed = fs::read_to_string(&source_path).map_err(|error| error.to_string())?;
        let original = changed.replacen(
            "amount >= discount_threshold",
            "amount > discount_threshold",
            1,
        );
        assert_ne!(
            original, changed,
            "fixture must contain the changed boundary"
        );
        fs::write(&source_path, original).map_err(|error| error.to_string())?;
        fixture_git_ok(&fixture.root, &["init"])?;
        fixture_git_ok(&fixture.root, &["config", "core.autocrlf", "false"])?;
        fixture_git_ok(
            &fixture.root,
            &["add", "Cargo.toml", "src/lib.rs", "tests/pricing.rs"],
        )?;
        fixture.commit("before boundary change")?;
        fs::write(&source_path, changed).map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn commit(&self, message: &str) -> Result<(), String> {
        fixture_git_ok(
            &self.root,
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "-am",
                message,
            ],
        )
    }

    fn run(&self, mut command: Command) -> Result<Output, String> {
        let stdout_path = self.scratch.join("stdout.log");
        let stderr_path = self.scratch.join("stderr.log");
        let stdout = fs::File::create(&stdout_path).map_err(|error| error.to_string())?;
        let stderr = fs::File::create(&stderr_path).map_err(|error| error.to_string())?;
        command
            .current_dir(&self.foreign)
            .env("RIPR_QUIET", "1")
            .env_remove("RIPR_PROGRESS")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        let mut child = OwnedProcess::spawn_isolated(command)
            .map_err(|error| format!("spawn navigation child: {error}"))?;
        let deadline = Instant::now() + Duration::from_secs(45);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            let oversized = [&stdout_path, &stderr_path].iter().any(|path| {
                fs::metadata(path).is_ok_and(|metadata| metadata.len() > MAX_CAPTURE_BYTES)
            });
            if Instant::now() >= deadline || oversized {
                child.terminate_tree()?;
                return Err("navigation child exceeded its time or output budget".to_string());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        child.terminate_tree()?;
        Ok(Output {
            status,
            stdout: read_capture(&stdout_path)?,
            stderr: read_capture(&stderr_path)?,
        })
    }

    fn check(&self, scope: &str, artifact: bool) -> Result<Output, String> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .args(["check", "--format", "agent", "--root"])
            .arg(&self.root);
        match scope {
            "diff" => {
                command.arg("--diff").arg(&self.diff);
            }
            "worktree" => {
                command.args(["--base", "HEAD", "--worktree"]);
            }
            "committed" => {
                command.args(["--base", "HEAD~1", "--committed"]);
            }
            _ => return Err(format!("unknown test scope {scope}")),
        }
        if artifact {
            command
                .arg("--write-artifact")
                .arg(self.scratch.join("saved 'check'.json"));
        }
        self.run(command)
    }

    fn follow(&self, printed: &str) -> Result<Output, String> {
        // Only product commands from this owned fixture reach this shell.
        // A script preserves the exact printed quoting; no whitespace split,
        // replacement of root/scope, or synthetic reconstruction is allowed.
        if !printed.starts_with("ripr explain ") && !printed.starts_with("ripr context ") {
            return Err(format!("unexpected navigation command: {printed}"));
        }
        let script = self.scratch.join("follow.sh");
        fs::write(
            &script,
            format!("ripr() {{ \"$RIPR_TEST_BINARY\" \"$@\"; }}\n{printed}\n"),
        )
        .map_err(|error| error.to_string())?;
        let mut command = Command::new("/bin/sh");
        command
            .arg(&script)
            .env("RIPR_TEST_BINARY", env!("CARGO_BIN_EXE_ripr"));
        self.run(command)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.scratch) {
            eprintln!(
                "remove navigation fixture {}: {error}",
                self.scratch.display()
            );
        }
    }
}

fn read_capture(path: &Path) -> Result<Vec<u8>, String> {
    let size = fs::metadata(path).map_err(|error| error.to_string())?.len();
    if size > MAX_CAPTURE_BYTES {
        return Err(format!(
            "capture {} exceeds {MAX_CAPTURE_BYTES} bytes",
            path.display()
        ));
    }
    fs::read(path).map_err(|error| error.to_string())
}

fn success_text(output: &Output) -> Result<String, String> {
    if !output.status.success() {
        return Err(format!(
            "navigation failed: {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    String::from_utf8(output.stdout.clone()).map_err(|error| error.to_string())
}

fn next_command(text: &str) -> Result<&str, String> {
    let commands: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("next="))
        .collect();
    match commands.as_slice() {
        [command] => Ok(command),
        _ => Err(format!("expected exactly one next command: {text}")),
    }
}

#[test]
fn compact_check_explain_context_journey_preserves_the_selected_subject() -> Result<(), String> {
    for scope in ["diff", "worktree", "committed"] {
        for artifact in [false, true] {
            let fixture = Fixture::new()?;
            if scope == "committed" {
                fixture.commit("changed boundary")?;
            }
            let checked = success_text(&fixture.check(scope, artifact)?)?;
            let id = checked
                .lines()
                .find(|line| line.starts_with("weakly_exposed|"))
                .and_then(|line| line.split_once("|id=").map(|(_, id)| id))
                .ok_or_else(|| {
                    format!("{scope}: real fixture must report a weak gap: {checked}")
                })?;
            let explain = next_command(&checked)?;
            assert!(explain.ends_with(" --format agent"), "{scope}: {explain}");
            assert_eq!(explain.contains("--from "), artifact, "{scope}: {explain}");
            let explained = success_text(&fixture.follow(explain)?)?;
            assert!(
                explained.starts_with("#ripr explain agent static-advisory"),
                "{explained}"
            );
            assert!(
                explained
                    .lines()
                    .any(|line| line.ends_with(&format!("|id={id}"))),
                "{explained}"
            );
            assert!(
                explained.lines().any(|line| line.starts_with("open=")),
                "{explained}"
            );
            let context = next_command(&explained)?;
            assert!(
                !context.contains("--format agent"),
                "context is JSON-only: {context}"
            );
            let packet_text = success_text(&fixture.follow(context)?)?;
            let packet: Value = serde_json::from_str(&packet_text)
                .map_err(|error| format!("context did not return JSON: {error}: {packet_text}"))?;
            assert_eq!(
                packet.pointer("/probe/id").and_then(Value::as_str),
                Some(id)
            );
            assert_eq!(packet["classification"], "weakly_exposed");
            assert_eq!(packet["source_currentness"], "candidate_current");
            // Removal control: the same route without the presentation option
            // still succeeds, but returns the ordinary human explanation.
            if scope == "diff" && !artifact {
                let ordinary = explain
                    .strip_suffix(" --format agent")
                    .ok_or("missing compact suffix for removal control")?;
                let human = success_text(&fixture.follow(ordinary)?)?;
                assert!(
                    !human.trim().is_empty(),
                    "human control must render evidence"
                );
                assert!(!human.starts_with("#ripr explain agent"), "{human}");
            }
        }
    }
    Ok(())
}
