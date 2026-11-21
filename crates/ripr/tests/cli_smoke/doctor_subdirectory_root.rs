//! #15304: `ripr doctor` run from a subdirectory diagnoses the root that
//! `ripr check` analyzes from there, not the invocation directory.

use super::{ignore_remove_dir_all, run_command_with_env, run_git, unique_temp_workspace};
use std::path::Path;

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

fn doctor_in(cwd: &Path, args: &[&str]) -> Result<(String, String), String> {
    let output = run_command_with_env(env!("CARGO_BIN_EXE_ripr"), cwd, args, &[])
        .map_err(|error| format!("run ripr {args:?} in {}: {error}", cwd.display()))?;
    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn tip_line(stdout: &str) -> Option<&str> {
    stdout.lines().find(|line| line.starts_with("- Tip:"))
}

fn detected_line(stdout: &str) -> Option<&str> {
    stdout
        .lines()
        .find(|line| line.starts_with("- Detected languages:"))
}

/// A Cargo workspace at the Git root with a Python project in `py/` and the
/// `ripr.toml` that `ripr init` writes. From `py/`, doctor used to detect
/// Python alone and tell the user to set `enabled = ["python"]`, which drops
/// the Rust workspace that `ripr check` from the same directory analyzes.
#[test]
fn doctor_from_a_subdirectory_diagnoses_the_root_check_analyzes() -> Result<(), String> {
    let workspace = unique_temp_workspace("doctor-subdir-root");
    let result = (|| -> Result<(), String> {
        write(
            &workspace.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/core\"]\nresolver = \"2\"\n",
        )?;
        write(
            &workspace.join("crates/core/Cargo.toml"),
            "[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        write(
            &workspace.join("crates/core/src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        )?;
        write(
            &workspace.join("py/pyproject.toml"),
            "[project]\nname = \"py\"\nversion = \"0.1.0\"\n",
        )?;
        write(
            &workspace.join("py/app.py"),
            "def add(a, b):\n    return a + b\n",
        )?;
        write(
            &workspace.join("ripr.toml"),
            "[languages]\nenabled = [\"rust\"]\n",
        )?;
        run_git(&workspace, &["init", "-q", "-b", "main"])?;
        let subdir = workspace.join("py");
        let canonical = std::fs::canonicalize(&workspace)
            .map_err(|error| format!("canonicalize workspace: {error}"))?;

        // Premise: implicit `ripr check` from py/ moves to the workspace root.
        let (_, check_stderr) = doctor_in(&subdir, &["check"])?;
        if !check_stderr.contains("resolved workspace root to") {
            return Err(format!(
                "premise: implicit check from py/ must resolve the workspace root:\n{check_stderr}"
            ));
        }

        let (at_root, _) = doctor_in(&workspace, &["doctor"])?;
        let (from_subdir, subdir_stderr) = doctor_in(&subdir, &["doctor"])?;

        // Same disclosure as check, so the move is never silent.
        if !subdir_stderr.contains("resolved workspace root to") {
            return Err(format!(
                "doctor from py/ must disclose the resolved root:\n{subdir_stderr}"
            ));
        }
        // `Root:` names the resolved root, not `.`.
        let root_line = from_subdir
            .lines()
            .find(|line| line.starts_with("Root: "))
            .ok_or_else(|| format!("doctor printed no Root: line:\n{from_subdir}"))?;
        let shown = Path::new(root_line.trim_start_matches("Root: "));
        let shown = std::fs::canonicalize(shown).unwrap_or_else(|_| shown.to_path_buf());
        if shown != canonical {
            return Err(format!(
                "Root: must name the resolved workspace root {}, got {root_line:?}",
                canonical.display()
            ));
        }
        // Detection and the tip match a run at the root.
        if detected_line(&from_subdir) != detected_line(&at_root) {
            return Err(format!(
                "detected languages differ between py/ and the root:\npy/:\n{from_subdir}\nroot:\n{at_root}"
            ));
        }
        if tip_line(&from_subdir) != tip_line(&at_root) {
            return Err(format!(
                "tip differs between py/ and the root:\npy/:\n{from_subdir}\nroot:\n{at_root}"
            ));
        }
        // Equal tips could both be absent: require the actual list, with both
        // the configured Rust and the detected Python.
        let tip = tip_line(&from_subdir)
            .ok_or_else(|| format!("doctor from py/ printed no tip:\n{from_subdir}"))?;
        if !tip.contains("Set `enabled = [\"rust\", \"python\"]`") {
            return Err(format!(
                "doctor from py/ must suggest the workspace list with Rust kept: {tip}"
            ));
        }
        if detected_line(&from_subdir)
            != Some("- Detected languages: rust (stable), python (preview)")
        {
            return Err(format!(
                "doctor from py/ must detect both languages at the resolved root:\n{from_subdir}"
            ));
        }

        // `--root .` still pins the invocation directory: this is the old
        // view, where py/ alone detects Python only.
        let (pinned, pinned_stderr) = doctor_in(&subdir, &["doctor", "--root", "."])?;
        if pinned_stderr.contains("resolved workspace root to") {
            return Err(format!(
                "`--root .` must not move the root:\n{pinned_stderr}"
            ));
        }
        if !pinned.lines().any(|line| line == "Root: .") {
            return Err(format!("`--root .` must diagnose py/ itself:\n{pinned}"));
        }
        if detected_line(&pinned) != Some("- Detected languages: python (preview)") {
            return Err(format!(
                "control: py/ alone detects Python only, unlike the root:\n{pinned}"
            ));
        }
        // Even there, the `rust` the workspace config enables explicitly is
        // kept in the replacement list (#15304 review).
        if !pinned.contains("enabled = [\"rust\", \"python\"]") {
            return Err(format!(
                "`--root .` must keep the explicitly enabled rust:\n{pinned}"
            ));
        }

        // JSON takes the same root: the same detected languages as a root run,
        // with Rust enabled rather than dropped.
        let languages = |cwd: &Path| -> Result<Vec<(String, bool)>, String> {
            let (json, _) = doctor_in(cwd, &["doctor", "--json"])?;
            let report: serde_json::Value = serde_json::from_str(&json)
                .map_err(|error| format!("doctor --json did not parse: {error}\n{json}"))?;
            Ok(report["detected_languages"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| {
                    (
                        entry["language"].as_str().unwrap_or_default().to_string(),
                        entry["enabled"].as_bool().unwrap_or(false),
                    )
                })
                .collect())
        };
        let json_subdir = languages(&subdir)?;
        let json_root = languages(&workspace)?;
        if json_subdir != json_root || !json_subdir.contains(&("rust".to_string(), true)) {
            return Err(format!(
                "doctor --json from py/ must report the root's languages: py/ {json_subdir:?}, root {json_root:?}"
            ));
        }
        Ok(())
    })();
    ignore_remove_dir_all(&workspace);
    result
}

/// A standalone Python repository with no Rust marker. Implicit doctor has
/// nothing to widen to. The tip drops `rust` only when it is the built-in
/// default (#7648); a `rust` the user wrote into `[languages].enabled` stays
/// in the replacement list (#15304 review).
#[test]
fn doctor_tip_keeps_explicit_rust_and_drops_only_the_default() -> Result<(), String> {
    let cases: [(&str, Option<&str>, Option<&str>); 3] = [
        (
            "explicit",
            Some("[languages]\nenabled = [\"rust\"]\n"),
            Some("enabled = [\"rust\", \"python\"]"),
        ),
        // Without a ripr.toml there is no file to edit, so doctor prints no
        // tip; it must not name a Rust-keeping list either.
        ("config-free", None, None),
        (
            "table-without-enabled",
            Some("[languages]\n\n[analysis]\nmode = \"fast\"\n"),
            Some("enabled = [\"python\"]"),
        ),
    ];
    for (label, config, expected) in cases {
        let workspace = unique_temp_workspace(&format!("doctor-standalone-python-{label}"));
        let result = (|| -> Result<(), String> {
            write(
                &workspace.join("pyproject.toml"),
                "[project]\nname = \"py\"\nversion = \"0.1.0\"\n",
            )?;
            write(
                &workspace.join("app.py"),
                "def add(a, b):\n    return a + b\n",
            )?;
            if let Some(config) = config {
                write(&workspace.join("ripr.toml"), config)?;
            }
            run_git(&workspace, &["init", "-q", "-b", "main"])?;
            let (stdout, stderr) = doctor_in(&workspace, &["doctor"])?;
            if stderr.contains("resolved workspace root to") {
                return Err(format!("{label}: a root run must not move:\n{stderr}"));
            }
            if !stdout.lines().any(|line| line == "Root: .") {
                return Err(format!("{label}: a root run keeps `Root: .`:\n{stdout}"));
            }
            match (expected, tip_line(&stdout)) {
                (Some(expected), Some(tip)) if tip.contains(&format!("Set `{expected}`")) => {}
                (None, None) if !stdout.contains("enabled = [\"rust\"") => {}
                (expected, tip) => {
                    return Err(format!(
                        "{label}: expected tip {expected:?}, got {tip:?}:\n{stdout}"
                    ));
                }
            }
            Ok(())
        })();
        ignore_remove_dir_all(&workspace);
        result?;
    }
    Ok(())
}
