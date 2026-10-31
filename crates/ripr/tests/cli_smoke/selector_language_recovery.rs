//! #10720: a fresh selector miss retains producer-owned run-scope limits.
#[cfg(feature = "lang-typescript")]
use super::{printed_ripr_args, rendered_path};
use super::{run_command, run_git, unique_temp_workspace};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Fixture {
    root: PathBuf,
    foreign: PathBuf,
    patch: PathBuf,
}

impl Fixture {
    fn new(language: &str, extension: &str, enabled: bool) -> TestResult<Self> {
        let root = unique_temp_workspace("selector language scope");
        let foreign = unique_temp_workspace("selector recovery decoy");
        fs::create_dir_all(root.join("src"))?;
        fs::create_dir_all(&foreign)?;
        let fixture = Self {
            patch: root.join("selected change.patch"),
            root,
            foreign,
        };
        fs::write(
            fixture.foreign.join("Cargo.toml"),
            "[package]\nname = \"selector_decoy\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        fs::create_dir_all(fixture.foreign.join("src"))?;
        fs::write(fixture.foreign.join("src/lib.rs"), "pub fn decoy() {}\n")?;
        run_git(&fixture.foreign, &["init", "--initial-branch=main"])?;
        run_git(
            &fixture.foreign,
            &["config", "user.email", "decoy@test.invalid"],
        )?;
        run_git(&fixture.foreign, &["config", "user.name", "Decoy Fixture"])?;
        run_git(&fixture.foreign, &["add", "."])?;
        run_git(&fixture.foreign, &["commit", "-qm", "unrelated repository"])?;
        fs::write(
            fixture.root.join("Cargo.toml"),
            "[package]\nname = \"selector_language_scope\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        fs::write(
            fixture.root.join("ripr.toml"),
            if enabled {
                format!("[languages]\nenabled = [\"rust\", \"{language}\"]\n")
            } else {
                "[languages]\nenabled = [\"rust\"]\n".to_string()
            },
        )?;
        let write_sources = |comparison: &str| -> TestResult {
            fs::write(
                fixture.root.join("src/lib.rs"),
                format!("pub fn limit(n: u32) -> bool {{\n    n {comparison} 10\n}}\n"),
            )?;
            for name in ["a", "b", "c", "z"] {
                let source = if language == "perl" {
                    format!("sub limit {{\n    return $_[0] {comparison} 10;\n}}\n")
                } else {
                    format!(
                        "export function limit(n: number): boolean {{\n    return n {comparison} 10;\n}}\n"
                    )
                };
                fs::write(fixture.root.join(format!("src/{name}.{extension}")), source)?;
            }
            Ok(())
        };
        write_sources(">")?;
        run_git(&fixture.root, &["init", "--initial-branch=main"])?;
        run_git(
            &fixture.root,
            &["config", "user.email", "selector@test.invalid"],
        )?;
        run_git(&fixture.root, &["config", "user.name", "Selector Fixture"])?;
        run_git(&fixture.root, &["add", "."])?;
        run_git(&fixture.root, &["commit", "-qm", "base"])?;
        write_sources(">=")?;
        run_git(&fixture.root, &["commit", "-qam", "changed boundaries"])?;
        let patch = super::common::fixture_git::fixture_git_output(
            &fixture.root,
            &["diff", "HEAD~1", "HEAD"],
        )?;
        fs::write(&fixture.patch, patch)?;
        // The implicit dirty default must take its worktree recovery branch.
        fs::write(
            fixture.root.join("src/lib.rs"),
            "pub fn limit(n: u32) -> bool {\n    n >= 10\n}\n// saved edit\n",
        )?;
        Ok(fixture)
    }

    fn run(&self, command: &str, selector: Option<&str>, scope: &[&str]) -> TestResult<Output> {
        let root = self.root.to_str().ok_or("non-UTF-8 fixture root")?;
        let mut args = vec![command, "--root", root, "--quiet"];
        args.extend_from_slice(scope);
        if let Some(selector) = selector {
            if command == "context" {
                args.push("--at");
            }
            args.push(selector);
        } else {
            args.push("--json");
        }
        Ok(run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&self.foreign),
            &args,
        )?)
    }

    fn check(&self, scope: &[&str]) -> TestResult<Value> {
        let output = self.run("check", None, scope)?;
        assert!(output.status.success(), "{output:?}");
        Ok(serde_json::from_slice(&output.stdout)?)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.foreign);
    }
}

fn miss(output: Output) -> TestResult<String> {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "selector misses keep stdout empty"
    );
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("no finding matched"), "{stderr}");
    Ok(stderr)
}

fn advisory<'a>(report: &'a Value, language: &str) -> TestResult<&'a Value> {
    report["preview_languages"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["language"] == language))
        .ok_or_else(|| format!("fixture has no {language} advisory: {report}").into())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn disabled_preview_misses_disclose_scope_and_replay_exact_listing() -> TestResult {
    let fixture = Fixture::new("typescript", "ts", false)?;
    let patch = fixture.patch.to_str().ok_or("non-UTF-8 patch")?;
    for (label, scope) in [
        ("committed", vec!["--base", "HEAD~1", "--committed"]),
        ("worktree", vec!["--base", "HEAD~1", "--worktree"]),
        ("dirty default", vec!["--base", "HEAD~1"]),
        ("supplied diff", vec!["--diff", patch, "--mode", "fast"]),
    ] {
        let report = fixture.check(&scope)?;
        let preview = advisory(&report, "typescript")?;
        assert_eq!(preview["analyzed"], false, "{label}: {report}");
        assert_eq!(preview["file_count"], 4, "{label}: {report}");
        let samples = preview["sample_paths"]
            .as_array()
            .ok_or("missing samples")?;
        assert_eq!(samples.len(), 3, "fixture must reach the sample cap");
        assert!(!samples.iter().any(|path| path == "src/z.ts"));
        assert!(
            report["findings"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
            "mixed Rust findings must survive the disabled preview: {report}"
        );
        for command in ["explain", "context"] {
            let stderr = miss(fixture.run(command, Some("src/z.ts:2"), &scope)?)?;
            assert!(
                stderr.contains("Analysis limitations for this run:"),
                "{label}/{command}: {stderr}"
            );
            assert!(stderr.contains("TypeScript"), "{stderr}");
            assert!(stderr.contains("preview adapter not enabled"), "{stderr}");
            assert!(stderr.contains("ripr.toml"), "{stderr}");
            assert!(!stderr.contains("not compiled"), "{stderr}");
            let listing = stderr
                .split('`')
                .find(|text| text.starts_with("ripr check "))
                .ok_or("no recovery listing")?;
            let words = printed_ripr_args(listing)?;
            let args: Vec<&str> = words.iter().map(String::as_str).collect();
            let replay = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&fixture.foreign), &args)?;
            assert!(replay.status.success(), "{listing}: {replay:?}");
            let replay: Value = serde_json::from_slice(&replay.stdout)?;
            assert_eq!(advisory(&replay, "typescript")?["analyzed"], false);
            if matches!(label, "committed" | "dirty default") {
                assert!(args.contains(&"--committed"), "{listing}");
            } else if label == "worktree" {
                assert!(args.contains(&"--worktree"), "{listing}");
            } else {
                // Printed paths use the host rendering (`/` separators on Windows).
                let printed_patch = rendered_path(patch);
                assert!(
                    args.contains(&printed_patch.as_str()) && args.contains(&"fast"),
                    "{listing}"
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn enabled_preview_hits_and_ordinary_misses_do_not_claim_unanalyzed_scope() -> TestResult {
    let fixture = Fixture::new("typescript", "ts", true)?;
    let scope = ["--base", "HEAD~1", "--worktree"];
    let report = fixture.check(&scope)?;
    assert_eq!(advisory(&report, "typescript")?["analyzed"], true);
    let finding = report["findings"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["language"] == "typescript"))
        .ok_or("enabled fixture must contain a TypeScript finding")?;
    let id = finding["id"].as_str().ok_or("missing finding id")?;
    for command in ["explain", "context"] {
        let hit = fixture.run(command, Some(id), &scope)?;
        assert!(hit.status.success(), "{hit:?}");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&hit.stdout),
            String::from_utf8_lossy(&hit.stderr)
        );
        assert!(
            !text.contains("Analysis limitations for this run:"),
            "{text}"
        );
        let stderr = miss(fixture.run(command, Some("probe:no-such-finding"), &scope)?)?;
        assert!(
            !stderr.contains("Analysis limitations for this run:"),
            "{stderr}"
        );
        assert!(!stderr.contains("adapter not enabled"), "{stderr}");
    }
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn unrelated_disabled_language_keeps_rust_hit_and_selector_specific_recoveries() -> TestResult {
    let fixture = Fixture::new("typescript", "ts", false)?;
    let scope = ["--base", "HEAD~1", "--committed"];
    let report = fixture.check(&scope)?;
    let id = report["findings"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["language"] == "rust"))
        .and_then(|row| row["id"].as_str())
        .ok_or("mixed fixture must have a Rust finding")?;
    for command in ["explain", "context"] {
        let hit = fixture.run(command, Some(id), &scope)?;
        assert!(hit.status.success(), "{hit:?}");
        assert!(
            !String::from_utf8_lossy(&hit.stdout).contains("Analysis limitations for this run:")
        );
        for (selector, retained) in [
            ("src/lib.rs:999", "src/lib.rs:999"),
            ("src/lib.rs:abc", "positive decimal line number"),
            ("seam:0123456789abcdef", "this looks like a seam id"),
        ] {
            let stderr = miss(fixture.run(command, Some(selector), &scope)?)?;
            assert!(stderr.contains(retained), "{stderr}");
            assert!(
                stderr.contains("Analysis limitations for this run:"),
                "{stderr}"
            );
        }
    }
    Ok(())
}

#[test]
fn artifact_misses_do_not_invent_fresh_language_scope() -> TestResult {
    let fixture = Fixture::new("typescript", "ts", false)?;
    let artifact = fixture.root.join("recorded.json");
    let artifact = artifact.to_str().ok_or("non-UTF-8 artifact")?;
    let output = fixture.run(
        "check",
        None,
        &[
            "--base",
            "HEAD~1",
            "--committed",
            "--write-artifact",
            artifact,
        ],
    )?;
    assert!(output.status.success(), "{output:?}");
    assert!(Path::new(artifact).is_file(), "artifact setup failed");
    for command in ["explain", "context"] {
        let stderr = miss(fixture.run(
            command,
            Some("probe:no-such-finding"),
            &["--from", artifact],
        )?)?;
        assert!(
            !stderr.contains("Analysis limitations for this run:"),
            "{stderr}"
        );
        assert!(!stderr.contains("adapter not enabled"), "{stderr}");
    }
    Ok(())
}

#[cfg(not(feature = "lang-perl"))]
#[test]
fn uncompiled_preview_misses_name_binary_prerequisites_not_config_enablement() -> TestResult {
    let fixture = Fixture::new("perl", "pl", false)?;
    let scope = ["--base", "HEAD~1", "--worktree"];
    let report = fixture.check(&scope)?;
    assert_eq!(advisory(&report, "perl")?["analyzed"], false);
    for command in ["explain", "context"] {
        let stderr = miss(fixture.run(command, Some("src/z.pl:2"), &scope)?)?;
        assert!(
            stderr.contains("Analysis limitations for this run:"),
            "{stderr}"
        );
        assert!(
            stderr.contains("not compiled into this ripr binary"),
            "{stderr}"
        );
        assert!(
            stderr.contains("lang-perl") && stderr.contains("fact exporter"),
            "{stderr}"
        );
        assert!(!stderr.contains("to enable add to ripr.toml"), "{stderr}");
    }
    Ok(())
}

#[cfg(feature = "lang-perl")]
#[test]
fn enabled_unavailable_preview_misses_keep_native_reason_and_recovery() -> TestResult {
    let fixture = Fixture::new("perl", "pl", true)?;
    let scope = ["--base", "HEAD~1", "--worktree"];
    let report = fixture.check(&scope)?;
    assert_eq!(advisory(&report, "perl")?["analyzed"], false);
    assert_eq!(report["language_runs"][0]["status"], "unavailable");
    let reason = report["language_runs"][0]["reason"]
        .as_str()
        .ok_or("missing producer reason")?;
    for command in ["explain", "context"] {
        let stderr = miss(fixture.run(command, Some("src/z.pl:2"), &scope)?)?;
        assert!(
            stderr.contains("did not complete successfully (unavailable)"),
            "{stderr}"
        );
        assert!(stderr.contains(reason), "{stderr}");
        assert!(!stderr.contains("adapter not enabled"), "{stderr}");
    }
    Ok(())
}
