//! #8839: test-edit disclosure uses the diff's two source sides, never placement lines.
#![cfg(feature = "lang-rust")]

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PRODUCTION: &str = "pub fn price(points: u32) -> u32 {\n    if points > 10 {\n        20\n    } else {\n        10\n    }\n}\n";
const TEST: &str = "#[test]\nfn above_threshold() {\n    assert_eq!(price(11), 20);\n    assert_eq!(price(20), 20);\n}\n";
const SECOND_TEST: &str = "#[test]\nfn still_above() {\n    assert_eq!(price(12), 20);\n}\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new(inline: bool, edit: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-review-test-changes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root)?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src"))?;
        std::fs::create_dir(fixture.0.join("tests"))?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname=\"pricing_fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n",
        )?;
        let before_test = if edit == "whole_test_delete" {
            format!("{TEST}{SECOND_TEST}")
        } else {
            TEST.to_string()
        };
        fixture.write(inline, PRODUCTION, &before_test, "")?;
        fixture_git_ok(&fixture.0, &["init", "-q"])?;
        fixture_git_ok(&fixture.0, &["config", "core.autocrlf", "false"])?;
        fixture.commit("before")?;
        let test = match edit {
            "delete" | "shifted_delete" => TEST.replace("    assert_eq!(price(20), 20);\n", ""),
            "replace" => TEST.replace("price(20)", "price(21)"),
            "whole_test_delete" => SECOND_TEST.to_string(),
            "test_file_comment" => format!("// changed test-file comment\n{TEST}"),
            _ => TEST.to_string(),
        };
        let shift = if edit.starts_with("shifted_") {
            (0..20)
                .map(|i| format!("// coordinate shift {i}\n"))
                .collect::<String>()
        } else {
            String::new()
        };
        fixture.write(
            inline,
            &PRODUCTION.replace("points > 10", "points >= 10"),
            &test,
            &shift,
        )?;
        fixture.commit("after")?;
        let patch = fixture_git_output(&fixture.0, &["diff", "--unified=0", "HEAD^", "HEAD"])?;
        if edit == "shifted_delete" {
            let removed_hunk = if inline {
                "@@ -15 +34,0 @@"
            } else {
                "@@ -5 +24,0 @@"
            };
            if !patch.contains(removed_hunk) {
                return Err(format!("setup lost old/new coordinate separation: {patch}").into());
            }
        }
        Ok(fixture)
    }

    fn write(
        &self,
        inline: bool,
        production: &str,
        test: &str,
        shift: &str,
    ) -> std::io::Result<()> {
        if inline {
            std::fs::write(
                self.0.join("src/lib.rs"),
                format!(
                    "{production}\n{shift}#[cfg(test)]\nmod tests {{\nuse super::price;\n{test}}}\n"
                ),
            )
        } else {
            std::fs::write(self.0.join("src/lib.rs"), production)?;
            std::fs::write(
                self.0.join("tests/pricing.rs"),
                format!("use pricing_fixture::price;\n{shift}{test}"),
            )
        }
    }

    fn commit(&self, message: &str) -> Result<(), String> {
        fixture_git_ok(&self.0, &["add", "."])?;
        fixture_git_ok(
            &self.0,
            &[
                "-c",
                "user.name=RIPR fixture",
                "-c",
                "user.email=ripr@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                message,
            ],
        )?;
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_resolved_test_attribution(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let warnings = report["warnings"].as_array().ok_or("missing warnings")?;
    assert!(
        !warnings.iter().any(|warning| warning["message"]
            .as_str()
            .is_some_and(|message| message.contains("Test-edit attribution is unresolved"))),
        "fully readable Git fixtures must resolve test-edit attribution: {report}"
    );
    Ok(())
}

#[test]
fn review_comments_discloses_removed_and_inline_test_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let mut failures = Vec::new();
    for (inline, edit, unchanged) in [
        (false, "delete", false),
        (false, "replace", false),
        (false, "whole_test_delete", false),
        (false, "shifted_delete", false),
        (false, "production_only", true),
        (false, "test_file_comment", false),
        (true, "delete", false),
        (true, "replace", false),
        (true, "whole_test_delete", false),
        (true, "production_only", true),
        (true, "shifted_delete", false),
        (true, "shifted_production_only", true),
    ] {
        let fixture = Fixture::new(inline, edit)?;
        let output = fixture.0.join("review.json");
        ripr::cli::run(vec![
            "ripr".to_string(),
            "review-comments".to_string(),
            "--root".to_string(),
            fixture.0.display().to_string(),
            "--base".to_string(),
            "HEAD^".to_string(),
            "--head".to_string(),
            "HEAD".to_string(),
            "--out".to_string(),
            output.display().to_string(),
        ])?;
        let report: Value = serde_json::from_slice(&std::fs::read(output)?)?;
        assert_resolved_test_attribution(&report)?;
        let comments = report["comments"].as_array().ok_or("missing comments")?;
        if comments.len() != 1 {
            return Err(format!("fixture must reach one unresolved recommendation: inline={inline}, edit={edit}: {report}").into());
        }
        let card = comments.first().ok_or("missing recommendation")?;
        let near_test = if edit == "whole_test_delete" {
            "still_above"
        } else {
            "above_threshold"
        };
        if card["kind"] != "predicate_boundary"
            || card["grip_class"] != "weakly_gripped"
            || card["suggested_test"]["near_test"] != near_test
            || card["placement"]["side"] != "RIGHT"
            || card["placement"]["line"] != 2
            || report["summary"]["suppressed"] != 0
        {
            return Err(format!(
                "test edits must preserve recommendation, classification and placement: {report}"
            )
            .into());
        }
        if report["summary"]["unchanged_tests"] != unchanged {
            failures.push(format!(
                "inline={inline}, edit={edit}: unchanged_tests={}, expected={unchanged}",
                report["summary"]["unchanged_tests"]
            ));
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n").into());
    }
    Ok(())
}

#[test]
fn review_comments_preserves_unchanged_tests_for_test_named_production()
-> Result<(), Box<dyn std::error::Error>> {
    let mut failures = Vec::new();
    for inline in [false, true] {
        let fixture = Fixture::new(inline, "production_only")?;
        let production = fixture.0.join("src/pricing_tests.rs");
        std::fs::rename(fixture.0.join("src/lib.rs"), &production)?;
        std::fs::write(
            fixture.0.join("src/lib.rs"),
            "mod pricing_tests; pub use pricing_tests::price;\n",
        )?;
        fixture.commit("test-shaped production baseline")?;
        let source = std::fs::read_to_string(&production)?;
        assert!(
            source.contains("points >= 10"),
            "nonempty production stimulus"
        );
        std::fs::write(&production, source.replace("points >= 10", "points > 10"))?;
        fixture.commit("production-only boundary edit")?;
        let patch = fixture_git_output(&fixture.0, &["diff", "--unified=0", "HEAD^", "HEAD"])?;
        assert!(patch.contains("diff --git a/src/pricing_tests.rs b/src/pricing_tests.rs"));
        assert!(
            !patch.contains("diff --git a/tests/"),
            "test file must remain unchanged"
        );
        let output = fixture.0.join("review.json");
        ripr::cli::run(vec![
            "ripr".to_string(),
            "review-comments".to_string(),
            "--root".to_string(),
            fixture.0.display().to_string(),
            "--base".to_string(),
            "HEAD^".to_string(),
            "--head".to_string(),
            "HEAD".to_string(),
            "--out".to_string(),
            output.display().to_string(),
        ])?;
        let report: Value = serde_json::from_slice(&std::fs::read(output)?)?;
        assert_resolved_test_attribution(&report)?;
        assert_eq!(report["summary"]["suppressed"], 0, "{report}");
        assert_eq!(
            report["summary"]["comments"], 0,
            "legacy placement is unchanged"
        );
        assert_eq!(report["summary"]["summary_only"], 1, "{report}");
        let cards = report["summary_only"]
            .as_array()
            .ok_or("missing summary_only")?;
        let card = cards.first().ok_or("missing retained recommendation")?;
        assert_eq!(card["kind"], "predicate_boundary");
        assert_eq!(card["grip_class"], "weakly_gripped");
        assert_eq!(card["suggested_test"]["near_test"], "above_threshold");
        if report["summary"]["unchanged_tests"] != true {
            failures.push(format!(
                "inline={inline}: expected unchanged tests: {report}"
            ));
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n").into());
    }
    Ok(())
}

#[test]
fn review_comments_marks_whole_file_deletions_unresolved() -> Result<(), Box<dyn std::error::Error>>
{
    for test_file in [true, false] {
        let fixture = Fixture::new(false, "production_only")?;
        let deleted = if test_file {
            std::fs::write(
                fixture.0.join("tests/retained.rs"),
                format!("use pricing_fixture::price;\n{SECOND_TEST}"),
            )?;
            "tests/pricing.rs"
        } else {
            std::fs::write(
                fixture.0.join("src/unused.rs"),
                "pub const UNUSED: u32 = 0;\n",
            )?;
            "src/unused.rs"
        };
        fixture.commit("whole-file deletion baseline")?;
        let production = fixture.0.join("src/lib.rs");
        let source = std::fs::read_to_string(&production)?;
        assert!(
            source.contains("points >= 10"),
            "nonempty production stimulus"
        );
        std::fs::write(&production, source.replace("points >= 10", "points > 10"))?;
        std::fs::remove_file(fixture.0.join(deleted))?;
        fixture.commit("whole-file deletion and production edit")?;
        let patch = fixture_git_output(&fixture.0, &["diff", "--unified=0", "HEAD^", "HEAD"])?;
        assert!(patch.contains(&format!("diff --git a/{deleted} b/{deleted}")));
        assert!(
            patch.contains("+++ /dev/null"),
            "must delete the whole file"
        );
        assert!(patch.contains("diff --git a/src/lib.rs b/src/lib.rs"));

        let output = fixture.0.join("review.json");
        ripr::cli::run(vec![
            "ripr".to_string(),
            "review-comments".to_string(),
            "--root".to_string(),
            fixture.0.display().to_string(),
            "--base".to_string(),
            "HEAD^".to_string(),
            "--head".to_string(),
            "HEAD".to_string(),
            "--out".to_string(),
            output.display().to_string(),
        ])?;
        let report: Value = serde_json::from_slice(&std::fs::read(output)?)?;
        assert_eq!(report["summary"]["unchanged_tests"], false, "{report}");
        let warnings = report["warnings"].as_array().ok_or("missing warnings")?;
        assert!(
            warnings.iter().any(|warning| warning["message"]
                .as_str()
                .is_some_and(|message| message.contains("Test-edit attribution is unresolved"))),
            "the deletion count cannot establish a confirmed test change: {report}"
        );
        let comments = report["comments"].as_array().ok_or("missing comments")?;
        assert_eq!(comments.len(), 1, "retain the recommendation: {report}");
        let card = comments.first().ok_or("missing retained recommendation")?;
        assert_eq!(card["kind"], "predicate_boundary");
        assert_eq!(card["grip_class"], "weakly_gripped");
        assert_eq!(
            card["suggested_test"]["near_test"],
            if test_file {
                "still_above"
            } else {
                "above_threshold"
            }
        );
        assert_eq!(card["placement"]["path"], "src/lib.rs");
        assert_eq!(card["placement"]["side"], "RIGHT");
        assert_eq!(card["placement"]["line"], 2);
        assert_eq!(report["summary"]["suppressed"], 0);
        assert_eq!(report["status"], "advisory");
        assert_eq!(report["analysis_scope"]["downstream_consumable"], true);
        let receipt: Value =
            serde_json::from_slice(&std::fs::read(fixture.0.join("run-receipt.json"))?)?;
        assert_eq!(receipt["status"], "complete");
    }
    Ok(())
}
