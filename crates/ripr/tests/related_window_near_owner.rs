//! #15026: a single-file crate relates every inline test to a changed owner
//! by file location. Within that tier, tests that call the owner's wrapper
//! must fill the eight-row related-test window ahead of tests that only share
//! the file and sort earlier by name.

use ripr::app::check_workspace_with_config;
use ripr::config::RiprConfig;
use ripr::domain::LanguageId;
use ripr::{CheckInput, Finding, Mode, OutputFormat};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-related-window-near-owner-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let target = self.0.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, content).map_err(|error| error.to_string())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname = \"sim\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n";

/// The owner has two reached callers (`score` and `score_twice`), so the
/// bounded helper chain declines and the wrapper tests stay `same_test_file`
/// rows, like strsim's `jaro_*` tests over `generic_jaro`.
fn source(owner_op: &str) -> String {
    let mut text = format!(
        "pub fn generic_score(a: &[u8], b: &[u8]) -> usize {{\n    let mut n = 0;\n    for (x, y) in a.iter().zip(b) {{\n        if x {owner_op} y {{\n            n += 1;\n        }}\n    }}\n    n\n}}\n\npub fn score(a: &str, b: &str) -> usize {{\n    generic_score(a.as_bytes(), b.as_bytes())\n}}\n\npub fn score_twice(a: &str) -> usize {{\n    2 * generic_score(a.as_bytes(), a.as_bytes())\n}}\n\npub fn bump(value: usize) -> usize {{\n    value + 1\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn generic_score_direct() {{\n        assert_eq!(generic_score(b\"ab\", b\"ab\"), 2);\n    }}\n"
    );
    for index in 0..8 {
        text.push_str(&format!(
            "\n    #[test]\n    fn bump_case_{index}() {{\n        assert_eq!(bump({index}), {});\n    }}\n",
            index + 1
        ));
    }
    for index in 0..4 {
        text.push_str(&format!(
            "\n    #[test]\n    fn score_case_{index}() {{\n        assert_eq!(score(\"a\", \"a\"), 1);\n    }}\n"
        ));
    }
    text.push_str("\n    #[test]\n    fn score_twice_case() {\n        assert_eq!(score_twice(\"a\"), 2);\n    }\n}\n");
    text
}

fn findings() -> Result<Vec<Finding>, String> {
    let scratch = Scratch::create()?;
    scratch.write("Cargo.toml", MANIFEST)?;
    let after = source("<");
    scratch.write("src/lib.rs", &after)?;
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,1 +4,1 @@\n-        if x <= y {\n+        if x < y {\n";
    scratch.write("change.diff", diff)?;
    let output = analyze_change(&scratch, LanguageId::Rust)?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 4)
        .collect())
}

fn analyze_change(scratch: &Scratch, language: LanguageId) -> Result<ripr::CheckOutput, String> {
    let mut config = RiprConfig::default();
    config.languages.enabled = vec![language];
    check_workspace_with_config(
        CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        },
        &config,
    )
}

#[test]
fn wrapper_tests_fill_the_window_before_file_only_neighbours() -> Result<(), String> {
    let found = findings()?;
    let finding = found
        .first()
        .ok_or_else(|| "setup: no finding on the changed predicate".to_string())?;
    let names: Vec<&str> = finding
        .related_tests
        .iter()
        .map(|test| test.name.as_str())
        .collect();
    assert!(
        finding.related_tests_total() > names.len(),
        "setup: the window is capped: {names:?}"
    );
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.name.starts_with("bump_case_")),
        "setup: file-only rows still fill free slots: {names:?}"
    );
    let first_bump = names
        .iter()
        .position(|name| name.starts_with("bump_case_"))
        .unwrap_or(names.len());
    let wrapper_rows: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.starts_with("score_"))
        .map(|(position, _)| position)
        .collect();
    assert_eq!(
        wrapper_rows.len(),
        5,
        "every wrapper test is in the window: {names:?}"
    );
    assert!(
        wrapper_rows.iter().all(|position| *position < first_bump),
        "wrapper tests lead the file-only rows: {names:?}"
    );
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn javascript_boundary_headline_follows_the_asserted_input() -> Result<(), String> {
    let scratch = Scratch::create()?;
    scratch.write("package.json", r#"{"type":"module"}"#)?;
    scratch.write(
        "index.js",
        "export function admitted(n) {\n  return n > 10;\n}\n",
    )?;
    scratch.write("change.diff", "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -2 +2 @@\n-  return n >= 10;\n+  return n > 10;\n")?;
    // Exchange only the inputs: the credited test must change even though the
    // names, locations, call paths and oracle strengths are identical.
    for (first, second, expected) in [(20, 10, "second"), (10, 20, "first")] {
        scratch.write(
            "index.test.js",
            &format!(
                r#"import {{test}} from 'node:test';
import assert from 'node:assert/strict';
import {{admitted}} from './index.js';
test('first', () => {{
  assert.equal(admitted({first}), true);
}});
test('second', () => {{
  assert.equal(admitted({second}), true);
}});
"#
            ),
        )?;
        let output = analyze_change(&scratch, LanguageId::TypeScript)?;
        let [finding] = output.findings.as_slice() else {
            return Err(format!(
                "expected one changed predicate, got {}",
                output.findings.len()
            ));
        };
        assert_eq!(finding.related_tests.len(), 2);
        assert_eq!(
            finding.headline_related_test().map(|row| row.name.as_str()),
            Some(expected)
        );
    }
    Ok(())
}

/// The first row calls itertools.count; only the second calls the changed owner.
#[cfg(feature = "lang-python")]
#[test]
fn python_headline_uses_the_credited_count_call() -> Result<(), String> {
    let scratch = Scratch::create()?;
    scratch.write(
        "seq.py",
        "def count(seq):\n    return sum(2 for _ in seq)\n",
    )?;
    scratch.write("change.diff", "diff --git a/seq.py b/seq.py\n--- a/seq.py\n+++ b/seq.py\n@@ -2 +2 @@\n-    return sum(1 for _ in seq)\n+    return sum(2 for _ in seq)\ndiff --git a/sandbox/tests/test_core.py b/sandbox/tests/test_core.py\n--- a/sandbox/tests/test_core.py\n+++ b/sandbox/tests/test_core.py\n@@ -4 +4 @@\n-    assert next(count(1)) == 1\n+    assert next(count()) == 0\n")?;
    scratch.write(
        "sandbox/tests/test_core.py",
        "from itertools import count\n\ndef test_unzip():\n    assert next(count()) == 0\n",
    )?;
    scratch.write(
        "tests/test_seq.py",
        "from seq import count\n\ndef test_count():\n    assert count(iter([1, 2, 3, 4])) == 4\n",
    )?;
    let output = analyze_change(&scratch, LanguageId::Python)?;
    let [finding] = output.findings.as_slice() else {
        return Err(format!(
            "expected one changed generator increment, got {}",
            output.findings.len()
        ));
    };
    let names: Vec<_> = finding
        .related_tests
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, ["test_unzip", "test_count"]);
    assert_eq!(
        finding.headline_related_test().map(|row| row.name.as_str()),
        Some("test_count")
    );
    Ok(())
}

/// Pin the public one-row projection to independent source facts. Both
/// Python constants bind the changed boundary only in test_at. The two
/// heuristic fixtures never call the changed owner.
#[cfg(all(feature = "lang-python", feature = "lang-typescript"))]
#[test]
fn public_packet_pairs_boundaries_and_discloses_heuristic_rows() -> Result<(), String> {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (fixture, observer) in [
        (
            "python_module_constant_boundary_rival_control",
            Some("test_at"),
        ),
        (
            "python_module_constant_boundary_test_local_control",
            Some("test_at"),
        ),
        ("python_related_test_name_similarity", None),
        ("javascript_js_preview", None),
    ] {
        let case = fixtures.join(fixture);
        let run = |command: &str, args: &[&str]| -> Result<String, String> {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
                .arg(command)
                .arg("--root")
                .arg(case.join("input"))
                .arg("--diff")
                .arg(case.join("diff.patch"))
                .args(args)
                .output()
                .map_err(|e| e.to_string())?;
            String::from_utf8(output.stdout).map_err(|e| e.to_string())
        };
        let report: serde_json::Value = serde_json::from_str(&run("check", &["--format", "json"])?)
            .map_err(|e| e.to_string())?;
        let finding = &report["findings"][0];
        let id = finding["id"].as_str().ok_or("missing fixture finding")?;
        let packet: serde_json::Value = serde_json::from_str(&run(
            "context",
            &["--at", id, "--max-related-tests", "1", "--json"],
        )?)
        .map_err(|e| e.to_string())?;
        let human = run("check", &["--format", "human"])?;
        let agent = run("explain", &["--format", "agent", id])?;
        let rows = packet["related_tests"]
            .as_array()
            .ok_or("missing packet rows")?;
        assert_eq!(rows.len(), 1, "{fixture}");
        if let Some(name) = observer {
            assert_eq!(rows[0]["name"], name, "{fixture}: {packet}");
            assert!(
                human
                    .lines()
                    .any(|l| l.trim_start().starts_with("Related test") && l.contains(name)),
                "{fixture}: {human}"
            );
            assert!(
                agent
                    .lines()
                    .any(|l| l.starts_with("t1=") && l.contains(name)),
                "{fixture}: {agent}"
            );
            assert_eq!(finding["oracle_strength"], "strong");
        } else {
            let message = "only name or file matches were retained";
            assert!(human.contains(message), "{fixture}: {human}");
            assert!(
                packet["missing"].as_array().is_some_and(|xs| xs
                    .iter()
                    .any(|s| s.as_str().is_some_and(|x| x.contains(message)))),
                "{fixture}: {packet}"
            );
            assert!(
                !agent.lines().any(|l| l.starts_with("t1=")),
                "{fixture}: {agent}"
            );
            assert_eq!(finding["oracle_strength"], "none");
        }
    }
    Ok(())
}

/// Calling the threshold and asserting another input in the same test does
/// not observe the change. The raw row still retains its exact-value grade.
#[cfg(feature = "lang-python")]
#[test]
fn python_headline_pairs_the_assertion_with_its_boundary_call() -> Result<(), String> {
    let scratch = Scratch::create()?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/python_module_constant_boundary_rival_control");
    for (target, source) in [
        ("change.diff", "diff.patch"),
        ("svc/a/src/pricing.py", "input/svc/a/src/pricing.py"),
    ] {
        scratch.write(
            target,
            &std::fs::read_to_string(fixture.join(source)).map_err(|e| e.to_string())?,
        )?;
    }
    scratch.write("svc/b/tests/test_pricing.py",
        "from svc.a.src.pricing import discounted_total\n\ndef test_above():\n    assert discounted_total(20000) == 18000\n\ndef test_split():\n    discounted_total(10000)\n    assert discounted_total(20000) == 18000\n")?;
    let output = analyze_change(&scratch, LanguageId::Python)?;
    let [finding] = output.findings.as_slice() else {
        return Err("expected one boundary finding".to_string());
    };
    assert_eq!(finding.related_tests.len(), 2);
    assert!(
        finding
            .related_tests
            .iter()
            .all(|row| row.oracle_strength == ripr::domain::OracleStrength::Strong)
    );
    assert!(finding.headline_related_test().is_none());
    assert!(
        finding
            .evidence
            .iter()
            .any(|s| s.starts_with("related_test_observer_missing: "))
    );
    Ok(())
}

/// These preview audit cases retain their independently graded advisory card
/// even though the producer discloses only a heuristic test link.
#[cfg(feature = "lang-typescript")]
#[test]
fn heuristic_preview_preserves_its_raw_test_and_repair_card_grade() -> Result<(), String> {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (fixture, probe_id, name, kind, strength) in [
        (
            "typescript_awaited_promise_reject",
            "probe:src_profile.ts:typescript_preview:9da98202",
            "rejects missing profile id broadly",
            "broad_error",
            "weak",
        ),
        (
            "typescript_probe_facts",
            "probe:src_notify.js:typescript_preview:2f0e6fff",
            "notify smoke",
            "smoke_only",
            "smoke",
        ),
        (
            "typescript_mocked_module_limit",
            "probe:src_discount.ts:typescript_preview:2396aec1",
            "at threshold discounts",
            "exact_value",
            "strong",
        ),
        (
            "javascript_js_preview",
            "probe:src_discount.js:typescript_preview:8152c8c0",
            "at threshold discounts",
            "exact_value",
            "strong",
        ),
        (
            "typescript_static_limit_taxonomy",
            "probe:src_dispatch.ts:typescript_preview:dd0f8848",
            "dispatch smoke",
            "smoke_only",
            "smoke",
        ),
    ] {
        let case = fixtures.join(fixture);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--format", "json", "--root"])
            .arg(case.join("input"))
            .arg("--diff")
            .arg(case.join("diff.patch"))
            .output()
            .map_err(|e| e.to_string())?;
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
        let finding = report["findings"]
            .as_array()
            .and_then(|fs| fs.iter().find(|f| f["id"] == probe_id))
            .ok_or_else(|| format!("missing {probe_id} in {fixture}"))?;
        assert_eq!(finding["oracle_kind"], "unknown", "{fixture}");
        assert_eq!(finding["oracle_strength"], "none", "{fixture}");
        let row = finding["related_tests"]
            .as_array()
            .and_then(|rs| rs.iter().find(|r| r["name"] == name))
            .ok_or_else(|| format!("missing {name} in {fixture}"))?;
        for graded in [row, &finding["typescript_preview_card"]] {
            assert_eq!(graded["oracle_kind"], kind, "{fixture}");
            assert_eq!(graded["oracle_strength"], strength, "{fixture}");
        }
    }
    Ok(())
}
