//! #15323: the repair brief's test anchor is labeled for what the repair does
//! with it. An inline `#[cfg(test)]` module is repaired by adding one new test
//! function and leaving existing tests unchanged, so its anchor must not read
//! "Test to extend". An integration test file keeps "Test to extend".

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const INLINE_OLD: &str = "pub fn is_adult(age: u32) -> bool {\n    age > 18\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn t() {\n        assert!(is_adult(30));\n    }\n}\n";
const FILE_OLD: &str = "pub fn is_adult(age: u32) -> bool {\n    age > 18\n}\n";
const FILE_TEST: &str = "use fx::is_adult;\n\n#[test]\nfn t() {\n    assert!(is_adult(30));\n}\n";

struct Repo(PathBuf);

impl Repo {
    fn create(lib_old: &str, test_file: Option<&str>) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-anchor-label-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", lib_old)?;
        if let Some(test) = test_file {
            repo.write("tests/t.rs", test)?;
        }
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        repo.write("src/lib.rs", &lib_old.replace("age > 18", "age >= 18"))?;
        repo.git(&["commit", "-qam", "change"])?;
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

    fn repair(&self) -> Result<String, String> {
        let output: Output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(["repair", "--base", "HEAD~1"])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn inline_module_anchor_is_not_labeled_test_to_extend() -> Result<(), String> {
    let repair = Repo::create(INLINE_OLD, None)?.repair()?;
    if !repair.contains("Ready to repair.")
        || !repair.contains("add one new test function")
        || !repair.contains("Nearby test (left unchanged): src/lib.rs:")
        || repair.contains("Test to extend")
    {
        return Err(format!("inline anchor label wrong:\n{repair}"));
    }
    Ok(())
}

#[test]
fn integration_file_anchor_keeps_test_to_extend() -> Result<(), String> {
    let repair = Repo::create(FILE_OLD, Some(FILE_TEST))?.repair()?;
    if !repair.contains("Ready to repair.")
        || !repair.contains("Test to extend: tests/t.rs:")
        || repair.contains("Nearby test (left unchanged)")
    {
        return Err(format!("file anchor label wrong:\n{repair}"));
    }
    Ok(())
}
