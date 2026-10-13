use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "common/mod.rs"]
mod common;

const CACHE_DIR_ENV: &str = "RIPR_CACHE_DIR";
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ripr-cache-cli-{label}-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}

fn remove_base(base: &Path) -> Result<(), String> {
    if base.exists() {
        fs::remove_dir_all(base).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Independent host spelling of the human `Cache dir:` header (#7326).
/// This copies the `human_path_text` rule instead of calling the production
/// helper, so a wrong parent with the same leaf, or `Path::display()` on
/// Windows, cannot satisfy the exact-binary tests.
fn expected_cache_dir_line(cache_dir: &Path) -> Option<String> {
    let text = cache_dir.to_str()?;
    let shown = if cfg!(windows) {
        match text.strip_prefix(r"\\?\") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.replace('\\', "/"),
            Some(_) => text.to_string(),
            None => text.replace('\\', "/"),
        }
    } else {
        text.to_string()
    };
    Some(format!("Cache dir: {shown}"))
}

fn stdout_has_cache_dir_line(stdout: &str, cache_dir: &Path) -> bool {
    let Some(expected) = expected_cache_dir_line(cache_dir) else {
        return false;
    };
    stdout.lines().any(|line| line == expected)
}

#[test]
fn cache_dir_line_matcher_rejects_same_leaf_wrong_parent() -> Result<(), String> {
    let cache_dir = Path::new("target/ripr/actual-cache");
    let expected = expected_cache_dir_line(cache_dir).ok_or("utf-8 cache dir")?;
    let matching = format!("{expected}\ncleared\n");
    let same_leaf = "Cache dir: other/ripr/actual-cache\ncleared\n";
    if !stdout_has_cache_dir_line(&matching, cache_dir) {
        return Err(format!("exact header must match: {expected}"));
    }
    if stdout_has_cache_dir_line(same_leaf, cache_dir) {
        return Err("same leaf under a different parent must not match".to_string());
    }
    Ok(())
}

/// The one spawn site for the exact `ripr` binary in this file.
fn run_ripr(base: &Path, cache_dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .current_dir(base)
        .env(CACHE_DIR_ENV, cache_dir)
        .env("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1")
        .output()
        .map_err(|error| format!("run exact ripr binary {args:?}: {error}"))
}

const HARNESS_SOURCE: &str = r#"use libtest_mimic::{Arguments, Failed, Trial};
fn check_shipping() -> Result<(), Failed> {
    assert_eq!(cache_harness_fixture::shipping(3), 12);
    Ok(())
}
fn main() {
    let args = Arguments::from_args();
    let trials = vec![Trial::test("shipping_is_four_per_kilo", check_shipping)];
    libtest_mimic::run(&args, trials).exit();
}
"#;

fn harness_cache_fixture(label: &str) -> Result<PathBuf, String> {
    let root = temp_dir(label);
    for dir in ["src", "tests", ".cargo"] {
        fs::create_dir_all(root.join(dir)).map_err(|error| error.to_string())?;
    }
    for (path, text) in [
        ("Cargo.toml", "[package]\nname='cache_harness_fixture'\nversion='0.1.0'\nedition='2024'\n\n[[test]]\nname='trials'\nharness=false\n\n[workspace]\n\n[dev-dependencies]\nlibtest-mimic='=0.8.2'\n"),
        ("ripr.toml", "[[analysis.test_harnesses]]\nregistration_id='trials'\ntarget='tests/trials.rs'\nkind='custom_harness'\nadapter='libtest_mimic_v1'\nmarker='libtest_mimic'\n"),
        ("src/lib.rs", "pub fn shipping(kilos: i32) -> i32 { kilos * 4 }\n"),
        ("tests/trials.rs", HARNESS_SOURCE),
        (".cargo/config.toml", "[build]\njobs = 1\n"),
    ] {
        fs::write(root.join(path), text).map_err(|error| error.to_string())?;
    }
    common::fixture_git::fixture_git_ok(&root, &["init", "-q"])?;
    common::fixture_git::fixture_git_ok(&root, &["add", "."])?;
    common::fixture_git::fixture_git_ok(&root, &["-c", "user.name=Cache fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "baseline"])?;
    Ok(root)
}

fn harness_exposure(root: &Path, cache: &Path) -> Result<(serde_json::Value, String), String> {
    let output = run_ripr(root, cache, &["check", "--root", ".", "--format", "repo-exposure-json", "--quiet"])?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!("repo exposure failed: {}\n{stderr}", output.status));
    }
    let value = serde_json::from_slice(&output.stdout).map_err(|error| format!("parse genuine exposure: {error}\n{stderr}"))?;
    Ok((value, stderr))
}

fn shipping_seam(value: &serde_json::Value) -> Result<&serde_json::Value, String> {
    let seams = value["seams"].as_array().ok_or("genuine nonempty seam array")?;
    let matching = seams.iter().filter(|seam| seam["owner"] == "src/lib.rs::shipping").collect::<Vec<_>>();
    match matching.as_slice() {
        [seam] => Ok(seam),
        _ => Err(format!("expected exactly one shipping seam, found {}", matching.len())),
    }
}

#[test]
fn classified_cache_cargo_config_edit_revert_rename_delete_matches_cold() -> Result<(), String> {
    let root = harness_cache_fixture("cargo-config-currentness")?;
    let cache = root.join("target/cache-history");
    let head = common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?;
    let (valid, _) = harness_exposure(&root, &cache)?;
    let seam = shipping_seam(&valid)?;
    assert_eq!(seam["grip_class"], "strongly_gripped");
    assert_eq!(seam["related_tests_total"], 1);
    assert_eq!(seam["related_tests"][0]["name"], "shipping_is_four_per_kilo");
    let (warm, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(warm["seams"], valid["seams"]);
    assert!(trace.contains("cache_hit"), "unchanged state must reuse classified facts: {trace}");
    let config = root.join(".cargo/config.toml");
    fs::write(&config, "[build\n").map_err(|error| error.to_string())?;
    let (changed_warm, _) = harness_exposure(&root, &cache)?;
    let (changed_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-malformed"))?;
    assert_eq!(shipping_seam(&changed_cold)?["grip_class"], "ungripped", "independent fresh metadata must withhold registered trials");
    assert_eq!(shipping_seam(&changed_cold)?["related_tests_total"], 0);
    assert_eq!(changed_warm["seams"], changed_cold["seams"], "same-HEAD Cargo config edit must invalidate stale favorable classified evidence");
    fs::write(&config, "[build]\njobs = 1\n").map_err(|error| error.to_string())?;
    let (restored, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(restored["seams"], valid["seams"]);
    assert!(trace.contains("cache_hit"), "original entries must remain reusable after revert: {trace}");
    fs::write(&config, "[build\n").map_err(|error| error.to_string())?;
    fs::rename(&config, root.join(".cargo/config")).map_err(|error| error.to_string())?;
    let (legacy, _) = harness_exposure(&root, &cache)?;
    let (legacy_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-legacy"))?;
    assert_eq!(shipping_seam(&legacy_cold)?["related_tests_total"], 0);
    assert_eq!(legacy["seams"], legacy_cold["seams"]);
    fs::rename(root.join(".cargo/config"), root.join(".cargo/config.disabled")).map_err(|error| error.to_string())?;
    let (inactive, _) = harness_exposure(&root, &cache)?;
    let (inactive_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-inactive"))?;
    assert_eq!(inactive["seams"], valid["seams"]);
    assert_eq!(inactive["seams"], inactive_cold["seams"]);
    fs::write(&config, "[build\n").map_err(|error| error.to_string())?;
    let _ = harness_exposure(&root, &cache)?;
    fs::remove_file(&config).map_err(|error| error.to_string())?;
    let (deleted, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(deleted["seams"], inactive_cold["seams"]);
    assert!(trace.contains("cache_hit"), "deletion returns to preserved absent-config state: {trace}");
    assert_eq!(common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?, head);
    remove_base(&root)
}

#[test]
fn cache_clear_force_preserves_unrelated_siblings_through_exact_binary() -> Result<(), String> {
    let base = temp_dir("mixed-root");
    let cache_dir = base.join("mixed-cache");
    let layer = cache_dir.join("repo-file-facts");
    fs::create_dir_all(&layer).map_err(|error| error.to_string())?;
    fs::write(layer.join("entry.json"), b"{}").map_err(|error| error.to_string())?;
    let sentinel = cache_dir.join("KEEP.txt");
    fs::write(&sentinel, b"unrelated user data").map_err(|error| error.to_string())?;

    let output = run_ripr(&base, &cache_dir, &["cache", "clear", "--force"])?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let safe = output.status.success()
        && !layer.exists()
        && sentinel.is_file()
        && cache_dir.is_dir()
        && stdout_has_cache_dir_line(&stdout, &cache_dir)
        && stdout.contains("Preserved the cache root and unrelated siblings");
    remove_base(&base)?;

    if !safe {
        return Err(format!(
            "exact-binary cache clear did not preserve mixed-root ownership boundary\nstatus: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status
        ));
    }
    Ok(())
}

#[test]
fn cache_help_is_positional_free_and_prints_the_subcommand_help() -> Result<(), String> {
    // #5024: help used to be recognized only as the sole argument, so
    // `ripr cache status --json --help` exited 2 with a self-referential
    // unknown-argument error. Pin the exact printed help body per route.
    let base = temp_dir("help-anywhere");
    let cache_dir = base.join("help-cache");
    fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;

    let run = |args: &[&str]| run_ripr(&base, &cache_dir, args);
    let status_help = run(&["cache", "status", "--json", "--help"])?;
    let status_help_stdout = String::from_utf8_lossy(&status_help.stdout).to_string();
    let clear_help = run(&["cache", "clear", "--dry-run", "--help"])?;
    let clear_help_stdout = String::from_utf8_lossy(&clear_help.stdout).to_string();
    let family_help = run(&["cache", "--help", "status"])?;
    let family_help_stdout = String::from_utf8_lossy(&family_help.stdout).to_string();
    remove_base(&base)?;

    if !(status_help.status.success()
        && status_help_stdout.contains("Usage: ripr cache status [--json]"))
    {
        return Err(format!(
            "cache status --json --help must print the status help and exit 0\nstatus: {}\nstdout:\n{status_help_stdout}",
            status_help.status
        ));
    }
    if !(clear_help.status.success()
        && clear_help_stdout.contains("Usage: ripr cache clear [--dry-run] [--force]"))
    {
        return Err(format!(
            "cache clear --dry-run --help must print the clear help and exit 0\nstatus: {}\nstdout:\n{clear_help_stdout}",
            clear_help.status
        ));
    }
    if !(family_help.status.success() && family_help_stdout.contains("ripr cache status [--json]"))
    {
        return Err(format!(
            "cache --help status must print the family usage and exit 0\nstatus: {}\nstdout:\n{family_help_stdout}",
            family_help.status
        ));
    }
    Ok(())
}

#[test]
fn cache_status_hint_names_a_published_command_that_clears_the_same_dir() -> Result<(), String> {
    // #4383: the hint used to route to `cargo xtask cache gc`, which does not
    // exist for users of the published crate.
    let base = temp_dir("status-hint");
    let cache_dir = base.join("hint-cache");
    let layer = cache_dir.join("repo-file-facts");
    fs::create_dir_all(&layer).map_err(|error| error.to_string())?;
    fs::write(layer.join("entry.json"), b"{}").map_err(|error| error.to_string())?;

    let run = |args: &[&str]| run_ripr(&base, &cache_dir, args);
    let status = run(&["cache", "status"])?;
    let hint = String::from_utf8_lossy(&status.stderr).to_string();
    let preview = run(&["cache", "clear", "--dry-run"])?;
    let preview_stdout = String::from_utf8_lossy(&preview.stdout).to_string();
    let entry_kept = layer.join("entry.json").is_file();
    remove_base(&base)?;

    let names_published_command = hint.contains("`ripr cache clear --dry-run`")
        && hint.contains("`ripr cache clear --force`")
        && !hint.contains("xtask");
    let preview_targets_same_dir = preview.status.success()
        && stdout_has_cache_dir_line(&preview_stdout, &cache_dir)
        && entry_kept;
    if !(status.status.success() && names_published_command && preview_targets_same_dir) {
        return Err(format!(
            "cache status hint must name a runnable published command for the same dir\nhint:\n{hint}\npreview status: {}\npreview stdout:\n{preview_stdout}",
            preview.status
        ));
    }
    Ok(())
}
