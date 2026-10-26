use super::cells::{Cell, cells};
use super::judge::{Limits, Outcome, Sample, Verdict, judge};
use super::{build_packet, parse_args};

type TestResult = Result<(), String>;

fn done(bytes: usize, millis: u128) -> Sample {
    Sample {
        bytes,
        millis,
        outcome: Outcome::Completed,
    }
}

fn steps(times: [u128; 3]) -> Vec<Sample> {
    vec![
        done(1 << 19, times[0]),
        done(1 << 20, times[1]),
        done(1 << 21, times[2]),
    ]
}

#[test]
fn linear_cost_passes() {
    let judged = judge(&steps([2_000, 4_100, 8_300]), &Limits::default());
    assert_eq!(judged.verdict, Verdict::Pass, "{}", judged.detail);
}

#[test]
fn quadratic_cost_fails() {
    let judged = judge(&steps([2_000, 8_000, 32_000]), &Limits::default());
    assert_eq!(judged.verdict, Verdict::Fail);
    assert!(judged.detail.contains("super-linear"), "{}", judged.detail);
}

#[test]
fn quadratic_cost_from_a_fast_first_step_still_fails() {
    // 300 ms to 4.8 s is 16x for 4x the size; the floor must not hide it.
    let judged = judge(&steps([300, 1_200, 4_800]), &Limits::default());
    assert_eq!(judged.verdict, Verdict::Fail, "{}", judged.detail);
}

#[test]
fn a_run_under_the_quiet_limit_carries_no_growth_signal() {
    let judged = judge(&steps([5, 40, 900]), &Limits::default());
    assert_eq!(judged.verdict, Verdict::Pass, "{}", judged.detail);
}

#[test]
fn a_linear_cost_over_the_ceiling_fails() {
    let judged = judge(&steps([20_000, 40_000, 80_000]), &Limits::default());
    assert_eq!(judged.verdict, Verdict::Fail);
    assert!(judged.detail.contains("ceiling"), "{}", judged.detail);
}

#[test]
fn an_incomplete_step_fails_whatever_its_time() {
    for outcome in [Outcome::Deadline, Outcome::Signalled, Outcome::Exit(2)] {
        let mut samples = steps([10, 20, 40]);
        samples[2].outcome = outcome;
        let judged = judge(&samples, &Limits::default());
        assert_eq!(judged.verdict, Verdict::Fail, "{outcome:?}");
    }
}

#[test]
fn no_samples_or_flat_sizes_fail_closed() {
    assert_eq!(judge(&[], &Limits::default()).verdict, Verdict::Fail);
    let flat = [done(10, 1), done(10, 1)];
    assert_eq!(judge(&flat, &Limits::default()).verdict, Verdict::Fail);
}

#[test]
fn the_matrix_covers_every_language_role_and_shape() {
    let all = cells(&[]);
    assert_eq!(all.len(), 11);
    let only = cells(&["python".to_string()]);
    assert_eq!(only.len(), 3);
    assert!(only.iter().all(|cell| cell.language == "python"));
    assert!(
        !only
            .iter()
            .any(|cell| cell.role == "test" && cell.shape == "one-line"),
        "a Python test cannot be a single line"
    );
}

#[test]
fn generated_files_reach_the_requested_size_and_shape() -> TestResult {
    for cell in cells(&[]) {
        let files = cell.files(64 * 1024);
        let big = largest(&files, &cell)?;
        assert!(big.len() >= 64 * 1024, "{} too small", cell.id());
        assert!(big.len() < 64 * 1024 + 512, "{} overshoots", cell.id());
        let text = String::from_utf8(big.clone()).map_err(|error| error.to_string())?;
        let lines = text.matches('\n').count();
        if cell.shape == "one-line" {
            assert!(lines <= 1, "{} has {lines} lines", cell.id());
        } else {
            assert!(lines > 100, "{} has {lines} lines", cell.id());
        }
        assert!(files.iter().any(|(path, _)| path == super::cells::DIFF));
    }
    Ok(())
}

fn largest(files: &[(String, Vec<u8>)], cell: &Cell) -> Result<Vec<u8>, String> {
    files
        .iter()
        .max_by_key(|(_, bytes)| bytes.len())
        .map(|(_, bytes)| bytes.clone())
        .ok_or_else(|| format!("{} generated no files", cell.id()))
}

#[test]
fn arguments_are_validated() {
    let refusal = |items: &[&str]| {
        let args: Vec<String> = items.iter().map(|s| (*s).to_string()).collect();
        parse_args(&args).err().unwrap_or_default()
    };
    assert!(refusal(&[]).contains("requires --binary"));
    assert!(refusal(&["--binary", "x", "--base-kib", "0"]).contains("at least 1"));
    assert!(refusal(&["--binary", "x", "--languages", "perl"]).contains("unknown language"));
    assert!(refusal(&["--binary", "x", "--max-doubling-ratio", "1.5"]).contains("below 2.0"));
    assert!(refusal(&["--binary", "x", "--max-doubling-ratio", "NaN"]).contains("finite number"));
    assert!(refusal(&["--binary", "x", "--nonsense"]).contains("unknown size-scaling argument"));
    let accepted: Vec<String> = ["--binary", "x", "--languages", "rust"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(
        parse_args(&accepted).map(|options| options.languages),
        Ok(vec!["rust".to_string()])
    );
}

#[cfg(unix)]
mod stub {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn stub(dir: &Path, script: &str) -> Result<std::path::PathBuf, String> {
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let path = dir.join("ripr-stub");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).map_err(|error| error.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        Ok(path)
    }

    fn options(dir: &Path, binary: std::path::PathBuf) -> Result<super::super::Options, String> {
        let mut args = vec![
            "--binary".to_string(),
            binary.to_string_lossy().to_string(),
            "--work".to_string(),
            dir.join("work").to_string_lossy().to_string(),
            "--base-kib".to_string(),
            "1".to_string(),
            "--languages".to_string(),
            "rust".to_string(),
        ];
        args.push("--ceiling-secs".to_string());
        args.push("30".to_string());
        parse_args(&args)
    }

    /// A report with one finding for the changed Rust file, as the real
    /// binary prints for these workspaces.
    const REPORT: &str = "printf '%s' '{\"findings\":[{\"probe\":{\"file\":\"src/lib.rs\"}}],\"analysis_outcome\":{\"outcome\":{\"limitations\":[]}}}'";

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ripr-size-scaling-{name}-{}", std::process::id()))
    }

    #[test]
    fn a_fast_binary_passes_every_cell_and_leaves_no_workspace() -> TestResult {
        let dir = scratch("fast");
        let binary = stub(&dir, &format!("{REPORT}\nexit 0"))?;
        let packet = build_packet(&options(&dir, binary)?)?;
        let leftover = fs::read_dir(dir.join("work"))
            .map_err(|error| error.to_string())?
            .count();
        let _ = fs::remove_dir_all(&dir);
        assert!(packet.passed);
        assert_eq!(packet.rows.len(), 4);
        assert_eq!(packet.binary_sha256.len(), 64);
        assert_eq!(leftover, 0, "workspaces were not removed");
        Ok(())
    }

    #[test]
    fn a_fast_exit_that_analyzed_nothing_fails_every_cell() -> TestResult {
        let dir = scratch("empty");
        let binary = stub(&dir, "printf '%s' '{\"findings\":[]}'\nexit 0")?;
        let packet = build_packet(&options(&dir, binary)?)?;
        let _ = fs::remove_dir_all(&dir);
        assert!(!packet.passed);
        assert!(packet.rows.iter().all(|row| row.verdict == "fail"));
        assert!(
            packet
                .rows
                .iter()
                .all(|row| row.samples.iter().all(|s| s.outcome == "not_analyzed"))
        );
        Ok(())
    }

    #[test]
    fn a_limitation_naming_the_large_file_fails_the_cell() -> TestResult {
        let dir = scratch("limited");
        let script = "printf '%s' '{\"findings\":[{\"probe\":{\"file\":\"src/lib.rs\"}}],\"analysis_outcome\":{\"outcome\":{\"limitations\":[{\"path\":\"src/big.rs\"}]}}}'\nexit 0";
        let binary = stub(&dir, script)?;
        let mut opts = options(&dir, binary)?;
        opts.languages = vec!["rust".to_string()];
        let packet = build_packet(&opts)?;
        let _ = fs::remove_dir_all(&dir);
        let production = packet
            .rows
            .iter()
            .filter(|row| row.role == "production")
            .all(|row| row.verdict == "fail");
        assert!(production, "a refused production file passed as fast");
        Ok(())
    }

    #[test]
    fn a_binary_that_refuses_with_an_error_exit_fails_every_cell() -> TestResult {
        let dir = scratch("refuse");
        let binary = stub(&dir, "exit 2")?;
        let packet = build_packet(&options(&dir, binary)?)?;
        let _ = fs::remove_dir_all(&dir);
        assert!(!packet.passed);
        assert!(packet.rows.iter().all(|row| row.verdict == "fail"));
        Ok(())
    }

    #[test]
    fn a_binary_whose_cost_grows_with_the_square_of_the_input_fails() -> TestResult {
        // Sleeps in proportion to the square of the largest file's size, so a
        // regression of this shape cannot pass as noise.
        let dir = scratch("quadratic");
        let script = format!(
            "size=$(cat src/big.rs tests/big.rs 2>/dev/null | wc -c)\nsleep $(awk -v s=\"$size\" 'BEGIN{{printf \"%.3f\", (s/1024)*(s/1024)*0.05}}')\n{REPORT}\nexit 0"
        );
        let binary = stub(&dir, &script)?;
        let mut opts = options(&dir, binary)?;
        opts.limits.quiet_ms = 100;
        opts.limits.noise_floor_ms = 50;
        let packet = build_packet(&opts)?;
        let _ = fs::remove_dir_all(&dir);
        assert!(
            !packet.passed,
            "{}",
            serde_json::to_string(&packet).unwrap_or_default()
        );
        Ok(())
    }
}
