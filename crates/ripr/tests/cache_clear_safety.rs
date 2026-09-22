use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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

#[test]
fn cache_clear_force_preserves_unrelated_siblings_through_exact_binary() -> Result<(), String> {
    let base = temp_dir("mixed-root");
    let cache_dir = base.join("mixed-cache");
    let layer = cache_dir.join("repo-file-facts");
    fs::create_dir_all(&layer).map_err(|error| error.to_string())?;
    fs::write(layer.join("entry.json"), b"{}").map_err(|error| error.to_string())?;
    let sentinel = cache_dir.join("KEEP.txt");
    fs::write(&sentinel, b"unrelated user data").map_err(|error| error.to_string())?;

    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["cache", "clear", "--force"])
        .current_dir(&base)
        .env(CACHE_DIR_ENV, &cache_dir)
        .output()
        .map_err(|error| format!("run exact ripr cache clear binary: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let safe = output.status.success()
        && !layer.exists()
        && sentinel.is_file()
        && cache_dir.is_dir()
        && stdout.contains(&format!("Cache dir: {}", cache_dir.display()))
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
