//! #7807: real multi-file Git changes must not count replacements as deletions.
use super::common::fixture_git::fixture_git_output;
use super::{run_command, run_git, unique_external_workspace};
use serde_json::Value;
use std::path::Path;

struct FixtureCleanup<'a>(&'a Path);

impl Drop for FixtureCleanup<'_> {
    fn drop(&mut self) {
        // Normal completion checks cleanup errors below; unwinding remains bounded.
        let _ = std::fs::remove_dir_all(self.0);
    }
}

#[test]
fn short_predicate_pairing_is_file_local_and_uses_shifted_new_coordinates()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_external_workspace("short-predicate-pairing")?;
    let _cleanup = FixtureCleanup(&root);
    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"short_pairing\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        )?;
        std::fs::write(root.join("src/lib.rs"), "mod a;\nmod b;\n")?;
        std::fs::write(
            root.join("src/a.rs"),
            "pub fn f(x: i32) -> bool {\n    if x > 0 {\n        true\n    } else {\n        false\n    }\n}\n",
        )?;
        std::fs::write(
            root.join("src/b.rs"),
            "pub fn g(n: i32) -> bool {\n    if n < 0 {\n        true\n    } else {\n        false\n    }\n}\n",
        )?;
        run_git(&root, &["init", "-q"])?;
        run_git(&root, &["config", "core.autocrlf", "false"])?;
        run_git(&root, &["add", "."])?;
        run_git(
            &root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "before",
            ],
        )?;
        std::fs::write(
            root.join("src/a.rs"),
            "// shifted é\npub fn f(x: i32) -> bool {\n    if x >= 0 {\n        true\n    } else {\n        false\n    }\n}\n",
        )?;
        std::fs::write(
            root.join("src/b.rs"),
            "pub fn g(n: i32) -> bool {\n    if n <= 0 {\n        true\n    } else {\n        false\n    }\n}\n",
        )?;
        let patch = fixture_git_output(&root, &["diff", "--unified=0", "HEAD", "--"])?;
        assert!(patch.contains("@@ -2 +3 @@"), "{patch}");
        assert!(patch.contains("@@ -2 +2 @@"), "{patch}");
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--base", "HEAD", "--worktree", "--json"],
        )?;
        assert!(output.status.success(), "{output:?}");
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["summary"]["changed_rust_files"], 2);
        assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        let predicates = findings
            .iter()
            .filter(|finding| finding["probe"]["family"] == "predicate")
            .map(|finding| {
                (
                    finding["probe"]["file"].as_str(),
                    finding["probe"]["line"].as_u64(),
                    finding["probe"]["expression"].as_str(),
                    finding["source_currentness"].as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            predicates,
            vec![
                (
                    Some("./src/a.rs"),
                    Some(3),
                    Some("x >= 0"),
                    Some("candidate_current")
                ),
                (
                    Some("./src/b.rs"),
                    Some(2),
                    Some("n <= 0"),
                    Some("candidate_current")
                ),
            ],
            "{report}"
        );
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root);
    result?;
    cleanup?;
    Ok(())
}
