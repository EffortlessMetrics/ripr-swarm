//! Structural package workspace markers shared by CLI and TypeScript discovery.
use super::{run_command_with_env, run_git, unique_external_workspace};
use serde_json::Value;
use std::fs;
#[cfg(feature = "lang-rust")]
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const SCALAR_MARKERS: &[&str] = &["null", "false", "true", "42", r#""packages/*""#];
// Discovery recognizes containers, not complete package-manager schemas.
const STRUCTURAL_MARKERS: &[&str] = &[
    "[]",
    r#"["packages/*"]"#,
    r#"{"packages":[],"nohoist":["**/example"]}"#,
    r#"{"packages":["packages/*"]}"#,
    "{}",
    "[42]",
    r#"{"packages":false}"#,
];

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_workspace_evidence_uses_structural_markers_and_package_fallback() -> TestResult {
    for (workspaces, structural) in SCALAR_MARKERS
        .iter()
        .map(|value| (*value, false))
        .chain(STRUCTURAL_MARKERS.iter().map(|value| (*value, true)))
    {
        let sandbox = unique_external_workspace("typescript-workspace-marker")?;
        let repo = sandbox.join("repo");
        let member = repo.join("packages/member");
        let cwd = member.join("src");
        fs::create_dir_all(&cwd)?;
        run_git(&repo, &["init", "-b", "main"])?;
        fs::write(
            repo.join("pnpm-workspace.yaml"),
            "packages: ['packages/*']\n",
        )?;
        fs::write(
            repo.join("ripr.toml"),
            "[languages]\nenabled = [\"typescript\"]\n",
        )?;
        fs::write(
            member.join("package.json"),
            format!(r#"{{"name":"member","workspaces":{workspaces}}}"#),
        )?;
        fs::write(
            cwd.join("price.ts"),
            "export function price(amount: number): number { return amount + 1; }\n",
        )?;
        let diff = sandbox.join("change.diff");
        fs::write(
            &diff,
            "--- a/packages/member/src/price.ts\n+++ b/packages/member/src/price.ts\n@@ -1 +1 @@\n-export function price(amount: number): number { return amount + 0; }\n+export function price(amount: number): number { return amount + 1; }\n",
        )?;
        let check = |expected_workspace: &str| -> TestResult {
            // Pin the outer analysis root so the CLI resolver cannot mask a
            // different package-discovery decision in the evidence consumer.
            let output = run_command_with_env(
                env!("CARGO_BIN_EXE_ripr"),
                &cwd,
                &[
                    "check",
                    "--root",
                    repo.to_str().ok_or("root must be UTF-8")?,
                    "--diff",
                    diff.to_str().ok_or("diff must be UTF-8")?,
                    "--format",
                    "json",
                ],
                &[("RIPR_CACHE_DIR", "")],
            )?;
            assert!(output.status.success(), "{output:?}");
            let report: Value = serde_json::from_slice(&output.stdout)?;
            let findings = report["findings"].as_array().ok_or("missing findings")?;
            assert!(
                !findings.is_empty(),
                "nonempty TypeScript subject: {report}"
            );
            for finding in findings {
                let evidence = finding["evidence"].as_array().ok_or("missing evidence")?;
                assert!(
                    evidence.contains(&Value::from("typescript_package_root: packages/member"))
                );
                assert!(
                    evidence.contains(&Value::from(format!(
                        "typescript_workspace_root: {expected_workspace}"
                    ))),
                    "workspaces={workspaces}: {evidence:?}"
                );
            }
            Ok(())
        };
        check(if structural { "packages/member" } else { "." })?;
        fs::remove_file(repo.join("pnpm-workspace.yaml"))?;
        // No enclosing workspace: retain the existing package-root fallback
        // for scalars, rather than inventing a Git root.
        check("packages/member")?;
        fs::remove_dir_all(sandbox)?;
    }
    Ok(())
}

#[cfg(feature = "lang-rust")]
#[test]
fn scalar_marker_preserves_cargo_uv_git_and_unmarked_root_boundaries() -> TestResult {
    let cases = [
        (
            "cargo-workspace",
            Some(("Cargo.toml", "[workspace]\nmembers = []\n")),
            true,
            "[]",
        ),
        (
            "cargo-package",
            Some((
                "Cargo.toml",
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
            )),
            true,
            "null",
        ),
        (
            "uv",
            Some(("pyproject.toml", "[tool.uv.workspace]\nmembers = []\n")),
            true,
            "false",
        ),
        ("git", None, true, "true"),
        ("unmarked", None, false, "null"),
    ];
    for (label, marker, git, workspaces) in cases {
        let sandbox = unique_external_workspace(label)?;
        let repo = sandbox.join("repo");
        let member = repo.join("packages/member");
        let cwd = member.join("src");
        fs::create_dir_all(&cwd)?;
        if git {
            run_git(&repo, &["init", "-b", "main"])?;
        }
        if let Some((name, contents)) = marker {
            fs::write(repo.join(name), contents)?;
            // This fixture tests root discovery with Rust source. Keep the uv
            // marker, but opt out of unavailable Python preview auto-detection.
            #[cfg(not(feature = "lang-python"))]
            if name == "pyproject.toml" {
                fs::write(
                    repo.join("ripr.toml"),
                    "[languages]\nenabled = [\"rust\"]\n",
                )?;
            }
        }
        fs::write(
            member.join("package.json"),
            format!(r#"{{"workspaces":{workspaces}}}"#),
        )?;
        // The unmarked fallback selects cwd itself; keep its subject under
        // src/ so the unchanged production source-role rule admits it.
        let source_file = if git {
            cwd.join("lib.rs")
        } else {
            fs::create_dir_all(cwd.join("src"))?;
            cwd.join("src/lib.rs")
        };
        fs::write(source_file, "pub fn price() -> u32 { 2 }\n")?;
        let source = if git {
            "packages/member/src/lib.rs"
        } else {
            "src/lib.rs"
        };
        let diff = sandbox.join("change.diff");
        fs::write(
            &diff,
            format!(
                "--- a/{source}\n+++ b/{source}\n@@ -1 +1 @@\n-pub fn price() -> u32 {{ 1 }}\n+pub fn price() -> u32 {{ 2 }}\n"
            ),
        )?;
        let output = run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &cwd,
            &[
                "check",
                "--diff",
                diff.to_str().ok_or("diff must be UTF-8")?,
                "--format",
                "json",
            ],
            &[("RIPR_CACHE_DIR", "")],
        )?;
        assert!(output.status.success(), "{label}: {output:?}");
        let report: Value = serde_json::from_slice(&output.stdout)?;
        fs::remove_dir_all(sandbox)?;
        assert_eq!(report["summary"]["probes"], 1, "{label}: {report}");
        assert_eq!(report["summary"]["findings"], 1, "{label}: {report}");
        let expected_root = if git {
            repo.to_str().ok_or("root must be UTF-8")?
        } else {
            "."
        };
        assert_eq!(report["root"], expected_root, "{label}");
    }
    Ok(())
}

#[cfg(feature = "lang-rust")]
#[test]
fn scalar_workspaces_do_not_select_a_nested_cli_root() -> TestResult {
    for workspaces in SCALAR_MARKERS {
        check_package_workspace_selection(workspaces, false)?;
    }
    Ok(())
}

#[cfg(feature = "lang-rust")]
#[test]
fn package_workspace_arrays_and_yarn_objects_keep_their_root() -> TestResult {
    for workspaces in STRUCTURAL_MARKERS {
        check_package_workspace_selection(workspaces, true)?;
    }
    Ok(())
}

#[cfg(feature = "lang-rust")]
fn check_package_workspace_selection(workspaces: &str, member_is_workspace: bool) -> TestResult {
    let sandbox = unique_external_workspace("package-workspace-selection")?;
    let repo = sandbox.join("repo");
    let member = repo.join("packages/member");
    let cwd = member.join("src");
    fs::create_dir_all(&cwd)?;
    run_git(&repo, &["init", "-b", "main"])?;
    fs::write(
        repo.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    )?;
    fs::write(
        member.join("package.json"),
        format!(r#"{{"name":"member","workspaces":{workspaces}}}"#),
    )?;
    fs::write(cwd.join("lib.rs"), "pub fn price() -> u32 { 2 }\n")?;
    let expected_root = if member_is_workspace { &member } else { &repo };
    let source_path = if member_is_workspace {
        "src/lib.rs"
    } else {
        "packages/member/src/lib.rs"
    };
    let diff = sandbox.join("change.diff");
    fs::write(
        &diff,
        format!(
            "--- a/{source_path}\n+++ b/{source_path}\n@@ -1 +1 @@\n-pub fn price() -> u32 {{ 1 }}\n+pub fn price() -> u32 {{ 2 }}\n"
        ),
    )?;
    let check = |root: Option<&Path>| -> TestResult<Value> {
        let mut args = vec![
            "check",
            "--diff",
            diff.to_str().ok_or("diff path must be UTF-8")?,
            "--format",
            "json",
        ];
        if let Some(root) = root {
            args.extend(["--root", root.to_str().ok_or("root must be UTF-8")?]);
        }
        let output = run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &cwd,
            &args,
            &[("RIPR_CACHE_DIR", "")],
        )?;
        assert!(
            output.status.success(),
            "workspaces={workspaces}, root={root:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let explicit = check(Some(expected_root))?;
    let implicit = check(None)?;
    // An explicit relative member root must remain selected even when its
    // manifest is not a workspace declaration. Its diff is member-relative.
    fs::write(
        &diff,
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn price() -> u32 { 1 }\n+pub fn price() -> u32 { 2 }\n",
    )?;
    let explicit_member = check(Some(Path::new("..")))?;
    // Clean the owned fixture before assertions, including the red control.
    fs::remove_dir_all(&sandbox)?;
    assert_eq!(explicit["summary"]["probes"], 1);
    assert_eq!(explicit["summary"]["findings"], 1);
    assert_eq!(explicit_member["root"], "..");
    assert_eq!(explicit_member["summary"]["probes"], 1);
    assert_eq!(explicit_member["summary"]["findings"], 1);
    assert_eq!(
        implicit["root"].as_str().map(Path::new),
        Some(expected_root.as_path()),
        "workspaces={workspaces}"
    );
    assert_eq!(implicit["findings"], explicit["findings"]);
    Ok(())
}
