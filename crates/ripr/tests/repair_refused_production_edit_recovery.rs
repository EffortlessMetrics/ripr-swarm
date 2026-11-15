//! After `ripr continue` refuses an attempt because production code was
//! edited, the recovery must not say a new attempt is not offered: the start
//! gate held only because that same edit left the inline test target
//! uncommitted, and undoing the edit lifts the hold.

use std::path::PathBuf;
use std::process::Command;

const CARGO_TOML: &str = "[package]\nname = \"ar\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
// The inline test pins both tail branch values exactly, so the boundary
// predicate is the only eligible repair start and a bare `ripr repair` starts
// it (branch values are their own `return_value` seams since #14385).
const OLD: &str = "pub fn shipping(weight: u32) -> u32 {\n    if weight > 20 {\n        15\n    } else {\n        5\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn positive() {\n        assert_eq!(shipping(30), 15);\n        assert_eq!(shipping(10), 5);\n    }\n}\n";

struct Repo(PathBuf);

impl Repo {
    fn create() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-refused-production-edit-{}",
            std::process::id()
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        std::fs::create_dir_all(repo.0.join("src")).map_err(|e| e.to_string())?;
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", OLD)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        repo.write("src/lib.rs", &OLD.replace("weight > 20", "weight > 25"))?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
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

    fn ripr(&self, args: &[&str]) -> Result<(Option<i32>, String), String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        Ok((
            output.status.code(),
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn refused_production_edit_recovery_points_at_a_start_that_then_works() -> Result<(), String> {
    let repo = Repo::create()?;
    let (code, started) = repo.ripr(&["repair", "--root", "."])?;
    assert_eq!(code, Some(0), "the repair must start:\n{started}");

    // A production edit instead of the test edit.
    repo.write("src/lib.rs", &OLD.replace("weight > 20", "weight > 26"))?;
    let (code, refused) = repo.ripr(&["continue", "--root", "."])?;
    assert_ne!(
        code,
        Some(0),
        "the production edit must be refused:\n{refused}"
    );
    assert!(
        refused.contains("undo the refused changes")
            && refused.contains("refusal code=edit_cage_violation"),
        "the refusal must come from the edit cage:\n{refused}"
    );
    assert!(
        refused.contains("once they are undone, `ripr repair --root ")
            && !refused.contains("a new attempt is not offered")
            && !refused.contains("--phase before"),
        "the recovery must point at asking the gate again:\n{refused}"
    );

    // The recovery it names: undo the edit, then start again.
    repo.git(&["checkout", "--", "src/lib.rs"])?;
    let (code, restarted) = repo.ripr(&["repair", "--root", "."])?;
    assert_eq!(
        code,
        Some(0),
        "a new attempt must start after the undo:\n{restarted}"
    );
    assert!(
        restarted.contains("Repair prepared for seam"),
        "the new start must print its summary:\n{restarted}"
    );
    Ok(())
}
