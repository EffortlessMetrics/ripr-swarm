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

#[test]
fn pure_short_conjunct_pairing_keeps_current_arms_and_unpaired_history()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_external_workspace("short-conjunct-pairing")?;
    let _cleanup = FixtureCleanup(&root);
    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        let old_chain = "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if m > 0 && n <= 0 => -1,\n        _ => 1,\n    }\n}\n";
        let old_single = "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if n <= 0 => -1,\n        _ => 1,\n    }\n}\n";
        let new_chain = "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if m > 0 && n < 0 => -1,\n        _ => 1,\n    }\n}\n";
        let shifted_single = "// é\npub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if n < 0 => -1,\n        _ => 1,\n    }\n}\n";
        let deleted =
            "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        _ => 1,\n    }\n}\n";
        let sources = [
            ("src/a.rs", old_chain, shifted_single),
            ("src/b.rs", old_single, new_chain),
            ("src/c.rs", old_chain, new_chain),
            ("src/d.rs", old_chain, deleted),
        ];
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"short_conjuncts\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        )?;
        std::fs::write(root.join("src/lib.rs"), "mod a;\nmod b;\nmod c;\nmod d;\n")?;
        for (path, before, _) in sources {
            std::fs::write(root.join(path), before)?;
        }
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
        for (path, _, after) in sources {
            std::fs::write(root.join(path), after)?;
        }
        let patch = fixture_git_output(&root, &["diff", "--unified=0", "HEAD", "--"])?;
        assert!(patch.contains("@@ -0,0 +1 @@"), "{patch}");
        assert!(patch.contains("@@ -3 +4 @@"), "{patch}");
        assert!(patch.contains("@@ -3 +3 @@"), "{patch}");
        assert!(patch.contains("@@ -3 +2,0 @@"), "{patch}");
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--base", "HEAD", "--worktree", "--json"],
        )?;
        assert!(output.status.success(), "{output:?}");
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["summary"]["changed_rust_files"], 4);
        assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        // Pin the whole public vector, so an old-only MatchArm or extra
        // base_deleted duplicate cannot satisfy family presence alone.
        let mut identities = findings
            .iter()
            .map(|finding| {
                (
                    finding["probe"]["file"].as_str(),
                    finding["probe"]["line"].as_u64(),
                    finding["probe"]["family"].as_str(),
                    finding["probe"]["expression"].as_str(),
                    finding["source_currentness"].as_str(),
                )
            })
            .collect::<Vec<_>>();
        identities.sort();
        assert_eq!(
            identities,
            vec![
                (
                    Some("./src/a.rs"),
                    Some(4),
                    Some("match_arm"),
                    Some("n if n < 0 =>"),
                    Some("candidate_current")
                ),
                (
                    Some("./src/a.rs"),
                    Some(4),
                    Some("predicate"),
                    Some("n < 0"),
                    Some("candidate_current")
                ),
                (
                    Some("./src/b.rs"),
                    Some(3),
                    Some("match_arm"),
                    Some("n if m > 0 && n < 0 =>"),
                    Some("candidate_current")
                ),
                (
                    Some("./src/b.rs"),
                    Some(3),
                    Some("predicate"),
                    Some("m > 0 && n < 0"),
                    Some("candidate_current")
                ),
                (
                    Some("./src/c.rs"),
                    Some(3),
                    Some("predicate"),
                    Some("m > 0 && n < 0"),
                    Some("candidate_current")
                ),
                (
                    Some("./src/d.rs"),
                    Some(2),
                    Some("match_arm"),
                    Some("n if m > 0 && n <= 0 => -1,"),
                    Some("base_deleted")
                ),
                (
                    Some("./src/d.rs"),
                    Some(2),
                    Some("predicate"),
                    Some("n if m > 0 && n <= 0 => -1,"),
                    Some("base_deleted")
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
