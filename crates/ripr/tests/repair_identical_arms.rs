//! #12076: end-to-end pin that `ripr repair` does not claim the identical-inline-arms
//! predicate subject is already checked (#12053 / #12073 unit coverage).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// User-facing grip phrase from `GRIPPED_SPOT_REASON` in `app/task_first.rs`.
const EXISTING_TESTS_ALREADY_CHECK: &str = "existing tests already check it";

fn clear_git_selectors(command: &mut Command) {
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
    ] {
        command.env_remove(key);
    }
}

struct Repo(PathBuf);

impl Repo {
    fn create_from_fixture(after_base: impl FnOnce(&mut String)) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-repair-identical-arms-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let repo = Self(root.clone());
        let fixture = fixture_dir();
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::copy(fixture.join("Cargo.toml"), root.join("Cargo.toml"))
            .map_err(|e| e.to_string())?;
        let mut lib =
            std::fs::read_to_string(fixture.join("src/lib.rs.txt")).map_err(|e| e.to_string())?;
        repo.write("src/lib.rs", &lib)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        after_base(&mut lib);
        repo.write("src/lib.rs", &lib)?;
        repo.git(&["commit", "-qam", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let mut command = Command::new("git");
        clear_git_selectors(&mut command);
        let output = command
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn ripr(&self, args: &[&str]) -> Result<Output, String> {
        let mut attempt = 0;
        loop {
            let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
            clear_git_selectors(&mut command);
            let launched = command.current_dir(&self.0).args(args).output();
            match launched {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && attempt < 20 => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                other => return other.map_err(|e| e.to_string()),
            }
        }
    }

    fn repair_with_base(&self) -> Result<(Output, String), String> {
        let output = self.ripr(&["repair", "--root", ".", "--base", "HEAD~1"])?;
        Ok((output.clone(), text(&output)))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rust-verdict-corpus/subjects/authored-closed-if-identical-arms")
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn apply_boundary_ge5(lib: &mut String) {
    let before = lib.matches("x > 5").count();
    assert_eq!(before, 1, "fixture must contain exactly one `x > 5`");
    *lib = lib.replacen("x > 5", "x >= 5", 1);
    assert_eq!(lib.matches("x > 5").count(), 0);
    assert_eq!(lib.matches("x >= 5").count(), 1);
}

fn repair_after_change(after_base: impl FnOnce(&mut String)) -> Result<(Output, String), String> {
    let repo = Repo::create_from_fixture(after_base)?;
    repo.repair_with_base()
}

#[test]
fn identical_inline_arms_repair_does_not_claim_existing_tests_already_check() -> Result<(), String>
{
    let (output, combined) = repair_after_change(apply_boundary_ge5)?;
    assert_eq!(
        output.status.code(),
        Some(3),
        "repair must refuse without starting:\n{combined}"
    );
    assert!(
        !combined.contains(EXISTING_TESTS_ALREADY_CHECK),
        "identical arms must not read as gripped:\n{combined}"
    );
    assert!(
        combined.contains("repair route not ready"),
        "expected the honest missing-evidence refusal:\n{combined}"
    );
    Ok(())
}

#[test]
fn differing_inline_arms_repair_still_claims_existing_tests_already_check() -> Result<(), String> {
    let (output, combined) = repair_after_change(|lib| {
        apply_boundary_ge5(lib);
        assert!(
            lib.contains("else { r = 1; }"),
            "fixture must contain exactly one identical else arm"
        );
        *lib = lib.replacen("else { r = 1; }", "else { r = 2; }", 1);
    })?;
    assert_eq!(
        output.status.code(),
        Some(3),
        "repair must still refuse:\n{combined}"
    );
    assert!(
        combined.contains(EXISTING_TESTS_ALREADY_CHECK),
        "differing arms must still read as gripped on current evidence:\n{combined}"
    );
    Ok(())
}
