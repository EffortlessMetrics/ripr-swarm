//! A refused export-star sibling cannot turn ambiguity into positive credit.

use super::{run_command_with_env, unique_temp_workspace, workspace_root};
use serde_json::Value;
use std::{fs, path::PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Subject(PathBuf);

impl Subject {
    fn new() -> TestResult<Self> {
        let fixture = workspace_root().join("fixtures/typescript_reexport_unparsed_star");
        let root = unique_temp_workspace("typescript-partial-star");
        fs::create_dir_all(std::env::temp_dir())?;
        fs::create_dir(&root)?;
        let subject = Self(root);
        fs::create_dir_all(subject.0.join("src"))?;
        fs::create_dir_all(subject.0.join("tests"))?;
        for path in [
            "ripr.toml",
            "src/price.ts",
            "src/barrel.ts",
            "src/other.ts",
            "tests/price.test.ts",
        ] {
            fs::copy(fixture.join("input").join(path), subject.0.join(path))?;
        }
        fs::copy(fixture.join("diff.patch"), subject.0.join("change.diff"))?;
        Ok(subject)
    }

    fn check(&self, cap: &str, class: &str, related: u64, partial: bool) -> TestResult {
        let output = run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &self.0,
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "change.diff",
                "--mode",
                "fast",
                "--format",
                "json",
            ],
            &[("RIPR_TS_MAX_FILE_READ_BYTES", cap)],
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "nonempty changed owner: {report}");
        let finding = &findings[0];
        assert_eq!(finding["probe"]["file"], "src/price.ts");
        assert_eq!(finding["probe"]["line"], 2);
        assert_eq!(finding["probe"]["family"], "return_value");
        assert_eq!(finding["classification"], class, "{finding}");
        assert_eq!(finding["related_tests_total"], related, "{finding}");
        if partial {
            assert_eq!(report["analysis_outcome"]["analysis_complete"], false);
            let limitations = report["analysis_outcome"]["outcome"]["limitations"]
                .as_array()
                .ok_or("missing limitations")?;
            assert!(
                limitations.iter().any(|limit| limit["bounded_detail"]
                    .as_str()
                    .is_some_and(|detail| detail.contains("file_read_capped"))),
                "{report}"
            );
        }
        Ok(())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn typescript_partial_star_index_never_retains_positive_credit() -> TestResult {
    let subject = Subject::new()?;
    // A known-empty sibling permits the only binding. Losing its facts must
    // withhold that credit rather than infer it still has no competing export.
    let padding = format!("//{}\n", "x".repeat(1024));
    fs::write(
        subject.0.join("src/other.ts"),
        format!("{padding}export const other = 1;\n"),
    )?;
    subject.check("4096", "exposed", 1, false)?;
    subject.check("512", "no_static_path", 0, true)?;
    // With a competing binding the complete index already refuses credit;
    // reducing the byte cap must never improve that verdict.
    fs::write(
        subject.0.join("src/other.ts"),
        format!("{padding}export function price(value) {{ return value + 10; }}\n"),
    )?;
    subject.check("4096", "no_static_path", 0, false)?;
    subject.check("512", "no_static_path", 0, true)?;
    // Parse failures also withhold absence evidence, and restoring a valid
    // empty module recovers the supported positive case.
    fs::write(subject.0.join("src/other.ts"), "export function broken(")?;
    subject.check("4096", "no_static_path", 0, false)?;
    fs::write(subject.0.join("src/other.ts"), "export const other = 1;\n")?;
    subject.check("4096", "exposed", 1, false)
}
