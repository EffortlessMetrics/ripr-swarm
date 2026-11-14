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
//! - a gate-admitted weak repair target (named missing discriminator over a
//!   weak direct test): ready on list, prepare, card and `ripr repair`, all
//!   naming the one test file the repair extends;
//! - a proposal-only seam (#13538): ready on list and prepare exactly when the
//!   CLI start gate admits it, naming the CLI edit cage's file; a seam with no
//!   proposal stays not ready on both;
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
        .split("names gap `")
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

/// #13382: the shared card projects live durable state through the actual
/// stdio transport; inspection does not rewrite or finish the attempt.
#[test]
fn repair_card_next_step_tracks_live_attempt_state_over_stdio() -> Result<(), String> {
    for failed in [false, true] {
        let weak = "#[test]\nfn below() { assert_eq!(discounted_total(50,100),50); }\n#[test]\nfn above() { assert_eq!(discounted_total(10000,100),9990); }\n";
        let fixture = Fixture::rust("attempt-state", Some(weak))?;
        let run = |args: &[&str]| {
            ripr()
                .current_dir(&fixture.0)
                .args(args)
                .output()
                .map_err(|e| e.to_string())
        };
        let start = run(&["repair"])?;
        assert!(
            start.status.success(),
            "{}",
            String::from_utf8_lossy(&start.stderr)
        );
        let manifests = std::fs::read_dir(fixture.0.join("target/ripr/repair-attempts"))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("attempt.json"))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        assert_eq!(manifests.len(), 1);
        let path = &manifests[0];
        let before: Value =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let id = before["repair_attempt_id"]
            .as_str()
            .ok_or("missing attempt id")?;
        let mut mcp = Mcp::spawn(&fixture.0)?;
        for after in [false, true] {
            if after {
                let (file, edit) = if failed {
                    (
                        "src/lib.rs",
                        format!("{}\n// forbidden production edit\n", rust_source(">")),
                    )
                } else {
                    (
                        "tests/p.rs",
                        format!(
                            "use fx::discounted_total;\n{weak}\n#[test]\nfn off_boundary() {{ assert_eq!(discounted_total(150,100),140); }}\n"
                        ),
                    )
                };
                std::fs::write(fixture.0.join(file), edit).map_err(|e| e.to_string())?;
                let result = run(&["continue", "--attempt", id])?;
                assert_eq!(
                    result.status.code(),
                    Some(if failed { 2 } else { 0 }),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let manifest: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let state = if !after {
                "awaiting_edit"
            } else if failed {
                "failed"
            } else {
                "ready_to_finish"
            };
            assert_eq!(manifest["state"], state);
            let refresh = mcp.tool("ripr_refresh", json!({ "source": "worktree" }))?;
            assert_eq!(refresh["attempt"]["state"], "completed", "{refresh}");
            let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
            let items = gaps["items"].as_array().ok_or("missing items")?;
            assert_eq!(items.len(), 1, "{gaps}");
            let card = mcp.tool(
                "ripr_get_repair_card",
                json!({"canonical_id": items[0]["canonical_id"]}),
            )?;
            assert!(card.get("failure").is_none(), "{card}");
            assert_eq!(card["card"]["attempt"]["state"], state, "{card}");
            assert_eq!(card["card"]["attempt"]["attempt_id"], id, "{card}");
            let action = card["card"]["canonical_next_action"].to_string();
            assert!(
                action.contains(if after {
                    "ripr agent status"
                } else {
                    "ripr continue"
                }) && action.contains(id),
                "{action}"
            );
            assert!(!action.contains("ripr:agent:packet"), "{action}");
            assert_eq!(
                std::fs::read(path).map_err(|e| e.to_string())?,
                bytes,
                "MCP inspection rewrote the manifest"
            );
        }
    }
    Ok(())
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

/// A weakly exposed boundary change whose weak test the repair extends: the
/// shared gate admits the seam, so every route must call the item ready, name
/// the same single edit file, and carry no `fix_site_not_established` claim.
/// Before this, `ripr_list_gaps` / `ripr_prepare_repair` demanded a strong test
/// (which would make the item exposed) while the card and `ripr repair` were
/// ready, and the recovery sent the agent to a `ripr_refresh` that could not
/// change the verdict.
#[test]
fn weak_test_repair_target_is_ready_with_one_fix_site_on_every_route() -> Result<(), String> {
    let lib = |op: &str| {
        format!(
            "pub fn discount(qty: u32) -> u32 {{\n    if qty {op} 10 {{ 5 }} else {{ 0 }}\n}}\n"
        )
    };
    let fixture = Fixture::create(
        "weak-extend",
        &[
            ("Cargo.toml", CARGO_TOML.to_string()),
            ("src/lib.rs", lib(">=")),
            (
                "tests/discount.rs",
                "use fx::discount;\n#[test]\nfn total_positive() {\n    assert!(discount(12) > 0);\n}\n".to_string(),
            ),
            (".gitignore", "target/\n.ripr/\n".to_string()),
        ],
        &[("src/lib.rs", lib(">"))],
    )?;
    let mut mcp = Mcp::spawn(&fixture.0)?;
    let refresh = mcp.tool("ripr_refresh", json!({}))?;
    if refresh["attempt"]["state"] != "completed" {
        return Err(format!("refresh did not complete: {refresh}"));
    }
    let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
    let items = gaps["items"].as_array().ok_or("gap list omitted items")?;
    let [item] = items.as_slice() else {
        return Err(format!("fixture must yield exactly one item: {gaps}"));
    };
    assert_eq!(item["class"], "weakly_exposed", "{item}");
    let canonical_id = item["canonical_id"].as_str().ok_or("item omitted id")?;
    let finding_id = item["finding_id"].as_str().unwrap_or(canonical_id);
    assert_eq!(item["repair_packet_ready"], true, "list_gaps: {item}");
    assert!(item["repair_ineligibility"].is_null(), "list_gaps: {item}");

    let prepare = mcp.tool(
        "ripr_prepare_repair",
        json!({ "canonical_id": canonical_id }),
    )?;
    assert_eq!(prepare["repair_packet_ready"], true, "prepare: {prepare}");
    assert_eq!(
        prepare["allowed_edit_surface"],
        json!(["tests/discount.rs"]),
        "{prepare}"
    );
    assert_eq!(prepare["fix_site"]["oracle_strength"], "weak", "{prepare}");

    let card = mcp.tool(
        "ripr_get_repair_card",
        json!({ "canonical_id": canonical_id }),
    )?;
    assert!(card["failure"].is_null(), "card refused: {card}");
    assert_eq!(
        card["card"]["allowed_files"],
        json!(["tests/discount.rs"]),
        "{card}"
    );

    let output = ripr()
        .current_dir(&fixture.0)
        .args(["repair", finding_id, "--root", "."])
        .output()
        .map_err(|e| e.to_string())?;
    let cli = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        cli.contains("Ready to repair") && cli.contains("Files you may change: tests/discount.rs"),
        "the CLI names the same ready item and edit file:\n{cli}"
    );
    Ok(())
}

/// With two weak direct tests the MCP edit surface must be the file the CLI
/// names as the test to extend, not whichever related test sorts first.
#[test]
fn two_weak_tests_name_the_same_edit_file_on_every_route() -> Result<(), String> {
    let lib = |op: &str| {
        format!(
            "pub fn discount(qty: u32) -> u32 {{\n    if qty {op} 10 {{ 5 }} else {{ 0 }}\n}}\n"
        )
    };
    let fixture = Fixture::create(
        "two-weak",
        &[
            ("Cargo.toml", CARGO_TOML.to_string()),
            ("src/lib.rs", lib(">=")),
            (
                "tests/aaa_first.rs",
                "use fx::discount;\n#[test]\nfn also_positive() {\n    assert!(discount(15) >= 1);\n}\n".to_string(),
            ),
            (
                "tests/discount.rs",
                "use fx::discount;\n#[test]\nfn total_positive() {\n    assert!(discount(12) > 0);\n}\n".to_string(),
            ),
            (".gitignore", "target/\n.ripr/\n".to_string()),
        ],
        &[("src/lib.rs", lib(">"))],
    )?;
    let mut mcp = Mcp::spawn(&fixture.0)?;
    mcp.tool("ripr_refresh", json!({}))?;
    let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
    let [item] = gaps["items"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    else {
        return Err(format!("fixture must yield exactly one item: {gaps}"));
    };
    let canonical_id = item["canonical_id"].as_str().ok_or("item omitted id")?;
    let finding_id = item["finding_id"].as_str().unwrap_or(canonical_id);
    let output = ripr()
        .current_dir(&fixture.0)
        .args(["repair", finding_id, "--root", "."])
        .output()
        .map_err(|e| e.to_string())?;
    let cli = String::from_utf8_lossy(&output.stdout).to_string();
    let cli_file = cli
        .split("Files you may change: ")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .map(str::trim)
        .ok_or_else(|| format!("CLI named no edit file:\n{cli}"))?
        .to_string();
    let prepare = mcp.tool(
        "ripr_prepare_repair",
        json!({ "canonical_id": canonical_id }),
    )?;
    let card = mcp.tool(
        "ripr_get_repair_card",
        json!({ "canonical_id": canonical_id }),
    )?;
    if prepare["repair_packet_ready"] == true {
        assert_eq!(
            prepare["allowed_edit_surface"],
            json!([cli_file]),
            "{prepare}"
        );
    } else {
        // Fail-closed is acceptable; naming a different edit file is not.
        assert!(prepare["allowed_edit_surface"].is_null(), "{prepare}");
    }
    assert_eq!(card["card"]["allowed_files"], json!([cli_file]), "{card}");
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

/// The first object tagged `"kind": "proposed"` anywhere in a card document.
fn proposed_target(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("proposed")
                && map.contains_key("file")
            {
                return Some(value);
            }
            map.values().find_map(proposed_target)
        }
        Value::Array(items) => items.iter().find_map(proposed_target),
        _ => None,
    }
}

/// The fixture's first repo-exposure seam id.
fn first_seam_id(fixture: &Fixture) -> Result<String, String> {
    let output = ripr()
        .current_dir(&fixture.0)
        .args([
            "check",
            "--root",
            ".",
            "--mode",
            "draft",
            "--format",
            "repo-exposure-json",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "repo-exposure failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let seams: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    seams
        .pointer("/seams/0/seam_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("fixture produced no seam: {seams}"))
}

/// `ripr agent repair --phase before` on the fixture's first seam: the CLI
/// start gate. `Ok(Some(path))` is the edit cage's `selected_target.path` of
/// the started attempt; `Ok(None)` is a refusal that published no attempt.
fn cli_start_cage_target(fixture: &Fixture) -> Result<Option<String>, String> {
    Ok(cli_start_policy(fixture)?.and_then(|policy| {
        policy
            .pointer("/selected_target/path")
            .and_then(Value::as_str)
            .map(str::to_string)
    }))
}

/// The retained edit-cage policy of a started attempt, or `None` on refusal.
fn cli_start_policy(fixture: &Fixture) -> Result<Option<Value>, String> {
    let seam = first_seam_id(fixture)?;
    let output = ripr()
        .current_dir(&fixture.0)
        .args([
            "agent",
            "repair",
            "--json",
            "--root",
            ".",
            "--seam-id",
            &seam,
            "--phase",
            "before",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let published = std::fs::read_dir(fixture.0.join("target/ripr/repair-attempts"))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
                    .count()
            })
            .unwrap_or(0);
        if published != 0 {
            return Err("a refused start must publish no attempt".to_string());
        }
        return Ok(None);
    }
    let started: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let attempt = started
        .pointer("/repair_attempt/attempt_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("start named no attempt: {started}"))?;
    let read = |path: PathBuf| -> Result<Value, String> {
        let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    };
    let manifest = read(
        fixture
            .0
            .join("target/ripr/repair-attempts")
            .join(attempt)
            .join("attempt.json"),
    )?;
    let baseline_path = manifest["artifacts"]
        .as_array()
        .and_then(|artifacts| artifacts.iter().find(|a| a["role"] == "edit_cage_baseline"))
        .and_then(|a| a["path"].as_str())
        .ok_or_else(|| format!("manifest has no edit_cage_baseline: {manifest}"))?;
    let baseline = read(fixture.0.join(baseline_path))?;
    baseline
        .pointer("/policy")
        .cloned()
        .map(Some)
        .ok_or_else(|| format!("baseline has no policy: {baseline}"))
}

/// `ripr agent card --json` for the fixture's first repo-exposure seam. Both
/// commands must exit successfully so a failed command cannot pass as "none".
fn cli_card_for_first_seam(fixture: &Fixture) -> Result<(Option<i32>, Value), String> {
    let seam_output = ripr()
        .current_dir(&fixture.0)
        .args([
            "check",
            "--root",
            ".",
            "--mode",
            "draft",
            "--format",
            "repo-exposure-json",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !seam_output.status.success() {
        return Err(format!(
            "repo-exposure failed: {}",
            String::from_utf8_lossy(&seam_output.stderr)
        ));
    }
    let seams: Value = serde_json::from_slice(&seam_output.stdout).map_err(|e| {
        format!(
            "repo-exposure is not JSON ({e}): {}",
            String::from_utf8_lossy(&seam_output.stderr)
                .chars()
                .take(500)
                .collect::<String>()
        )
    })?;
    let seam_id = seams
        .pointer("/seams/0/seam_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("fixture produced no seam: {seams}"))?;
    let card_output = ripr()
        .current_dir(&fixture.0)
        .args([
            "agent",
            "card",
            "--root",
            ".",
            "--seam-id",
            seam_id,
            "--json",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    // A typed refusal (exit 3) legitimately carries its JSON envelope; any
    // other status is an operational failure that must not read as "none".
    let code = card_output.status.code();
    if !matches!(code, Some(0 | 3)) {
        return Err(format!(
            "agent card failed ({code:?}): {}",
            String::from_utf8_lossy(&card_output.stderr)
                .chars()
                .take(500)
                .collect::<String>()
        ));
    }
    let card = serde_json::from_slice(&card_output.stdout).map_err(|e| {
        format!(
            "card is not JSON ({e}): {}",
            String::from_utf8_lossy(&card_output.stderr)
        )
    })?;
    Ok((code, card))
}

/// #13204: a seam whose only repair target is the producer-owned new-test
/// proposal. Through a real finding (real producer, real seam inventory, the
/// MCP server over stdio), MCP discloses the same
/// `proposed_new_test` kind and file the CLI `agent card` projects for the same
/// seam; a seam with no test layout to propose into discloses none.
#[test]
fn proposed_new_test_is_disclosed_by_mcp_and_agrees_with_the_cli_card() -> Result<(), String> {
    let base = [
        ("Cargo.toml", CARGO_TOML.to_string()),
        ("src/lib.rs", rust_source(">=")),
        ("tests/smoke.rs", "#[test]\nfn smoke() {}\n".to_string()),
        (".gitignore", "target/\n.ripr/\n".to_string()),
    ];
    let fixture = Fixture::create("proposed", &base, &[("src/lib.rs", rust_source(">"))])?;

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
    // #13538: the shared gate admits the proposal as the seam's one repair
    // target, so the item is ready exactly when the CLI start gate admits it.
    assert_eq!(item["repair_packet_ready"], true, "{item}");
    assert!(item["repair_ineligibility"].is_null(), "{item}");
    let disclosed = &item["proposed_new_test"];
    assert_eq!(disclosed["kind"], "integration", "{item}");
    let mcp_file = disclosed["file"]
        .as_str()
        .ok_or_else(|| format!("MCP disclosed no proposed file: {item}"))?;
    assert_eq!(mcp_file, "tests/discounted_total.rs", "{item}");

    // The CLI card for the same seam projects the same kind and file.
    let (card_code, card) = cli_card_for_first_seam(&fixture)?;
    assert_eq!(card_code, Some(0), "the proposal-only card starts: {card}");
    let cli = proposed_target(&card)
        .ok_or_else(|| format!("the CLI card projects no proposed target: {card}"))?;
    assert_eq!(cli["file"].as_str(), Some(mcp_file), "{cli} vs {item}");
    assert_eq!(cli["proposal_kind"], disclosed["kind"], "{cli} vs {item}");

    // Readiness agreement: the CLI start gate admits the same seam, and the
    // edit cage it enforces is the file `ripr_prepare_repair` names.
    let cage = cli_start_cage_target(&fixture)?
        .ok_or("the CLI start gate refused the seam MCP lists as ready")?;
    assert_eq!(cage, mcp_file, "CLI cage vs MCP disclosure: {item}");
    let canonical_id = item["canonical_id"].as_str().ok_or("item omitted id")?;
    let prepare = mcp.tool(
        "ripr_prepare_repair",
        json!({ "canonical_id": canonical_id }),
    )?;
    assert_eq!(prepare["repair_packet_ready"], true, "{prepare}");
    assert_eq!(prepare["allowed_edit_surface"], json!([cage]), "{prepare}");
    assert_eq!(prepare["fix_site"]["file"], json!(cage), "{prepare}");
    assert_eq!(
        prepare["fix_site"]["new_test_kind"], "integration",
        "{prepare}"
    );

    // Negative: with no tests/ layout the producer admits no proposal to
    // propose into, so the same item is still held and both surfaces name
    // none (the fixture varies only the layout: the unrelated smoke test).
    let no_layout = Fixture::rust("proposed-none", None)?;
    let mut mcp = Mcp::spawn(&no_layout.0)?;
    mcp.tool("ripr_refresh", json!({}))?;
    let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
    let item = gaps["items"]
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| format!("no-layout fixture listed no item: {gaps}"))?;
    assert_eq!(
        item["repair_packet_ready"], false,
        "control stays held: {item}"
    );
    assert!(
        cli_start_cage_target(&no_layout)?.is_none(),
        "the CLI start gate must also refuse the seam with no proposal"
    );
    let no_layout_id = item["canonical_id"].as_str().ok_or("item omitted id")?;
    let held = mcp.tool(
        "ripr_prepare_repair",
        json!({ "canonical_id": no_layout_id }),
    )?;
    assert_eq!(held["repair_packet_ready"], false, "{held}");
    assert!(held["allowed_edit_surface"].is_null(), "{held}");
    assert!(
        item.get("proposed_new_test").is_none(),
        "no proposal may be disclosed without one: {item}"
    );
    let (_, card) = cli_card_for_first_seam(&no_layout)?;
    assert!(
        proposed_target(&card).is_none(),
        "the CLI card must also project no proposal: {card}"
    );
    Ok(())
}

/// #13538: the inline proposal-only path. A private owner with one governed,
/// empty inline test module and no `tests/` layout has the module as its only
/// producer-owned target. The shared verdict is the proposal (not a plain
/// inline test), MCP lists the item ready, and `ripr_prepare_repair` stays
/// confined to the governed module exactly as the CLI cage is, while naming the
/// proposal (no existing test) as its fix site.
#[test]
fn inline_proposal_only_item_is_ready_and_confined_like_the_cli_cage() -> Result<(), String> {
    let lib = |op: &str| {
        format!(
            "fn discounted_total(amount: i32, limit: i32) -> i32 {{\n    if amount {op} limit {{\n        amount - 10\n    }} else {{\n        amount\n    }}\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n}}\n"
        )
    };
    let fixture = Fixture::create(
        "inline-proposed",
        &[
            ("Cargo.toml", CARGO_TOML.to_string()),
            ("src/lib.rs", lib(">=")),
            (".gitignore", "target/\n.ripr/\n".to_string()),
        ],
        &[("src/lib.rs", lib(">"))],
    )?;
    let mut mcp = Mcp::spawn(&fixture.0)?;
    mcp.tool("ripr_refresh", json!({}))?;
    let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
    let [item] = gaps["items"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    else {
        return Err(format!("fixture must yield exactly one item: {gaps}"));
    };
    assert_eq!(item["proposed_new_test"]["kind"], "inline_unit", "{item}");
    assert_eq!(item["proposed_new_test"]["module"], "tests", "{item}");
    assert_eq!(item["repair_packet_ready"], true, "{item}");
    assert!(item["repair_ineligibility"].is_null(), "{item}");

    let policy = cli_start_policy(&fixture)?
        .ok_or("the CLI start gate refused the seam MCP lists as ready")?;
    assert_eq!(policy["selected_target"]["path"], "src/lib.rs", "{policy}");
    assert_eq!(policy["inline_test_module_target"], true, "{policy}");

    let canonical_id = item["canonical_id"].as_str().ok_or("item omitted id")?;
    let prepare = mcp.tool(
        "ripr_prepare_repair",
        json!({ "canonical_id": canonical_id }),
    )?;
    assert_eq!(prepare["repair_packet_ready"], true, "{prepare}");
    assert_eq!(
        prepare["allowed_edit_surface"],
        json!(["src/lib.rs"]),
        "{prepare}"
    );
    assert_eq!(
        prepare["fix_site"]["new_test_kind"], "inline_unit",
        "{prepare}"
    );
    assert_eq!(
        prepare["inline_test_module"]["module"], "tests",
        "{prepare}"
    );
    assert_eq!(
        prepare["inline_test_module"]["file"], "src/lib.rs",
        "{prepare}"
    );
    Ok(())
}

/// What the caller reports about its own run of the focused test at finish.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReportedRun {
    /// The agent passes no `--test-result`.
    None,
    /// The agent reports the new test ran and passed.
    Passed,
    /// The agent reports the new test ran and failed.
    Failed,
}

/// The last line of a `check --format agent` report: its `verify=` record.
fn agent_check_verify_line(fixture: &Fixture, worktree: bool) -> Result<String, String> {
    let mut command = ripr();
    command
        .current_dir(&fixture.0)
        .args(["check", "--quiet", "--format", "agent", "--base", "main"]);
    if worktree {
        command.arg("--worktree");
    }
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "agent check failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .map(str::to_string)
        .ok_or_else(|| "agent check printed nothing".to_string())
}

/// #14079: the six-step agent contract, end to end through the portable CLI
/// and the MCP server on one repository, for each thing a caller can report
/// about its own test run.
///
/// 1. both surfaces identify the one gap;
/// 2. both hand out one bounded unit naming the one test file, and edit
///    nothing;
/// 3. the caller makes one test change inside that file;
/// 4. the finish re-records static evidence as `improved`, and both surfaces
///    read the gap as noticed afterwards;
/// 5. execution stays a separate axis: the receipt records that ripr ran
///    nothing, and the caller's reported run (passed, failed or absent) is
///    read back as exactly that, never as the static result and never as
///    "none recorded" once one was recorded;
/// 6. the result is durable: a new MCP server process reads the same attempt,
///    receipt state and digest-bound retained evidence.
#[test]
fn six_step_agent_contract_holds_through_cli_and_mcp() -> Result<(), String> {
    for reported in [ReportedRun::None, ReportedRun::Passed, ReportedRun::Failed] {
        six_step_agent_contract(reported).map_err(|error| format!("{reported:?}: {error}"))?;
    }
    Ok(())
}

fn six_step_agent_contract(reported: ReportedRun) -> Result<(), String> {
    use sha2::{Digest as _, Sha256};
    const TEST_FILE: &str = "tests/p.rs";
    const NEW_TEST: &str = "discounted_total_boundary_discriminator";
    let weak = "#[test]\nfn below() { assert_eq!(discounted_total(50,100),50); }\n#[test]\nfn above() { assert_eq!(discounted_total(10000,100),9990); }\n";
    let fixture = Fixture::rust("six-step", Some(weak))?;
    let read = |path: &str| std::fs::read(fixture.0.join(path)).map_err(|e| format!("{path}: {e}"));
    let run = |args: &[&str]| {
        ripr()
            .current_dir(&fixture.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())
    };
    let stdout_json = |output: &std::process::Output| -> Result<Value, String> {
        serde_json::from_slice(&output.stdout).map_err(|e| {
            format!(
                "stdout is not JSON ({e}): {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
    };
    let production_before = read("src/lib.rs")?;
    let test_before = read(TEST_FILE)?;

    // 1. Gap identified, by the CLI and by MCP.
    let verify = agent_check_verify_line(&fixture, false)?;
    assert!(
        verify.starts_with("verify=refused|") && verify.contains("reason=gaps_remain"),
        "{verify}"
    );
    let mut mcp = Mcp::spawn(&fixture.0)?;
    let refresh = mcp.tool("ripr_refresh", json!({}))?;
    assert_eq!(refresh["attempt"]["state"], "completed", "{refresh}");
    let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
    let items = gaps["items"].as_array().ok_or("gap list omitted items")?;
    assert_eq!(items.len(), 1, "{gaps}");
    assert_eq!(items[0]["class"], "weakly_exposed", "{gaps}");
    assert_eq!(items[0]["repair_packet_ready"], true, "{gaps}");
    let canonical_id = items[0]["canonical_id"].clone();

    // 2. One bounded unit: the same single test file on both surfaces, no
    //    command route from MCP, and no edit by either.
    let prepare = mcp.tool(
        "ripr_prepare_repair",
        json!({ "canonical_id": canonical_id }),
    )?;
    assert_eq!(prepare["repair_packet_ready"], true, "{prepare}");
    assert_eq!(
        prepare["allowed_edit_surface"],
        json!([TEST_FILE]),
        "{prepare}"
    );
    assert_eq!(prepare["command_routes"], json!([]), "{prepare}");
    let start = run(&["repair", "--root", ".", "--base", "main", "--json"])?;
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let packet = stdout_json(&start)?;
    let id = packet["repair_attempt"]["attempt_id"]
        .as_str()
        .ok_or_else(|| format!("repair --json omitted the attempt id: {packet}"))?
        .to_string();
    let manifest_path = fixture
        .0
        .join("target/ripr/repair-attempts")
        .join(&id)
        .join("attempt.json");
    let manifest = |label: &str| -> Result<Value, String> {
        let bytes = std::fs::read(&manifest_path).map_err(|e| format!("{label}: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{label}: {e}"))
    };
    assert_eq!(manifest("after start")?["state"], "awaiting_edit");
    let waiting = mcp.tool("ripr_get_receipt_status", json!({ "receipt_id": id }))?;
    assert_eq!(waiting["status"], "awaiting_edit", "{waiting}");
    assert_eq!(
        read("src/lib.rs")?,
        production_before,
        "ripr edited production"
    );
    assert_eq!(read(TEST_FILE)?, test_before, "ripr edited the test file");

    // 3. One test change, inside the one allowed file.
    let mut edited = test_before.clone();
    edited.extend_from_slice(
        format!("#[test]\nfn {NEW_TEST}() {{ assert_eq!(discounted_total(100, 100), 100); }}\n")
            .as_bytes(),
    );
    std::fs::write(fixture.0.join(TEST_FILE), &edited).map_err(|e| e.to_string())?;

    // 4. Finish: static evidence is re-recorded. The caller's own test run,
    //    when it reports one, is a claim bound to this attempt and HEAD.
    let result_file = fixture.0.with_extension("focused-result.json");
    let mut finish = vec![
        "agent",
        "repair",
        "--root",
        ".",
        "--attempt",
        &id,
        "--phase",
        "after",
    ];
    let result_arg = result_file.display().to_string();
    if reported != ReportedRun::None {
        let head = ripr_fixture_head(&fixture)?;
        let event = if reported == ReportedRun::Passed {
            "ok"
        } else {
            "failed"
        };
        let document = json!({
            "schema_version": "0.1",
            "kind": "focused_test_result",
            "attempt_id": id,
            "head": head,
            "test_file": TEST_FILE,
            "libtest_events": [
                {"type": "test", "event": "started", "name": NEW_TEST},
                {"type": "test", "event": event, "name": NEW_TEST}
            ]
        });
        std::fs::write(&result_file, document.to_string()).map_err(|e| e.to_string())?;
        finish.extend(["--test-result", &result_arg]);
    }
    let finished = run(&finish)?;
    let _ = std::fs::remove_file(&result_file);
    let finish_stdout = String::from_utf8_lossy(&finished.stdout).to_string();
    assert!(
        finished.status.success(),
        "{finish_stdout}\n{}",
        String::from_utf8_lossy(&finished.stderr)
    );
    assert!(finish_stdout.contains("improved"), "{finish_stdout}");
    let receipt = mcp.tool("ripr_get_receipt_status", json!({ "receipt_id": id }))?;
    assert_eq!(receipt["status"], "improved", "{receipt}");
    let verify = agent_check_verify_line(&fixture, true)?;
    assert!(
        verify.starts_with("verify=complete|") && verify.ends_with("findings=1|exposed=1"),
        "{verify}"
    );
    let refresh = mcp.tool("ripr_refresh", json!({ "source": "worktree" }))?;
    assert_eq!(refresh["attempt"]["state"], "completed", "{refresh}");
    let gaps = mcp.tool("ripr_list_gaps", json!({}))?;
    assert_eq!(gaps["items"][0]["class"], "exposed", "{gaps}");
    assert_eq!(gaps["items"].as_array().map(Vec::len), Some(1), "{gaps}");

    // 5. Execution is a separate axis. ripr ran nothing in every variant;
    //    the static movement is `improved` in every variant, a reported
    //    failure included; and the caller's report reads back as itself.
    let verification = &receipt["receipt"]["document"]["verification"];
    assert_eq!(verification["status"], "verification_not_run", "{receipt}");
    assert_eq!(verification["commands_run"], json!([]), "{receipt}");
    let status = stdout_json(&run(&[
        "agent",
        "status",
        "--root",
        ".",
        "--attempt",
        &id,
        "--json",
    ])?)?;
    assert_eq!(
        status["attempt"]["receipt"]["movement"], "improved",
        "{status}"
    );
    assert_eq!(
        status["attempt"]["receipt"]["verification_status"], "verification_not_run",
        "{status}"
    );
    let test_run = &status["test_run"];
    let aggregate = run(&["status", "--root", "."])?;
    let aggregate = String::from_utf8_lossy(&aggregate.stdout).to_string();
    let counts = |passed: u64, failed: u64| {
        format!(
            "reported by the caller, not run by ripr: 1 test(s) ran in `{TEST_FILE}` ({passed} passed, {failed} failed)"
        )
    };
    match reported {
        ReportedRun::None => {
            assert_eq!(test_run["status"], "not_recorded", "{status}");
            assert!(test_run.get("passed").is_none(), "{status}");
            for text in [&aggregate, &finish_stdout] {
                assert!(text.contains("Test run: none recorded."), "{text}");
                assert!(!text.contains("reported by the caller"), "{text}");
            }
        }
        ReportedRun::Passed | ReportedRun::Failed => {
            let (passed, failed) = if reported == ReportedRun::Passed {
                (1, 0)
            } else {
                (0, 1)
            };
            assert_eq!(test_run["status"], "caller_reported", "{status}");
            assert_eq!(test_run["file"], TEST_FILE, "{status}");
            assert_eq!(test_run["ran"], 1, "{status}");
            assert_eq!(test_run["passed"], passed, "{status}");
            assert_eq!(test_run["failed"], failed, "{status}");
            let next_step = test_run["next_step"].as_str().unwrap_or_default();
            assert_eq!(
                next_step.contains("failing test"),
                reported == ReportedRun::Failed,
                "{status}"
            );
            for text in [&aggregate, &finish_stdout] {
                assert!(text.contains(&counts(passed, failed)), "{text}");
                assert!(!text.contains("none recorded"), "{text}");
                assert_eq!(
                    text.contains("The reported run has a failing test"),
                    reported == ReportedRun::Failed,
                    "{text}"
                );
            }
        }
    }

    // 6. Durable: a new server process, with no session state, reads the
    //    same attempt and receipt state, and every retained terminal file
    //    still matches the bytes the manifest binds.
    drop(mcp);
    let mut restarted = Mcp::spawn(&fixture.0)?;
    let listing = restarted.tool("ripr_list_repair_attempts", json!({}))?;
    let rows = listing["items"].as_array().ok_or("listing omitted items")?;
    assert_eq!(rows.len(), 1, "{listing}");
    assert_eq!(rows[0]["attempt_id"], id.as_str(), "{listing}");
    assert_eq!(rows[0]["validation"], "valid", "{listing}");
    assert_eq!(rows[0]["receipt_status"], "improved", "{listing}");
    let attempt = restarted.tool("ripr_get_repair_attempt", json!({ "attempt_id": id }))?;
    assert_eq!(attempt["origin"], "durable_store", "{attempt}");
    assert_eq!(attempt["terminal_receipt"], "issued", "{attempt}");
    let finished_manifest = manifest("after finish")?;
    let terminal = finished_manifest["terminal_artifacts"]
        .as_array()
        .ok_or("manifest omitted terminal_artifacts")?;
    let mut roles = Vec::new();
    for artifact in terminal {
        let path = artifact["path"].as_str().ok_or("terminal artifact path")?;
        let bytes = read(path)?;
        assert_eq!(artifact["bytes"], bytes.len() as u64, "{artifact}");
        assert_eq!(
            artifact["sha256"],
            format!("sha256:{:x}", Sha256::digest(&bytes)),
            "{artifact}"
        );
        roles.push(artifact["role"].as_str().unwrap_or_default().to_string());
    }
    roles.sort();
    let mut expected_roles = vec!["agent_receipt", "agent_verify"];
    if reported != ReportedRun::None {
        expected_roles.push("focused_test_result");
    }
    assert_eq!(roles, expected_roles, "{finished_manifest}");
    assert_eq!(read("src/lib.rs")?, production_before, "production changed");
    assert_eq!(
        read(TEST_FILE)?,
        edited,
        "the test file changed after the edit"
    );
    Ok(())
}

/// The fixture repository's HEAD, read from its git metadata without another
/// process: `.git/HEAD` names the branch ref the fixture committed on.
fn ripr_fixture_head(fixture: &Fixture) -> Result<String, String> {
    let git = fixture.0.join(".git");
    let head = std::fs::read_to_string(git.join("HEAD")).map_err(|e| e.to_string())?;
    let reference = head
        .trim()
        .strip_prefix("ref: ")
        .ok_or_else(|| format!("fixture HEAD is not a branch ref: {head}"))?;
    std::fs::read_to_string(git.join(reference))
        .map(|sha| sha.trim().to_string())
        .map_err(|e| format!("read {reference}: {e}"))
}
