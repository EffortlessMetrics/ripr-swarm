//! #10710: `ripr check` never ends a gap finding in a dead end.
//!
//! - every `ripr agent stub` command `check` prints succeeds for its own
//!   finding;
//! - a gap that has neither a repair start nor a stub prints one plain step:
//!   write the test by hand, and no repair route exists for it yet.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"dd\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const HAND_WRITTEN: &str = "Write this test by hand:";
const NO_ROUTE: &str = "ripr has no repair route for this kind of change yet";

struct Repo(PathBuf);

impl Repo {
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-dead-end-{}-{}",
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

    fn ripr(&self, args: &[&str]) -> Result<(bool, String), String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        Ok((
            output.status.success(),
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }

    fn check(&self) -> Result<String, String> {
        Ok(self.ripr(&["check", "--base", "HEAD~1"])?.1)
    }

    /// Run every printed `ripr agent stub ...` command (not its PowerShell
    /// spelling) and require it to succeed. Returns how many were printed.
    fn every_printed_stub_succeeds(&self, check: &str) -> Result<usize, String> {
        let mut printed = 0;
        for line in check.lines().map(str::trim) {
            let Some(arguments) = line.strip_prefix("ripr agent stub ") else {
                continue;
            };
            printed += 1;
            let arguments: Vec<&str> = arguments.split_whitespace().collect();
            let (ok, output) = self.ripr(&[&["agent", "stub"][..], &arguments[..]].concat())?;
            assert!(
                ok && !output.contains("no test stub for seam"),
                "check printed a stub command that refuses: `{line}`\n{output}\n\ncheck said:\n{check}"
            );
        }
        Ok(printed)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn async_case() -> Result<Repo, String> {
    Repo::create(
        "pub async fn get(x: u32) -> u32 {\n    x + 1\n}\n",
        "pub async fn get(x: u32) -> u32 {\n    x + 2\n}\n",
        "use dd::get;\n\n#[test]\nfn t() {\n    let _future = get(1);\n}\n",
    )
}

fn generic_case() -> Result<Repo, String> {
    Repo::create(
        "pub fn pick<T: PartialOrd + Copy>(a: T, b: T) -> T {\n    if a > b { a } else { b }\n}\n",
        "pub fn pick<T: PartialOrd + Copy>(a: T, b: T) -> T {\n    if a >= b { a } else { b }\n}\n",
        "use dd::pick;\n\n#[test]\nfn t() {\n    assert_eq!(pick(1, 2), 2);\n    assert_eq!(pick(3, 2), 3);\n}\n",
    )
}

fn field_case() -> Result<Repo, String> {
    Repo::create(
        "pub struct P {\n    pub x: u32,\n    pub y: u32,\n}\n\npub fn mk() -> P {\n    P { x: 1, y: 2 }\n}\n",
        "pub struct P {\n    pub x: u32,\n    pub y: u32,\n}\n\npub fn mk() -> P {\n    P { x: 1, y: 3 }\n}\n",
        "use dd::mk;\n\n#[test]\nfn t() {\n    assert_eq!(mk().x, 1);\n}\n",
    )
}

fn side_effect_case() -> Result<Repo, String> {
    Repo::create(
        "use std::cell::Cell;\n\npub fn bump(c: &Cell<u32>) {\n    c.set(c.get() + 1);\n}\n",
        "use std::cell::Cell;\n\npub fn bump(c: &Cell<u32>) {\n    c.set(c.get() + 2);\n}\n",
        "use dd::bump;\nuse std::cell::Cell;\n\n#[test]\nfn t() {\n    let c = Cell::new(0);\n    bump(&c);\n    assert!(c.get() > 0);\n}\n",
    )
}

#[test]
fn async_gap_with_no_route_says_to_write_the_test_by_hand() -> Result<(), String> {
    let repo = async_case()?;
    let check = repo.check()?;
    assert!(
        check.contains("No test stub here:") && !check.contains("Write a test for it:"),
        "precondition, the stub refuses and none is printed:\n{check}"
    );
    assert!(
        check.contains(HAND_WRITTEN) && check.contains(NO_ROUTE),
        "check must name the hand-written test and the missing route:\n{check}"
    );
    assert!(
        check.contains("run the future it returns to completion"),
        "an async owner's test must await the future, not call the function plainly:\n{check}"
    );
    assert_eq!(repo.every_printed_stub_succeeds(&check)?, 0);
    Ok(())
}

#[test]
fn side_effect_gap_with_no_route_says_to_write_the_test_by_hand() -> Result<(), String> {
    let repo = side_effect_case()?;
    let check = repo.check()?;
    assert!(
        check.contains("No test stub here:"),
        "precondition, the stub refuses:\n{check}"
    );
    assert!(
        check.contains(HAND_WRITTEN) && check.contains(NO_ROUTE),
        "check must name the hand-written test and the missing route:\n{check}"
    );
    assert_eq!(repo.every_printed_stub_succeeds(&check)?, 0);
    Ok(())
}

#[test]
fn generic_gap_with_a_repair_start_is_not_told_there_is_no_route() -> Result<(), String> {
    let repo = generic_case()?;
    let check = repo.check()?;
    assert!(
        check.contains("ripr repair ") && check.contains("No test stub here:"),
        "precondition, a repair start and a refused stub:\n{check}"
    );
    assert!(
        !check.contains(NO_ROUTE),
        "a finding with a repair start has a route:\n{check}"
    );
    assert_eq!(repo.every_printed_stub_succeeds(&check)?, 0);
    Ok(())
}

#[test]
fn field_gap_with_two_seams_prints_only_the_stub_that_works() -> Result<(), String> {
    let repo = field_case()?;
    let check = repo.check()?;
    assert!(
        !check.contains("Choose a test-stub seam"),
        "the unchanged field's seam is not offered as a choice:\n{check}"
    );
    assert_eq!(
        repo.every_printed_stub_succeeds(&check)?,
        1,
        "exactly the changed field's stub is printed:\n{check}"
    );
    Ok(())
}
