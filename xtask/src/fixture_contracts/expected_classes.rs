//! Authored class expectations are independent of regenerated golden output (#12928).
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const CLASSES: &[&str] = &[
    "exposed",
    "weakly_exposed",
    "reachable_unrevealed",
    "no_static_path",
    "infection_unknown",
    "propagation_unknown",
    "static_unknown",
];

fn required(name: &str) -> bool {
    name.starts_with("python_adversarial_")
        || name.contains("_negative")
        || name.contains("_control")
}

/// A JSON array preserves duplicate anchors so they can be rejected, rather than
/// silently accepting the last value as a JSON object parser would.
fn declarations(spec: &str) -> Result<Option<BTreeMap<String, String>>, String> {
    let mut lines = spec.lines();
    let mut result = None;
    while let Some(line) = lines.next() {
        if line.trim() != "```expected-classes" {
            continue;
        }
        if result.is_some() {
            return Err("duplicate expected-classes block".into());
        }
        let mut body = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line.trim() == "```" {
                closed = true;
                break;
            }
            body.push_str(line);
            body.push('\n');
        }
        if !closed {
            return Err("unterminated expected-classes block".into());
        }
        let rows: Vec<(String, String)> = serde_json::from_str(&body).map_err(|err| {
            format!("invalid expected-classes array of [finding ID, class] pairs: {err}")
        })?;
        let mut map = BTreeMap::new();
        for (anchor, class) in rows {
            if anchor.trim().is_empty() || !CLASSES.contains(&class.as_str()) {
                return Err(format!("invalid expectation {anchor:?} -> {class:?}"));
            }
            if map.insert(anchor.clone(), class).is_some() {
                return Err(format!("duplicate expected-classes anchor {anchor}"));
            }
        }
        result = Some(map);
    }
    Ok(result)
}

pub(crate) fn validate(spec: &str, output: &str, mandatory: bool) -> Result<bool, String> {
    let Some(expected) = declarations(spec)? else {
        return if mandatory {
            Err("missing mandatory expected-classes block".into())
        } else {
            Ok(false)
        };
    };
    let json: serde_json::Value = serde_json::from_str(output).map_err(|err| err.to_string())?;
    let findings = json
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .ok_or("check.json findings must be an array")?;
    let mut seen = BTreeSet::new();
    for finding in findings {
        let id = finding
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or("finding has no ID")?;
        let class = finding
            .get("classification")
            .and_then(serde_json::Value::as_str)
            .ok_or("finding has no classification")?;
        if !seen.insert(id) {
            return Err(format!("duplicate finding ID {id}"));
        }
        if expected.get(id).map(String::as_str) != Some(class) {
            return Err(format!(
                "expected-classes mismatch at {id}: declared {:?}, actual {class}",
                expected.get(id)
            ));
        }
    }
    for id in expected.keys() {
        if !seen.contains(id.as_str()) {
            return Err(format!(
                "expected-classes anchor missing from findings: {id}"
            ));
        }
    }
    Ok(true)
}

pub(crate) fn validate_output(fixture: &Path, output: &str) -> Result<bool, String> {
    let name = fixture
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid fixture name")?;
    let spec = fs::read_to_string(fixture.join("SPEC.md")).map_err(|err| err.to_string())?;
    validate(&spec, output, required(name)).map_err(|err| format!("{}: {err}", fixture.display()))
}

/// Returns (with, without) per language. Empty findings still count: an authored
/// empty array explicitly guards a no-findings fixture against new findings.
pub(crate) fn inventory(root: &Path) -> Result<BTreeMap<String, [usize; 2]>, String> {
    let mut counts = BTreeMap::<String, [usize; 2]>::new();
    let mut errors = Vec::new();
    for entry in fs::read_dir(root.join("fixtures")).map_err(|err| err.to_string())? {
        let path = entry.map_err(|err| err.to_string())?.path();
        let check = path.join("expected/check.json");
        if !check.is_file() || !path.join("SPEC.md").is_file() {
            continue;
        }
        let output = fs::read_to_string(check).map_err(|err| err.to_string())?;
        let value: serde_json::Value =
            serde_json::from_str(&output).map_err(|err| err.to_string())?;
        let languages: BTreeSet<&str> = value
            .pointer("/summary/changed_files_by_language")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|row| {
                row.get("files")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|n| n > 0)
            })
            .filter_map(|row| row.get("language").and_then(serde_json::Value::as_str))
            .collect();
        let declared = match validate_output(&path, &output) {
            Ok(declared) => declared,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
        for language in languages {
            counts.entry(language.to_string()).or_default()[usize::from(!declared)] += 1;
        }
    }
    let ratchet: BTreeMap<String, usize> = serde_json::from_str(
        &fs::read_to_string(root.join("policy/fixture_expected_classes.json"))
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    for (language, floor) in ratchet {
        let actual = counts.get(&language).map_or(0, |count| count[0]);
        if actual < floor {
            errors.push(format!(
                "expected-classes {language} ratchet: {actual} < {floor}"
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SPEC: &str = "```expected-classes\n[[\"a\",\"weakly_exposed\"]]\n```";
    const GOOD: &str = r#"{"findings":[{"id":"a","classification":"weakly_exposed"}]}"#;
    #[test]
    fn declared_classes_reject_wrong_rebless_and_accept_valid_output() -> Result<(), String> {
        assert!(validate(SPEC, GOOD, true)?);
        assert!(validate(SPEC, &GOOD.replace("weakly_exposed", "exposed"), true).is_err());
        assert!(validate(SPEC, r#"{"findings":[]}"#, true).is_err());
        assert!(validate(SPEC, &GOOD.replace("\"a\"", "\"b\""), true).is_err());
        Ok(())
    }
    #[test]
    fn declarations_fail_closed_on_invalid_or_ambiguous_input() {
        for spec in [
            SPEC.replace("weakly_exposed", "bogus"),
            format!("{SPEC}\n{SPEC}"),
            SPEC.replace(" ]]", "]]").replace(
                "[\"a\",\"weakly_exposed\"]",
                "[\"a\",\"weakly_exposed\"],[\"a\",\"exposed\"]",
            ),
            SPEC.trim_end_matches('`').to_string(),
        ] {
            assert!(validate(&spec, GOOD, true).is_err(), "{spec}");
        }
        assert!(validate("", GOOD, true).is_err());
        assert_eq!(validate("", GOOD, false), Ok(false));
        assert!(validate("```expected-classes\n[]\n```", GOOD, true).is_err());
    }
    #[test]
    fn expected_classes_inventory_counts_declarations_and_enforces_floor() -> Result<(), String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-12928-inventory-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).map_err(|err| err.to_string())?;
        let outcome = (|| {
            fs::create_dir_all(root.join("policy")).map_err(|err| err.to_string())?;
            let ratchet = root.join("policy/fixture_expected_classes.json");
            fs::write(&ratchet, r#"{"rust":1}"#).map_err(|err| err.to_string())?;
            let output = r#"{"summary":{"changed_files_by_language":[{"language":"rust","files":1}]},"findings":[{"id":"a","classification":"weakly_exposed"}]}"#;
            for (name, spec) in [("sample", SPEC), ("uncontracted", "")] {
                let fixture = root.join("fixtures").join(name);
                fs::create_dir_all(fixture.join("expected")).map_err(|err| err.to_string())?;
                fs::write(fixture.join("SPEC.md"), spec).map_err(|err| err.to_string())?;
                fs::write(fixture.join("expected/check.json"), output)
                    .map_err(|err| err.to_string())?;
            }
            let counts = inventory(&root)?;
            if counts.get("rust") != Some(&[1, 1]) {
                return Err(format!(
                    "incorrect authored/uncontracted ledger: {counts:?}"
                ));
            }
            fs::write(root.join("fixtures/sample/SPEC.md"), "").map_err(|err| err.to_string())?;
            let error = inventory(&root)
                .err()
                .ok_or("coverage floor accepted removal of a declaration")?;
            if !error.contains("expected-classes rust ratchet: 0 < 1") {
                return Err(format!("wrong underflow refusal: {error}"));
            }
            fs::write(root.join("fixtures/sample/SPEC.md"), SPEC).map_err(|err| err.to_string())?;
            fs::write(&ratchet, r#"{"rust":1,"python":1}"#).map_err(|err| err.to_string())?;
            let error = inventory(&root)
                .err()
                .ok_or("absent language bypassed its coverage floor")?;
            if !error.contains("expected-classes python ratchet: 0 < 1") {
                return Err(format!("wrong absent-language refusal: {error}"));
            }
            Ok(())
        })();
        let cleanup = fs::remove_dir_all(&root).map_err(|err| err.to_string());
        outcome?;
        cleanup
    }

    #[test]
    fn mandatory_name_families() {
        for name in [
            "python_adversarial_sink",
            "rust_negative",
            "typescript_control_case",
        ] {
            assert!(required(name));
        }
        assert!(!required("boundary_gap"));
    }
}
