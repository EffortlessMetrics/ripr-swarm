//! `receipt write` and `feedback record` write under `target/ripr/{receipts,
//! feedback}`, never the cache, so an explicit `RIPR_CACHE_DIR` must not waive
//! the escaping-link check, and `receipt write` must be guarded without
//! `--root` (#11477).

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    base: PathBuf,
    repo: PathBuf,
    victim: PathBuf,
    cache: PathBuf,
}

impl Fixture {
    fn new(label: &str, linked_child: &str) -> Result<Self, String> {
        let base = std::env::temp_dir().join(format!(
            "ripr-link-containment-{label}-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let victim = base.join("victim");
        let cache = base.join("cache");
        fs::create_dir_all(repo.join("target/ripr")).map_err(|e| e.to_string())?;
        fs::create_dir_all(&victim).map_err(|e| e.to_string())?;
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&victim, repo.join("target/ripr").join(linked_child))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            repo,
            victim,
            cache,
        })
    }

    fn run(&self, args: &[&str]) -> Result<std::process::Output, String> {
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(args)
            .current_dir(&self.repo)
            .env("RIPR_CACHE_DIR", &self.cache)
            .output()
            .map_err(|e| format!("run ripr {args:?}: {e}"))
    }

    fn assert_refused_and_untouched(&self, output: &std::process::Output) -> Result<(), String> {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() || !stderr.contains("resolves outside the workspace") {
            return Err(format!(
                "expected escaping-link refusal, got {:?}: {stderr}",
                output.status
            ));
        }
        let leaked = count_files(&self.victim);
        if leaked != 0 {
            return Err(format!("{leaked} file(s) written through the link"));
        }
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| {
                    let path = entry.path();
                    if path.is_dir() { count_files(&path) } else { 1 }
                })
                .sum()
        })
        .unwrap_or(0)
}

#[test]
fn receipt_write_refuses_escaping_receipts_link_even_with_cache_dir_set() -> Result<(), String> {
    let fixture = Fixture::new("receipt", "receipts")?;
    let output = fixture.run(&[
        "receipt",
        "write",
        "--gap",
        "g",
        "--verify-command",
        "c",
        "--status",
        "passed",
    ])?;
    fixture.assert_refused_and_untouched(&output)
}

#[test]
fn feedback_record_refuses_escaping_feedback_link_even_with_cache_dir_set() -> Result<(), String> {
    let fixture = Fixture::new("feedback", "feedback")?;
    let output = fixture.run(&[
        "feedback",
        "record",
        "--snapshot",
        "snap",
        "--reason",
        "useful_actionable",
        "--root",
        ".",
    ])?;
    fixture.assert_refused_and_untouched(&output)
}
