//! A non-empty unparseable `--diff` maps to `unsupported_input`, so the agent
//! and github formats take the incomplete-analysis path and never report a
//! clean result (#13658). The empty-diff `no_scope` case is pinned elsewhere.
use super::{DeclaredHunkScratch, run_command, unique_external_workspace};
use std::fs;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn unparseable_diff_routes_to_incomplete_analysis_not_a_clean_result() -> TestResult {
    let root = unique_external_workspace("diff-unparseable-routing")?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    fs::create_dir_all(&root)?;
    let diff = root.join("garbage.diff");
    fs::write(&diff, "this is not a diff\n")?;
    let diff = diff.to_string_lossy().into_owned();
    let agent = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["check", "--diff", &diff, "--format", "agent", "--quiet"],
    )?;
    let agent = String::from_utf8_lossy(&agent.stdout).into_owned();
    assert!(agent.contains("outcome=unsupported_input"), "{agent}");
    assert!(agent.contains("\nstop=analysis incomplete"), "{agent}");
    assert!(!agent.contains("done="), "{agent}");
    assert!(!agent.contains("verify=complete"), "{agent}");
    let github = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["check", "--diff", &diff, "--format", "github", "--quiet"],
    )?;
    let github = String::from_utf8_lossy(&github.stdout).into_owned();
    assert!(
        github.contains("::warning title=ripr analysis incomplete::"),
        "{github}"
    );
    assert!(
        !github.contains("No static exposure findings found"),
        "{github}"
    );
    Ok(())
}

/// A real docs-only git-style diff parses to one changed file and no analyzable
/// source, so the producer reports a complete-but-empty outcome. The agent and
/// github formats must say nothing was analyzed instead of reporting a clean
/// result (#13658).
#[test]
fn docs_only_diff_is_not_reported_clean_on_stdout() -> TestResult {
    let root = unique_external_workspace("diff-docs-only")?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    fs::create_dir_all(&root)?;
    let diff = root.join("docs.diff");
    fs::write(
        &diff,
        "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n # Title\n+More words.\n",
    )?;
    let diff = diff.to_string_lossy().into_owned();
    let run = |format: &str| -> TestResult<String> {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--diff", &diff, "--format", format, "--quiet"],
        )?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };

    // Pin the stimulus first: one parsed changed file, no analyzable source.
    let json: serde_json::Value = serde_json::from_str(&run("json")?)?;
    let outcome = &json["analysis_outcome"]["outcome"];
    assert!(
        matches!(
            outcome["kind"].as_str(),
            Some("no_changed_lines" | "no_behavioral_candidates")
        ),
        "{outcome}"
    );
    assert_eq!(outcome["counts"]["changed_file_count"], 1, "{outcome}");
    assert_eq!(json["summary"]["changed_rust_files"], 0, "{json}");

    let agent = run("agent")?;
    assert!(
        agent.contains("\nstop=nothing was analyzed (no changed source in scope)"),
        "{agent}"
    );
    assert!(!agent.contains("done="), "{agent}");
    assert!(!agent.contains("verify=complete"), "{agent}");
    let github = run("github")?;
    assert!(
        github.contains("::warning title=ripr nothing analyzed::"),
        "{github}"
    );
    assert!(
        !github.contains("No static exposure findings found"),
        "{github}"
    );
    Ok(())
}

/// Writes a one-crate workspace whose `src/lib.rs` matches the new side of
/// `lib_section`, and the patch built from `sections`.
fn intake_workspace(name: &str, sections: &[&str]) -> TestResult<(std::path::PathBuf, String)> {
    let root = unique_external_workspace(name)?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"intake\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    fs::write(
        root.join("src/lib.rs"),
        "// note\npub fn keep(x: u32) -> u32 {\n    x + 1\n}\n",
    )?;
    let diff = root.join("intake.diff");
    fs::write(&diff, sections.concat())?;
    let diff = diff.to_string_lossy().into_owned();
    Ok((root, diff))
}

const LIB_COMMENT: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -0,0 +1 @@\n+// note\n";
const DELETED_PRICING: &str = "diff --git a/src/pricing.rs b/src/pricing.rs\ndeleted file mode 100644\nindex 3333333..0000000\n--- a/src/pricing.rs\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-pub fn price(qty: u32) -> u32 {\n-    qty * 9\n-}\n";
const BINARY_TAX: &str = "diff --git a/src/tax.rs b/src/tax.rs\nindex 4444444..5555555 100644\nBinary files a/src/tax.rs and b/src/tax.rs differ\n";
const BINARY_LOGO: &str = "diff --git a/logo.png b/logo.png\nindex 6666666..7777777 100644\nBinary files a/logo.png and b/logo.png differ\n";

/// #15324: a deleted source file beside another changed file is named on
/// stderr, and the run still reads complete over the other change.
#[test]
fn a_deleted_file_beside_another_change_is_named() -> TestResult {
    let (root, diff) = intake_workspace(
        "diff-deleted-beside-change",
        &[LIB_COMMENT, DELETED_PRICING],
    )?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["check", "--diff", &diff, "--format", "json"],
    )?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let outcome = &json["analysis_outcome"]["outcome"];
    // Pin the stimulus: lib.rs is the one parsed changed file.
    assert_eq!(outcome["counts"]["changed_file_count"], 1, "{outcome}");
    assert_eq!(json["summary"]["changed_rust_files"], 1, "{json}");
    assert_eq!(
        json["analysis_outcome"]["analysis_complete"], true,
        "{json}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "ripr: diff deleted 1 file(s) (src/pricing.rs); deleted files produce no probes \
             (deleted behavior has no new-side code to analyze), so this result covers only \
             the other changed files."
        ),
        "{stderr}"
    );
    let (control_root, control) = intake_workspace("diff-no-deletion-control", &[LIB_COMMENT])?;
    let _control_cleanup = DeclaredHunkScratch(control_root.clone());
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&control_root),
        &["check", "--diff", &control, "--format", "json"],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("diff deleted"), "{stderr}");
    Ok(())
}

/// #15324: a source file git printed as binary (a `-diff` attribute) is not
/// analyzed, so the outcome is incomplete with a typed limitation naming the
/// file, the cause and the recovery. A binary asset, or a binary source
/// outside the requested `--path` scope, changes nothing.
#[test]
fn a_source_file_rendered_as_binary_makes_the_outcome_incomplete() -> TestResult {
    let limitation_for = |name: &str, sections: &[&str]| -> TestResult<serde_json::Value> {
        let (root, diff) = intake_workspace(name, sections)?;
        let _cleanup = DeclaredHunkScratch(root.clone());
        let mut args = vec!["check", "--diff", &diff, "--format", "json", "--quiet"];
        // The scope control asks only for lib.rs, so tax.rs was not requested.
        if name.ends_with("scope-control") {
            args.extend(["--path", "src/lib.rs"]);
        }
        let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&root), &args)?;
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        // Pin the stimulus: the text hunk on lib.rs parsed.
        assert_eq!(json["summary"]["changed_rust_files"], 1, "{json}");
        Ok(json)
    };

    let json = limitation_for("diff-binary-source", &[LIB_COMMENT, BINARY_TAX])?;
    assert_eq!(
        json["analysis_outcome"]["analysis_complete"], false,
        "{json}"
    );
    let limitations = json["analysis_outcome"]["outcome"]["limitations"]
        .as_array()
        .ok_or("limitations missing")?;
    let limitation = limitations
        .iter()
        .find(|item| item["kind"] == "language_scope_unsupported")
        .ok_or("a binary-rendered source file must carry a typed limitation")?;
    assert_eq!(limitation["producer_stage"], "diff_parse", "{limitation}");
    assert_eq!(limitation["affected_items"], 1, "{limitation}");
    assert!(
        limitation["bounded_detail"]
            .as_str()
            .is_some_and(
                |text| text.contains("rendered by git as binary") && text.ends_with("src/tax.rs")
            ),
        "{limitation}"
    );
    assert!(
        limitation["recovery"]["detail"]
            .as_str()
            .is_some_and(|text| text.contains("`-diff` or `binary` attribute")
                && text.contains("git diff --text")),
        "{limitation}"
    );

    for (name, sections) in [
        ("diff-binary-asset-control", &[LIB_COMMENT, BINARY_LOGO][..]),
        ("diff-text-only-control", &[LIB_COMMENT][..]),
        (
            "diff-binary-source-scope-control",
            &[LIB_COMMENT, BINARY_TAX][..],
        ),
    ] {
        let json = limitation_for(name, sections)?;
        assert_eq!(
            json["analysis_outcome"]["analysis_complete"], true,
            "{name}: {json}"
        );
    }
    Ok(())
}
