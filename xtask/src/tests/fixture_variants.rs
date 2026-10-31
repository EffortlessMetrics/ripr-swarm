//! Guards for the in-suite fixture variant builds (#11197).
//!
//! Run the real builds in a child with a test-owned Cargo target: unrelated
//! tests may otherwise rewrite the shared binary while this guard stamps it.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const CHILD: &str = "tests::fixture_variants::variant_guard_child";
const MODE: &str = "RIPR_VARIANT_GUARD_TEST_MODE";

struct OwnedTarget(PathBuf);

impl OwnedTarget {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("missing repository root")?;
        // Resolve relative Cargo target paths from the child’s repository cwd.
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("target"));
        let parent = root.join(target).join("ripr/variant-guard-tests");
        fs::create_dir_all(&parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        let path = parent.join(format!(
            "ripr-variant-guard-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|e| format!("acquire {}: {e}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for OwnedTarget {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.0) {
            eprintln!("remove owned variant target {}: {e}", self.0.display());
        }
    }
}

#[test]
fn perl_variant_build_never_rewrites_shared_debug_binary() -> Result<(), String> {
    let target = OwnedTarget::new()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing repository root")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target_text = target
        .0
        .to_str()
        .ok_or("variant test target must be UTF-8")?;
    for mode in ["isolated", "redirected"] {
        let result = crate::run::capture_output_measured(
            &exe.to_string_lossy(),
            &[CHILD.to_owned(), "--exact".into(), "--nocapture".into()],
            Some(root),
            &[(MODE, mode), ("CARGO_TARGET_DIR", target_text)],
            Duration::from_secs(1800),
            "owned variant guard child",
        )?
        .output;
        assert!(
            !result.timed_out
                && result.status.is_some_and(|status| status.success())
                && result.stdout.contains("running 1 test")
                && result.stdout.contains("1 passed; 0 failed"),
            "{mode} child failed or did not run exactly one test: status={:?}, timed_out={}, stdout={}, stderr={}",
            result.status,
            result.timed_out,
            result.stdout,
            result.stderr
        );
    }
    Ok(())
}

#[test]
fn variant_guard_child() -> Result<(), String> {
    let Ok(mode) = std::env::var(MODE) else {
        return Ok(());
    };
    use crate::reports::{ripr_fixture_binary, ripr_perl_fixture_binary};
    ripr_fixture_binary()?;
    let shared = crate::ripr_debug_binary();
    let before = binary_stamp(&shared)?;
    match mode.as_str() {
        "isolated" => {
            let perl = ripr_perl_fixture_binary()?;
            assert!(
                Path::new(&perl).is_file(),
                "perl variant binary must exist: {perl}"
            );
        }
        "redirected" => {
            // Counterexample: the variant build inherits the shared target.
            crate::run::run_with_envs(
                "cargo",
                &["build", "-p", "ripr", "--features", "lang-perl"],
                &[],
            )?;
        }
        _ => return Err(format!("unknown variant guard mode: {mode}")),
    }
    let verdict = unchanged_binary(&shared, before);
    if mode == "redirected" {
        let error = verdict
            .err()
            .ok_or("redirected variant did not rewrite shared binary")?;
        assert_eq!(error, format!("variant build rewrote {}", shared.display()));
    } else {
        verdict?;
    }
    Ok(())
}

fn binary_stamp(path: &Path) -> Result<(SystemTime, u64), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("stat {}: {error}", path.display()))?;
    let modified = metadata
        .modified()
        .map_err(|error| format!("mtime {}: {error}", path.display()))?;
    Ok((modified, metadata.len()))
}

fn unchanged_binary(path: &Path, before: (SystemTime, u64)) -> Result<(), String> {
    if before != binary_stamp(path)? {
        return Err(format!("variant build rewrote {}", path.display()));
    }
    Ok(())
}
