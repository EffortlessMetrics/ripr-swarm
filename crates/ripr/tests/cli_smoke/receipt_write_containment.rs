use super::{
    assert_success, ignore_remove_dir_all, init_git_fixture_repo, spawn_command,
    unique_temp_workspace,
};
use std::path::{Path, PathBuf};
use std::process::Output;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}

fn receipt(cwd: &Path, root: &Path, cache: &str, out: Option<&Path>) -> TestResult<Output> {
    let root = root.to_str().ok_or("fixture root is not UTF-8")?;
    let mut args = vec![
        "receipt",
        "write",
        "--gap",
        "g",
        "--verify-command",
        "true",
        "--status",
        "passed",
        "--root",
        root,
    ];
    if let Some(out) = out {
        args.extend(["--out", out.to_str().ok_or("fixture output is not UTF-8")?]);
    }
    Ok(spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(cwd),
        &args,
        &[("RIPR_CACHE_DIR", cache)],
        None,
        Some(b""),
    )?)
}

#[test]
fn default_receipt_checks_cwd_even_with_a_different_root() -> TestResult {
    let base = unique_temp_workspace("receipt-default-cwd-containment");
    std::fs::create_dir_all(&base)?;
    let _cleanup = Cleanup(base.clone());
    for (index, link) in ["target", "target/ripr/receipts"].iter().enumerate() {
        for cache_set in [false, true] {
            let slot = base.join(format!("{index}-{cache_set}"));
            let repo = slot.join("repo");
            let outside = slot.join("outside");
            let selected = slot.join("selected");
            for dir in [&repo, &outside, &selected] {
                std::fs::create_dir_all(dir)?;
            }
            init_git_fixture_repo(&repo)?;
            let sentinel = b"owned outside bytes must not change\n";
            std::fs::write(outside.join("g.json"), sentinel)?;
            let target = repo.join(link);
            std::fs::create_dir_all(target.parent().ok_or("link has no parent")?)?;
            std::os::unix::fs::symlink(&outside, &target)?;
            let cache = slot.join("cache");
            let cache = if cache_set {
                cache.to_str().ok_or("cache is not UTF-8")?
            } else {
                ""
            };
            // Both an existing foreign root and a missing root must not hide
            // the link in the actual cwd-relative default output tree.
            for root in [&selected, &slot.join("missing")] {
                let output = receipt(&repo, root, cache, None)?;
                assert!(
                    !output.status.success(),
                    "escaping default write succeeded: {output:?}"
                );
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("resolves outside the workspace"),
                    "wrong refusal: {output:?}"
                );
                assert_eq!(std::fs::read(outside.join("g.json"))?, sentinel);
                assert_eq!(std::fs::read_dir(&outside)?.count(), 1);
            }
        }
    }
    std::fs::remove_dir_all(&base)?;
    assert!(!base.exists());
    Ok(())
}

#[test]
fn receipt_cwd_guard_preserves_contained_and_explicit_outputs() -> TestResult {
    let base = unique_temp_workspace("receipt-cwd-positive-controls");
    std::fs::create_dir_all(&base)?;
    let _cleanup = Cleanup(base.clone());
    let repo = base.join("repo");
    let selected = base.join("selected");
    let outside = base.join("outside");
    for dir in [&repo, &selected, &outside] {
        std::fs::create_dir_all(dir)?;
    }
    init_git_fixture_repo(&repo)?;
    let cache = base.join("cache");
    let cache = cache.to_str().ok_or("cache is not UTF-8")?;
    let output = receipt(&repo, &selected, cache, None)?;
    assert_success(&output);
    let default = repo.join("target/ripr/receipts/g.json");
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&default)?)?;
    assert_eq!(value["canonical_gap_id"], "g");
    std::fs::remove_dir_all(repo.join("target/ripr/receipts"))?;
    let contained = repo.join("contained-receipts");
    std::fs::create_dir_all(&contained)?;
    std::os::unix::fs::symlink(&contained, repo.join("target/ripr/receipts"))?;
    assert_success(&receipt(&repo, &selected, cache, None)?);
    assert!(contained.join("g.json").is_file());
    // Explicit paths keep their existing caller-selected semantics, even
    // when cwd's unused default tree is redirected and --root is elsewhere.
    std::fs::remove_file(repo.join("target/ripr/receipts"))?;
    std::os::unix::fs::symlink(&outside, repo.join("target/ripr/receipts"))?;
    for out in [PathBuf::from("chosen.json"), outside.join("chosen.json")] {
        let output = receipt(&repo, &selected, cache, Some(&out))?;
        assert_success(&output);
        let written = if out.is_absolute() {
            out
        } else {
            repo.join(out)
        };
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(written)?)?;
        assert_eq!(value["canonical_gap_id"], "g");
    }
    assert!(!outside.join("g.json").exists());
    std::fs::remove_dir_all(&base)?;
    assert!(!base.exists());
    Ok(())
}
