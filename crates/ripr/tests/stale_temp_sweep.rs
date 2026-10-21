//! A run terminated between creating and renaming a cache temp file strands
//! `.ripr-atomic-*.tmp`. The next run that writes into that directory must
//! remove it once it is old enough that no live writer can own it, and must
//! leave a young one alone.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CACHE_DIR_ENV: &str = "RIPR_CACHE_DIR";

const LIB_RS: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n}\n";
const TEST_RS: &str =
    "use fx::discount;\n#[test]\nfn small() {\n    assert!(discount(5, 2) > 0);\n}\n";
const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 12 { price * qty * 9 / 10 } else { price * qty }\n }\n";

fn run_check(root: &Path, diff: &Path, cache: &Path) -> Result<serde_json::Value, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--root"])
        .arg(root)
        .arg("--diff")
        .arg(diff)
        .args(["--format", "json"])
        .env(CACHE_DIR_ENV, cache)
        .output()
        .map_err(|error| format!("run ripr check: {error}"))?;
    if output.status.success() {
        let report: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("decode check report: {error}"))?;
        assert!(
            report["summary"]["probes"]
                .as_u64()
                .is_some_and(|count| count > 0)
        );
        return Ok(report);
    }
    Err(format!(
        "ripr check failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn plant(dir: &Path, name: &str, age: Duration) -> Result<PathBuf, String> {
    let path = dir.join(name);
    let file = fs::File::create(&path).map_err(|error| error.to_string())?;
    file.set_modified(SystemTime::now() - age)
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn first_cache_entry_dir(cache: &Path) -> Option<PathBuf> {
    let mut stack = vec![cache.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                return path.parent().map(Path::to_path_buf);
            }
        }
    }
    None
}

#[test]
fn next_run_removes_an_old_stranded_temp_file_and_keeps_a_young_one() -> Result<(), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!("ripr-stale-temp-{}-{nonce}", std::process::id()));
    let root = base.join("fx");
    let cache = base.join("cache");
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    fs::write(root.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    fs::write(root.join("src/lib.rs"), LIB_RS).map_err(|error| error.to_string())?;
    fs::write(root.join("tests/discount.rs"), TEST_RS).map_err(|error| error.to_string())?;
    let diff = base.join("change.diff");
    fs::write(&diff, DIFF).map_err(|error| error.to_string())?;

    run_check(&root, &diff, &cache)?;
    let entry_dir = first_cache_entry_dir(&cache)
        .ok_or_else(|| "first run must populate the cache".to_string())?;

    // What a run terminated between create and rename leaves behind, and what an
    // in-flight writer in another process looks like.
    let old = plant(
        &entry_dir,
        ".ripr-atomic-111-222-0.tmp",
        Duration::from_hours(1),
    )?;
    let young = plant(&entry_dir, ".ripr-atomic-111-333-1.tmp", Duration::ZERO)?;

    // Invalidate the cache so the next run writes into the same directory.
    fs::write(
        root.join("src/lib.rs"),
        format!("{LIB_RS}// changed so the cached facts are stale\n"),
    )
    .map_err(|error| error.to_string())?;
    run_check(&root, &diff, &cache)?;

    let old_survived = old.exists();
    let young_survived = young.exists();
    assert!(
        !old_survived,
        "an hour-old stranded temp file must be swept"
    );
    assert!(
        young_survived,
        "a young temp file may belong to a live writer and must be kept"
    );
    // A warm run must also recover interruption residue. Pin every entry's
    // bytes and modification time so rewriting the cache cannot fake cleanup.
    let expected = run_check(&root, &diff, &cache)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&entry_dir).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            entries.push((
                path.clone(),
                fs::read(&path).map_err(|error| error.to_string())?,
                fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .map_err(|error| error.to_string())?,
            ));
        }
    }
    assert!(!entries.is_empty(), "a warm run needs real cache entries");
    let unrelated = plant(
        &entry_dir,
        ".ripr-atomic-unowned.tmp",
        Duration::from_hours(1),
    )?;
    for cycle in 0..3 {
        let stranded = plant(
            &entry_dir,
            &format!(".ripr-atomic-111-444-{cycle}.tmp"),
            Duration::from_hours(1),
        )?;
        assert_eq!(run_check(&root, &diff, &cache)?, expected);
        assert!(
            !stranded.exists(),
            "warm run {cycle} must sweep aged residue"
        );
        assert!(
            young.exists(),
            "warm run {cycle} must preserve a young writer"
        );
        assert!(
            unrelated.exists(),
            "warm run {cycle} must preserve unrelated files"
        );
        for (path, bytes, modified) in &entries {
            assert_eq!(fs::read(path).map_err(|error| error.to_string())?, *bytes);
            assert_eq!(
                fs::metadata(path)
                    .and_then(|meta| meta.modified())
                    .map_err(|error| error.to_string())?,
                *modified,
                "warm run {cycle} must not rewrite a valid cache entry",
            );
        }
        assert_eq!(
            fs::read_dir(&entry_dir)
                .map_err(|error| error.to_string())?
                .count(),
            entries.len() + 2,
            "warm run {cycle} must not accumulate files",
        );
    }
    let _ = fs::remove_dir_all(&base);
    Ok(())
}
