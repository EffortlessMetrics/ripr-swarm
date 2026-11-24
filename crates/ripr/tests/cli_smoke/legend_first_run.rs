//! The Evidence legend is a first-run aid: later runs keep one pointer line.
use super::{DeclaredHunkScratch, run_command, run_command_with_env, unique_external_workspace};
use std::fs;
use std::path::Path;
use std::process::Output;
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const LEGEND: &str = "Stages (static evidence)";
const POINTER: &str = "Terms: rerun with `--legend`";
const FANOUT_DRIFT: &str = "ripr: --output-dir input drift: diff, config, source, suppression, date or rendering environment changed; retry on stable inputs";

fn workspace(label: &str) -> TestResult<(std::path::PathBuf, DeclaredHunkScratch)> {
    let project = unique_external_workspace(label)?;
    let cleanup = DeclaredHunkScratch(project.clone());
    fs::create_dir_all(project.join("src"))?;
    fs::create_dir_all(project.join("tests"))?;
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(project.join("src/lib.rs"), "pub fn price() -> u32 { 2 }\n")?;
    fs::write(
        project.join("tests/price.rs"),
        "use pricing::price;\n#[test]\nfn exact_price() { assert_eq!(price(), 2); }\n",
    )?;
    fs::write(
        project.join("change.diff"),
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn price() -> u32 { 1 }\n+pub fn price() -> u32 { 2 }\n",
    )?;
    Ok((project, cleanup))
}

fn check(project: &Path, extra: &[&str]) -> TestResult<String> {
    check_diff(project, "change.diff", extra)
}

fn check_diff(project: &Path, diff: &str, extra: &[&str]) -> TestResult<String> {
    let mut args = vec!["check", "--diff", diff];
    args.extend_from_slice(extra);
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(project), &args)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn legend_prints_on_the_first_run_then_collapses_to_a_pointer() -> TestResult {
    let (project, _cleanup) = workspace("legend-first-run")?;
    let marker = project.join("target/ripr/legend-seen");

    let first = check(&project, &[])?;
    assert!(first.contains(LEGEND), "{first}");
    assert!(!first.contains(POINTER), "{first}");
    assert!(marker.is_file(), "first run must record the legend as seen");

    let second = check(&project, &[])?;
    assert!(!second.contains(LEGEND), "{second}");
    assert!(second.contains(POINTER), "{second}");
    // Only the ten legend lines became one line; the finding is unchanged.
    assert_eq!(
        first.lines().count() - 9,
        second.lines().count(),
        "legend must collapse by exactly nine lines"
    );

    let forced = check(&project, &["--legend"])?;
    assert!(forced.contains(LEGEND), "{forced}");
    assert!(!forced.contains(POINTER), "{forced}");
    Ok(())
}

#[test]
fn machine_formats_and_a_run_without_a_legend_do_not_consume_the_first_run() -> TestResult {
    let (project, _cleanup) = workspace("legend-machine")?;
    let marker = project.join("target/ripr/legend-seen");

    check(&project, &["--format", "json"])?;
    assert!(!marker.exists(), "json must not record the legend as seen");

    let first = check(&project, &[])?;
    assert!(first.contains(LEGEND), "{first}");
    assert!(marker.is_file());
    Ok(())
}

#[test]
fn a_forced_legend_is_not_a_first_run_and_a_run_without_a_digest_does_not_count() -> TestResult {
    let (project, _cleanup) = workspace("legend-forced")?;
    let marker = project.join("target/ripr/legend-seen");

    // A run with no selected finding prints no legend, so it is not a first run.
    fs::write(project.join("empty.diff"), "")?;
    let empty = check_diff(&project, "empty.diff", &[])?;
    assert!(
        !empty.contains(LEGEND) && !empty.contains(POINTER),
        "{empty}"
    );
    assert!(!marker.exists(), "no digest must not record the legend");

    // `--legend` is a lookup: it prints the definitions but is not the first run.
    let forced = check(&project, &["--legend"])?;
    assert!(forced.contains(LEGEND), "{forced}");
    assert!(!marker.exists(), "a forced legend must not write state");
    let first = check(&project, &[])?;
    assert!(first.contains(LEGEND), "{first}");
    assert!(marker.is_file());
    let next = check(&project, &[])?;
    assert!(next.contains(POINTER) && !next.contains(LEGEND), "{next}");
    Ok(())
}

fn utc_day() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400
}

fn run_fanout(project: &Path, label: &str) -> TestResult<(Output, bool)> {
    let before = utc_day();
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(project),
        &[
            "check",
            "--diff",
            "change.diff",
            "--quiet",
            "--output-dir",
            label,
        ],
    )?;
    Ok((output, before != utc_day()))
}

fn fanout_human(
    project: &Path,
    label: &str,
    mut run: impl FnMut(&str) -> TestResult<(Output, bool)>,
) -> TestResult<String> {
    let (output, day_changed) = run(label)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    // A midnight refusal can leave files in the first output directory.
    // Fanout requires a fresh destination, including when the first set is partial.
    let (output, destination) = if !output.status.success()
        && day_changed
        && stderr.lines().any(|line| line == FANOUT_DRIFT)
    {
        let retry_label = format!("{label}-retry");
        (run(&retry_label)?.0, project.join(retry_label))
    } else {
        (output, project.join(label))
    };
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(fs::read_to_string(destination.join("human.txt"))?)
}

// Fanout members always carry the full legend and never consume the standalone
// first run: the `--output-dir` branch returns before any legend logic, so a
// refactor that moved it after that logic would record state here.
#[test]
fn fanout_members_keep_the_full_legend_and_record_no_first_run() -> TestResult {
    let (project, _cleanup) = workspace("legend-fanout")?;
    for label in ["first", "second"] {
        let human = fanout_human(&project, label, |label| run_fanout(&project, label))?;
        assert!(human.contains(LEGEND), "{label}: {human}");
        assert!(!human.contains(POINTER), "{label}: {human}");
    }
    assert!(
        !project.join("target/ripr/legend-seen").exists(),
        "a fanout run must not record the standalone first run"
    );
    Ok(())
}

#[test]
fn fanout_legend_retries_only_midnight_drift_with_a_fresh_destination() -> TestResult {
    let (project, _cleanup) = workspace("legend-fanout-midnight")?;
    for (label, day_changed, diagnostic, expected_attempts) in [
        ("midnight", true, FANOUT_DRIFT, 2),
        ("same-day", false, FANOUT_DRIFT, 1),
        ("other-error", true, "ripr: another output error", 1),
        ("twice", true, FANOUT_DRIFT, 2),
    ] {
        let first = project.join(label);
        fs::create_dir(&first)?;
        fs::write(first.join("human.txt"), "partial output")?;
        let mut attempts = 0;
        let result = fanout_human(&project, label, |destination| {
            attempts += 1;
            if attempts == 1 || label == "twice" {
                // Use the real CLI's unsuccessful status for an occupied
                // destination for the simulated refusal.
                fs::create_dir_all(project.join(destination))?;
                let (mut output, _) = run_fanout(&project, destination)?;
                assert!(!output.status.success());
                output.stderr = format!("{diagnostic}\n").into_bytes();
                Ok((output, day_changed))
            } else {
                run_fanout(&project, destination)
            }
        });
        assert_eq!(attempts, expected_attempts, "{label}");
        if label == "midnight" {
            assert!(result?.contains(LEGEND));
        } else {
            assert_eq!(result.unwrap_err().to_string().trim(), diagnostic);
        }
        assert_eq!(
            fs::read_to_string(first.join("human.txt"))?,
            "partial output"
        );
    }
    Ok(())
}

#[test]
fn a_directory_at_the_marker_path_is_not_proof_the_legend_was_shown() -> TestResult {
    let (project, _cleanup) = workspace("legend-marker-dir")?;
    let marker = project.join("target/ripr/legend-seen");
    fs::create_dir_all(&marker)?;
    for _ in 0..2 {
        let out = check(&project, &[])?;
        assert!(out.contains(LEGEND), "{out}");
        assert!(!out.contains(POINTER), "{out}");
    }
    assert!(marker.is_dir());
    Ok(())
}

// A regular file where the `target/ripr` directory belongs makes the marker
// write fail on every host and for every user (a read-only mode would not stop
// root). The facts cache is redirected so the analysis itself still completes.
#[test]
fn an_unwritable_output_tree_shows_the_full_legend_every_run() -> TestResult {
    let (project, _cleanup) = workspace("legend-unwritable")?;
    fs::create_dir_all(project.join("target"))?;
    fs::write(project.join("target/ripr"), b"not a directory")?;
    let cache = project.join("external-cache");
    let cache_arg = cache.to_string_lossy().into_owned();
    for _ in 0..2 {
        let output = run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &project,
            &["check", "--diff", "change.diff"],
            &[("RIPR_CACHE_DIR", cache_arg.as_str())],
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let out = String::from_utf8(output.stdout)?;
        assert!(out.contains(LEGEND), "{out}");
        assert!(!out.contains(POINTER), "{out}");
    }
    assert!(project.join("target/ripr").is_file());
    Ok(())
}

// A newline and a colon are not valid in a Windows directory name, so the
// fixture can only be created on Unix.
#[cfg(unix)]
#[test]
fn a_root_named_like_one_legend_line_does_not_count_as_a_displayed_legend() -> TestResult {
    let base = unique_external_workspace("legend-spoof-root")?;
    let _cleanup = DeclaredHunkScratch(base.clone());
    let root = base.join("n\n    Stages (static evidence): reach = ");
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"empty\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "")?;
    let diff = root.join("empty.diff");
    fs::write(&diff, "")?;
    // The root must reach the rendered header, so pass it explicitly and run
    // from outside it; a bare `.` root would never show the injected line.
    let root_arg = root.to_string_lossy().into_owned();
    let diff_arg = diff.to_string_lossy().into_owned();
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&base),
        &["check", "--root", &root_arg, "--diff", &diff_arg],
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out = String::from_utf8(output.stdout)?;
    assert!(
        out.contains("\n    Stages (static evidence): reach = "),
        "the injected root line should reach the header: {out}"
    );
    assert!(
        !out.contains("propagation = changed effect reaches output"),
        "an empty diff prints no digest, so no real legend: {out}"
    );
    assert!(!root.join("target/ripr/legend-seen").exists());
    Ok(())
}
