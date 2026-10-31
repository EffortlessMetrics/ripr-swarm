//! #11397 / #11381: one weak-test gap gets one repair-readiness answer from
//! `ripr pilot`, `ripr agent card`, `ripr repair` and the MCP gap list.
//!
//! The scenario is the real-command audit's: `discount` has a boundary
//! predicate and one inline test that only asserts `> 0`, and the predicate
//! is edited (`>` to `>=`). With the edit uncommitted, `ripr repair` refuses
//! because the inline test module lives in the dirty production file; every
//! other surface must name that same cause and the same next step instead of
//! "Ready to repair." beside "no ready command", a card log of "0 findings",
//! or a producer-side fix-site gate. The committed control must keep saying
//! ready everywhere the CLI would start.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str =
    "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n";

fn source(op: &str) -> String {
    format!(
        "pub fn discount(qty: u32, limit: u32) -> u32 {{\n    if qty {op} limit {{ 10 }} else {{ 0 }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn weak() {{\n        assert!(discount(12, 10) > 0);\n    }}\n}}\n"
    )
}

struct Repo(PathBuf);

impl Repo {
    /// Base commit with `>`; the `>=` edit is committed or left in the tree.
    fn create(commit_change: bool) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-readiness-agree-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        // The guard exists before anything else can fail, so a partial
        // fixture is still removed.
        let repo = Self(root);
        std::fs::create_dir_all(repo.0.join("src")).map_err(|e| e.to_string())?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write(".gitignore", "target/\n.ripr/\n")?;
        repo.write("src/lib.rs", &source(">"))?;
        repo.git(&["-c", "init.defaultBranch=main", "init", "-q"])?;
        repo.git(&["config", "user.name", "fixture"])?;
        repo.git(&["config", "user.email", "fixture@ripr.invalid"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        repo.write("src/lib.rs", &source(">="))?;
        if commit_change {
            repo.git(&["commit", "-qam", "change"])?;
        }
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Run `ripr` in the repo; returns stdout and stderr joined.
    fn ripr(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The 16-hex gap id `ripr pilot` prints on its `gap <id>:` line.
fn pilot_seam_id(pilot: &str) -> Result<String, String> {
    pilot
        .lines()
        .find_map(|line| line.trim().strip_prefix("gap "))
        .and_then(|rest| rest.split(':').next())
        .map(str::to_string)
        .filter(|id| id.len() == 16)
        .ok_or_else(|| format!("pilot printed no gap id:\n{pilot}"))
}

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Mcp {
    fn spawn(root: &Path) -> Result<Self, String> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["mcp", "--stdio", "--root"])
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn ripr mcp: {e}"))?;
        let stdin = child.stdin.take().ok_or("mcp stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("mcp stdout")?);
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 0,
        })
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let request = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {
                "name": name, "arguments": arguments,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                    "io.modelcontextprotocol/clientInfo": {"name": "agree", "version": "1"}
                }
            }
        });
        writeln!(self.stdin, "{request}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        loop {
            let mut line = String::new();
            if self
                .stdout
                .read_line(&mut line)
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err(format!("mcp closed before replying to {name}"));
            }
            let reply: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if reply.get("id").and_then(Value::as_str) == Some(id.as_str()) {
                return reply
                    .pointer("/result/structuredContent")
                    .cloned()
                    .ok_or_else(|| format!("{name} reply has no structuredContent: {reply}"));
            }
        }
    }

    /// The one gap item after a refresh from the given source.
    fn only_gap(&mut self, source: &str) -> Result<Value, String> {
        let refresh = self.tool("ripr_refresh", json!({ "source": source }))?;
        if refresh["attempt"]["state"] != "completed" {
            return Err(format!("refresh did not complete: {refresh}"));
        }
        let gaps = self.tool("ripr_list_gaps", json!({}))?;
        let items = gaps["items"].as_array().ok_or("gap list omitted items")?;
        if items.len() != 1 {
            return Err(format!("scenario must yield exactly one gap: {gaps}"));
        }
        Ok(items[0].clone())
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn uncommitted_weak_test_gap_gets_one_cause_on_every_surface() -> Result<(), String> {
    let repo = Repo::create(false)?;
    let pilot = repo.ripr(&["pilot", "--root", "."])?;
    let seam = pilot_seam_id(&pilot)?;
    let hold = "uncommitted";

    // The gate the others must follow: `ripr repair` refuses the dirty file.
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(repair.contains("No repair attempt started"), "{repair}");
    assert!(repair.contains("Commit or stash `src/lib.rs`"), "{repair}");

    // Pilot offers the command only behind the same commit-or-stash step.
    let steps = pilot
        .split_once("Next, in order:")
        .map(|(_, steps)| steps)
        .ok_or_else(|| format!("pilot printed no ordered steps:\n{pilot}"))?;
    let precondition = steps
        .find("Before step 1: if `src/lib.rs` has uncommitted edits, commit or stash them")
        .ok_or_else(|| format!("pilot dropped the commit-or-stash step:\n{pilot}"))?;
    let start = steps
        .find("1. ripr agent repair")
        .ok_or_else(|| format!("pilot printed no repair start step:\n{pilot}"))?;
    assert!(precondition < start, "{pilot}");

    // Card: not ready, the same cause, the same next step, no contradiction.
    let card = repo.ripr(&["agent", "card", "--root", ".", "--seam-id", &seam])?;
    assert!(card.contains("Not ready to repair yet"), "{card}");
    assert!(card.contains(hold), "{card}");
    assert!(
        card.contains(&format!(
            "commit or stash `src/lib.rs`, then run `ripr repair {seam}"
        )),
        "{card}"
    );
    assert!(!card.contains("Ready to repair."), "{card}");
    assert!(!card.contains("no ready command"), "{card}");
    assert!(!card.contains("no bounded repair route"), "{card}");

    // Verbose diagnostics count the finding the card was built from.
    let verbose = repo.ripr(&[
        "agent",
        "card",
        "--root",
        ".",
        "--seam-id",
        &seam,
        "--verbose",
    ])?;
    assert!(
        verbose.contains("analysis complete — 1 probe, 1 finding"),
        "{verbose}"
    );
    assert!(!verbose.contains("0 findings"), "{verbose}");
    assert!(verbose.contains("repair start: held"), "{verbose}");

    // MCP: not ready, with the CLI's own typed gate cause and wording.
    let mut mcp = Mcp::spawn(&repo.0)?;
    let gap = mcp.only_gap("worktree")?;
    assert_eq!(gap["repair_packet_ready"], false, "{gap}");
    assert_eq!(gap["repair_ineligibility"], "seam_gate_refused", "{gap}");
    let canonical = gap["canonical_id"].as_str().ok_or("gap omitted its id")?;
    let prepare = mcp.tool("ripr_prepare_repair", json!({ "canonical_id": canonical }))?;
    let detail = prepare.to_string();
    assert!(
        detail.contains("uncommitted changes; commit or stash them first"),
        "{detail}"
    );
    Ok(())
}

#[test]
fn committed_weak_test_gap_keeps_saying_ready_where_repair_starts() -> Result<(), String> {
    let repo = Repo::create(true)?;
    let pilot = repo.ripr(&["pilot", "--root", "."])?;
    let seam = pilot_seam_id(&pilot)?;

    let card = repo.ripr(&["agent", "card", "--root", ".", "--seam-id", &seam])?;
    assert!(card.contains("Ready to repair."), "{card}");
    assert!(!card.contains("Not ready"), "{card}");
    assert!(!card.contains("no ready command"), "{card}");
    assert!(
        card.contains(&format!("Next: ripr repair {seam}")),
        "{card}"
    );

    // The command the card names starts, and the started card names the
    // attempt's next step instead of "no ready command" (#11381).
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(repair.contains("repair started"), "{repair}");
    assert!(
        repair.contains("The repair has started and is waiting for your test."),
        "{repair}"
    );
    assert!(
        repair.contains("write the test, then run `ripr continue"),
        "{repair}"
    );
    assert!(!repair.contains("no ready command"), "{repair}");
    assert!(!repair.contains("no bounded repair route"), "{repair}");
    Ok(())
}
