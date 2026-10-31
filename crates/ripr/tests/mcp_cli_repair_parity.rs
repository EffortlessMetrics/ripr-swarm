//! #10718: `ripr_prepare_repair` and `ripr_get_repair_card` refuse the same
//! item with the same typed cause `ripr repair` and `ripr agent card` give.
//!
//! One table of committed-branch fixtures drives the MCP server (refresh,
//! list, then per item prepare and card) and the CLI on the same repository,
//! and compares the refusal class each surface names:
//!
//! - an already-gripped seam: `seam_already_gripped` (prepare / `repair`) and
//!   `policy_omitted` (card / `agent card`), never `seam_not_found`;
//! - a route-not-ready seam: `seam_gate_refused` (prepare / `repair`), never
//!   the producer-side `fix_site_not_established`;
//! - an item no seam resolves (Python): `seam_not_found` carrying the CLI's
//!   "covers Rust files only" cause on prepare and card alike.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[workspace]\n";

fn rust_source(op: &str) -> String {
    format!(
        "pub fn discounted_total(amount: i32, limit: i32) -> i32 {{\n    if amount {op} limit {{\n        amount - 10\n    }} else {{\n        amount\n    }}\n}}\n"
    )
}

#[cfg(feature = "lang-python")]
fn python_source(op: &str) -> String {
    format!(
        "def discounted_total(amount, limit):\n    if amount {op} limit:\n        return amount - 10\n    return amount\n"
    )
}

struct Fixture(PathBuf);

impl Fixture {
    fn create(
        name: &str,
        base: &[(&str, String)],
        head: &[(&str, String)],
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-mcp-cli-parity-{name}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let fixture = Self(root);
        fixture.git(&["-c", "init.defaultBranch=main", "init", "-q"])?;
        fixture.git(&["config", "user.name", "ripr fixture"])?;
        fixture.git(&["config", "user.email", "fixture@ripr.invalid"])?;
        fixture.git(&["config", "commit.gpgSign", "false"])?;
        fixture.write_all(base)?;
        fixture.git(&["add", "-A"])?;
        fixture.git(&["commit", "-qm", "base"])?;
        fixture.git(&["checkout", "-q", "-b", "journey"])?;
        fixture.write_all(head)?;
        fixture.git(&["add", "-A"])?;
        fixture.git(&["commit", "-qm", "change"])?;
        Ok(fixture)
    }

    fn write_all(&self, files: &[(&str, String)]) -> Result<(), String> {
        for (path, text) in files {
            let target = self.0.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&target, text).map_err(|e| e.to_string())?;
        }
        Ok(())
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

    fn rust(name: &str, test: Option<&str>) -> Result<Self, String> {
        let mut base = vec![
            ("Cargo.toml", CARGO_TOML.to_string()),
            ("src/lib.rs", rust_source(">=")),
            (".gitignore", "target/\n.ripr/\n".to_string()),
        ];
        if let Some(test) = test {
            base.push(("tests/p.rs", format!("use fx::discounted_total;\n{test}")));
        }
        Self::create(name, &base, &[("src/lib.rs", rust_source(">"))])
    }

    #[cfg(feature = "lang-python")]
    fn python(name: &str, test: &str) -> Result<Self, String> {
        let base = [
            (
                "pyproject.toml",
                "[project]\nname = \"fx\"\nversion = \"0.0.0\"\n".to_string(),
            ),
            ("pricing.py", python_source(">=")),
            (
                "test_p.py",
                format!("from pricing import discounted_total\n{test}"),
            ),
            (".gitignore", ".ripr/\n__pycache__/\n".to_string()),
        ];
        Self::create(name, &base, &[("pricing.py", python_source(">"))])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ripr() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
}

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Mcp {
    fn spawn(root: &Path) -> Result<Self, String> {
        let mut child = ripr()
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

    /// One `tools/call`; returns the structured content (document or the
    /// `failure` block's parent).
    fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                    "io.modelcontextprotocol/clientInfo": {"name": "parity", "version": "1"}
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
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What every surface said about the one item of a fixture.
struct Observed {
    check_class: String,
    prepare_reason: String,
    prepare_detail: String,
    card_code: Option<String>,
    card_detail: String,
    repair_text: String,
    cli_card_kind: Option<String>,
    cli_card_text: String,
}

fn observe(fixture: &Fixture) -> Result<Observed, String> {
    let mut mcp = Mcp::spawn(&fixture.0)?;
    let refresh = mcp.tool("ripr_refresh", json!({}))?;
    if refresh["attempt"]["state"] != "completed" {
        return Err(format!("refresh did not complete: {refresh}"));
    }
    let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
    let items = gaps["items"].as_array().ok_or("gap list omitted items")?;
    if items.len() != 1 {
        return Err(format!("fixture must yield exactly one item: {gaps}"));
    }
    let item = &items[0];
    let canonical_id = item["canonical_id"].as_str().ok_or("item omitted id")?;
    let finding_id = item["finding_id"].as_str().unwrap_or(canonical_id);

    let prepare = mcp.tool(
        "ripr_prepare_repair",
        json!({ "canonical_id": canonical_id }),
    )?;
    let prepare_reason = prepare["ineligibility"]["reason"]
        .as_str()
        .or_else(|| prepare["failure"]["code"].as_str())
        .unwrap_or("ready")
        .to_string();
    let prepare_detail = prepare["ineligibility"]["detail"]
        .as_str()
        .or_else(|| prepare["failure"]["detail"].as_str())
        .unwrap_or_default()
        .to_string();
    let card = mcp.tool(
        "ripr_get_repair_card",
        json!({ "canonical_id": canonical_id }),
    )?;
    let card_code = card["failure"]["code"].as_str().map(str::to_string);
    let card_detail = card["failure"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // The CLI on the same fixture: a refused `repair` publishes no attempt.
    let output = ripr()
        .current_dir(&fixture.0)
        .args(["repair", finding_id, "--root", "."])
        .output()
        .map_err(|e| e.to_string())?;
    let repair_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // `agent card` takes a seam id; the CLI's own refusal text names it.
    let seam_id = repair_text
        .split("names seam `")
        .nth(1)
        .and_then(|rest| rest.split('`').next())
        .map(str::to_string);
    let (cli_card_kind, cli_card_text) = match seam_id {
        Some(seam_id) => {
            let output = ripr()
                .current_dir(&fixture.0)
                .args([
                    "agent",
                    "card",
                    "--root",
                    ".",
                    "--seam-id",
                    &seam_id,
                    "--json",
                ])
                .output()
                .map_err(|e| e.to_string())?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let kind = serde_json::from_str::<Value>(&stdout)
                .ok()
                .and_then(|doc| doc["error"]["kind"].as_str().map(str::to_string));
            (kind, stdout)
        }
        None => (None, String::new()),
    };
    Ok(Observed {
        check_class: item["class"].as_str().unwrap_or_default().to_string(),
        prepare_reason,
        prepare_detail,
        card_code,
        card_detail,
        repair_text,
        cli_card_kind,
        cli_card_text,
    })
}

#[test]
fn already_gripped_seam_is_refused_with_one_cause_on_every_surface() -> Result<(), String> {
    let fixture = Fixture::rust(
        "gripped",
        Some(
            "#[test]\nfn boundary() {\n    let r = discounted_total(100, 100);\n    assert_eq!(r, 90);\n}\n",
        ),
    )?;
    let seen = observe(&fixture)?;
    assert!(
        !seen.check_class.is_empty(),
        "the item must be a listed check class"
    );
    assert!(
        seen.repair_text.contains("already check it"),
        "precondition: the CLI repair gate names the seam already gripped:\n{}",
        seen.repair_text
    );
    assert_eq!(
        seen.prepare_reason, "seam_already_gripped",
        "{}",
        seen.prepare_detail
    );
    assert_eq!(
        seen.cli_card_kind.as_deref(),
        Some("policy_omitted"),
        "precondition: the CLI card refuses the gripped seam by policy:\n{}",
        seen.cli_card_text
    );
    assert_eq!(
        seen.card_code.as_deref(),
        seen.cli_card_kind.as_deref(),
        "MCP card must refuse like `agent card` (not seam_not_found): {}",
        seen.card_detail
    );
    assert!(
        !seen.card_detail.contains("no classified seam"),
        "{}",
        seen.card_detail
    );
    Ok(())
}

#[test]
fn route_not_ready_seam_is_seam_gate_refused_not_a_producer_gate() -> Result<(), String> {
    for (name, test) in [
        (
            "weak",
            Some("#[test]\nfn boundary() {\n    assert!(discounted_total(100, 100) > 0);\n}\n"),
        ),
        ("no-test", None),
    ] {
        let fixture = Fixture::rust(name, test)?;
        let seen = observe(&fixture)?;
        assert!(
            seen.repair_text
                .contains("not ready for an automatic repair: repair route not ready"),
            "{name}: precondition, the CLI gate says the route is not ready:\n{}",
            seen.repair_text
        );
        assert_eq!(
            seen.prepare_reason, "seam_gate_refused",
            "{name}: prepare must carry the gate's cause, not a producer gate: {}",
            seen.prepare_detail
        );
        assert!(
            seen.prepare_detail.contains("repair route not ready"),
            "{name}: prepare detail must carry the CLI's cause: {}",
            seen.prepare_detail
        );
        assert_ne!(seen.prepare_reason, "fix_site_not_established", "{name}");
        // The card side already matched: the CLI card and the MCP card agree.
        match seen.cli_card_kind.as_deref() {
            Some(kind) => assert_eq!(seen.card_code.as_deref(), Some(kind), "{name}"),
            None => assert_eq!(
                seen.card_code.as_deref(),
                None,
                "{name}: the CLI mints a card, so MCP must not refuse: {}",
                seen.card_detail
            ),
        }
    }
    Ok(())
}

/// #11111 burn-down: `ripr agent stub` refuses an async owner, so neither
/// surface may offer that stub command as the route. The MCP server reads the
/// workspace sources like the CLI does and carries the same named refusal.
#[test]
fn async_owner_refusal_names_the_stub_refusal_on_both_surfaces() -> Result<(), String> {
    let lib = |op: &str| format!("pub async fn dbl(x: i32) -> i32 {{\n    x {op} 2\n}}\n");
    let fixture = Fixture::create(
        "async-owner",
        &[
            ("Cargo.toml", CARGO_TOML.to_string()),
            ("src/lib.rs", lib("*")),
            (
                "tests/p.rs",
                "use fx::dbl;\n\n#[test]\nfn dbl_positive() {\n    let v = block_on(dbl(2));\n    assert!(v > 0);\n}\nfn block_on<T>(_f: impl std::future::Future<Output = T>) -> T {\n    unimplemented!()\n}\n".to_string(),
            ),
            (".gitignore", "target/\n.ripr/\n".to_string()),
        ],
        &[("src/lib.rs", lib("+"))],
    )?;
    let seen = observe(&fixture)?;
    let named = "ripr cannot write a fill-in skeleton here because the owner is async";
    assert!(
        seen.repair_text.contains(named) && !seen.repair_text.contains("--kind return_value"),
        "precondition: the CLI names the stub's own refusal:\n{}",
        seen.repair_text
    );
    assert_eq!(
        seen.prepare_reason, "seam_gate_refused",
        "{}",
        seen.prepare_detail
    );
    assert!(
        seen.prepare_detail.contains(named)
            && !seen.prepare_detail.contains("--kind return_value")
            && !seen.prepare_detail.contains("--root"),
        "MCP must carry the same named refusal, with no stub command and no --root: {}",
        seen.prepare_detail
    );
    Ok(())
}

// The Python fixture needs the Python adapter compiled in to yield a listed
// item at all (the Rust-only feature lane builds without it).
#[cfg(feature = "lang-python")]
#[test]
fn item_no_seam_resolves_carries_the_cli_cause_on_prepare_and_card() -> Result<(), String> {
    for (name, test) in [
        (
            "py-weak",
            "def test_boundary():\n    assert discounted_total(100, 100) > 0\n",
        ),
        (
            "py-strong",
            "def test_boundary():\n    assert discounted_total(100, 100) == 90\n    assert discounted_total(101, 100) == 91\n    assert discounted_total(99, 100) == 99\n",
        ),
    ] {
        let fixture = Fixture::python(name, test)?;
        let seen = observe(&fixture)?;
        let cause = "covers Rust files only";
        assert!(
            seen.repair_text.contains(cause),
            "{name}: precondition, the CLI repair gate names the Rust-only inventory:\n{}",
            seen.repair_text
        );
        assert_eq!(
            seen.prepare_reason, "seam_not_found",
            "{name}: {}",
            seen.prepare_detail
        );
        assert!(
            seen.prepare_detail.contains(cause),
            "{name}: prepare must carry the CLI's cause: {}",
            seen.prepare_detail
        );
        assert_eq!(seen.card_code.as_deref(), Some("seam_not_found"), "{name}");
        assert!(
            seen.card_detail.contains(cause),
            "{name}: card must carry the CLI's cause: {}",
            seen.card_detail
        );
        assert!(
            !seen.check_class.is_empty(),
            "{name}: the item must be a listed check class"
        );
    }
    Ok(())
}
