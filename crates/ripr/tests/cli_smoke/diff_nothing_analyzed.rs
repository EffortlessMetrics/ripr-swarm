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
const BINARY_ARTIFACT: &str = "diff --git a/target/ripr/cache/x.rs b/target/ripr/cache/x.rs\nindex 8888888..9999999 100644\nBinary files a/target/ripr/cache/x.rs and b/target/ripr/cache/x.rs differ\n";
const BINARY_QUOTED: &str = "diff --git \"a/src/caf\\303\\251.rs\" \"b/src/caf\\303\\251.rs\"\nindex 4444444..5555555 100644\nBinary files \"a/src/caf\\303\\251.rs\" and \"b/src/caf\\303\\251.rs\" differ\n";
const BINARY_COPY: &str = "diff --git a/src/lib.rs b/src/fee.rs\nsimilarity index 80%\ncopy from src/lib.rs\ncopy to src/fee.rs\nindex 4444444..5555555 100644\nBinary files a/src/lib.rs and b/src/fee.rs differ\n";
const BINARY_NOPREFIX: &str = "diff --git src/tax.rs src/tax.rs\nindex 4444444..5555555 100644\nBinary files src/tax.rs and src/tax.rs differ\n";
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
/// file, the cause and the recovery. A binary asset, a binary source outside
/// the requested scope (matched by its decoded name), or one under ripr's own
/// artifact root changes nothing.
#[test]
fn a_source_file_rendered_as_binary_makes_the_outcome_incomplete() -> TestResult {
    let limitation_for = |name: &str, sections: &[&str]| -> TestResult<serde_json::Value> {
        let (root, diff) = intake_workspace(name, sections)?;
        let _cleanup = DeclaredHunkScratch(root.clone());
        let mut args = vec!["check", "--diff", &diff, "--format", "json", "--quiet"];
        // The scope control asks only for lib.rs, so tax.rs was not requested.
        if name.ends_with("source-scope-control") {
            args.extend(["--path", "src/lib.rs"]);
        }
        // The decoded name is what an exact exclude matches.
        if name.ends_with("quoted-exclude-control") {
            args.extend(["--exclude", "src/café.rs"]);
        }
        if name.ends_with("noprefix-exclude-control") {
            args.extend(["--exclude", "src/tax.rs"]);
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

    let quoted = limitation_for("diff-binary-quoted-source", &[LIB_COMMENT, BINARY_QUOTED])?;
    assert_eq!(
        quoted["analysis_outcome"]["analysis_complete"], false,
        "{quoted}"
    );
    assert!(
        quoted.to_string().contains("not analyzed: src/café.rs"),
        "{quoted}"
    );

    // #15665: a `--no-prefix` binary section is named by its whole path.
    let noprefix = limitation_for(
        "diff-binary-noprefix-source",
        &[LIB_COMMENT, BINARY_NOPREFIX],
    )?;
    assert_eq!(
        noprefix["analysis_outcome"]["analysis_complete"], false,
        "{noprefix}"
    );
    assert!(
        noprefix.to_string().contains("not analyzed: src/tax.rs"),
        "{noprefix}"
    );

    for (name, sections) in [
        ("diff-binary-asset-control", &[LIB_COMMENT, BINARY_LOGO][..]),
        ("diff-text-only-control", &[LIB_COMMENT][..]),
        (
            "diff-binary-source-scope-control",
            &[LIB_COMMENT, BINARY_TAX][..],
        ),
        (
            "diff-binary-artifact-control",
            &[LIB_COMMENT, BINARY_ARTIFACT][..],
        ),
        (
            "diff-binary-quoted-exclude-control",
            &[LIB_COMMENT, BINARY_QUOTED][..],
        ),
        (
            "diff-binary-noprefix-exclude-control",
            &[LIB_COMMENT, BINARY_NOPREFIX][..],
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

/// #15324: a binary-rendered copy destination is read whole as added source
/// (#15121), so it is analyzed and is not named as unread.
#[test]
fn a_binary_rendered_copy_read_as_added_source_is_not_named_unread() -> TestResult {
    let (root, diff) = intake_workspace("diff-binary-copy", &[LIB_COMMENT, BINARY_COPY])?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    fs::write(
        root.join("src/lib.rs"),
        "// note\npub mod fee;\npub fn keep(x: u32) -> u32 {\n    x + 1\n}\n",
    )?;
    fs::write(
        root.join("src/fee.rs"),
        "pub fn fee(q: u32) -> u32 {\n    q * 2\n}\n",
    )?;
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["check", "--diff", &diff, "--format", "json", "--quiet"],
    )?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    // Pin the stimulus: the copy destination was read as an added Rust file.
    assert_eq!(json["summary"]["changed_rust_files"], 2, "{json}");
    assert!(
        !json.to_string().contains("rendered by git as binary"),
        "{json}"
    );
    Ok(())
}

// Real `git diff --binary` output: a `chmod +x` and a changed PNG (#15862).
const MODE_ONLY_LIB: &str =
    "diff --git a/src/lib.rs b/src/lib.rs\nold mode 100644\nnew mode 100755\n";
const BINARY_PATCH_LOGO: &str = "diff --git a/logo.png b/logo.png\nindex 8352675d67aed6625ece79af41c27fdb4ee2e867..1592e5c60f1a460928916dc5681fee1a9bd10868 100644\nGIT binary patch\nliteral 3\nKcmZQzWCj2L2ml2D\n\nliteral 3\nKcmZQzWC8#H2LJ>B\n\n";
// Lookalikes with the same valid header: not a Git mode, and a bare marker.
const INVALID_MODE_LIB: &str =
    "diff --git a/src/lib.rs b/src/lib.rs\nold mode invalid\nnew mode invalid\n";
const BARE_BINARY_MARKER: &str = "diff --git a/logo.png b/logo.png\nGIT binary patch\nliteral 0\n";

/// #15862: a well-formed binary- or mode-only diff keeps the `malformed_diff`
/// schema token, but the human limitation line must not call it unparseable
/// next to the detail that says it is well formed. Lookalikes with an invalid
/// mode or a bare binary marker, and garbage input, keep the parse label.
#[test]
fn a_mode_only_diff_is_not_labelled_unparseable() -> TestResult {
    let run = |name: &str, sections: &[&str]| -> TestResult<(String, String, serde_json::Value)> {
        let (root, diff) = intake_workspace(name, sections)?;
        let _cleanup = DeclaredHunkScratch(root.clone());
        let human = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--diff", &diff],
        )?;
        let json = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--diff", &diff, "--format", "json"],
        )?;
        let json: serde_json::Value = serde_json::from_slice(&json.stdout)?;
        // Every case carries the same schema token.
        let limitations = &json["analysis_outcome"]["outcome"]["limitations"];
        assert_eq!(limitations[0]["kind"], "malformed_diff", "{name}: {json}");
        Ok((
            String::from_utf8_lossy(&human.stdout).into_owned(),
            String::from_utf8_lossy(&human.stderr).into_owned(),
            json,
        ))
    };
    for (name, section) in [
        ("diff-mode-only-label", MODE_ONLY_LIB),
        ("diff-binary-patch-label", BINARY_PATCH_LOGO),
    ] {
        let (human, stderr, json) = run(name, &[section])?;
        let detail = &json["analysis_outcome"]["outcome"]["limitations"][0]["bounded_detail"];
        assert!(
            detail
                .as_str()
                .is_some_and(|detail| detail.contains("well formed")),
            "{name}: {json}"
        );
        assert!(
            human.contains("Limitation: the diff has only binary or file-mode changes during"),
            "{name}: {human}"
        );
        assert!(!human.contains("could not be parsed"), "{name}: {human}");
        assert!(
            stderr.contains("only binary files or file modes"),
            "{name}: {stderr}"
        );
        assert!(
            !stderr.contains("not a valid unified diff"),
            "{name}: {stderr}"
        );
    }
    for (name, section) in [
        ("diff-invalid-mode-label", INVALID_MODE_LIB),
        ("diff-bare-binary-marker-label", BARE_BINARY_MARKER),
        ("diff-garbage-label-control", "this is not a diff\n"),
    ] {
        let (human, stderr, json) = run(name, &[section])?;
        assert!(
            human.contains("Limitation: the diff could not be parsed during"),
            "{name}: {human}"
        );
        assert!(!human.contains("binary or file-mode"), "{name}: {human}");
        assert!(!json.to_string().contains("well formed"), "{name}: {json}");
        assert!(
            !stderr.contains("only binary files or file modes"),
            "{name}: {stderr}"
        );
    }
    Ok(())
}
