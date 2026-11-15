//! ripr works with no network (air-gapped CI, sandboxes), and the one route
//! where Git itself reaches for the network, a partial clone fetching objects
//! it never downloaded, fails fast with a cause the user can act on.
//!
//! Every command here runs with the network sealed off. The environment seal
//! points every proxy at a closed local port, limits Git to the `file`
//! transport and puts Cargo offline, on every platform. On Linux, when an
//! unprivileged network namespace is available (`unshare -rn`), each command
//! also runs inside one with no interfaces up, so even a direct socket fails.
//! The test prints which seal applied; the environment seal is never skipped.
use super::{DeclaredHunkScratch, unique_external_workspace};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::OnceLock;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment that makes any attempted network use fail at once.
const NETWORK_SEAL: [(&str, &str); 10] = [
    ("HTTP_PROXY", "http://127.0.0.1:9"),
    ("HTTPS_PROXY", "http://127.0.0.1:9"),
    ("ALL_PROXY", "http://127.0.0.1:9"),
    ("http_proxy", "http://127.0.0.1:9"),
    ("https_proxy", "http://127.0.0.1:9"),
    ("all_proxy", "http://127.0.0.1:9"),
    ("NO_PROXY", ""),
    ("no_proxy", ""),
    ("GIT_ALLOW_PROTOCOL", "file"),
    ("CARGO_NET_OFFLINE", "true"),
];

/// Whether this host can run a child in a fresh network namespace.
fn network_namespace_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let available = cfg!(target_os = "linux")
            && Command::new("unshare")
                .args(["-rn", "true"])
                .output()
                .is_ok_and(|output| output.status.success());
        eprintln!(
            "offline_network: seal = environment{}",
            if available {
                " + network namespace (unshare -rn)"
            } else {
                " only (no unprivileged network namespace on this host)"
            }
        );
        available
    })
}

fn sealed_ripr(cwd: &Path, args: &[&str]) -> std::io::Result<Output> {
    let ripr = env!("CARGO_BIN_EXE_ripr");
    let mut command = if network_namespace_available() {
        let mut command = Command::new("unshare");
        command.args(["-rn", ripr]);
        command
    } else {
        Command::new(ripr)
    };
    command
        .args(args)
        .current_dir(cwd)
        .env("RIPR_CACHE_DIR", "")
        .envs(NETWORK_SEAL);
    command.output()
}

fn git(cwd: &Path, args: &[&str]) -> TestResult {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=ripr",
            "-c",
            "user.email=ripr@example.invalid",
        ])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(cwd)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// A one-crate repository whose last commit weakens a boundary the test
/// checks only loosely.
fn write_committed_change(repo: &Path) -> TestResult {
    fs::create_dir_all(repo.join("src"))?;
    fs::create_dir_all(repo.join("tests"))?;
    git(repo, &["init", "-q"])?;
    fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"limits\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    fs::write(repo.join(".gitignore"), "/target/\n")?;
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn over(x: i32) -> bool { x > 3 }\n",
    )?;
    fs::write(
        repo.join("tests/over.rs"),
        "#[test]\nfn big_is_over() { assert!(limits::over(5)); }\n",
    )?;
    git(repo, &["add", "-A"])?;
    git(repo, &["commit", "-q", "-m", "base"])?;
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn over(x: i32) -> bool { x >= 3 }\n",
    )?;
    git(repo, &["commit", "-q", "-a", "-m", "change"])?;
    Ok(())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn everyday_commands_complete_with_the_network_sealed() -> TestResult {
    let outer = unique_external_workspace("offline-commands")?;
    let _cleanup = DeclaredHunkScratch(outer.clone());
    let repo = outer.join("repo");
    write_committed_change(&repo)?;

    let check = sealed_ripr(&repo, &["check", "--base", "HEAD~1", "--format", "json"])?;
    assert!(check.status.success(), "check: {}", text(&check.stderr));
    let report: Value = serde_json::from_slice(&check.stdout)?;
    assert!(
        report["summary"]["findings"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "sealed check must analyze the change, not succeed empty: {report}"
    );

    for args in [
        &["doctor"][..],
        &["check", "--base", "HEAD~1"],
        &["pilot", "--root", "."],
        &[
            "first-pr", "--root", ".", "--base", "HEAD~1", "--head", "HEAD",
        ],
        &["cache", "status", "--json"],
        &["help", "--json"],
        &["init", "--ci", "github"],
    ] {
        let output = sealed_ripr(&repo, args)?;
        assert!(
            output.status.success(),
            "ripr {args:?} must complete with no network (exit {:?}): {}",
            output.status.code(),
            text(&output.stderr)
        );
    }
    Ok(())
}

/// A blob-filtered clone holds only the checked-out tree. Diffing against an
/// older base needs that base's blobs, so Git fetches them on demand from the
/// promisor remote. With the remote unreachable, ripr must name the partial
/// clone and the network, not send the user to `git fsck` for a healthy
/// object store.
#[cfg(unix)]
#[test]
fn offline_partial_clone_names_the_missing_download_not_object_damage() -> TestResult {
    let outer = unique_external_workspace("offline-partial-clone")?;
    let _cleanup = DeclaredHunkScratch(outer.clone());
    let origin = outer.join("origin");
    write_committed_change(&origin)?;
    git(&origin, &["config", "uploadpack.allowFilter", "true"])?;
    let origin_url = format!(
        "file://{}",
        origin.to_str().ok_or("origin path must be UTF-8")?
    );
    git(
        &outer,
        &["clone", "-q", "--filter=blob:none", &origin_url, "clone"],
    )?;
    // The remote is gone: the base-side blob can no longer be fetched.
    fs::rename(&origin, outer.join("origin-unreachable"))?;

    let started = std::time::Instant::now();
    let output = sealed_ripr(&outer.join("clone"), &["check", "--base", "HEAD~1"])?;
    let elapsed = started.elapsed();
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("partial clone"), "{stderr}");
    assert!(stderr.contains("network"), "{stderr}");
    assert!(
        !stderr.contains("damaged object store") && !stderr.contains("git fsck"),
        "an unreachable remote is not local object damage: {stderr}"
    );
    assert!(
        elapsed < std::time::Duration::from_mins(1),
        "the failed fetch must fail fast, took {elapsed:?}"
    );
    Ok(())
}
