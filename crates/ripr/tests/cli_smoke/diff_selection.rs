//! Real Git input must keep the selected source's native filename and live text.
use super::{run_command, run_git, unique_external_workspace};
use serde_json::Value;
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const BEFORE: &str =
    "pub fn changed(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}\n";
const STAGED: &str =
    "pub fn changed(amount: i32, threshold: i32) -> bool {\n    amount <= threshold\n}\n";
const AFTER: &str =
    "pub fn changed(amount: i32, threshold: i32) -> bool {\n    amount > threshold\n}\n";

fn check(cwd: &Path, scope: &[&str]) -> TestResult<Value> {
    let mut args = vec!["check", "--base", "HEAD", "--mode", "fast", "--json"];
    args.extend_from_slice(scope);
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(cwd), &args)?;
    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn git_named_control_paths_select_the_live_predicate_from_root_and_subdirectory() -> TestResult {
    for (escape, control) in [('a', '\x07'), ('b', '\x08'), ('v', '\x0b'), ('f', '\x0c')] {
        let root = unique_external_workspace(&format!("diff-selection-{escape} 'quoted'"))?;
        fs::create_dir_all(root.join("src/nested directory"))?;
        let path = format!("src/byte{control}.rs");
        let mimic = format!("src/byte{escape}.rs");
        fs::write(root.join(&path), BEFORE)?;
        fs::write(root.join(&mimic), BEFORE.replace("changed", "decoy"))?;
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname=\"diff_selection_{escape}\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\npath={}\n",
                serde_json::to_string(&path)?
            ),
        )?;
        run_git(&root, &["init", "-b", "main"])?;
        run_git(&root, &["config", "user.name", "Fixture"])?;
        run_git(&root, &["config", "user.email", "fixture@example.invalid"])?;
        run_git(&root, &["add", "."])?;
        run_git(&root, &["commit", "-m", "baseline"])?;
        fs::write(root.join(&path), STAGED)?;
        run_git(&root, &["add", "--", &path])?;
        fs::write(root.join(&path), AFTER)?;
        fs::write(
            root.join("src/untracked.rs"),
            AFTER.replace("changed", "untracked"),
        )?;

        // NUL inventories and real producer bytes establish fixture subjects
        // independently of RIPR's path decoder. The index differs from both sides.
        for args in [
            vec!["diff", "--name-only", "-z", "HEAD", "--"],
            vec!["diff", "--cached", "--name-only", "-z", "--"],
            vec!["diff", "--name-only", "-z", "--"],
        ] {
            let inventory = run_command("git", Some(&root), &args)?;
            assert!(inventory.status.success());
            assert_eq!(inventory.stdout, format!("{path}\0").as_bytes());
        }
        let patch = run_command(
            "git",
            Some(&root),
            &[
                "-c",
                "core.quotePath=true",
                "diff",
                "--unified=0",
                "HEAD",
                "--",
            ],
        )?;
        assert!(patch.status.success());
        assert!(String::from_utf8(patch.stdout)?.contains(&format!("byte\\{escape}.rs")));

        let explicit = check(&root, &["--worktree"])?;
        let implicit = check(&root.join("src/nested directory"), &[])?;
        for report in [&explicit, &implicit] {
            let findings = report["findings"]
                .as_array()
                .ok_or("missing findings array")?;
            // A changed returned comparison seeds both families on ordinary
            // filenames too. Pin that contract without accepting duplicate probes.
            let mut families = findings
                .iter()
                .map(|finding| finding["probe"]["family"].as_str())
                .collect::<Vec<_>>();
            families.sort_unstable();
            assert_eq!(
                families,
                vec![Some("predicate"), Some("return_value")],
                "{escape}: {report}"
            );
            for finding in findings {
                assert_eq!(finding["probe"]["file"], format!("./{path}"));
                assert_eq!(finding["probe"]["line"], 2);
                assert_eq!(finding["probe"]["expression"], "amount > threshold");
                assert_eq!(finding["canonical_gap"]["owner"], "changed");
            }
            assert_eq!(report["head"]["source"], "working_tree");
            assert_eq!(report["summary"]["changed_rust_files"], 1);
            assert_eq!(
                report["untracked_working_tree_source_paths"],
                serde_json::json!(["src/untracked.rs"])
            );
            assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
        }
        assert_eq!(implicit["findings"], explicit["findings"]);
        let committed = check(&root, &["--committed"])?;
        assert_eq!(committed["findings"], serde_json::json!([]));
        assert_eq!(committed["head"]["source"], "commit");
        fs::remove_dir_all(root)?;
    }
    Ok(())
}
