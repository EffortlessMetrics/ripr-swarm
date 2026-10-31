//! #10983: a no-action check report must not acquire current authority merely
//! by being composed into a ledger, including across roots or after test edits.
#![cfg(feature = "lang-python")]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const WEAK: &str = "import unittest\nfrom calculator import discount\n\nclass DiscountTests(unittest.TestCase):\n    def test_boundary(self):\n        self.assertIsInstance(discount(100), (int, float))\n";
const STRONG: &str = "import unittest\nfrom calculator import discount\n\nclass DiscountTests(unittest.TestCase):\n    def test_boundary(self):\n        self.assertIsInstance(discount(100), (int, float))\n        self.assertEqual(discount(100), 100)\n";

fn cmd(root: &Path, bin: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new(bin)
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "{bin} {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    cmd(root, "git", args)
}
fn ripr(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    cmd(root, env!("CARGO_BIN_EXE_ripr"), args)
}
fn text(p: &Path) -> Result<&str, String> {
    p.to_str().ok_or_else(|| "non-UTF8 test path".into())
}
/// A path as the product prints it in output and JSON: `/` separators on
/// Windows, the filesystem spelling elsewhere.
fn printed(p: &Path) -> Result<String, String> {
    let text = text(p)?;
    Ok(if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.to_string()
    })
}
fn check(root: &Path) -> Result<Value, String> {
    let bytes = ripr(
        root,
        &[
            "check",
            "--root",
            text(root)?,
            "--base",
            "HEAD~1",
            "--worktree",
            "--json",
        ],
    )?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn first_pr(root: &Path, input: &Path, output: &Path, ledger: &Path) -> Result<Value, String> {
    ripr(
        root,
        &[
            "first-pr",
            "--root",
            text(root)?,
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--check-output",
            text(input)?,
            "--gap-ledger",
            text(ledger)?,
            "--out-dir",
            text(output)?,
        ],
    )?;
    serde_json::from_slice(&fs::read(output.join("start-here.json")).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn fixture() -> Result<PathBuf, String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ripr-10983-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(p.join("tests")).map_err(|e| e.to_string())?;
    fs::write(p.join(".gitignore"), "/target/\n").map_err(|e| e.to_string())?;
    fs::write(p.join("calculator.py"), "def discount(total):\n    if total >= 100:\n        return total * 0.9\n    return total\n").map_err(|e| e.to_string())?;
    fs::write(p.join("tests/test_calculator.py"), WEAK).map_err(|e| e.to_string())?;
    git(&p, &["-c", "init.templateDir=", "init", "-q"])?;
    git(&p, &["config", "user.name", "synthetic"])?;
    git(&p, &["config", "user.email", "fixture@example.invalid"])?;
    git(&p, &["config", "commit.gpgSign", "false"])?;
    git(&p, &["add", "."])?;
    git(&p, &["commit", "-qm", "base"])?;
    fs::write(
        p.join("calculator.py"),
        "def discount(total):\n    if total > 100:\n        return total * 0.9\n    return total\n",
    )
    .map_err(|e| e.to_string())?;
    git(&p, &["add", "calculator.py"])?;
    git(&p, &["commit", "-qm", "change"])?;
    Ok(p)
}

fn unknown_root_cases(
    root: &Path,
    produced: &Value,
    expected_records: usize,
) -> Result<(), String> {
    for state in ["missing", "null", "relative", "unresolvable", "malformed"] {
        let mut input = produced.clone();
        match state {
            "missing" => {
                input.as_object_mut().ok_or("check object")?.remove("root");
            }
            "relative" => input["root"] = serde_json::json!("relative/path"),
            "null" => input["root"] = Value::Null,
            "unresolvable" => {
                input["root"] = serde_json::json!(text(&root.join("absent-producer-root"))?)
            }
            _ => input["root"] = serde_json::json!(["malformed"]),
        }
        let input_path = root
            .join("target")
            .join(format!("unknown-{expected_records}-{state}.json"));
        fs::create_dir_all(root.join("target")).map_err(|e| e.to_string())?;
        let original = serde_json::to_vec(&input).map_err(|e| e.to_string())?;
        fs::write(&input_path, &original).map_err(|e| e.to_string())?;
        let ledger = root
            .join("target")
            .join(format!("unknown-{expected_records}-{state}-ledger.json"));
        let packet = first_pr(
            root,
            &input_path,
            &root
                .join("target")
                .join(format!("unknown-{expected_records}-{state}")),
            &ledger,
        )?;
        let ledger: Value = serde_json::from_slice(&fs::read(ledger).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        assert_eq!(
            ledger["check_output_provenance"].get("source_root_raw"),
            input.get("root")
        );
        let wrapped_path = root
            .join("target")
            .join(format!("unknown-{expected_records}-{state}-wrapped.json"));
        let ledger_path = root
            .join("target")
            .join(format!("unknown-{expected_records}-{state}-ledger.json"));
        ripr(
            root,
            &[
                "reports",
                "gap-ledger",
                "--root",
                text(root)?,
                "--records",
                text(&ledger_path)?,
                "--out",
                text(&wrapped_path)?,
                "--out-md",
                text(
                    &root
                        .join("target")
                        .join(format!("unknown-{expected_records}-{state}-wrapped.md")),
                )?,
            ],
        )?;
        let wrapped: Value =
            serde_json::from_slice(&fs::read(wrapped_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        assert_eq!(
            wrapped["check_output_provenance"].get("source_root_raw"),
            input.get("root")
        );
        assert_eq!(
            ledger["records"].as_array().map(Vec::len),
            Some(expected_records),
            "{state}: {ledger}"
        );
        assert_eq!(
            packet["selected"]["reason_code"], "source_currentness_unavailable",
            "{state}: {packet}"
        );
        assert!(
            packet["commands"].get("regenerate_gap_ledger").is_none(),
            "{state}: {packet}"
        );
        for id in ["check_output", "gap_ledger"] {
            let artifact = packet["artifacts"]
                .as_array()
                .ok_or("artifacts")?
                .iter()
                .find(|v| v["id"] == id)
                .ok_or("artifact")?;
            assert!(
                artifact["regeneration_command"].is_null(),
                "{state}: {artifact}"
            );
        }
        #[cfg(unix)]
        {
            let command = packet["selected"]["next_command"]
                .as_str()
                .ok_or("missing inspection")?;
            assert!(
                !command.contains('>'),
                "inspection must not redirect: {command}"
            );
            let bin_dir = Path::new(env!("CARGO_BIN_EXE_ripr"))
                .parent()
                .ok_or("binary directory")?;
            let mut paths = vec![bin_dir.to_path_buf()];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            let result = Command::new("sh")
                .current_dir(root)
                .env(
                    "PATH",
                    std::env::join_paths(paths).map_err(|e| e.to_string())?,
                )
                .args(["-c", command])
                .output()
                .map_err(|e| e.to_string())?;
            assert!(
                result.status.success(),
                "{state}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        assert_eq!(
            fs::read(&input_path).map_err(|e| e.to_string())?,
            original,
            "{state}: input changed"
        );
    }
    Ok(())
}

#[test]
fn zero_record_foreign_check_preserves_the_input() -> Result<(), String> {
    let a = fixture()?;
    fs::create_dir_all(a.join("src")).map_err(|e| e.to_string())?;
    fs::write(
        a.join("Cargo.toml"),
        "[package]\nname = \"case\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        a.join("src/lib.rs"),
        "pub fn boundary(x: i32) -> bool { x >= 10 }\n",
    )
    .map_err(|e| e.to_string())?;
    git(&a, &["add", "."])?;
    git(&a, &["commit", "-qm", "rust base"])?;
    fs::write(
        a.join("src/lib.rs"),
        "pub fn boundary(x: i32) -> bool { x > 10 }\n",
    )
    .map_err(|e| e.to_string())?;
    git(&a, &["add", "src/lib.rs"])?;
    git(&a, &["commit", "-qm", "rust change"])?;
    let b = a.with_extension("zero-other");
    git(
        a.parent().ok_or("missing parent")?,
        &["clone", "-q", "--no-hardlinks", text(&a)?, text(&b)?],
    )?;
    let produced = check(&a)?;
    assert!(
        produced["summary"]["findings"].as_u64().unwrap_or(0) > 0,
        "real Rust findings required: {produced}"
    );
    let input = a.with_extension("zero-check.json");
    let bytes = serde_json::to_vec(&produced).map_err(|e| e.to_string())?;
    fs::write(&input, &bytes).map_err(|e| e.to_string())?;
    let same = first_pr(
        &a,
        &input,
        &a.join("target/zero-same"),
        &a.join("target/zero-same-ledger.json"),
    )?;
    let same_ledger: Value = serde_json::from_slice(
        &fs::read(a.join("target/zero-same-ledger.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(same_ledger["status"], "no_records", "{same_ledger}");
    assert_eq!(same_ledger["records"].as_array().map(Vec::len), Some(0));
    assert_eq!(same["selected"]["state"], "blocked_artifact", "{same}");
    let same_check = same["artifacts"]
        .as_array()
        .ok_or("artifacts")?
        .iter()
        .find(|v| v["id"] == "check_output")
        .ok_or("check artifact")?;
    assert!(
        same_check["regeneration_command"].as_str().is_some(),
        "{same_check}"
    );
    assert!(
        same["selected"]["next_command"]
            .as_str()
            .is_some_and(|s| s.contains("repo-exposure")),
        "{same}"
    );
    assert_eq!(fs::read(&input).map_err(|e| e.to_string())?, bytes);
    unknown_root_cases(&a, &produced, 0)?;
    let other = first_pr(
        &b,
        &input,
        &b.join("target/zero-other"),
        &b.join("target/zero-other-ledger.json"),
    )?;
    let other_ledger: Value = serde_json::from_slice(
        &fs::read(b.join("target/zero-other-ledger.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(other_ledger["status"], "no_records", "{other_ledger}");
    assert_eq!(other_ledger["records"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        other["selected"]["reason_code"], "check_output_wrong_root",
        "{other}"
    );
    assert_eq!(other["selected"]["output_state"], "wrong_root", "{other}");
    assert_eq!(other_ledger["input_root"], printed(&a)?);
    assert_eq!(other_ledger["root"], printed(&b)?);
    assert!(
        other["commands"].get("regenerate_gap_ledger").is_none(),
        "{other}"
    );
    for id in ["check_output", "gap_ledger"] {
        let artifact = other["artifacts"]
            .as_array()
            .ok_or("artifacts")?
            .iter()
            .find(|v| v["id"] == id)
            .ok_or("artifact")?;
        assert!(artifact["regeneration_command"].is_null(), "{artifact}");
    }
    #[cfg(unix)]
    {
        let command = other["selected"]["next_command"]
            .as_str()
            .ok_or("missing inspection")?;
        let bin_dir = Path::new(env!("CARGO_BIN_EXE_ripr"))
            .parent()
            .ok_or("binary directory")?;
        let mut paths = vec![bin_dir.to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let result = Command::new("sh")
            .current_dir(&b)
            .env(
                "PATH",
                std::env::join_paths(paths).map_err(|e| e.to_string())?,
            )
            .args(["-c", command])
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            result.status.success(),
            "inspection failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::read(&input).map_err(|e| e.to_string())?,
            bytes,
            "foreign input changed"
        );
    }
    fs::remove_dir_all(&a).map_err(|e| e.to_string())?;
    fs::remove_dir_all(&b).map_err(|e| e.to_string())?;
    fs::remove_file(input).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn new_import_config_or_extension_changes_strong_producer_result() -> Result<(), String> {
    let root = fixture()?;
    fs::create_dir_all(root.join("reach")).map_err(|e| e.to_string())?;
    let source = root.join("reach/ledger.py");
    let test = root.join("tests/test_ledger.py");
    let before =
        "def _pad():\n    return 2\n\n\ndef render():\n    return \"|\" + \" \" * _pad() + \"|\"\n";
    let after = before.replace("return 2", "return 3");
    let witness = "from reach.ledger import render\n\n\ndef test_render():\n    assert render() == \"|  |\"\n";
    fs::write(root.join("reach/__init__.py"), "").map_err(|e| e.to_string())?;
    fs::write(&source, before).map_err(|e| e.to_string())?;
    fs::write(&test, witness).map_err(|e| e.to_string())?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "import base"])?;
    fs::write(&source, &after).map_err(|e| e.to_string())?;
    git(&root, &["add", "reach/ledger.py"])?;
    git(&root, &["commit", "-qm", "import change"])?;
    let head = git(&root, &["rev-parse", "HEAD"])?;
    let strong = check(&root)?;
    assert_eq!(strong["summary"]["findings"], 1, "{strong}");
    assert_eq!(strong["summary"]["exposed"], 1, "{strong}");
    let input = root.with_extension("import-strong.json");
    fs::write(
        &input,
        serde_json::to_vec(&strong).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for (name, bytes) in [
        ("pytest.ini", "[pytest]\npythonpath = outside\n"),
        ("reach/ledger.so", ""),
    ] {
        fs::write(root.join(name), bytes).map_err(|e| e.to_string())?;
        let fresh = check(&root)?;
        assert_eq!(fresh["summary"]["weakly_exposed"], 1, "{name}: {fresh}");
        assert_eq!(fresh["summary"]["findings"], 1, "{name}: {fresh}");
        assert_eq!(git(&root, &["rev-parse", "HEAD"])?, head);
        assert_eq!(
            fs::read_to_string(&source).map_err(|e| e.to_string())?,
            after
        );
        assert_eq!(
            fs::read_to_string(&test).map_err(|e| e.to_string())?,
            witness
        );
        let packet = first_pr(
            &root,
            &input,
            &root.join("target").join(name.replace('/', "-")),
            &root
                .join("target")
                .join(format!("{}.json", name.replace('/', "-"))),
        )?;
        assert_ne!(
            packet["selected"]["output_state"], "no_actionable_gap",
            "{name}: {packet}"
        );
        fs::remove_file(root.join(name)).map_err(|e| e.to_string())?;
        assert_eq!(check(&root)?["summary"]["exposed"], 1, "restored {name}");
    }
    fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    fs::remove_file(input).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn newly_discovered_foreign_write_changes_strong_producer_result() -> Result<(), String> {
    let root = fixture()?;
    fs::create_dir_all(root.join("hostile")).map_err(|e| e.to_string())?;
    let source = root.join("hostile/settings.py");
    let test = root.join("tests/test_settings.py");
    let before = "RATES = {\"gold\": 10, \"silver\": 5}\n\n\ndef price(tier):\n    return RATES.get(tier, 0)\n";
    let after = before.replace("\"silver\": 5", "\"silver\": 6");
    let witness =
        "from hostile.settings import price\n\ndef test_it():\n    assert price(\"silver\") == 5\n";
    fs::write(&source, before).map_err(|e| e.to_string())?;
    fs::write(&test, witness).map_err(|e| e.to_string())?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "table base"])?;
    fs::write(&source, &after).map_err(|e| e.to_string())?;
    git(&root, &["add", "hostile/settings.py"])?;
    git(&root, &["commit", "-qm", "table change"])?;
    let head = git(&root, &["rev-parse", "HEAD"])?;
    let strong = check(&root)?;
    assert_eq!(strong["summary"]["findings"], 1, "{strong}");
    assert_eq!(strong["summary"]["exposed"], 1, "{strong}");
    for state in ["missing", "null", "malformed"] {
        let mut altered = strong.clone();
        if state == "missing" {
            altered
                .as_object_mut()
                .ok_or("check object")?
                .remove("source_subject");
        } else if state == "null" {
            altered["source_subject"] = Value::Null;
        } else {
            altered["source_subject"] = serde_json::json!({"corpus": ["not an authority"]});
            altered["head"] = serde_json::json!(["malformed"]);
            altered["base"] = Value::Null;
        }
        let altered_path = root.join("target").join(format!("{state}-input.json"));
        fs::create_dir_all(root.join("target")).map_err(|e| e.to_string())?;
        fs::write(
            &altered_path,
            serde_json::to_vec(&altered).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let ledger_path = root.join("target").join(format!("{state}-ledger.json"));
        let packet = first_pr(
            &root,
            &altered_path,
            &root.join("target").join(state),
            &ledger_path,
        )?;
        assert_ne!(
            packet["selected"]["output_state"], "no_actionable_gap",
            "{packet}"
        );
        let ledger: Value =
            serde_json::from_slice(&fs::read(ledger_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        assert_eq!(
            ledger["check_output_provenance"].get("source_subject"),
            altered.get("source_subject")
        );
        for key in ["base", "head"] {
            assert_eq!(ledger["check_output_provenance"].get(key), altered.get(key));
        }
        let wrapped = root.join("target").join(format!("{state}-wrapped.json"));
        ripr(
            &root,
            &[
                "reports",
                "gap-ledger",
                "--root",
                text(&root)?,
                "--records",
                text(&root.join("target").join(format!("{state}-ledger.json")))?,
                "--out",
                text(&wrapped)?,
                "--out-md",
                text(&root.join("target").join(format!("{state}-wrapped.md")))?,
            ],
        )?;
        let wrapped: Value = serde_json::from_slice(&fs::read(wrapped).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        assert_eq!(
            wrapped["check_output_provenance"],
            ledger["check_output_provenance"]
        );
    }
    let input = root.with_extension("foreign-write-strong.json");
    fs::write(
        &input,
        serde_json::to_vec(&strong).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        root.join("hostile/other.py"),
        "from hostile.settings import RATES\n\nRATES[\"silver\"] = 5\n",
    )
    .map_err(|e| e.to_string())?;
    let changed = check(&root)?;
    assert_eq!(changed["summary"]["findings"], 1, "{changed}");
    assert_eq!(changed["summary"]["static_unknown"], 1, "{changed}");
    assert_eq!(git(&root, &["rev-parse", "HEAD"])?, head);
    assert_eq!(
        fs::read_to_string(&source).map_err(|e| e.to_string())?,
        after
    );
    assert_eq!(
        fs::read_to_string(&test).map_err(|e| e.to_string())?,
        witness
    );
    let packet = first_pr(
        &root,
        &input,
        &root.join("target/foreign-write"),
        &root.join("target/foreign-write-ledger.json"),
    )?;
    assert_ne!(
        packet["selected"]["output_state"], "no_actionable_gap",
        "{packet}"
    );
    fs::remove_file(root.join("hostile/other.py")).map_err(|e| e.to_string())?;
    let restored = check(&root)?;
    assert_eq!(restored["summary"]["exposed"], 1, "{restored}");
    fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    fs::remove_file(input).map_err(|e| e.to_string())?;
    Ok(())
}

/// Pins the honest fail-closed state until a producer-owned corpus authority
/// lands (#7452). A fresh, current strong check is useful evidence but cannot
/// yet support a current no-action conclusion: discovery membership and the
/// producer's dependency/config decisions have no verifiable authority. When
/// #7452 delivers that authority, flip these assertions to expect
/// `no_actionable_gap` for this same fixture.
#[test]
fn current_strong_check_stays_limited_until_corpus_authority() -> Result<(), String> {
    let root = fixture()?;
    fs::write(root.join("tests/test_calculator.py"), STRONG).map_err(|e| e.to_string())?;
    let produced = check(&root)?;
    assert_eq!(produced["summary"]["exposed"], 1, "{produced}");
    let input = root.with_extension("current-strong.json");
    fs::write(
        &input,
        serde_json::to_vec(&produced).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let packet = first_pr(
        &root,
        &input,
        &root.join("target/current-strong"),
        &root.join("target/current-strong-ledger.json"),
    )?;
    assert_eq!(packet["status"], "blocked", "{packet}");
    assert_ne!(
        packet["selected"]["output_state"], "no_actionable_gap",
        "{packet}"
    );
    assert_eq!(
        packet["selected"]["reason_code"], "source_currentness_unavailable",
        "{packet}"
    );
    fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    fs::remove_file(input).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn no_action_does_not_promote_unbound_or_stale_preview_check() -> Result<(), String> {
    let a = fixture()?;
    let b = a.with_extension("other");
    let parent = a.parent().ok_or("missing parent")?;
    git(
        parent,
        &["clone", "-q", "--no-hardlinks", text(&a)?, text(&b)?],
    )?;
    fs::write(a.join("tests/test_calculator.py"), STRONG).map_err(|e| e.to_string())?;
    let strong = check(&a)?;
    assert_eq!(
        strong["summary"]["exposed"], 1,
        "nonempty strong control: {strong}"
    );
    assert_eq!(strong["root"], printed(&a)?);
    unknown_root_cases(&a, &strong, 1)?;
    let input = a.with_extension("strong.json");
    fs::write(
        &input,
        serde_json::to_vec(&strong).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Existing preview corpus authority is unavailable: a current strong
    // artifact is useful evidence but is not a current no-action conclusion.
    let same = first_pr(
        &a,
        &input,
        &a.join("target/same"),
        &a.join("target/same-ledger.json"),
    )?;
    assert_eq!(
        same["status"], "blocked",
        "strong preview subject must be explicitly limited: {same}"
    );
    assert_ne!(same["selected"]["output_state"], "no_actionable_gap");
    assert_eq!(
        same["selected"]["reason_code"],
        "source_currentness_unavailable"
    );
    let message = same["selected"]["message"]
        .as_str()
        .ok_or("missing limitation")?;
    assert!(message.contains("limitation remains"), "{message}");
    let markdown =
        fs::read_to_string(a.join("target/same/start-here.md")).map_err(|e| e.to_string())?;
    assert!(markdown.contains("do not infer no action"), "{markdown}");
    // A pre-fix no-action packet cannot become valid merely by asking the
    // validate-only route to check its shape and version.
    let mut historical = same.clone();
    historical["status"] = serde_json::json!("no_action");
    historical["selected"] = serde_json::json!({"state": "no_action", "output_state": "no_actionable_gap", "reason": "historical", "records_total": 1});
    let packet_path = a.join("target/same/start-here.json");
    fs::write(
        &packet_path,
        serde_json::to_vec_pretty(&historical).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let checked = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(&a)
        .args([
            "first-pr",
            "--root",
            text(&a)?,
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--check-output",
            text(&input)?,
            "--gap-ledger",
            text(&a.join("target/same-ledger.json"))?,
            "--out-dir",
            text(&a.join("target/same"))?,
            "--check",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    assert_eq!(
        checked.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8_lossy(&checked.stderr).contains("source_currentness_unavailable"));
    let direct_ledger = a.join("target/same-ledger.json");
    let retained_ledger = fs::read(&direct_ledger).map_err(|e| e.to_string())?;
    historical["inputs"]
        .as_object_mut()
        .ok_or("packet inputs")?
        .remove("check_output");
    fs::write(
        &packet_path,
        serde_json::to_vec_pretty(&historical).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for kind in ["missing", "malformed", "unclassifiable"] {
        match kind {
            "missing" => fs::remove_file(&direct_ledger).map_err(|e| e.to_string())?,
            "malformed" => fs::write(&direct_ledger, "{").map_err(|e| e.to_string())?,
            _ => fs::write(&direct_ledger, "{}").map_err(|e| e.to_string())?,
        }
        if kind == "unclassifiable" {
            historical["inputs"]["gap_ledger_sha256"] =
                serde_json::json!(format!("sha256:{:x}", Sha256::digest(b"{}")));
            fs::write(
                &packet_path,
                serde_json::to_vec_pretty(&historical).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        let checked = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&a)
            .args([
                "first-pr",
                "--root",
                text(&a)?,
                "--base",
                "HEAD~1",
                "--head",
                "HEAD",
                "--out-dir",
                text(&a.join("target/same"))?,
                "--check",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(checked.status.code(), Some(2));
        let expected = if kind == "unclassifiable" {
            "source kind is unavailable"
        } else {
            "source ledger is missing or malformed"
        };
        assert!(
            String::from_utf8_lossy(&checked.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
    fs::write(&direct_ledger, retained_ledger).map_err(|e| e.to_string())?;
    // A ledger-to-ledger projection must carry the original check producer's
    // identity and its limitation; otherwise changing source_kind to records
    // would bypass no-action admission.
    let rewrapped_ledger = a.join("target/rewrapped-ledger.json");
    let rewrapped_md = a.join("target/rewrapped-ledger.md");
    ripr(
        &a,
        &[
            "reports",
            "gap-ledger",
            "--root",
            text(&a)?,
            "--records",
            text(&direct_ledger)?,
            "--out",
            text(&rewrapped_ledger)?,
            "--out-md",
            text(&rewrapped_md)?,
        ],
    )?;
    let direct: Value =
        serde_json::from_slice(&fs::read(&direct_ledger).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let rewrapped: Value =
        serde_json::from_slice(&fs::read(&rewrapped_ledger).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    for key in ["source_subject", "base", "base_commit", "head", "mode"] {
        assert_eq!(
            direct["check_output_provenance"][key], strong[key],
            "producer {key} must survive projection: {direct}"
        );
    }
    assert_eq!(rewrapped["inputs"]["source_kind"], "records");
    assert_eq!(
        rewrapped["check_output_provenance"],
        direct["check_output_provenance"]
    );
    assert_eq!(rewrapped["input_root"], printed(&a)?);
    let rendered_ledger = fs::read_to_string(&rewrapped_md).map_err(|e| e.to_string())?;
    assert!(
        rendered_ledger.contains("Check-output producer root"),
        "{rendered_ledger}"
    );
    assert!(rendered_ledger.contains(&printed(&a)?), "{rendered_ledger}");
    ripr(
        &a,
        &[
            "first-pr",
            "--root",
            text(&a)?,
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--gap-ledger",
            text(&rewrapped_ledger)?,
            "--out-dir",
            text(&a.join("target/rewrapped"))?,
        ],
    )?;
    let projected: Value = serde_json::from_slice(
        &fs::read(a.join("target/rewrapped/start-here.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(
        projected["selected"]["reason_code"], "source_currentness_unavailable",
        "{projected}"
    );
    let mut legacy = rewrapped.clone();
    legacy
        .as_object_mut()
        .ok_or("ledger object")?
        .remove("check_output_provenance");
    legacy
        .as_object_mut()
        .ok_or("ledger object")?
        .remove("input_root");
    let legacy_path = a.join("target/legacy-rewrapped.json");
    fs::write(
        &legacy_path,
        serde_json::to_vec(&legacy).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    ripr(
        &a,
        &[
            "first-pr",
            "--root",
            text(&a)?,
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--gap-ledger",
            text(&legacy_path)?,
            "--out-dir",
            text(&a.join("target/legacy"))?,
        ],
    )?;
    let legacy_packet: Value = serde_json::from_slice(
        &fs::read(a.join("target/legacy/start-here.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(
        legacy_packet["selected"]["reason_code"], "source_currentness_unavailable",
        "{legacy_packet}"
    );
    let other_check = check(&b)?;
    assert_eq!(
        other_check["summary"]["weakly_exposed"], 1,
        "different-root weak control: {other_check}"
    );
    let other = first_pr(
        &b,
        &input,
        &b.join("target/other"),
        &b.join("target/other-ledger.json"),
    )?;
    assert_eq!(other["selected"]["output_state"], "wrong_root", "{other}");
    assert!(
        other["commands"].get("regenerate_gap_ledger").is_none(),
        "{other}"
    );
    for id in ["check_output", "gap_ledger"] {
        let artifact = other["artifacts"]
            .as_array()
            .ok_or("artifacts")?
            .iter()
            .find(|entry| entry["id"] == id)
            .ok_or("artifact")?;
        assert!(artifact["regeneration_command"].is_null(), "{artifact}");
    }
    let ledger: Value = serde_json::from_slice(
        &fs::read(b.join("target/other-ledger.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(
        ledger["input_root"],
        text(&a)?,
        "producer root must survive derivation: {ledger}"
    );
    assert_eq!(ledger["root"], printed(&b)?);
    #[cfg(unix)]
    {
        // Execute the literal wrong-root inspection; it must use B's weak
        // worktree and leave A's supplied artifact untouched.
        let before = fs::read(&input).map_err(|e| e.to_string())?;
        let command = other["selected"]["next_command"]
            .as_str()
            .ok_or("missing recovery")?;
        let bin_dir = Path::new(env!("CARGO_BIN_EXE_ripr"))
            .parent()
            .ok_or("missing binary directory")?;
        let mut paths = vec![bin_dir.to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(paths).map_err(|e| e.to_string())?;
        let result = Command::new("sh")
            .current_dir(&b)
            .env("PATH", path)
            .args(["-c", command])
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            result.status.success(),
            "recovery failed: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::read(&input).map_err(|e| e.to_string())?,
            before,
            "foreign input changed"
        );
        assert!(
            String::from_utf8_lossy(&result.stdout).contains("1 weakly_exposed"),
            "selected-root inspection: {}",
            String::from_utf8_lossy(&result.stdout)
        );
    }
    fs::write(a.join("tests/test_calculator.py"), WEAK).map_err(|e| e.to_string())?;
    let weak = check(&a)?;
    assert_eq!(
        weak["summary"]["weakly_exposed"], 1,
        "same-root stale control: {weak}"
    );
    let stale = first_pr(
        &a,
        &input,
        &a.join("target/stale"),
        &a.join("target/stale-ledger.json"),
    )?;
    assert_eq!(
        stale["status"], "blocked",
        "old strong input cannot yield no action: {stale}"
    );
    assert_ne!(stale["selected"]["output_state"], "no_actionable_gap");
    let refreshed = a.with_extension("weak.json");
    fs::write(
        &refreshed,
        serde_json::to_vec(&weak).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let recovered = first_pr(
        &a,
        &refreshed,
        &a.join("target/recovered"),
        &a.join("target/recovered-ledger.json"),
    )?;
    assert_eq!(
        recovered["status"], "actionable",
        "refresh should expose the actual gap: {recovered}"
    );
    fs::remove_dir_all(&a).map_err(|e| e.to_string())?;
    fs::remove_dir_all(&b).map_err(|e| e.to_string())?;
    fs::remove_file(input).map_err(|e| e.to_string())?;
    fs::remove_file(refreshed).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn legacy_unknown_empty_diff_is_limited_but_explicit_repo_exposure_is_no_action()
-> Result<(), String> {
    let root = fixture()?;
    // Exact pre-fix empty-diff fixture: it names no producing source kind.
    let legacy = "{\n  \"schema_version\": \"0.1\",\n  \"tool\": \"ripr\",\n  \"kind\": \"gap_decision_ledger\",\n  \"status\": \"empty_diff\",\n  \"summary\": {\n    \"records_total\": 0\n  },\n  \"records\": []\n}\n";
    let legacy_path = root.join("target/legacy-empty.json");
    fs::create_dir_all(root.join("target")).map_err(|e| e.to_string())?;
    fs::write(&legacy_path, legacy).map_err(|e| e.to_string())?;
    let run = |input: &Path, name: &str, base: &str| -> Result<Value, String> {
        let output = root.join("target").join(name);
        ripr(
            &root,
            &[
                "first-pr",
                "--root",
                text(&root)?,
                "--base",
                base,
                "--head",
                "HEAD",
                "--gap-ledger",
                text(input)?,
                "--out-dir",
                text(&output)?,
            ],
        )?;
        serde_json::from_slice(
            &fs::read(output.join("start-here.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    };
    let limited = run(&legacy_path, "legacy-empty", "HEAD~1")?;
    assert_eq!(limited["status"], "blocked", "{limited}");
    assert_eq!(
        limited["selected"]["reason_code"], "source_currentness_unavailable",
        "{limited}"
    );
    let explicit = root.join("target/explicit-empty.json");
    fs::write(&explicit, include_str!("../../../fixtures/first_successful_pr/empty-diff/inputs/reports/gap-decision-ledger.json")).map_err(|e| e.to_string())?;
    assert!(
        String::from_utf8_lossy(&git(&root, &["diff", "--name-only", "HEAD~1", "HEAD"])?)
            .contains("calculator.py"),
        "fixture must contain a real HEAD~1..HEAD change so the HEAD..HEAD empty diff is meaningful"
    );
    let allowed = run(&explicit, "explicit-empty", "HEAD")?;
    assert_eq!(allowed["status"], "no_action", "{allowed}");
    assert_eq!(allowed["selected"]["output_state"], "clean", "{allowed}");
    assert!(
        allowed["inputs"]["gap_ledger_sha256"]
            .as_str()
            .is_some_and(|s| s.starts_with("sha256:"))
    );
    fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(())
}
