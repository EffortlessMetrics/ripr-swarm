//! #10702: `ripr check` and `ripr repair` agree on whether a static gap
//! remains, or the check screen prints the one reason they differ.
//!
//! - an `exposed` finding says "nothing to repair here", never "missing
//!   evidence" or "no seam", while repair reports the seam as gripped;
//! - check never says it found no seam in the finding's file when repair
//!   lists seams in that file.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"ar\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

struct Repo(PathBuf);

impl Repo {
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-check-repair-agree-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        std::fs::create_dir_all(repo.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(repo.0.join("tests")).map_err(|e| e.to_string())?;
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", old_src)?;
        repo.write("tests/t.rs", test)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        repo.write("src/lib.rs", new_src)?;
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

    fn ripr(&self, args: &[&str]) -> Result<String, String> {
        let output: Output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
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
fn exposed_change_says_nothing_to_repair_in_both_commands() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "use ar::dbl;\n\n#[test]\nfn exact() {\n    assert_eq!(dbl(2), 6);\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: exposed"),
        "precondition, check reads exposed:\n{check}"
    );
    assert!(
        check.contains("no repair start offered: nothing to repair here")
            && check.contains("already notice this change (exposed)"),
        "check must say there is nothing to repair:\n{check}"
    );
    for leak in ["no seam in this finding's file", "missing", "found no seam"] {
        let advisory: Vec<&str> = check
            .lines()
            .filter(|line| line.contains("no repair start offered"))
            .collect();
        assert!(
            advisory.iter().all(|line| !line.contains(leak)),
            "an exposed finding must not print {leak:?}: {advisory:?}"
        );
    }
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("already grip it (exposed)") && !repair.contains("missing evidence"),
        "repair must agree that no gap remains:\n{repair}"
    );
    Ok(())
}

#[test]
fn check_never_reports_no_seam_where_repair_lists_seams_in_the_file() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn label(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1 => \"one\",\n        _ => \"many\",\n    }\n}\n",
        "pub fn label(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1 => \"uno\",\n        _ => \"many\",\n    }\n}\n",
        "use ar::label;\n\n#[test]\nfn labels() {\n    assert!(!label(1).is_empty());\n}\n",
    )?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("visible seams under"),
        "precondition, repair lists seams in src/lib.rs:\n{repair}"
    );
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        !check.contains("found no seam") && !check.contains("no seam in this finding's file"),
        "check must not claim there is no seam while repair lists seams:\n{check}"
    );
    assert!(
        check.contains(
            "seams in this finding's file, but could not tie one to this finding's changed line"
        ),
        "check must name the one reason it cannot bind a seam:\n{check}"
    );
    Ok(())
}
