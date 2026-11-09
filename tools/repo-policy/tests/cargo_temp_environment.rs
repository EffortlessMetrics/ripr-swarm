//! Exercise the checked-in Cargo temp defaults through a real build script and
//! executable. Linux Cargo propagation is covered; this is not MSVC link proof.
#![cfg(target_os = "linux")]

#[path = "../test_support.rs"]
mod test_support;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

const TEMP_NAMES: [&str; 3] = ["TEMP", "TMP", "TMPDIR"];
const PROBE: &str = r#"pub fn observe(kind: &str) -> Result<(), Box<dyn std::error::Error>> {
    let stage = std::env::var("RIPR_TEMP_PROBE_STAGE")?;
    let receipts = std::path::PathBuf::from(std::env::var_os("RIPR_TEMP_PROBE_RECEIPTS")
        .ok_or("missing fixture receipt root")?);
    let mut observed = format!("stage={stage}\nkind={kind}\n");
    for name in ["TEMP", "TMP", "TMPDIR"] {
        let path = std::path::PathBuf::from(std::env::var_os(name)
            .ok_or("missing temp variable")?);
        observed.push_str(&format!("{name}={}\n", path.display()));
        // The bootstrap already compiled this script. Creating the observed
        // path lets the old force=true policy reach the behavioral oracle
        // instead of failing first because its checkout target is absent.
        std::fs::create_dir_all(&path)?;
        std::fs::write(path.join(format!("{stage}-{kind}-{name}.tmp")), name)?;
    }
    observed.push_str(&format!("temp_dir={}\n", std::env::temp_dir().display()));
    std::fs::write(receipts.join(format!("{stage}-{kind}.txt")), observed)?;
    Ok(())
}
"#;
const BUILD_SCRIPT: &str = r#"#[path = "probe.rs"]
mod probe;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=RIPR_TEMP_PROBE_STAGE");
    probe::observe("build")
}
"#;
const MAIN: &str = r#"#[path = "../probe.rs"]
mod probe;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::observe("runtime")?;
    println!("cargo-temp-probe-ran");
    Ok(())
}
"#;

struct Fixture {
    owned: PathBuf,
    checkout: PathBuf,
    receipts: PathBuf,
    supplied: [PathBuf; 3],
}

impl Fixture {
    fn new(label: &str) -> Result<Self, String> {
        let owned = test_support::scratch(label)?;
        let owned = owned.canonicalize().map_err(|e| e.to_string())?;
        let fixture = Self {
            checkout: owned.join("source checkout"),
            receipts: owned.join("receipts"),
            supplied: TEMP_NAMES.map(|name| owned.join(format!("managed {name} temp"))),
            owned,
        };
        for path in [&fixture.receipts, &fixture.owned.join("cargo-home")] {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        for path in &fixture.supplied {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        for (name, content) in [
            (
                "Cargo.toml",
                "[workspace]\n[package]\nname = \"cargo-temp-probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "Cargo.lock",
                "version = 4\n\n[[package]]\nname = \"cargo-temp-probe\"\nversion = \"0.1.0\"\n",
            ),
            (
                ".cargo/config.toml",
                include_str!("../../../.cargo/config.toml"),
            ),
            (
                "rust-toolchain.toml",
                include_str!("../../../rust-toolchain.toml"),
            ),
            ("probe.rs", PROBE),
            ("build.rs", BUILD_SCRIPT),
            ("src/main.rs", MAIN),
        ] {
            let path = fixture.checkout.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(path, content).map_err(|e| e.to_string())?;
        }
        Ok(fixture)
    }

    fn cargo(&self, stage: &str, supplied: bool, bootstrap: bool) -> Result<Output, String> {
        let target = self.owned.join("cargo output");
        let intermediates = self.owned.join("cargo intermediates");
        // Use the same bounded native process-group owner as worktree_binding.
        // Both storage overrides defeat inherited Cargo configuration; the
        // fixture has no dependencies and never needs the caller's Cargo home.
        let mut command = std::process::Command::new("timeout");
        command
            .args(["--kill-after=5s", "60s", env!("CARGO")])
            .arg("--config")
            .arg(config_path("build.build-dir", &intermediates)?)
            .current_dir(&self.checkout)
            .env("CARGO_HOME", self.owned.join("cargo-home"))
            .env("CARGO_TARGET_DIR", &target)
            .env("CARGO_BUILD_BUILD_DIR", &intermediates)
            .env("CARGO_BUILD_JOBS", "1")
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "0")
            .env("CARGO_TERM_COLOR", "never")
            .env("CARGO_NET_OFFLINE", "true")
            .env("RUSTUP_AUTO_INSTALL", "0")
            .env("RIPR_TEMP_PROBE_STAGE", stage)
            .env("RIPR_TEMP_PROBE_RECEIPTS", &self.receipts);
        for name in [
            "CARGO_BUILD_TARGET",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_LOG",
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        ] {
            command.env_remove(name);
        }
        for (name, path) in TEMP_NAMES.iter().zip(&self.supplied) {
            if supplied {
                command.env(name, path);
            } else {
                command.env_remove(name);
            }
            if bootstrap {
                command
                    .arg("--config")
                    .arg(config_path(&format!("env.{name}.value"), path)?);
            }
        }
        command
            .args([
                "run",
                "--offline",
                "--locked",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(target);
        command
            .output()
            .map_err(|e| format!("fixture Cargo spawn: {e}"))
    }

    fn verify(&self, stage: &str, output: &Output, expected: &[PathBuf; 3]) -> Result<(), String> {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Refuse setup/compile failures before the temp-routing assertion. The
        // old force=true policy must fail on observed paths after Cargo succeeds.
        assert!(
            output.status.success(),
            "Cargo did not finish successfully at {stage}:\n{stdout}\n{stderr}"
        );
        assert!(
            stdout.lines().any(|line| {
                serde_json::from_str::<Value>(line).is_ok_and(|message| {
                    message["reason"] == "build-finished" && message["success"] == true
                })
            }),
            "missing successful Cargo build marker at {stage}:\n{stdout}"
        );
        assert!(
            stdout.lines().any(|line| line == "cargo-temp-probe-ran"),
            "runtime probe did not execute at {stage}:\n{stdout}"
        );
        for kind in ["build", "runtime"] {
            let receipt = self.receipts.join(format!("{stage}-{kind}.txt"));
            let observed = fs::read_to_string(&receipt)
                .map_err(|e| format!("missing {stage} {kind} observation: {e}"))?;
            let mut wanted = format!("stage={stage}\nkind={kind}\n");
            for (name, path) in TEMP_NAMES.iter().zip(expected) {
                wanted.push_str(&format!("{name}={}\n", path.display()));
            }
            wanted.push_str(&format!("temp_dir={}\n", expected[2].display()));
            assert_eq!(
                observed, wanted,
                "Cargo temp routing changed {stage} {kind} paths after successful compilation"
            );
            for (name, path) in TEMP_NAMES.iter().zip(expected) {
                let artifact = path.join(format!("{stage}-{kind}-{name}.tmp"));
                assert_eq!(
                    fs::read_to_string(&artifact).map_err(|e| format!(
                        "missing temp artifact {}: {e}",
                        artifact.display()
                    ))?,
                    *name
                );
            }
        }
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Every command is synchronous and timeout bounds its process group.
        // This exclusive scratch root is never a shared Cargo or proof location.
        if let Err(e) = fs::remove_dir_all(&self.owned) {
            eprintln!("retain fixture {}: {e}", self.owned.display());
        }
    }
}

fn config_path(key: &str, path: &Path) -> Result<String, String> {
    let value = path
        .to_str()
        .ok_or_else(|| format!("fixture Cargo config path is not UTF-8: {}", path.display()))?;
    let value = serde_json::to_string(value).map_err(|e| e.to_string())?;
    Ok(format!("{key}={value}"))
}

#[test]
fn managed_temp_survives_cargo_without_a_checkout_target() -> Result<(), String> {
    let fixture = Fixture::new("policy-cargo-managed-temp")?;
    assert!(!fixture.checkout.join("target").exists());
    let bootstrap = fixture.cargo("bootstrap", true, true)?;
    fixture.verify("bootstrap", &bootstrap, &fixture.supplied)?;
    assert!(!fixture.checkout.join("target").exists());

    let observed = fixture.cargo("managed", true, false)?;
    fixture.verify("managed", &observed, &fixture.supplied)?;
    assert!(
        !fixture.checkout.join("target").exists(),
        "managed Cargo temp routing created checkout-local output"
    );
    Ok(())
}

#[test]
fn absent_temp_variables_use_the_prepared_checkout_fallback() -> Result<(), String> {
    let fixture = Fixture::new("policy-cargo-fallback-temp")?;
    let fallback = fixture.checkout.join("target");
    fs::create_dir(&fallback).map_err(|e| e.to_string())?;
    let output = fixture.cargo("fallback", false, false)?;
    fixture.verify(
        "fallback",
        &output,
        &std::array::from_fn(|_| fallback.clone()),
    )?;
    Ok(())
}
