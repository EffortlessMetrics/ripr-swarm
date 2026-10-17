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
    run_ripr_with_home_environment(base, cache_dir, args, None)
}

fn run_ripr_with_home_environment(
    base: &Path,
    cache_dir: &Path,
    args: &[&str],
    home: Option<HomeEnvironment>,
) -> Result<std::process::Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .args(args)
        .current_dir(base)
        .env(CACHE_DIR_ENV, cache_dir)
        .env("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1");
    if let Some(home) = home {
        configure_home_environment(&mut command, home)?;
    }
    command
        .output()
        .map_err(|error| format!("run exact ripr binary {args:?}: {error}"))
}

#[derive(Clone, Copy, Debug)]
enum HomeEnvironment {
    Absent,
    EmptyHome,
    EmptyCargoHome,
    RelativeHome,
    RelativeCargoHome,
}

fn configure_home_environment(command: &mut Command, case: HomeEnvironment) -> Result<(), String> {
    // Keep the already selected toolchain available while changing only the
    // child process's Cargo-home inputs. Never change the test process's home.
    let rustup_home = home::rustup_home().map_err(|error| error.to_string())?;
    let rustup_home = if rustup_home.is_absolute() {
        rustup_home
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(rustup_home)
    };
    command
        .env("RUSTUP_HOME", rustup_home)
        .env_remove("CARGO_HOME")
        .env_remove("HOME")
        .env_remove("USERPROFILE");
    match case {
        HomeEnvironment::Absent => {}
        HomeEnvironment::EmptyHome => {
            command.env("HOME", "").env("USERPROFILE", "");
        }
        HomeEnvironment::EmptyCargoHome => {
            command.env("CARGO_HOME", "");
        }
        HomeEnvironment::RelativeHome => {
            command.env(
                if cfg!(windows) { "USERPROFILE" } else { "HOME" },
                "relative-home",
            );
        }
        HomeEnvironment::RelativeCargoHome => {
            command.env("CARGO_HOME", "relative-cargo");
        }
    }
    Ok(())
}

#[test]
fn classified_cache_cargo_home_fallback_and_relative_home_match_cold() -> Result<(), String> {
    let root = temp_dir("cargo-home-currentness");
    for dir in ["src", "relative-home/.cargo", "relative-cargo"] {
        fs::create_dir_all(root.join(dir)).map_err(|error| error.to_string())?;
    }
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='cache_home_fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(root.join("src/lib.rs"),
        "pub fn shipping(kilos: i32) -> i32 { kilos * 4 }\n#[cfg(test)] mod tests { #[test] fn shipping_is_four_per_kilo() { assert_eq!(super::shipping(3), 12); } }\n")
        .map_err(|error| error.to_string())?;
    for path in [
        "relative-home/.cargo/config.toml",
        "relative-cargo/config.toml",
    ] {
        fs::write(root.join(path), "[build]\njobs=1\n").map_err(|error| error.to_string())?;
    }
    common::fixture_git::fixture_git_ok(&root, &["init", "--quiet"])?;
    common::fixture_git::fixture_git_ok(&root, &["add", "."])?;
    common::fixture_git::fixture_git_ok(
        &root,
        &[
            "-c",
            "user.name=Cache fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "Owned Cargo home fixture",
        ],
    )?;
    let head = common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?;
    let caller = root.parent().ok_or("fixture parent")?;
    let root_arg = root.to_str().ok_or("fixture root must be UTF-8")?;
    for case in [
        HomeEnvironment::Absent,
        HomeEnvironment::EmptyHome,
        HomeEnvironment::EmptyCargoHome,
        HomeEnvironment::RelativeHome,
        HomeEnvironment::RelativeCargoHome,
    ] {
        let mut cargo = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cargo
            .current_dir(&root)
            .args([
                "metadata",
                "--offline",
                "--no-deps",
                "--format-version",
                "1",
            ])
            .env("CARGO_TARGET_DIR", root.join("target/cargo-home-oracle"));
        configure_home_environment(&mut cargo, case)?;
        let metadata = cargo.output().map_err(|error| error.to_string())?;
        if !metadata.status.success() {
            return Err(format!(
                "independent Cargo metadata must work for {case:?}: {}",
                String::from_utf8_lossy(&metadata.stderr)
            ));
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&metadata.stdout).map_err(|error| error.to_string())?;
        assert_eq!(metadata["packages"][0]["name"], "cache_home_fixture");
        let cache = root.join(format!("target/home-{case:?}"));
        let run = |cache: &Path| -> Result<(serde_json::Value, String), String> {
            let output = run_ripr_with_home_environment(
                caller,
                cache,
                &[
                    "check",
                    "--root",
                    root_arg,
                    "--format",
                    "repo-exposure-json",
                    "--quiet",
                ],
                Some(case),
            )?;
            if !output.status.success() {
                return Err(format!(
                    "home fallback CLI {case:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok((
                serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?,
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        };
        let (initial, _) = run(&cache)?;
        let seam = shipping_seam(&initial)?;
        assert_eq!(seam["seam_id"], "23276a9e23c0dce5");
        assert_eq!(seam["grip_class"], "strongly_gripped");
        assert_eq!(seam["related_tests_total"], 1);
        assert_eq!(
            seam["related_tests"][0]["name"],
            "shipping_is_four_per_kilo"
        );
        let corpus = initial
            .pointer("/source_subject/corpus")
            .ok_or("usable Cargo home must publish the actual corpus provenance field")?;
        let digest = corpus["files_content_hash"]
            .as_str()
            .ok_or("nonempty Rust corpus digest")?;
        assert!(
            !digest.is_empty(),
            "nonempty comparable corpus for {case:?}"
        );
        let (warm, trace) = run(&cache)?;
        assert!(
            trace.contains("phase=total status=cache_hit"),
            "usable fallback must reuse classified facts for {case:?}: {trace}"
        );
        let (cold, _) = run(&root.join(format!("target/cold-{case:?}")))?;
        assert_eq!(warm["seams"], initial["seams"]);
        assert_eq!(warm["seams"], cold["seams"]);
        assert_eq!(warm.pointer("/source_subject/corpus"), Some(corpus));
        assert_eq!(cold.pointer("/source_subject/corpus"), Some(corpus));
        let config = match case {
            HomeEnvironment::RelativeHome => Some(root.join("relative-home/.cargo/config.toml")),
            HomeEnvironment::RelativeCargoHome => Some(root.join("relative-cargo/config.toml")),
            _ => None,
        };
        if let Some(config) = config {
            fs::write(&config, "[build\n").map_err(|error| error.to_string())?;
            let (edited, edited_trace) = run(&cache)?;
            let (edited_cold, _) = run(&root.join(format!("target/edited-cold-{case:?}")))?;
            assert!(
                !edited_trace.contains("phase=total status=cache_hit"),
                "relative home edit must miss the old classified key despite different caller cwd"
            );
            assert_eq!(edited["seams"], edited_cold["seams"]);
            assert_eq!(edited.pointer("/source_subject/corpus"), Some(corpus));
            assert_eq!(edited_cold.pointer("/source_subject/corpus"), Some(corpus));
            fs::write(&config, "[build]\njobs=1\n").map_err(|error| error.to_string())?;
            let (restored, trace) = run(&cache)?;
            assert_eq!(restored.pointer("/source_subject/corpus"), Some(corpus));
            assert!(
                trace.contains("phase=total status=cache_hit"),
                "relative home revert must reuse preserved facts: {trace}"
            );
        }
    }
    assert_eq!(
        common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?,
        head
    );
    remove_base(&root)
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
        (
            "Cargo.toml",
            "[package]\nname='cache_harness_fixture'\nversion='0.1.0'\nedition='2024'\n\n[[test]]\nname='trials'\nharness=false\n\n[workspace]\n\n[dev-dependencies]\nlibtest-mimic='=0.8.2'\n",
        ),
        (
            "ripr.toml",
            "[[analysis.test_harnesses]]\nregistration_id='trials'\ntarget='tests/trials.rs'\nkind='custom_harness'\nadapter='libtest_mimic_v1'\nmarker='libtest_mimic'\n",
        ),
        (
            "src/lib.rs",
            "pub fn shipping(kilos: i32) -> i32 { kilos * 4 }\n",
        ),
        ("tests/trials.rs", HARNESS_SOURCE),
        (".cargo/config.toml", "[build]\njobs = 1\n"),
    ] {
        fs::write(root.join(path), text).map_err(|error| error.to_string())?;
    }
    common::fixture_git::fixture_git_ok(&root, &["init", "-q"])?;
    common::fixture_git::fixture_git_ok(&root, &["add", "."])?;
    common::fixture_git::fixture_git_ok(
        &root,
        &[
            "-c",
            "user.name=Cache fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "baseline",
        ],
    )?;
    Ok(root)
}

fn harness_exposure(root: &Path, cache: &Path) -> Result<(serde_json::Value, String), String> {
    harness_report(root, cache, "repo-exposure-json")
}

fn harness_report(
    root: &Path,
    cache: &Path,
    format: &str,
) -> Result<(serde_json::Value, String), String> {
    let output = run_ripr(
        root,
        cache,
        &["check", "--root", ".", "--format", format, "--quiet"],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!("repo exposure failed: {}\n{stderr}", output.status));
    }
    let value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse genuine exposure: {error}\n{stderr}"))?;
    Ok((value, stderr))
}

#[test]
fn compact_classified_cache_cargo_config_change_matches_cold_and_preserves_revert_hit()
-> Result<(), String> {
    let root = harness_cache_fixture("compact-cargo-config-currentness")?;
    let cache = root.join("target/cache-history");
    let run = |cache: &Path| harness_report(&root, cache, "repo-exposure-summary-json");
    let (original, _) = run(&cache)?;
    assert_eq!(original["metrics"]["raw_seams"], 1);
    assert_eq!(original["metrics"]["grip_class"]["strongly_gripped"], 1);
    assert_eq!(original["top_files"][0]["file"], "src/lib.rs");
    let (warm, trace) = run(&cache)?;
    assert_eq!(warm, original);
    assert!(
        trace.contains("cache_hit"),
        "compact facts must be reused: {trace}"
    );
    fs::write(root.join(".cargo/config.toml"), "[build\n").map_err(|error| error.to_string())?;
    let (edited, _) = run(&cache)?;
    let (cold, _) = run(&root.join("target/cache-cold-malformed"))?;
    assert_eq!(cold["metrics"]["raw_seams"], 1);
    assert_eq!(cold["metrics"]["grip_class"]["strongly_gripped"], 0);
    assert_eq!(cold["metrics"]["grip_class"]["ungripped"], 1);
    assert_eq!(
        edited, cold,
        "compact warm output must reflect the current Cargo premise"
    );
    fs::write(root.join(".cargo/config.toml"), "[build]\njobs = 1\n")
        .map_err(|error| error.to_string())?;
    let (restored, trace) = run(&cache)?;
    assert_eq!(restored, original);
    assert!(
        trace.contains("cache_hit"),
        "compact original entry must survive revert: {trace}"
    );
    remove_base(&root)
}

fn shipping_seam(value: &serde_json::Value) -> Result<&serde_json::Value, String> {
    let seams = value["seams"]
        .as_array()
        .ok_or("genuine nonempty seam array")?;
    let matching = seams
        .iter()
        .filter(|seam| seam["owner"] == "src/lib.rs::shipping")
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [seam] => Ok(seam),
        _ => Err(format!(
            "expected exactly one shipping seam, found {}",
            matching.len()
        )),
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
    assert_eq!(
        seam["related_tests"][0]["name"],
        "shipping_is_four_per_kilo"
    );
    let (warm, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(warm["seams"], valid["seams"]);
    assert!(
        trace.contains("cache_hit"),
        "unchanged state must reuse classified facts: {trace}"
    );
    let config = root.join(".cargo/config.toml");
    fs::write(&config, "[build\n").map_err(|error| error.to_string())?;
    let (changed_warm, _) = harness_exposure(&root, &cache)?;
    let (changed_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-malformed"))?;
    assert_eq!(
        shipping_seam(&changed_cold)?["grip_class"],
        "ungripped",
        "independent fresh metadata must withhold registered trials"
    );
    assert_eq!(shipping_seam(&changed_cold)?["related_tests_total"], 0);
    assert_eq!(
        changed_warm["seams"], changed_cold["seams"],
        "same-HEAD Cargo config edit must invalidate stale favorable classified evidence"
    );
    fs::write(&config, "[build]\njobs = 1\n").map_err(|error| error.to_string())?;
    let (restored, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(restored["seams"], valid["seams"]);
    assert!(
        trace.contains("cache_hit"),
        "original entries must remain reusable after revert: {trace}"
    );
    fs::write(&config, "[build\n").map_err(|error| error.to_string())?;
    fs::rename(&config, root.join(".cargo/config")).map_err(|error| error.to_string())?;
    let (legacy, _) = harness_exposure(&root, &cache)?;
    let (legacy_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-legacy"))?;
    assert_eq!(shipping_seam(&legacy_cold)?["related_tests_total"], 0);
    assert_eq!(legacy["seams"], legacy_cold["seams"]);
    fs::rename(
        root.join(".cargo/config"),
        root.join(".cargo/config.disabled"),
    )
    .map_err(|error| error.to_string())?;
    let (inactive, _) = harness_exposure(&root, &cache)?;
    let (inactive_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-inactive"))?;
    assert_eq!(inactive["seams"], valid["seams"]);
    assert_eq!(inactive["seams"], inactive_cold["seams"]);
    fs::write(&config, "[build\n").map_err(|error| error.to_string())?;
    let _ = harness_exposure(&root, &cache)?;
    fs::remove_file(&config).map_err(|error| error.to_string())?;
    let (deleted, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(deleted["seams"], inactive_cold["seams"]);
    assert!(
        trace.contains("cache_hit"),
        "deletion returns to preserved absent-config state: {trace}"
    );
    assert_eq!(
        common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?,
        head
    );
    remove_base(&root)
}

#[test]
fn classified_cache_saved_test_edit_revert_rename_delete_matches_cold() -> Result<(), String> {
    let root = harness_cache_fixture("test-currentness-journey")?;
    let cache = root.join("target/cache-history");
    let head = common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?;
    let (original, _) = harness_exposure(&root, &cache)?;
    assert_eq!(shipping_seam(&original)?["grip_class"], "strongly_gripped");
    let test = root.join("tests/trials.rs");
    fs::write(
        &test,
        HARNESS_SOURCE.replace(
            "assert_eq!(cache_harness_fixture::shipping(3), 12);",
            "let _ = cache_harness_fixture::shipping(3);",
        ),
    )
    .map_err(|error| error.to_string())?;
    let (edited, _) = harness_exposure(&root, &cache)?;
    let (edited_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-edited"))?;
    assert_eq!(edited["seams"], edited_cold["seams"]);
    assert_ne!(
        shipping_seam(&edited)?["grip_class"],
        "strongly_gripped",
        "removing the exact oracle must remove its grip"
    );
    fs::write(&test, HARNESS_SOURCE).map_err(|error| error.to_string())?;
    let (reverted, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(reverted["seams"], original["seams"]);
    assert!(
        trace.contains("cache_hit"),
        "source revert must reuse preserved original facts: {trace}"
    );
    fs::rename(&test, root.join("tests/renamed.rs")).map_err(|error| error.to_string())?;
    let (renamed, _) = harness_exposure(&root, &cache)?;
    let (renamed_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-renamed"))?;
    assert_eq!(renamed["seams"], renamed_cold["seams"]);
    assert_eq!(
        shipping_seam(&renamed)?["related_tests_total"],
        0,
        "registration is bound to the original target"
    );
    fs::rename(root.join("tests/renamed.rs"), &test).map_err(|error| error.to_string())?;
    let (returned, trace) = harness_exposure(&root, &cache)?;
    assert_eq!(returned["seams"], original["seams"]);
    assert!(trace.contains("cache_hit"));
    fs::remove_file(&test).map_err(|error| error.to_string())?;
    let (deleted, _) = harness_exposure(&root, &cache)?;
    let (deleted_cold, _) = harness_exposure(&root, &root.join("target/cache-cold-deleted"))?;
    assert_eq!(deleted["seams"], deleted_cold["seams"]);
    assert_eq!(shipping_seam(&deleted)?["related_tests_total"], 0);
    assert_eq!(
        common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?,
        head
    );
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
#[cfg(target_os = "linux")]
fn directory_swap_cache_snapshot(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            snapshots.extend(directory_swap_cache_snapshot(&path)?);
        } else {
            snapshots.push((
                path.clone(),
                fs::read(path).map_err(|error| error.to_string())?,
            ));
        }
    }
    Ok(snapshots)
}

#[cfg(target_os = "linux")]
fn directory_swap_preserves_current_cache_output(format: &str) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    // Literal expectations were derived from the parser's seam contract,
    // independently of the fingerprint or a cache-isolated comparison run.
    const BEFORE: &str = "pub fn alpha(x: i32) -> bool { x >= 2 }\n";
    const AFTER: &str = "pub fn bravo(x: i32) -> bool { true }  \n";
    assert_eq!(BEFORE.len(), AFTER.len());
    let root = temp_dir(format);
    let source = root.join("src/lib.rs");
    let staged = root.join("target/staged-src/lib.rs");
    fs::create_dir_all(source.parent().ok_or("source parent")?)
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(staged.parent().ok_or("staged parent")?)
        .map_err(|error| error.to_string())?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='swap_fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(root.join(".gitignore"), "target/\n").map_err(|error| error.to_string())?;
    fs::write(
        root.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"swap_fixture\"\nversion = \"0.1.0\"\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(&source, BEFORE).map_err(|error| error.to_string())?;
    fs::write(&staged, AFTER).map_err(|error| error.to_string())?;
    common::fixture_git::fixture_git_ok(&root, &["init", "-q"])?;
    common::fixture_git::fixture_git_ok(&root, &["add", "."])?;
    common::fixture_git::fixture_git_ok(
        &root,
        &[
            "-c",
            "user.name=Cache fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "baseline",
        ],
    )?;

    let old_file = fs::File::options()
        .write(true)
        .open(&source)
        .map_err(|error| error.to_string())?;
    let new_file = fs::File::options()
        .write(true)
        .open(&staged)
        .map_err(|error| error.to_string())?;
    let mtime = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let size_and_mtime = |metadata: &fs::Metadata| -> Result<_, String> {
        Ok((
            metadata.len(),
            metadata.modified().map_err(|error| error.to_string())?,
        ))
    };
    // Real ctimes need not coincide. The seam-cache stat_identity tests hold
    // legacy fields equal in recorded tuples and independently check extraction;
    // this journey retains the actual CLI, output, restart and cache contracts.
    old_file
        .set_modified(mtime)
        .map_err(|error| error.to_string())?;
    new_file
        .set_modified(mtime)
        .map_err(|error| error.to_string())?;
    let before_stat = fs::metadata(&source).map_err(|error| error.to_string())?;
    let replacement_stat = fs::metadata(&staged).map_err(|error| error.to_string())?;
    assert_eq!(
        size_and_mtime(&before_stat)?,
        size_and_mtime(&replacement_stat)?
    );
    assert_ne!(
        (before_stat.dev(), before_stat.ino()),
        (replacement_stat.dev(), replacement_stat.ino())
    );
    let cache = root.join("target/cache-history");
    let assert_subject = |value: &serde_json::Value, replaced: bool| -> Result<(), String> {
        let count = if replaced { 1 } else { 2 };
        if format == "repo-exposure-json" {
            let seams = value["seams"].as_array().ok_or("nonempty seam array")?;
            assert_eq!(seams.len(), count);
            let owner = if replaced {
                "src/lib.rs::bravo"
            } else {
                "src/lib.rs::alpha"
            };
            for seam in seams {
                assert_eq!(seam["owner"], owner);
                assert_eq!(seam["expression"], if replaced { "true" } else { "x >= 2" });
                assert_eq!(seam["grip_class"], "ungripped");
                assert_eq!(seam["related_tests_total"], 0);
            }
            let mut kinds = seams
                .iter()
                .map(|seam| seam["kind"].as_str().ok_or("seam kind"))
                .collect::<Result<Vec<_>, _>>()?;
            kinds.sort();
            let expected = if replaced {
                vec!["return_value"]
            } else {
                vec!["predicate_boundary", "return_value"]
            };
            assert_eq!(kinds, expected);
        } else {
            assert_eq!(value["metrics"]["raw_seams"], count);
            assert_eq!(value["metrics"]["grip_class"]["ungripped"], count);
            assert_eq!(
                value["top_files"]
                    .as_array()
                    .ok_or("top files array")?
                    .len(),
                1
            );
            assert_eq!(value["top_files"][0]["file"], "src/lib.rs");
            assert_eq!(value["top_files"][0]["raw_seams"], count);
        }
        Ok(())
    };
    let run = |cache: &Path| -> Result<_, String> {
        let (mut value, trace) = harness_report(&root, cache, format)?;
        // A full artifact's generation time and its enclosing content digest
        // vary between invocations. Keep all semantic and input identity fields.
        if let Some(artifact) = value
            .get_mut("artifact")
            .and_then(serde_json::Value::as_object_mut)
        {
            artifact.remove("generated_at");
            artifact.remove("content_sha256");
        }
        Ok((value, trace))
    };
    let (cold, _) = run(&cache)?;
    assert_subject(&cold, false)?;
    let (warm, trace) = run(&cache)?;
    assert_eq!(warm, cold);
    assert!(
        trace.contains("cache_hit"),
        "initial restarted process must hit: {trace}"
    );
    if format == "repo-exposure-json" {
        assert!(
            trace.contains("phase=collect_workspace_state status=fingerprint"),
            "stat mapping must actually be consumed: {trace}"
        );
    }
    let preserved = directory_swap_cache_snapshot(&cache)?;
    assert!(
        !preserved.is_empty(),
        "real producer must publish cache entries"
    );

    // Renaming containing directories leaves child inode timestamps intact.
    // Both discovery inputs remain regular files at the same relative path.
    fs::rename(root.join("src"), root.join("target/old-src")).map_err(|error| error.to_string())?;
    fs::rename(root.join("target/staged-src"), root.join("src"))
        .map_err(|error| error.to_string())?;
    let after_stat = fs::metadata(&source).map_err(|error| error.to_string())?;
    assert_eq!(size_and_mtime(&before_stat)?, size_and_mtime(&after_stat)?);
    assert_eq!(
        (replacement_stat.ctime(), replacement_stat.ctime_nsec()),
        (after_stat.ctime(), after_stat.ctime_nsec()),
        "renaming the containing directory must preserve the child ctime"
    );
    assert_ne!(before_stat.ino(), after_stat.ino());
    assert_eq!(
        fs::read_to_string(&source).map_err(|error| error.to_string())?,
        AFTER
    );
    let (fresh, _) = run(&root.join("target/cache-independent"))?;
    assert_subject(&fresh, true)?;
    let (recovered, _) = run(&cache)?;
    assert_eq!(
        recovered, fresh,
        "directory replacement must match all current-input cold semantics"
    );
    let (restarted, trace) = run(&cache)?;
    assert_eq!(restarted, fresh);
    assert!(
        trace.contains("cache_hit"),
        "replacement facts must warm-hit after restart: {trace}"
    );
    // No cache clearing: the earlier source and both fact generations survive.
    assert_eq!(
        fs::read_to_string(root.join("target/old-src/lib.rs")).map_err(|error| error.to_string())?,
        BEFORE
    );
    for (path, bytes) in preserved {
        assert_eq!(
            fs::read(path).map_err(|error| error.to_string())?,
            bytes,
            "earlier cache entries must remain intact"
        );
    }
    remove_base(&root)
}

#[cfg(target_os = "linux")]
#[test]
fn full_directory_swap_cache_matches_cold_after_restart() -> Result<(), String> {
    directory_swap_preserves_current_cache_output("repo-exposure-json")
}

#[cfg(target_os = "linux")]
#[test]
fn compact_directory_swap_cache_matches_cold_after_restart() -> Result<(), String> {
    directory_swap_preserves_current_cache_output("repo-exposure-summary-json")
}
