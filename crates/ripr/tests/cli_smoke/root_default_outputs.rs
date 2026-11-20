//! #15378: `gate check --root <member>` without destinations writes its
//! guidance and decision under the member, where `reports ci-summary --root`
//! and the gate report's own hints read them.

use super::{run_command, unique_temp_workspace};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn git(root: &std::path::Path, args: &[&str]) -> TestResult {
    let mut all = vec![
        "-c",
        "user.name=RIPR test",
        "-c",
        "user.email=ripr@example.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    all.extend_from_slice(args);
    let output = run_command("git", Some(root), &all)?;
    assert!(output.status.success(), "git {args:?}: {output:?}");
    Ok(())
}

#[test]
fn gate_check_with_a_member_root_writes_reports_where_ci_summary_reads() -> TestResult {
    let repo = unique_temp_workspace("root-default-outputs");
    let member = repo.join("crates/itoa");
    std::fs::create_dir_all(member.join("src"))?;
    let result: TestResult = (|| {
        std::fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"itoa\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        std::fs::write(
            member.join("src/lib.rs"),
            "pub fn is_large(n: u32) -> bool {\n    n > 10\n}\n",
        )?;
        git(&repo, &["init", "-q", "--initial-branch=main"])?;
        git(&repo, &["add", "."])?;
        git(&repo, &["commit", "-q", "-m", "base"])?;
        std::fs::write(
            member.join("src/lib.rs"),
            "pub fn is_large(n: u32) -> bool {\n    n >= 10\n}\n",
        )?;
        git(&repo, &["commit", "-q", "-am", "change"])?;

        let ripr = env!("CARGO_BIN_EXE_ripr");
        let summary_args = ["reports", "ci-summary", "--root", "crates/itoa"];
        let before = run_command(ripr, Some(&repo), &summary_args)?;
        let before_text = String::from_utf8(before.stdout)?;
        assert!(
            !before_text.contains("changed-behavior spot"),
            "control: a member with no reports has no findings: {before_text}"
        );

        let gate = run_command(
            ripr,
            Some(&repo),
            &[
                "gate",
                "check",
                "--root",
                "crates/itoa",
                "--base",
                "HEAD~1",
                "--mode",
                "visible-only",
            ],
        )?;
        assert!(gate.status.success(), "{gate:?}");
        for report in [
            "target/ripr/review/comments.json",
            "target/ripr/reports/gate-decision.json",
            "target/ripr/reports/gate-decision.md",
        ] {
            assert!(
                member.join(report).is_file(),
                "missing under member: {report}"
            );
            assert!(!repo.join(report).exists(), "written under cwd: {report}");
        }

        // A relative root outside the invoking directory: the defaulted
        // guidance is guarded by the root's own containment, not refused as
        // an escaping `--guidance-out` the user never passed.
        let sibling = repo.join("crates/other");
        std::fs::create_dir_all(&sibling)?;
        let from_sibling = run_command(
            ripr,
            Some(&sibling),
            &[
                "gate",
                "check",
                "--root",
                "../itoa",
                "--base",
                "HEAD~1",
                "--mode",
                "visible-only",
            ],
        )?;
        assert!(from_sibling.status.success(), "{from_sibling:?}");
        assert!(!sibling.join("target").exists(), "written under cwd");

        let after = run_command(ripr, Some(&repo), &summary_args)?;
        let after_text = String::from_utf8(after.stdout)?;
        assert!(
            after_text.contains("changed-behavior spot"),
            "ci-summary --root must read the member's gate reports: {after_text}"
        );
        Ok(())
    })();
    std::fs::remove_dir_all(&repo)?;
    result
}
