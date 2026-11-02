//! Verifies ordinary `check --diff` exposes file-fact cache reuse in verbose
//! JSON and stays silent in default JSON (#4975).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CACHE_DIR_ENV: &str = "RIPR_CACHE_DIR";

const LIB_RS: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n}\n";
const TEST_RS: &str =
    "use fx::discount;\n#[test]\nfn small() {\n    assert!(discount(5, 2) > 0);\n}\n";
const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 12 { price * qty * 9 / 10 } else { price * qty }\n }\n";

fn run_check_json(
    root: &Path,
    diff: &Path,
    cache: &Path,
    verbose: bool,
) -> Result<serde_json::Value, String> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ripr"));
    cmd.args(["check", "--root"])
        .arg(root)
        .arg("--diff")
        .arg(diff)
        .args(["--format", "json"])
        .env(CACHE_DIR_ENV, cache);
    if verbose {
        cmd.arg("--verbose");
    }
    let output = cmd
        .output()
        .map_err(|error| format!("run ripr check: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ripr check failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("decode check report: {error}"))
}

fn fixture_root() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "ripr-check-file-fact-cache-{}-{nonce}",
        std::process::id()
    ));
    let root = base.join("fx");
    let cache = base.join("cache");
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    fs::write(root.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    fs::write(root.join("src/lib.rs"), LIB_RS).map_err(|error| error.to_string())?;
    fs::write(root.join("tests/discount.rs"), TEST_RS).map_err(|error| error.to_string())?;
    let diff = base.join("change.diff");
    fs::write(&diff, DIFF).map_err(|error| error.to_string())?;
    Ok((root, diff, cache))
}

#[test]
fn default_json_omits_file_fact_cache_verbose_json_names_cold_then_warm() -> Result<(), String> {
    let (root, diff, cache) = fixture_root()?;
    let default_cold = run_check_json(&root, &diff, &cache, false)?;
    if default_cold.get("file_fact_cache").is_some() {
        return Err(format!(
            "default JSON must not carry file_fact_cache: {}",
            default_cold["file_fact_cache"]
        ));
    }

    let verbose_cold = run_check_json(&root, &diff, &cache, true)?;
    let cold = verbose_cold
        .get("file_fact_cache")
        .ok_or("verbose cold JSON missing file_fact_cache")?;
    if cold["hits"].as_u64().unwrap_or(1) != 0 {
        return Err(format!("cold hits: {cold}"));
    }
    if cold["misses"].as_u64().unwrap_or(0) == 0 {
        return Err(format!("cold misses: {cold}"));
    }

    let verbose_warm = run_check_json(&root, &diff, &cache, true)?;
    let warm = verbose_warm
        .get("file_fact_cache")
        .ok_or("verbose warm JSON missing file_fact_cache")?;
    if warm["hits"].as_u64().unwrap_or(0) == 0 {
        return Err(format!("warm hits: {warm}"));
    }
    if warm["misses"].as_u64().unwrap_or(1) != 0 {
        return Err(format!("warm misses: {warm}"));
    }

    // Canonical analysis fields stay aligned across cache temperature.
    if default_cold["summary"] != verbose_warm["summary"]
        || verbose_cold["findings"] != verbose_warm["findings"]
    {
        return Err("cache telemetry changed canonical check output".into());
    }
    Ok(())
}

#[test]
fn blocked_file_fact_store_still_reports_check_with_failure_fact() -> Result<(), String> {
    let (root, diff, cache) = fixture_root()?;
    fs::create_dir_all(cache.join("repo-file-facts")).map_err(|error| error.to_string())?;
    fs::write(cache.join("repo-file-facts/blocker"), b"x").map_err(|error| error.to_string())?;

    let report = run_check_json(&root, &diff, &cache, true)?;
    if report["summary"]["probes"].as_u64().unwrap_or(0) == 0 {
        return Err(format!("check lost probes: {}", report["summary"]));
    }
    let cache_fact = report
        .get("file_fact_cache")
        .ok_or("expected verbose file_fact_cache")?;
    if cache_fact["store_errors"].as_u64().unwrap_or(0) == 0 {
        return Err(format!("expected store_errors > 0: {cache_fact}"));
    }
    Ok(())
}
