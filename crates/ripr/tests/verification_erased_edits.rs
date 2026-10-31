//! Verifier-only edits are disclosed, never probed (RIPR-SPEC-0368, #10749).
//!
//! A contract attribute or a Creusot/Prusti ghost item is read by a verifier,
//! not a test. Editing only such text must not vanish as a silent
//! `no_behavioral_candidates`, and must never become an actionable "add a
//! co-located test" finding. Runtime code that merely carries a verifier word
//! stays ordinary.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const MANIFEST: &str =
    "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n";

/// Positional whole-file diff of `old` to `new` (equal line counts).
fn diff(old: &str, new: &str) -> String {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    assert_eq!(old.len(), new.len(), "positional edit only");
    let mut body = String::new();
    let mut at = 0;
    while at < old.len() {
        if old[at] == new[at] {
            body.push_str(&format!(" {}\n", old[at]));
            at += 1;
            continue;
        }
        let start = at;
        while at < old.len() && old[at] != new[at] {
            at += 1;
        }
        for line in &old[start..at] {
            body.push_str(&format!("-{line}\n"));
        }
        for line in &new[start..at] {
            body.push_str(&format!("+{line}\n"));
        }
    }
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{n} +1,{n} @@\n{body}",
        n = old.len()
    )
}

struct Run {
    json: Value,
}

impl Run {
    fn kind(&self) -> &str {
        self.json["analysis_outcome"]["outcome"]["kind"]
            .as_str()
            .unwrap_or_default()
    }

    fn limitation_kinds(&self) -> Vec<&str> {
        self.json["analysis_outcome"]["outcome"]["limitations"]
            .as_array()
            .map(|items| items.iter().filter_map(|l| l["kind"].as_str()).collect())
            .unwrap_or_default()
    }

    fn finding_lines(&self) -> Vec<u64> {
        self.json["findings"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|f| f["probe"]["line"].as_u64())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// `old` becomes `new` in `src/lib.rs`; `tests/it.rs` holds `test`.
fn check(old: &str, new: &str, test: &str) -> Result<Run, String> {
    let base: PathBuf = std::env::temp_dir().join(format!(
        "ripr-verification-erased-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("fx");
    let write = |path: PathBuf, text: &str| {
        fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
    };
    fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    write(root.join("Cargo.toml"), MANIFEST)?;
    write(root.join("src/lib.rs"), new)?;
    write(root.join("tests/it.rs"), test)?;
    write(base.join("change.diff"), &diff(old, new))?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--mode", "draft", "--root"])
        .arg(&root)
        .arg("--diff")
        .arg(base.join("change.diff"))
        .args(["--format", "json"])
        .env("RIPR_CACHE_DIR", base.join("cache"))
        .output()
        .map_err(|e| format!("run ripr check: {e}"))?;
    let _ = fs::remove_dir_all(&base);
    if !output.status.success() {
        return Err(format!(
            "ripr check failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let json = serde_json::from_slice(&output.stdout).map_err(|e| format!("parse json: {e}"))?;
    Ok(Run { json })
}

const TEST: &str = "use fx::f;\n#[test]\nfn pins() {\n    assert_eq!(f(1), 2);\n}\n";

#[test]
fn a_contract_only_edit_is_disclosed_not_silent() -> Result<(), String> {
    let old = "#[requires(x > 0)]\npub fn f(x: i32) -> i32 {\n    x + 1\n}\n";
    let new = "#[requires(x > 1)]\npub fn f(x: i32) -> i32 {\n    x + 1\n}\n";
    let run = check(old, new, TEST)?;
    assert_eq!(run.kind(), "no_behavioral_candidates", "{:?}", run.json);
    assert_eq!(run.limitation_kinds(), ["verification_only_change"]);
    assert!(run.finding_lines().is_empty());
    Ok(())
}

#[test]
fn every_recognised_contract_spelling_is_disclosed() -> Result<(), String> {
    for (before, after) in [
        ("#[ensures(result > 0)]", "#[ensures(result > 1)]"),
        ("#[kani::requires(x > 0)]", "#[kani::requires(x > 1)]"),
        (
            "use creusot_std::prelude::*;\n#[variant(n - i)]",
            "use creusot_std::prelude::*;\n#[variant(n - i - 1)]",
        ),
        (
            "use prusti_contracts::*;\n#[invariant(x >= 0)]",
            "use prusti_contracts::*;\n#[invariant(x >= 1)]",
        ),
        (
            "use flux_rs::attrs::*;\n#[spec(fn(i32) -> i32)]",
            "use flux_rs::attrs::*;\n#[spec(fn(i32{v: v > 0}) -> i32)]",
        ),
    ] {
        let old = format!("{before}\npub fn f(x: i32) -> i32 {{\n    x + 1\n}}\n");
        let new = format!("{after}\npub fn f(x: i32) -> i32 {{\n    x + 1\n}}\n");
        let run = check(&old, &new, TEST)?;
        assert_eq!(
            run.limitation_kinds(),
            ["verification_only_change"],
            "{before}"
        );
        assert!(run.finding_lines().is_empty(), "{before}");
    }
    Ok(())
}

#[test]
fn ghost_and_logic_only_edits_yield_no_actionable_finding() -> Result<(), String> {
    let old = "use creusot_std::prelude::*;\n#[logic]\npub fn spec(x: i32) -> i32 {\n    if x > 3 { x } else { 0 }\n}\npub fn f(x: i32) -> i32 {\n    proof_assert!(x > 0);\n    snapshot!(x);\n    x + 1\n}\n";
    let new = "use creusot_std::prelude::*;\n#[logic]\npub fn spec(x: i32) -> i32 {\n    if x > 4 { x } else { 0 }\n}\npub fn f(x: i32) -> i32 {\n    proof_assert!(x > 1);\n    snapshot!(x + 1);\n    x + 1\n}\n";
    let run = check(old, new, TEST)?;
    assert!(
        run.finding_lines().is_empty(),
        "ghost-only edits must seed no probe: {:?}",
        run.json["findings"]
    );
    assert_eq!(run.kind(), "no_behavioral_candidates", "{:?}", run.json);
    assert_eq!(run.limitation_kinds(), ["verification_only_change"]);
    Ok(())
}

#[test]
fn a_mixed_diff_keeps_the_runtime_finding_and_adds_the_disclosure() -> Result<(), String> {
    let old = "#[requires(x > 0)]\npub fn f(x: i32) -> i32 {\n    x + 1\n}\n";
    let new = "#[requires(x > 1)]\npub fn f(x: i32) -> i32 {\n    x + 2\n}\n";
    let run = check(old, new, TEST)?;
    assert!(
        run.finding_lines().contains(&3),
        "{:?}",
        run.json["findings"]
    );
    assert!(!run.finding_lines().contains(&1));
    assert!(
        run.limitation_kinds().contains(&"verification_only_change"),
        "{:?}",
        run.json
    );
    Ok(())
}

#[test]
fn runtime_items_carrying_verifier_words_stay_ordinary() -> Result<(), String> {
    for attr in [
        "#[check(ghost)]",
        "#[trusted]",
        "#[pure]",
        // A user-defined attribute that shares a verifier word, no verifier import.
        "#[logic]",
        "#[invariant(x)]",
        "#[variant(x)]",
        "#[spec]",
        // The runtime `contracts` crate expands to debug_assert!.
        "#[contracts::requires(x > 0)]",
    ] {
        let old = format!("{attr}\npub fn f(x: i32) -> i32 {{\n    x + 1\n}}\n");
        let new = format!("{attr}\npub fn f(x: i32) -> i32 {{\n    x + 2\n}}\n");
        let run = check(&old, &new, TEST)?;
        assert!(run.finding_lines().contains(&3), "{attr}: {:?}", run.json);
        assert!(
            !run.limitation_kinds().contains(&"verification_only_change"),
            "{attr}"
        );
    }
    Ok(())
}

#[test]
fn snapshot_and_ghost_names_without_creusot_stay_ordinary() -> Result<(), String> {
    let old = "pub fn f(x: i32) -> i32 {\n    snapshot!(x);\n    x + 1\n}\n";
    let new = "pub fn f(x: i32) -> i32 {\n    snapshot!(x + 1);\n    x + 1\n}\n";
    let run = check(old, new, TEST)?;
    assert!(
        !run.limitation_kinds().contains(&"verification_only_change"),
        "{:?}",
        run.json
    );
    Ok(())
}

#[test]
fn a_wholly_replaced_logic_file_is_still_disclosed() -> Result<(), String> {
    let old = "// a\n// b\n// c\n// d\n// e\n";
    let new = "use creusot_std::prelude::*;\n#[logic]\npub fn spec(x: i32) -> i32 {\n    x\n}\n";
    let run = check(old, new, TEST)?;
    assert!(run.finding_lines().is_empty(), "{:?}", run.json["findings"]);
    assert_eq!(run.limitation_kinds(), ["verification_only_change"]);
    Ok(())
}
