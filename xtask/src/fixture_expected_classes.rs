//! Machine-checked expected classes for fixtures (#12928).
//!
//! A fixture `SPEC.md` may carry one fenced `expected-classes` block that maps
//! a finding anchor (the `id` of a finding in `expected/check.json`) to the
//! static class the fixture states it must have:
//!
//! ````text
//! ```expected-classes
//! probe:src_lib.rs:predicate:908b0e30 -> no_static_path
//! ```
//! ````
//!
//! The block is the human-reviewed statement; `expected/check.json` is the
//! blessable golden. This module is the single owner of comparing the two, so a
//! `goldens bless` (or a hand edit) cannot re-pin a different class without the
//! SPEC block changing in the same commit. Fixtures whose purpose is a
//! discriminating negative must carry the block.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The seven-value static class vocabulary.
pub(crate) const CLASS_VOCABULARY: [&str; 7] = [
    "exposed",
    "weakly_exposed",
    "reachable_unrevealed",
    "no_static_path",
    "infection_unknown",
    "propagation_unknown",
    "static_unknown",
];

const FENCE_OPEN: &str = "```expected-classes";
const FENCE_CLOSE: &str = "```";

/// Ratchet of the minimum number of fixtures carrying an expectation, per language.
pub(crate) const RATCHET_PATH: &str = "policy/fixture-expected-classes-ratchet.toml";

/// Fixtures whose purpose is a discriminating negative must declare a block.
pub(crate) fn expectation_is_mandatory(fixture_name: &str) -> bool {
    fixture_name.starts_with("python_adversarial_")
        || fixture_name.contains("_negative")
        || fixture_name.contains("_control")
}

/// Parse the `expected-classes` block of a SPEC. `Ok(None)` when absent.
pub(crate) fn parse_expected_classes(
    spec_text: &str,
) -> Result<Option<Vec<(String, String)>>, String> {
    let mut entries = Vec::new();
    let mut inside = false;
    let mut seen_block = false;
    for (index, raw) in spec_text.lines().enumerate() {
        let line = raw.trim();
        if !inside {
            if line.starts_with(FENCE_OPEN) && line != FENCE_OPEN {
                return Err(format!(
                    "line {}: malformed fence `{line}`; use exactly ```expected-classes",
                    index + 1
                ));
            }
            if line == FENCE_OPEN {
                if seen_block {
                    return Err("more than one `expected-classes` block".to_string());
                }
                inside = true;
                seen_block = true;
            }
            continue;
        }
        if line == FENCE_CLOSE {
            inside = false;
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((anchor, class)) = line.split_once("->") else {
            return Err(format!(
                "line {}: expected `<finding id> -> <class>`, found `{line}`",
                index + 1
            ));
        };
        let (anchor, class) = (anchor.trim(), class.trim());
        if anchor.is_empty() || class.is_empty() {
            return Err(format!(
                "line {}: empty anchor or class in `{line}`",
                index + 1
            ));
        }
        entries.push((anchor.to_string(), class.to_string()));
    }
    if inside {
        return Err("unterminated `expected-classes` block".to_string());
    }
    if seen_block && entries.is_empty() {
        return Err("`expected-classes` block declares no entries".to_string());
    }
    Ok(seen_block.then_some(entries))
}

fn findings_by_id(check_json: &str) -> Result<BTreeMap<String, String>, String> {
    let value: Value =
        serde_json::from_str(check_json).map_err(|err| format!("check.json is not JSON: {err}"))?;
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "check.json has no `findings` array".to_string())?;
    let mut map = BTreeMap::new();
    for finding in findings {
        if let (Some(id), Some(class)) = (
            finding.get("id").and_then(Value::as_str),
            finding.get("classification").and_then(Value::as_str),
        ) && map.insert(id.to_string(), class.to_string()).is_some()
        {
            return Err(format!("check.json has duplicate finding id `{id}`"));
        }
    }
    Ok(map)
}

/// Compare a SPEC's declared classes against a `check.json` text.
pub(crate) fn class_violations(
    fixture_name: &str,
    spec_text: &str,
    check_json: &str,
) -> Vec<String> {
    let declared = match parse_expected_classes(spec_text) {
        Ok(declared) => declared,
        Err(err) => return vec![format!("fixtures/{fixture_name}/SPEC.md: {err}")],
    };
    if declared.is_none() && !expectation_is_mandatory(fixture_name) {
        return Vec::new();
    }
    let actual = match findings_by_id(check_json) {
        Ok(actual) => actual,
        Err(err) => return vec![format!("fixtures/{fixture_name}: {err}")],
    };
    let Some(declared) = declared else {
        if !actual.is_empty() {
            return vec![format!(
                "fixtures/{fixture_name}/SPEC.md has no `expected-classes` block; discriminating-negative fixtures must state their expected class per finding"
            )];
        }
        return Vec::new();
    };
    let mut violations = Vec::new();
    if expectation_is_mandatory(fixture_name) {
        for id in actual.keys() {
            if !declared.iter().any(|(anchor, _)| anchor == id) {
                violations.push(format!(
                    "fixtures/{fixture_name}/SPEC.md does not declare finding `{id}`; discriminating-negative fixtures must declare every finding"
                ));
            }
        }
    }
    let mut seen = BTreeMap::new();
    for (anchor, class) in &declared {
        if seen.insert(anchor.as_str(), class.as_str()).is_some() {
            violations.push(format!(
                "fixtures/{fixture_name}/SPEC.md declares finding `{anchor}` more than once"
            ));
        }
        if !CLASS_VOCABULARY.contains(&class.as_str()) {
            violations.push(format!(
                "fixtures/{fixture_name}/SPEC.md declares `{anchor}` as `{class}`, which is outside the class vocabulary ({})",
                CLASS_VOCABULARY.join(", ")
            ));
            continue;
        }
        match actual.get(anchor) {
            None => violations.push(format!(
                "fixtures/{fixture_name}/SPEC.md declares finding `{anchor}` but expected/check.json has no such finding"
            )),
            Some(found) if found != class => violations.push(format!(
                "fixtures/{fixture_name}: SPEC.md states `{anchor}` is `{class}` but expected/check.json says `{found}`; change the SPEC block in the same commit only if the new class is intended"
            )),
            Some(_) => {}
        }
    }
    violations
}

/// Compare a fixture directory's SPEC with its blessed `expected/check.json`.
pub(crate) fn fixture_dir_violations(fixture: &Path) -> Vec<String> {
    let name = fixture
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let (Ok(spec), Ok(check)) = (
        fs::read_to_string(fixture.join("SPEC.md")),
        fs::read_to_string(fixture.join("expected/check.json")),
    ) else {
        return Vec::new();
    };
    class_violations(name, &spec, &check)
}

/// Compare a fixture's SPEC with a freshly produced `check.json` (bless guard).
pub(crate) fn bless_violations(
    fixture: &Path,
    new_check_json: &Path,
) -> Result<Vec<String>, String> {
    let name = fixture
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let spec = fs::read_to_string(fixture.join("SPEC.md")).unwrap_or_default();
    let check = fs::read_to_string(new_check_json)
        .map_err(|err| format!("failed to read {}: {err}", new_check_json.display()))?;
    Ok(class_violations(name, &spec, &check))
}

fn fixture_language(check_json: &str) -> String {
    // The language with the most changed files; ties break alphabetically.
    let mut best: Option<(u64, String)> = None;
    if let Some(entries) = serde_json::from_str::<Value>(check_json)
        .ok()
        .and_then(|value| {
            value
                .pointer("/summary/changed_files_by_language")
                .and_then(Value::as_array)
                .cloned()
        })
    {
        for entry in &entries {
            let (Some(language), Some(files)) = (
                entry.get("language").and_then(Value::as_str),
                entry.get("files").and_then(Value::as_u64),
            ) else {
                continue;
            };
            let better = files > 0
                && best.as_ref().is_none_or(|(count, name)| {
                    files > *count || (files == *count && language < name.as_str())
                });
            if better {
                best = Some((files, language.to_string()));
            }
        }
    }
    best.map_or_else(|| "rust".to_string(), |(_, language)| language)
}

/// Per-language `(with expectation, without)` counts over `fixtures_dir`.
pub(crate) fn ledger(fixtures_dir: &Path) -> BTreeMap<String, (usize, usize)> {
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(fixtures_dir) else {
        return counts;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let (Ok(spec), Ok(check)) = (
            fs::read_to_string(path.join("SPEC.md")),
            fs::read_to_string(path.join("expected/check.json")),
        ) else {
            continue;
        };
        let slot = counts.entry(fixture_language(&check)).or_default();
        if matches!(parse_expected_classes(&spec), Ok(Some(_))) {
            slot.0 += 1;
        } else {
            slot.1 += 1;
        }
    }
    counts
}

/// One printable ledger line, for example `rust 12/400, python 30/250`.
pub(crate) fn ledger_line(counts: &BTreeMap<String, (usize, usize)>) -> String {
    let parts: Vec<String> = counts
        .iter()
        .map(|(language, (with, without))| format!("{language} {with} with / {without} without"))
        .collect();
    format!("fixture expected-class ledger: {}", parts.join(", "))
}

/// Parse `language = N` ratchet lines (comments and blanks ignored).
pub(crate) fn parse_ratchet(text: &str) -> Result<BTreeMap<String, usize>, String> {
    let mut floors = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((language, count)) = line.split_once('=') else {
            return Err(format!(
                "{RATCHET_PATH}: expected `language = N`, found `{line}`"
            ));
        };
        let count: usize = count
            .trim()
            .parse()
            .map_err(|err| format!("{RATCHET_PATH}: `{line}` is not an integer floor: {err}"))?;
        floors.insert(language.trim().to_string(), count);
    }
    Ok(floors)
}

/// The ratchet may only rise: fewer declared fixtures than the floor fails.
pub(crate) fn ratchet_violations(
    counts: &BTreeMap<String, (usize, usize)>,
    ratchet_text: &str,
) -> Vec<String> {
    let floors = match parse_ratchet(ratchet_text) {
        Ok(floors) => floors,
        Err(err) => return vec![err],
    };
    floors
        .iter()
        .filter_map(|(language, floor)| {
            let have = counts.get(language).map_or(0, |(with, _)| *with);
            (have < *floor).then(|| {
                format!(
                    "{RATCHET_PATH}: {language} has {have} fixtures with an `expected-classes` block, below the ratchet floor {floor}; the count may only rise"
                )
            })
        })
        .collect()
}

/// Ledger line plus ratchet violations for the repository's fixtures.
pub(crate) fn ledger_report(fixtures_dir: &Path, ratchet_path: &Path) -> (String, Vec<String>) {
    let shown = ratchet_path.display();
    let counts = ledger(fixtures_dir);
    let violations = match fs::read_to_string(ratchet_path) {
        Ok(text) => ratchet_violations(&counts, &text),
        Err(err) => vec![format!("{shown}: cannot read ratchet: {err}")],
    };
    (ledger_line(&counts), violations)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECK: &str = r#"{"findings":[{"id":"probe:a:1","classification":"weakly_exposed"},{"id":"probe:a:2","classification":"no_static_path"}]}"#;

    fn spec(block: &str) -> String {
        format!("# Fixture\n\n## Then\n\n```expected-classes\n{block}\n```\n")
    }

    #[test]
    fn matching_block_passes() {
        let text = spec("probe:a:1 -> weakly_exposed\nprobe:a:2 -> no_static_path");
        assert!(class_violations("f", &text, CHECK).is_empty());
    }

    #[test]
    fn class_disagreeing_with_check_json_fails() {
        let text = spec("probe:a:1 -> exposed");
        let violations = class_violations("f", &text, CHECK);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].contains("says `weakly_exposed`"),
            "{violations:?}"
        );
    }

    #[test]
    fn flipped_golden_class_is_caught() {
        let text = spec("probe:a:2 -> no_static_path");
        let flipped = CHECK.replace("no_static_path", "exposed");
        assert_eq!(class_violations("f", &text, &flipped).len(), 1);
    }

    #[test]
    fn class_outside_vocabulary_and_unknown_anchor_fail() {
        let text = spec("probe:a:1 -> mutant_dead\nprobe:zz -> exposed");
        let violations = class_violations("f", &text, CHECK);
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations[0].contains("outside the class vocabulary"));
        assert!(violations[1].contains("no such finding"));
    }

    #[test]
    fn malformed_and_duplicate_blocks_fail() {
        assert_eq!(
            class_violations("f", &spec("probe:a:1 weakly_exposed"), CHECK).len(),
            1
        );
        let dup = spec("probe:a:1 -> weakly_exposed\nprobe:a:1 -> weakly_exposed");
        assert_eq!(class_violations("f", &dup, CHECK).len(), 1);
        let twice = format!(
            "{}{}",
            spec("probe:a:1 -> weakly_exposed"),
            spec("probe:a:2 -> no_static_path")
        );
        assert_eq!(class_violations("f", &twice, CHECK).len(), 1);
    }

    #[test]
    fn missing_block_is_optional_except_for_negative_fixtures() {
        let bare = "# Fixture\n";
        assert!(class_violations("plain_case", bare, CHECK).is_empty());
        assert_eq!(class_violations("x_negative_case", bare, CHECK).len(), 1);
        assert_eq!(class_violations("x_control", bare, CHECK).len(), 1);
        assert_eq!(
            class_violations("python_adversarial_y", bare, CHECK).len(),
            1
        );
        assert!(class_violations("x_negative_case", bare, r#"{"findings":[]}"#).is_empty());
    }

    #[test]
    fn empty_or_comment_only_block_and_variant_fence_fail() {
        assert_eq!(class_violations("f", &spec(""), CHECK).len(), 1);
        assert_eq!(class_violations("f", &spec("# nothing"), CHECK).len(), 1);
        let variant = "```expected-classes text\nprobe:a:1 -> weakly_exposed\n```\n";
        assert_eq!(class_violations("f", variant, CHECK).len(), 1);
    }

    #[test]
    fn mandatory_fixture_must_declare_every_finding() {
        let partial = spec("probe:a:1 -> weakly_exposed");
        let violations = class_violations("x_negative_case", &partial, CHECK);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("probe:a:2"), "{violations:?}");
        assert!(class_violations("plain_case", &partial, CHECK).is_empty());
    }

    #[test]
    fn duplicate_check_json_ids_fail() {
        let dup = r#"{"findings":[{"id":"p","classification":"exposed"},{"id":"p","classification":"exposed"}]}"#;
        assert_eq!(class_violations("f", &spec("p -> exposed"), dup).len(), 1);
    }

    #[test]
    fn non_expectation_fixture_tolerates_findingless_check_json() {
        assert!(class_violations("plain_case", "# Fixture\n", "{}").is_empty());
    }

    #[test]
    fn language_is_the_one_with_changed_files() {
        let json = r#"{"summary":{"changed_files_by_language":[{"language":"markdown","files":0},{"language":"typescript","files":2},{"language":"javascript","files":2}]}}"#;
        assert_eq!(fixture_language(json), "javascript");
        let one = r#"{"summary":{"changed_files_by_language":[{"language":"markdown","files":0},{"language":"python","files":1}]}}"#;
        assert_eq!(fixture_language(one), "python");
    }

    #[test]
    fn ratchet_only_rises() {
        let mut counts = BTreeMap::new();
        counts.insert("rust".to_string(), (3, 10));
        assert!(ratchet_violations(&counts, "rust = 3\n").is_empty());
        assert_eq!(
            ratchet_violations(&counts, "rust = 4\npython = 1\n").len(),
            2
        );
        assert_eq!(ratchet_violations(&counts, "rust three").len(), 1);
    }

    #[test]
    fn ledger_line_lists_each_language() {
        let mut counts = BTreeMap::new();
        counts.insert("python".to_string(), (2, 5));
        assert_eq!(
            ledger_line(&counts),
            "fixture expected-class ledger: python 2 with / 5 without"
        );
    }
}
