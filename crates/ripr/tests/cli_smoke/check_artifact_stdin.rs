//! Stdin admission and named-file recovery for RIPR-SPEC-0140 (#5112).

use super::{
    assert_success, context_reuse_matches_fresh, ignore_remove_dir_all, spawn_command,
    unique_temp_workspace,
};
use std::path::Path;
use std::process::Output;

#[cfg(feature = "lang-python")]
const PYTHON_PATCH: &str = "--- a/src/discount.py\n+++ b/src/discount.py\n@@ -1,4 +1,4 @@\n def discount(amount):\n-    if amount > 100:\n+    if amount >= 100:\n         return 10\n     return 0\n";
#[cfg(feature = "lang-python")]
const PYTHON_COMMENT: &str = "--- a/src/discount.py\n+++ b/src/discount.py\n@@ -1,4 +1,5 @@\n+# pricing documentation\n def discount(amount):\n     if amount >= 100:\n         return 10\n     return 0\n";
#[cfg(feature = "lang-typescript")]
const JAVASCRIPT_PATCH: &str = "--- a/src/discount.js\n+++ b/src/discount.js\n@@ -1,3 +1,3 @@\n export function discount(amount) {\n-  return amount > 100;\n+  return amount >= 100;\n }\n";

fn run(root: &Path, args: &[&str], stdin: Option<&[u8]>) -> Result<Output, String> {
    spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        args,
        &[],
        None,
        stdin,
    )
    .map_err(|error| format!("run ripr {args:?}: {error}"))
}

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(root.join(path), bytes).map_err(|error| format!("write {path}: {error}"))
}

/// Analysis identity is independent of the provenance-bound inspect route
/// (#7257/#7258). Named `--write-artifact`, stdin `--diff -`, and a literal
/// `./-` file must agree on findings while `detail_route` follows the source.
fn without_canonical_next_action(value: &serde_json::Value) -> serde_json::Value {
    let mut stripped = value.clone();
    if let Some(object) = stripped.as_object_mut() {
        object.remove("canonical_next_action");
    }
    stripped
}

fn canonical_next_action(value: &serde_json::Value) -> Result<&serde_json::Value, String> {
    value
        .get("canonical_next_action")
        .ok_or_else(|| format!("check JSON must embed canonical_next_action: {value}"))
}

fn assert_shared_canonical_decision(
    left: &serde_json::Value,
    right: &serde_json::Value,
    message: &str,
) -> Result<(), String> {
    let left_action = canonical_next_action(left)?;
    let right_action = canonical_next_action(right)?;
    if left_action["schema_version"] != "canonical_next_action.v1"
        || right_action["schema_version"] != "canonical_next_action.v1"
    {
        return Err(format!(
            "{message}: schema_version must be canonical_next_action.v1\nleft: {left_action}\nright: {right_action}"
        ));
    }
    for key in ["producer", "action_class"] {
        if left_action[key] != right_action[key] {
            return Err(format!(
                "{message}: canonical action {key} diverged\nleft: {}\nright: {}",
                left_action[key], right_action[key]
            ));
        }
    }
    if left_action["stop"]["case"] != right_action["stop"]["case"]
        || left_action["subject"]["item"] != right_action["subject"]["item"]
    {
        return Err(format!(
            "{message}: selected item diverged\nleft: {left_action}\nright: {right_action}"
        ));
    }
    Ok(())
}

fn inspect_route(value: &serde_json::Value) -> Result<&str, String> {
    canonical_next_action(value)?["stop"]["detail_route"]
        .as_str()
        .ok_or_else(|| format!("canonical action must carry detail_route: {value}"))
}

fn exercise(root: &Path, patch: &str, unrelated: &str, selector: &str) -> Result<(), String> {
    write(root, "change.diff", patch)?;
    let named = run(
        root,
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--json",
            "--write-artifact",
            "accepted.json",
        ],
        None,
    )?;
    assert_success(&named);
    let parsed: serde_json::Value = serde_json::from_slice(&named.stdout)
        .map_err(|error| format!("parse named check: {error}"))?;
    assert_eq!(parsed["summary"]["probes"], 1);
    assert_eq!(parsed["findings"].as_array().map(Vec::len), Some(1));
    let accepted = std::fs::read(root.join("accepted.json"))
        .map_err(|error| format!("read accepted artifact: {error}"))?;
    let ordinary = run(
        root,
        &["check", "--root", ".", "--diff", "-", "--json"],
        Some(patch.as_bytes()),
    )?;
    assert_success(&ordinary);
    let stdin_parsed: serde_json::Value = serde_json::from_slice(&ordinary.stdout)
        .map_err(|error| format!("parse stdin check: {error}"))?;
    assert_eq!(
        without_canonical_next_action(&parsed),
        without_canonical_next_action(&stdin_parsed),
        "ordinary stdin must retain named-file analysis"
    );
    assert_shared_canonical_decision(
        &parsed,
        &stdin_parsed,
        "ordinary stdin must retain the named-file next-action decision",
    )?;
    let named_route = inspect_route(&parsed)?;
    if !named_route.contains("--from ") || !named_route.contains("accepted.json") {
        return Err(format!(
            "named --write-artifact inspect route must follow the artifact:\n{named_route}"
        ));
    }
    if named_route.contains("--diff -") {
        return Err(format!(
            "named artifact inspect route must not collapse to stdin:\n{named_route}"
        ));
    }
    let stdin_route = inspect_route(&stdin_parsed)?;
    if !stdin_route.contains("--diff -") {
        return Err(format!(
            "stdin inspect route must keep --diff -:\n{stdin_route}"
        ));
    }
    if stdin_route.contains("--from ") {
        return Err(format!(
            "stdin inspect route must not invent an artifact --from:\n{stdin_route}"
        ));
    }

    // The unrelated cwd entry must not control admission or identity. Both
    // streams used to record the same identity despite different findings.
    // Admission receives EOF; no analysis/progress may happen first.
    for dash in [None, Some(""), Some(unrelated)] {
        if let Some(bytes) = dash {
            write(root, "-", bytes)?;
        }
        for destination in ["new.json", "accepted.json"] {
            let refused = run(
                root,
                &[
                    "check",
                    "--root",
                    ".",
                    "--diff",
                    "-",
                    "--json",
                    "--write-artifact",
                    destination,
                ],
                None,
            )?;
            let stderr = String::from_utf8_lossy(&refused.stderr);
            assert_eq!(refused.status.code(), Some(2), "{stderr}");
            // #6834: the refusal is a parseable envelope on stdout, not
            // silence. The machine-readable document names the fallback
            // identity and stays non-consumable; the human recovery text
            // below is unchanged.
            let refusal: serde_json::Value = serde_json::from_slice(&refused.stdout)
                .map_err(|error| format!("parse refusal envelope: {error}"))?;
            assert_eq!(
                refusal["analysis_scope"]["run_status"], "analysis_failed",
                "{refusal}"
            );
            assert_eq!(
                refusal["analysis_scope"]["downstream_consumable"], false,
                "{refusal}"
            );
            assert!(
                refusal["run_limitations"][0]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("--write-artifact")),
                "{refusal}"
            );
            assert_eq!(
                refusal["findings"].as_array().map(Vec::len),
                Some(0),
                "{refusal}"
            );
            assert!(
                stderr.contains(&format!(
                    "--write-artifact {destination} cannot be combined with --diff -"
                )),
                "{stderr}"
            );
            assert!(
                stderr.contains("save stdin to a named diff file"),
                "{stderr}"
            );
            assert!(stderr.contains("pass --diff <path>"), "{stderr}");
            assert!(!stderr.contains("ripr progress:"), "analysis ran: {stderr}");
            assert!(
                !root.join("new.json").exists(),
                "refusal created an artifact"
            );
            assert_eq!(
                std::fs::read(root.join("accepted.json"))
                    .map_err(|error| format!("read preserved artifact: {error}"))?,
                accepted,
                "refusal replaced the accepted artifact"
            );
        }
    }

    // Follow the recovery instruction: the saved named diff still writes
    // and reuses the nonempty result after every refusal.
    let recovered = run(
        root,
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--json",
            "--write-artifact",
            "accepted.json",
        ],
        None,
    )?;
    assert_success(&recovered);
    assert_eq!(
        std::fs::read(root.join("accepted.json"))
            .map_err(|error| format!("read recovered artifact: {error}"))?,
        accepted
    );
    let fresh = run(
        root,
        &[
            "context",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--at",
            selector,
            "--json",
        ],
        None,
    )?;
    assert_success(&fresh);
    let reused = run(
        root,
        &[
            "context",
            "--root",
            ".",
            "--from",
            "accepted.json",
            "--at",
            selector,
            "--json",
        ],
        None,
    )?;
    assert_success(&reused);
    context_reuse_matches_fresh(&fresh.stdout, &reused.stdout)?;

    // Only the exact '-' sentinel is stdin. An explicit path to a literal
    // file named '-' remains a supported named-file artifact source.
    write(root, "-", patch)?;
    let literal_path = "./-".to_string();
    let literal = run(
        root,
        &[
            "check",
            "--root",
            ".",
            "--diff",
            &literal_path,
            "--json",
            "--write-artifact",
            "literal.json",
        ],
        None,
    )?;
    assert_success(&literal);
    let literal_parsed: serde_json::Value = serde_json::from_slice(&literal.stdout)
        .map_err(|error| format!("parse literal-file check: {error}"))?;
    assert_eq!(
        without_canonical_next_action(&parsed),
        without_canonical_next_action(&literal_parsed),
        "literal ./- must retain named-file analysis"
    );
    assert_shared_canonical_decision(
        &parsed,
        &literal_parsed,
        "literal ./- must retain the named-file next-action decision",
    )?;
    let literal_route = inspect_route(&literal_parsed)?;
    if !literal_route.contains("--from ") || !literal_route.contains("literal.json") {
        return Err(format!(
            "literal-file --write-artifact inspect route must follow literal.json:\n{literal_route}"
        ));
    }
    if literal_route.contains("--diff -") {
        return Err(format!(
            "literal-file inspect route must not collapse to the stdin sentinel:\n{literal_route}"
        ));
    }
    // --from treats --diff as an assertion. The exact stdin sentinel must
    // never be silently accepted as the literal file above.
    for command in ["explain", "context"] {
        let mut args = vec![
            command,
            "--root",
            ".",
            "--from",
            "literal.json",
            "--diff",
            "-",
        ];
        if command == "context" {
            args.push("--at");
        }
        args.push(selector);
        let refused = run(root, &args, None)?;
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert_eq!(refused.status.code(), Some(2), "{stderr}");
        assert!(refused.stdout.is_empty());
        assert!(
            stderr.contains("--from cannot be combined with --diff -"),
            "{stderr}"
        );
        assert!(stderr.contains("pass --diff <path>"), "{stderr}");
        let named_args = args
            .iter()
            .map(|arg| {
                if *arg == "-" {
                    literal_path.as_str()
                } else {
                    *arg
                }
            })
            .collect::<Vec<_>>();
        assert_success(&run(root, &named_args, None)?);
    }
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn python_stdin_artifact_refusal_preserves_named_recovery() -> Result<(), String> {
    let root = unique_temp_workspace("python-stdin-artifact");
    let result = (|| {
        std::fs::create_dir_all(root.join("src"))
            .and_then(|()| std::fs::create_dir_all(root.join("tests")))
            .map_err(|error| format!("create Python fixture: {error}"))?;
        write(
            &root,
            "ripr.toml",
            "[languages]\nenabled = [\"rust\", \"python\"]\n",
        )?;
        write(
            &root,
            "src/discount.py",
            "def discount(amount):\n    if amount >= 100:\n        return 10\n    return 0\n",
        )?;
        write(
            &root,
            "tests/test_discount.py",
            "from src.discount import discount\n\ndef test_below():\n    assert discount(99) == 0\n\ndef test_above():\n    assert discount(101) == 10\n",
        )?;
        exercise(&root, PYTHON_PATCH, PYTHON_COMMENT, "src/discount.py:2")
    })();
    ignore_remove_dir_all(&root);
    result
}

#[cfg(feature = "lang-typescript")]
#[test]
fn javascript_stdin_artifact_refusal_preserves_named_recovery() -> Result<(), String> {
    let root = unique_temp_workspace("javascript-stdin-artifact");
    let result = (|| {
        std::fs::create_dir_all(root.join("src"))
            .and_then(|()| std::fs::create_dir_all(root.join("tests")))
            .map_err(|error| format!("create JavaScript fixture: {error}"))?;
        write(
            &root,
            "ripr.toml",
            "[languages]\nenabled = [\"rust\", \"typescript\"]\n",
        )?;
        write(
            &root,
            "package.json",
            "{\"name\":\"artifact-recovery\",\"type\":\"module\",\"private\":true}\n",
        )?;
        write(
            &root,
            "src/discount.js",
            "export function discount(amount) {\n  return amount >= 100;\n}\n",
        )?;
        write(
            &root,
            "tests/discount.test.js",
            "import { expect, test } from \"vitest\";\nimport { discount } from \"../src/discount.js\";\ntest(\"below\", () => { expect(discount(99)).toBe(false); });\ntest(\"above\", () => { expect(discount(101)).toBe(true); });\n",
        )?;
        exercise(&root, JAVASCRIPT_PATCH, "", "src/discount.js:2")
    })();
    ignore_remove_dir_all(&root);
    result
}
