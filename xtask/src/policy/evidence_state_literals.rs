//! Ratchet against new inline consumer evidence-state literals (#2958).
//!
//! `domain::EvidenceState` owns the five gap states. A production line that
//! names `gap_state` next to one of the five quoted words is a bespoke copy of
//! that vocabulary. Existing copies are counted per file in
//! `policy/evidence-state-literals.txt`; a new or additional copy fails, and a
//! baseline above the real count fails too, so the ceiling only goes down.
//! Test code (everything after `#[cfg(test)]`, and test files) is not counted.

use std::collections::BTreeMap;

const BASELINE_PATH: &str = "policy/evidence-state-literals.txt";
const OWNER_PATH: &str = "crates/ripr/src/domain/evidence_state.rs";
const SOURCE_ROOT: &str = "crates/ripr/src/";
const STATE_WORDS: [&str; 5] = [
    "actionable",
    "already_observed",
    "internal_only",
    "static_limitation",
    "unknown",
];

fn is_scanned_path(path: &str) -> bool {
    path.starts_with(SOURCE_ROOT)
        && path.ends_with(".rs")
        && path != OWNER_PATH
        && !path.contains("/tests/")
        && !path.ends_with("/tests.rs")
        && !path.ends_with("_tests.rs")
}

fn line_names_state_literal(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || !trimmed.contains("gap_state") {
        return false;
    }
    STATE_WORDS.iter().any(|word| {
        trimmed.contains(&format!("\"{word}\"")) || trimmed.contains(&format!("gap_state: {word}"))
    })
}

/// Number of production lines in `text` that spell a gap-state word inline.
pub(crate) fn literal_count(text: &str) -> usize {
    text.lines()
        .take_while(|line| line.trim() != "#[cfg(test)]")
        .filter(|line| line_names_state_literal(line))
        .count()
}

/// Compares measured per-file counts with the baseline ceilings.
pub(crate) fn ratchet_violations(
    counts: &BTreeMap<String, usize>,
    baseline: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (path, count) in counts {
        let allowed = baseline.get(path).copied().unwrap_or(0);
        if *count > allowed {
            violations.push(format!(
                "{path} spells {count} gap-state word(s) inline, baseline allows {allowed}\n  reason: #2958: use `domain::EvidenceState` (`as_str`, `parse`, `is_actionable_word`) instead of a new inline literal; do not raise the baseline"
            ));
        }
    }
    for (path, allowed) in baseline {
        let count = counts.get(path).copied().unwrap_or(0);
        if count < *allowed {
            violations.push(format!(
                "{BASELINE_PATH} lists {allowed} for {path} but only {count} remain\n  reason: lower or remove the entry so the ratchet only tightens"
            ));
        }
    }
    violations
}

fn read_baseline() -> Result<BTreeMap<String, usize>, String> {
    let mut baseline = BTreeMap::new();
    for record in crate::read_pipe_records(BASELINE_PATH, 2)? {
        let count = record[1]
            .parse::<usize>()
            .map_err(|err| format!("{BASELINE_PATH}: bad count `{}`: {err}", record[1]))?;
        if baseline.insert(record[0].clone(), count).is_some() {
            return Err(format!("{BASELINE_PATH}: duplicate entry `{}`", record[0]));
        }
    }
    Ok(baseline)
}

/// Violations for the tracked file set, for `check-architecture`.
pub(crate) fn violations(files: &[String]) -> Result<Vec<String>, String> {
    let baseline = read_baseline()?;
    let mut counts = BTreeMap::new();
    let mut scanned = 0usize;
    for file in files.iter().filter(|file| is_scanned_path(file)) {
        scanned += 1;
        let count = literal_count(&crate::read_text_lossy(std::path::Path::new(file))?);
        if count > 0 {
            counts.insert(file.clone(), count);
        }
    }
    if scanned == 0 {
        return Err(
            "evidence-state literal ratchet scanned zero files; zero subjects is not a pass"
                .to_string(),
        );
    }
    Ok(ratchet_violations(&counts, &baseline))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_inline_state_words_next_to_gap_state_only() {
        let text = "let a = json!({\"gap_state\": \"actionable\"});\n\
                    let b = format!(\"gap_state: static_limitation\");\n\
                    let c = gap_state == \"unknown\";\n\
                    let d = \"actionable\";\n\
                    let e = json!({\"gap_state\": state.as_str()});\n\
                    // gap_state \"actionable\" in a comment\n";
        assert_eq!(literal_count(text), 3);
    }

    #[test]
    fn test_modules_are_not_counted() {
        let text = "fn x() {}\n#[cfg(test)]\nmod tests { const A: &str = \"gap_state\"; const B: &str = \"actionable\"; fn t() { let _ = (\"gap_state\", \"actionable\"); } }\n";
        assert_eq!(literal_count(text), 0);
        assert!(!is_scanned_path("crates/ripr/src/lsp/tests.rs"));
        assert!(!is_scanned_path(OWNER_PATH));
        assert!(is_scanned_path("crates/ripr/src/lsp/actions.rs"));
    }

    #[test]
    fn negative_experiment_a_new_literal_fails_and_a_stale_baseline_fails() {
        let baseline = BTreeMap::from([("crates/ripr/src/a.rs".to_string(), 2usize)]);
        let same = BTreeMap::from([("crates/ripr/src/a.rs".to_string(), 2usize)]);
        assert!(ratchet_violations(&same, &baseline).is_empty());

        let grown = BTreeMap::from([("crates/ripr/src/a.rs".to_string(), 3usize)]);
        let failures = ratchet_violations(&grown, &baseline);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("baseline allows 2"));

        let new_file = BTreeMap::from([
            ("crates/ripr/src/a.rs".to_string(), 2usize),
            ("crates/ripr/src/b.rs".to_string(), 1usize),
        ]);
        let failures = ratchet_violations(&new_file, &baseline);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("crates/ripr/src/b.rs") && failures[0].contains("allows 0"));

        let shrunk = BTreeMap::from([("crates/ripr/src/a.rs".to_string(), 1usize)]);
        let failures = ratchet_violations(&shrunk, &baseline);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("only 1 remain"));
    }
}
