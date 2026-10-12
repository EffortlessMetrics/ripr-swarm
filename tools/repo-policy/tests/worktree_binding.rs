//! A serialized fixture target challenges cached build-script relocation.
//! Normal development targets remain worktree-private. This is not a guarantee
//! for concurrent writers or arbitrary source changes with preserved mtimes.
#![cfg(target_os = "linux")]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[path = "../source_identity.rs"]
mod source_identity;

const PROBE: &str = r#"#[path = "../source_identity.rs"]
mod source_identity;
fn main() -> Result<(), String> {
    let current = source_identity::source_identity(std::path::Path::new("."))?;
    println!("embedded={} current={current}", env!("REPO_POLICY_SOURCE_ID"));
    if current != env!("REPO_POLICY_SOURCE_ID") {
        return Err("probe executable has stale source identity".into());
    }
    Ok(())
}
"#;
const FRONTDOOR: &str = r#"#[path = "../source_identity.rs"]
mod source_identity;
#[test]
fn cargo_selected_executable_matches_current_source() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let expected = source_identity::source_identity(&root)?;
    let executable = env!("CARGO_BIN_EXE_policy-worktree-probe");
    println!("harness_root={}", root.canonicalize().map_err(|e| e.to_string())?.display());
    println!("selected_executable={executable}");
    let output = std::process::Command::new(executable)
        .current_dir(&root).output().map_err(|e| e.to_string())?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success(), "stale executable: {output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(),
        format!("embedded={expected} current={expected}"));
    Ok(())
}
"#;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let root = parent.join(format!(
            "policy-worktree-binding-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(Self(root.canonicalize().map_err(|e| e.to_string())?))
    }

    fn write_root(&self, name: &str) -> Result<PathBuf, String> {
        let root = self.0.join(name);
        for (path, text) in [
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"tools/repo-policy\"]\nresolver = \"2\"\n",
            ),
            (
                "rust-toolchain.toml",
                include_str!("../../../rust-toolchain.toml"),
            ),
            (
                "tools/repo-policy/Cargo.toml",
                "[package]\nname = \"policy-worktree-probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nsha2 = \"0.10\"\n[build-dependencies]\nsha2 = \"0.10\"\n",
            ),
            ("tools/repo-policy/build.rs", include_str!("../build.rs")),
            (
                "tools/repo-policy/source_identity.rs",
                include_str!("../source_identity.rs"),
            ),
            ("tools/repo-policy/src/main.rs", PROBE),
            ("tools/repo-policy/tests/frontdoor.rs", FRONTDOOR),
            ("tools/repo-policy/identity.txt", name),
        ] {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(path, text).map_err(|e| e.to_string())?;
        }
        Ok(root)
    }

    fn cargo(&self, root: &Path, args: &[&str]) -> Result<Output, String> {
        // timeout owns the Cargo/rustc process group; no inherited outer target
        // or build flags can turn this fixture into a concurrent shared build.
        let mut command = std::process::Command::new("timeout");
        command
            .args(["--kill-after=5s", "60s", env!("CARGO")])
            .args(args)
            .current_dir(root)
            .env("CARGO_TARGET_DIR", self.0.join("serialized-target"))
            .env("CARGO_BUILD_JOBS", "1")
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "0")
            .env("CARGO_PROFILE_TEST_DEBUG", "0");
        for name in [
            "CARGO_BUILD_TARGET",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_LOG",
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        ] {
            command.env_remove(name);
        }
        command.output().map_err(|e| e.to_string())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.0) {
            eprintln!("retain fixture {}: {e}", self.0.display());
        }
    }
}

fn backdate(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            backdate(&entry.map_err(|e| e.to_string())?.path())?;
        }
    }
    fs::File::open(path)
        .and_then(|file| {
            file.set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_hours(1)))
        })
        .map_err(|e| format!("backdate {}: {e}", path.display()))
}

fn messages(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn artifact<'a>(messages: &'a [Value], kind: &str) -> Result<&'a Value, String> {
    messages
        .iter()
        .find(|message| {
            message["reason"] == "compiler-artifact"
                && message["target"]["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|value| value == kind))
                && message["package_id"]
                    .as_str()
                    .is_some_and(|id| id.contains("#policy-worktree-probe@"))
        })
        .ok_or_else(|| format!("missing probe {kind} compiler artifact"))
}

fn verify_run(root: &Path, output: &Output, cached: bool) -> Result<PathBuf, String> {
    let messages = messages(output);
    // A compiler/setup failure must never count as the intended stale-identity
    // discriminator: compilation and a fresh B harness precede the oracle.
    assert!(
        messages
            .iter()
            .any(|message| message["reason"] == "build-finished" && message["success"] == true),
        "probe compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(artifact(&messages, "custom-build")?["fresh"], cached);
    assert_eq!(artifact(&messages, "test")?["fresh"], false);
    let binary = artifact(&messages, "bin")?;
    let executable = binary["executable"]
        .as_str()
        .ok_or("missing Cargo-selected executable")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("harness_root={}", root.display())),
        "wrong harness root: {stdout}"
    );
    assert!(
        stdout.contains(&format!("selected_executable={executable}")),
        "harness did not use Cargo-selected executable: {stdout}"
    );
    let frontdoor = stdout
        .lines()
        .filter(|line| serde_json::from_str::<Value>(line).is_err())
        .collect::<Vec<_>>()
        .join("\n");
    println!(
        "compiled root={} cached_script={cached} harness_fresh=false selected_executable={executable}",
        root.display()
    );
    assert!(
        output.status.success(),
        "behavioral stale-identity failure after compilation:\n{frontdoor}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = source_identity::source_identity(root)?;
    assert!(
        stdout.contains(&format!("embedded={expected} current={expected}")),
        "wrong production source digest: {stdout}"
    );
    let script = messages
        .iter()
        .find(|message| {
            message["reason"] == "build-script-executed"
                && message["package_id"] == binary["package_id"]
        })
        .ok_or("missing production build-script output")?;
    let out = Path::new(
        script["out_dir"]
            .as_str()
            .ok_or("missing build-script out_dir")?,
    )
    .parent()
    .ok_or("missing build-script output parent")?
    .join("output");
    let text = fs::read_to_string(out).map_err(|e| e.to_string())?;
    let watched: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("cargo:rerun-if-changed="))
        .collect();
    assert_eq!(watched.len(), 4);
    // Check B watches after its behavioral oracle. This lets an independent
    // relative-watch mutant reach B's stale executable instead of failing A
    // merely because of its watch spelling.
    if cached {
        for (path, relative) in watched.iter().zip([
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "tools/repo-policy",
        ]) {
            assert!(
                Path::new(path).is_absolute(),
                "relative build-script watch: {path}"
            );
            assert_eq!(
                Path::new(path).canonicalize().map_err(|e| e.to_string())?,
                root.join(relative)
                    .canonicalize()
                    .map_err(|e| e.to_string())?
            );
        }
    }
    assert!(
        text.lines()
            .any(|line| line == format!("cargo:rustc-env=REPO_POLICY_SOURCE_ID={expected}"))
    );
    assert!(text.lines().any(|line| line
        == format!(
            "cargo:rustc-env=REPO_POLICY_COMPILER_ID={}",
            env!("REPO_POLICY_COMPILER_ID")
        )));
    println!(
        "verified root={} cached_script={cached} source_id={expected}\n{text}",
        root.display()
    );
    let harness = artifact(&messages, "test")?["executable"]
        .as_str()
        .ok_or("missing Cargo-selected harness executable")?;
    Ok(PathBuf::from(harness))
}

#[test]
fn cached_production_build_script_binds_the_current_backdated_worktree() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let a = fixture.write_root("a")?;
    let b = fixture.write_root("b")?;
    let lock = fixture.cargo(&a, &["generate-lockfile", "--offline"])?;
    assert!(lock.status.success(), "fixture lockfile setup: {lock:?}");
    fs::copy(a.join("Cargo.lock"), b.join("Cargo.lock")).map_err(|e| e.to_string())?;
    backdate(&a)?;
    backdate(&b)?;
    assert_ne!(
        source_identity::source_identity(&a)?,
        source_identity::source_identity(&b)?
    );
    let args = [
        "test",
        "--offline",
        "--locked",
        "--message-format=json",
        "-p",
        "policy-worktree-probe",
        "--test",
        "frontdoor",
        "--",
        "--nocapture",
    ];
    let harness = verify_run(&a, &fixture.cargo(&a, &args)?, false)?;
    // Remove only this fixture's Cargo-selected harness output to force a new
    // B harness without touching any backdated source or watched directory.
    // Touching B's test source would also dirty a relative directory watch and
    // conceal the preserved-mtime regression this fixture must discriminate.
    assert!(harness.starts_with(fixture.0.join("serialized-target")));
    fs::remove_file(harness).map_err(|e| e.to_string())?;
    verify_run(&b, &fixture.cargo(&b, &args)?, true)?;
    Ok(())
}
