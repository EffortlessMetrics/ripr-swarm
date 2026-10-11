//! Both real front doors retain the same predecessor report snapshots.
use std::path::PathBuf;
use std::process::Command;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "xtask-executable-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("policy")).map_err(|e| e.to_string())?;
        let fixture = Self(root);
        fixture.git(&["init", "--quiet"])?;
        for name in ["allowed", "demoted", "ordinary", "unregistered"] {
            std::fs::write(fixture.0.join(name), format!("{name}\n")).map_err(|e| e.to_string())?;
            fixture.git(&["-c", "core.filemode=false", "add", name])?;
            fixture.git(&["update-index", "--chmod=-x", name])?;
        }
        for name in ["allowed", "demoted", "unregistered"] {
            fixture.git(&["update-index", "--chmod=+x", name])?;
        }
        Ok(fixture)
    }
    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.0)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "fixture git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            eprintln!("retain fixture {}: {e}", self.0.display());
        }
    }
}

#[test]
fn retained_xtask_frontdoor_matches_lightweight_policy_snapshots() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let allowlist = fixture.0.join("policy/executable_allowlist.txt");
    for (case, rows, expected) in [
        (
            "allowed",
            "allowed\ndemoted\nunregistered\n",
            include_str!("../../tools/repo-policy/tests/executable-files/allowed.md"),
        ),
        (
            "unregistered",
            "allowed\ndemoted\n",
            include_str!("../../tools/repo-policy/tests/executable-files/unregistered.md"),
        ),
        (
            "missing",
            "allowed\ndemoted\nunregistered\ndeleted\n",
            include_str!("../../tools/repo-policy/tests/executable-files/missing.md"),
        ),
        (
            "demoted",
            "allowed\ndemoted\nunregistered\n",
            include_str!("../../tools/repo-policy/tests/executable-files/demoted.md"),
        ),
        (
            "mixed_order",
            "z-missing\nallowed\ndemoted\na-missing\n",
            include_str!("../../tools/repo-policy/tests/executable-files/mixed_order.md"),
        ),
        (
            "missing_optional",
            "",
            include_str!("../../tools/repo-policy/tests/executable-files/missing_optional.md"),
        ),
    ] {
        if case == "demoted" {
            fixture.git(&["update-index", "--chmod=-x", "demoted"])?;
        }
        if case == "missing_optional" {
            std::fs::remove_file(&allowlist).map_err(|e| e.to_string())?;
            for name in ["allowed", "unregistered"] {
                fixture.git(&["update-index", "--chmod=-x", name])?;
            }
        } else {
            std::fs::write(&allowlist, rows).map_err(|e| e.to_string())?;
        }
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("check-executable-files")
            .current_dir(&fixture.0)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(
            output.status.success(),
            matches!(case, "allowed" | "missing_optional"),
            "case {case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(fixture.0.join("target/ripr/reports/executable-files.md"))
                .map_err(|e| e.to_string())?,
            expected.as_bytes(),
            "case {case}"
        );
    }
    Ok(())
}
