//! #6485: a changed compound field write (`self.n = self.n + 1` rewritten as
//! `self.n += 1`) in the authored grid of the verdict corpus.
//!
//! - A test that calls the owner once on a local binding and checks nothing
//!   places the unread write (RIPR-SPEC-0228 example 5), even when neighbour
//!   tests in the same module assert on other fields: the finding is a gap
//!   (`weakly_exposed`), not `static_unknown`. Any later use of the binding,
//!   a field read, a macro that may hide an assertion or a second call keeps
//!   the abstention.
//! - A receiver call inside a `for` loop whose pattern does not bind the
//!   receiver reaches the owner through the binding declared around the loop.
//!   A pattern that binds the receiver name refuses. Reach alone credits
//!   nothing: an exact read after a loop that may run zero times still does
//!   not confirm the write.

use ripr::domain::{RelationReason, StageState};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const OWNER: &str = "#[derive(Debug, Default)]\npub struct Stats {\n    pub hits: u32,\n    pub misses: u32,\n}\n\nimpl Stats {\n    pub fn record_hit(&mut self) {\n        self.hits = self.hits + 1;\n    }\n\n    pub fn record_miss(&mut self) {\n        self.misses += 1;\n    }\n}\n";

/// A neighbour test that pins another field exactly, like the grid's
/// `one_hit_counts_one`: it relates to the owner by file only.
const NEIGHBOUR: &str = "    #[test]\n    fn one_hit_counts_one() {\n        let mut stats = Stats::default();\n        stats.record_hit();\n        assert_eq!(stats.hits, 1);\n    }\n";

/// The `field_construction` finding of the changed `self.misses += 1` (base
/// `self.misses = self.misses + 1`) with `tests` inside the owner file's
/// `#[cfg(test)] mod tests`.
fn finding(tests: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-field-write-loop-unread-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    let source = format!("{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{tests}}}\n");
    write("src/lib.rs", &source)?;
    let lines: Vec<&str> = source.lines().collect();
    let changed_line = lines
        .iter()
        .position(|line| line.contains("self.misses += 1;"))
        .ok_or("owner has no `self.misses += 1;` line")?
        + 1;
    let hunk: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index + 1 == changed_line {
                format!(
                    "-{}\n+{line}\n",
                    line.replace("self.misses += 1;", "self.misses = self.misses + 1;")
                )
            } else {
                format!(" {line}\n")
            }
        })
        .collect();
    write(
        "diff.patch",
        &format!(
            "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{n} +1,{n} @@\n{hunk}",
            n = lines.len()
        ),
    )?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.line == changed_line
                && finding.probe.family == ripr::domain::ProbeFamily::FieldConstruction
        })
        .ok_or_else(|| "no field_construction finding on the changed line".to_string())
}

fn relation(found: &Finding, name: &str) -> Option<RelationReason> {
    found
        .related_tests
        .iter()
        .find(|test| test.name == name)
        .and_then(|test| test.relation_reason)
}

fn smoke(lines: &str) -> String {
    format!("{NEIGHBOUR}\n    #[test]\n    fn miss_smoke() {{\n{lines}\n    }}\n")
}

#[test]
fn an_unread_write_beside_asserting_neighbours_is_a_gap() -> Result<(), String> {
    let found = finding(&smoke(
        "        let mut stats = Stats::default();\n        stats.record_miss();",
    ))?;
    assert_eq!(
        relation(&found, "miss_smoke"),
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    assert_eq!(
        relation(&found, "one_hit_counts_one"),
        Some(RelationReason::SameTestFile),
        "{found:#?}"
    );
    assert_eq!(found.class, ExposureClass::WeaklyExposed, "{found:#?}");
    Ok(())
}

#[test]
fn any_other_use_of_an_unread_test_keeps_the_abstention() -> Result<(), String> {
    for (case, tests) in [
        (
            "a helper may assert on the binding",
            smoke(
                "        let mut stats = Stats::default();\n        stats.record_miss();\n        verify(&stats);\n    }\n\n    fn verify(stats: &Stats) {\n        assert_eq!(stats.misses, 1);",
            ),
        ),
        (
            "an unread macro may hide an assertion",
            smoke(
                "        let mut stats = Stats::default();\n        stats.record_miss();\n        check!(stats);",
            ),
        ),
        (
            "a non-assertion check reads the field",
            smoke(
                "        let mut stats = Stats::default();\n        stats.record_miss();\n        let n = stats.misses;\n        if n != 1 {\n            panic!(\"wrong\");\n        }",
            ),
        ),
        (
            "a format capture reads the binding",
            smoke(
                "        let mut stats = Stats::default();\n        stats.record_miss();\n        println!(\"{stats:?}\");",
            ),
        ),
        (
            "the owner runs twice",
            smoke(
                "        let mut stats = Stats::default();\n        stats.record_miss();\n        stats.record_miss();",
            ),
        ),
    ] {
        let found = finding(&tests).map_err(|error| format!("{case}: {error}"))?;
        assert_eq!(
            found.class,
            ExposureClass::StaticUnknown,
            "{case}: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn a_receiver_call_in_a_for_loop_reaches_the_owner() -> Result<(), String> {
    let found = finding(&smoke(
        "        let mut stats = Stats::default();\n        for _ in 0..3 {\n            stats.record_miss();\n        }",
    ))?;
    assert_eq!(
        relation(&found, "miss_smoke"),
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    assert_eq!(found.ripr.reach.state, StageState::Yes, "{found:#?}");
    Ok(())
}

#[test]
fn a_for_pattern_that_binds_the_receiver_does_not_reach_the_owner() -> Result<(), String> {
    let found = finding(&smoke(
        "        let mut stats = Stats::default();\n        for mut stats in [Stats::default()] {\n            stats.record_miss();\n        }",
    ))?;
    assert_ne!(
        relation(&found, "miss_smoke"),
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn an_exact_read_after_a_loop_that_may_not_run_does_not_credit() -> Result<(), String> {
    // The table rows include one where the loop runs zero times; ripr does
    // not evaluate the rows, so the read does not confirm the write (the
    // grid's `grid-delete-table` cell stays abstained).
    let found = finding(&format!(
        "{NEIGHBOUR}\n    #[test]\n    fn miss_table() {{\n        for (times, want) in [(0, 0), (1, 1), (3, 3)] {{\n            let mut stats = Stats::default();\n            for _ in 0..times {{\n                stats.record_miss();\n            }}\n            assert_eq!(stats.misses, want, \"{{times}} misses\");\n        }}\n    }}\n"
    ))?;
    assert_eq!(
        relation(&found, "miss_table"),
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}
