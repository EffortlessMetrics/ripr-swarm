//! Regression gate for the flake ledger (`policy/flaky-tests.toml`).
//!
//! The project rule is to fix the root cause, never to retry. This gate makes
//! that rule structural: nextest may not retry, every ledger entry is complete,
//! a `fixed` entry still has the guard test that pins its cause, and an `open`
//! entry names the issue that owns it.
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "target" && name != ".git" && name != "node_modules" {
                rust_sources(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn workspace_source_text() -> String {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["crates", "xtask", "tools"] {
        rust_sources(&root.join(dir), &mut files);
    }
    files.sort();
    files
        .iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_test_fn(sources: &str, name: &str) -> bool {
    sources.lines().any(|line| {
        line.trim_start().strip_prefix("fn ").is_some_and(|rest| {
            rest.strip_prefix(name)
                .is_some_and(|after| after.starts_with('('))
        })
    })
}

const REQUIRED: &[&str] = &[
    "id",
    "test",
    "lane",
    "class",
    "status",
    "first_seen",
    "symptom",
    "root_cause",
];

/// Violations in `ledger`; `sources` is the text searched for guard tests.
fn ledger_violations(ledger: &str, sources: &str) -> Vec<String> {
    let value: toml::Table = match toml::from_str(ledger) {
        Ok(value) => value,
        Err(error) => return vec![format!("ledger is not valid TOML: {error}")],
    };
    let Some(entries) = value.get("flake").and_then(toml::Value::as_array) else {
        return vec!["ledger has no [[flake]] entries".to_string()];
    };
    let mut violations = Vec::new();
    let mut ids = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let label = text("id").unwrap_or("<missing id>");
        let at = format!("flake #{} `{label}`", index + 1);
        for key in REQUIRED {
            if text(key).is_none() {
                violations.push(format!("{at}: `{key}` is required and nonblank"));
            }
        }
        if let Some(id) = text("id")
            && !ids.insert(id.to_string())
        {
            violations.push(format!("{at}: duplicate id"));
        }
        let evidence_rows = entry
            .get("evidence")
            .and_then(toml::Value::as_array)
            .map_or(0, Vec::len);
        match text("status") {
            Some("fixed") => {
                if evidence_rows == 0 {
                    violations.push(format!("{at}: a fixed flake cites its evidence runs"));
                }
                match text("guard") {
                    None => violations.push(format!(
                        "{at}: a fixed flake names the guard test that pins its cause"
                    )),
                    Some(guard) if !has_test_fn(sources, guard) => violations.push(format!(
                        "{at}: guard test `{guard}` no longer exists; restore it or amend the entry"
                    )),
                    Some(_) => {}
                }
            }
            Some("open") => {
                let issue = entry.get("issue").and_then(toml::Value::as_integer);
                if issue.is_none_or(|number| number <= 0) {
                    violations.push(format!(
                        "{at}: an open flake names the GitHub issue (`issue = N`) that owns its fix"
                    ));
                }
            }
            Some(other) => violations.push(format!(
                "{at}: status must be `fixed` or `open`, got `{other}`"
            )),
            None => {}
        }
    }
    violations
}

/// Violations in a nextest config: no profile may retry or mark results flaky.
fn retry_violations(config: &str) -> Vec<String> {
    let value: toml::Table = match toml::from_str(config) {
        Ok(value) => value,
        Err(error) => return vec![format!("nextest config is not valid TOML: {error}")],
    };
    let mut violations = Vec::new();
    let mut scan = |scope: &str, table: &toml::Value| {
        if let Some(retries) = table.get("retries") {
            let zero = retries.as_integer() == Some(0);
            if !zero {
                violations.push(format!(
                    "{scope}: `retries` must be 0 (fix the flake; retries hide it), got {retries}"
                ));
            }
        }
    };
    if let Some(profiles) = value.get("profile").and_then(toml::Value::as_table) {
        for (name, profile) in profiles {
            scan(&format!("profile.{name}"), profile);
            if let Some(overrides) = profile.get("overrides").and_then(toml::Value::as_array) {
                for (index, item) in overrides.iter().enumerate() {
                    scan(&format!("profile.{name}.overrides[{index}]"), item);
                }
            }
        }
    }
    if let Some(overrides) = value.get("overrides").and_then(toml::Value::as_array) {
        for (index, item) in overrides.iter().enumerate() {
            scan(&format!("overrides[{index}]"), item);
        }
    }
    violations
}

#[test]
fn committed_flake_ledger_is_complete_and_its_guards_exist() -> Result<(), String> {
    let ledger = fs::read_to_string(repo_root().join("policy/flaky-tests.toml"))
        .map_err(|error| format!("read policy/flaky-tests.toml: {error}"))?;
    let violations = ledger_violations(&ledger, &workspace_source_text());
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

#[test]
fn nextest_never_retries_a_failing_test() -> Result<(), String> {
    let config = fs::read_to_string(repo_root().join(".config/nextest.toml"))
        .map_err(|error| format!("read .config/nextest.toml: {error}"))?;
    let violations = retry_violations(&config);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

const GOOD: &str = r#"
[[flake]]
id = "a"
test = "t"
lane = "l"
class = "c"
status = "fixed"
first_seen = "2026-10-31"
evidence = ["run 1"]
symptom = "s"
root_cause = "r"
guard = "guard_exists"
"#;

#[test]
fn a_complete_entry_with_a_live_guard_passes() {
    assert_eq!(
        ledger_violations(GOOD, "    fn guard_exists() {}"),
        Vec::<String>::new()
    );
}

#[test]
fn a_fixed_entry_whose_guard_was_deleted_fails() {
    let violations = ledger_violations(GOOD, "fn other() {}");
    assert!(
        violations.iter().any(|v| v.contains("guard_exists")),
        "{violations:?}"
    );
    // A prefix of another test's name is not the guard.
    let violations = ledger_violations(GOOD, "fn guard_exists_not() {}");
    assert!(!violations.is_empty());
}

#[test]
fn incomplete_duplicate_and_ownerless_entries_fail() {
    let open_without_issue = GOOD
        .replace("status = \"fixed\"", "status = \"open\"")
        .replace("guard = \"guard_exists\"", "");
    assert!(
        ledger_violations(&open_without_issue, "")
            .iter()
            .any(|v| v.contains("issue"))
    );
    let owned = format!("{open_without_issue}\nissue = 12\n");
    assert_eq!(ledger_violations(&owned, ""), Vec::<String>::new());
    let duplicate = format!("{GOOD}\n{GOOD}");
    assert!(
        ledger_violations(&duplicate, "fn guard_exists() {}")
            .iter()
            .any(|v| v.contains("duplicate"))
    );
    let blank = GOOD.replace("root_cause = \"r\"", "root_cause = \"  \"");
    assert!(
        ledger_violations(&blank, "fn guard_exists() {}")
            .iter()
            .any(|v| v.contains("root_cause"))
    );
}

#[test]
fn any_nonzero_retry_setting_fails_the_gate() {
    assert!(retry_violations("[profile.ci]\nretries = 0\n").is_empty());
    assert!(retry_violations("[profile.ci]\nfailure-output = \"final\"\n").is_empty());
    for config in [
        "[profile.ci]\nretries = 2\n",
        "[profile.ci]\nretries = { backoff = \"fixed\", count = 3 }\n",
        "[profile.ci]\nretries = 0\n[[profile.ci.overrides]]\nfilter = \"all()\"\nretries = 1\n",
        "[[overrides]]\nfilter = \"all()\"\nretries = 1\n",
    ] {
        assert!(!retry_violations(config).is_empty(), "{config}");
    }
}
