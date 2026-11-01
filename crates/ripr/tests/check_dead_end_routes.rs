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
    fn printed_stub_count(&self, check: &str) -> Result<usize, String> {
        Ok(self.printed_stub_outputs(check)?.len())
    }

    /// The output of each printed stub command, each required to succeed.
    fn printed_stub_outputs(&self, check: &str) -> Result<Vec<String>, String> {
        let mut outputs = Vec::new();
        for line in check.lines().map(str::trim) {
            let Some(arguments) = line.strip_prefix("ripr agent stub ") else {
                continue;
            };
            let arguments: Vec<&str> = arguments.split_whitespace().collect();
            let (ok, output) = self.ripr(&[&["agent", "stub"][..], &arguments[..]].concat())?;
            assert!(
                ok && !output.contains("no test stub for seam"),
                "check printed a stub command that refuses: `{line}`\n{output}\n\ncheck said:\n{check}"
            );
            outputs.push(output);
        }
        Ok(outputs)
    }

    /// A copy of a committed fixture's `input` tree, checked with its
    /// `diff.patch` (the way the golden harness runs it).
    fn from_fixture(name: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/{name}"));
        let root = std::env::temp_dir().join(format!(
            "ripr-dead-end-fixture-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        copy_tree(&fixture.join("input"), &root)?;
        std::fs::copy(fixture.join("diff.patch"), root.join("diff.patch"))
            .map_err(|e| e.to_string())?;
        Ok(Self(root))
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            if entry.file_name() != "target" {
                copy_tree(&entry.path(), &target)?;
            }
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
        check.contains(HAND_WRITTEN)
            && check.contains("no repair start offered: ripr cannot build a repair packet")
            && !check.contains(NO_ROUTE),
        "check must name the hand-written test beside the advisory's own recovery, \
         without also claiming there is no repair route:\n{check}"
    );
    assert!(
        check.contains("run the future it returns to completion"),
        "an async owner's test must await the future, not call the function plainly:\n{check}"
    );
    assert_eq!(repo.printed_stub_count(&check)?, 0);
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
        check.contains(HAND_WRITTEN)
            && check.contains("no repair start offered: ripr cannot build a repair packet")
            && !check.contains(NO_ROUTE),
        "check must name the hand-written test beside the advisory's own recovery, \
         without also claiming there is no repair route:\n{check}"
    );
    assert_eq!(repo.printed_stub_count(&check)?, 0);
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
    assert_eq!(repo.printed_stub_count(&check)?, 0);
    Ok(())
}

#[test]
fn field_gap_with_two_seams_prints_only_the_stub_that_works() -> Result<(), String> {
    let repo = field_case()?;
    let check = repo.check()?;
    assert!(
        !check.contains("Choose a test-stub spot"),
        "the unchanged field's seam is not offered as a choice:\n{check}"
    );
    assert_eq!(
        repo.printed_stub_count(&check)?,
        1,
        "exactly the changed field's stub is printed:\n{check}"
    );
    Ok(())
}

#[test]
fn two_ungripped_fields_print_the_changed_fields_stub_not_the_first() -> Result<(), String> {
    // Neither field is asserted, so both field seams could be stubbed; the
    // finding changed only `y`, and the printed stub must be for `y`.
    let repo = Repo::create(
        "pub struct P {\n    pub x: u32,\n    pub y: u32,\n}\n\npub fn mk() -> P {\n    P { x: 1, y: 2 }\n}\n",
        "pub struct P {\n    pub x: u32,\n    pub y: u32,\n}\n\npub fn mk() -> P {\n    P { x: 1, y: 3 }\n}\n",
        "use dd::mk;\n\n#[test]\nfn t() {\n    let _built = mk();\n}\n",
    )?;
    let check = repo.check()?;
    let outputs = repo.printed_stub_outputs(&check)?;
    assert_eq!(
        outputs.len(),
        1,
        "precondition, exactly one stub is printed so the loop below is not vacuous:\n{check}"
    );
    for output in outputs {
        assert!(
            output.contains("`y: 3`") && !output.contains("`x: 1`"),
            "a stub for the unchanged field must not be offered as the answer:\n{output}\n\ncheck said:\n{check}"
        );
    }
    Ok(())
}

#[test]
fn hidden_row_stub_command_runs_and_succeeds() -> Result<(), String> {
    let repo = Repo::from_fixture("struct_literal_untouched_field_builder")?;
    let (_, check) = repo.ripr(&[
        "check",
        "--root",
        ".",
        "--diff",
        "diff.patch",
        "--mode",
        "fast",
    ])?;
    let hidden = check
        .split_once("Hidden:")
        .map(|(_, rest)| rest.to_string())
        .ok_or_else(|| format!("precondition, the fixture has a hidden row:\n{check}"))?;
    assert!(
        hidden.contains("Choose a test-stub spot"),
        "precondition, the hidden row offers its one validated stub as a choice:\n{check}"
    );
    assert_eq!(
        repo.printed_stub_count(&hidden)?,
        1,
        "the hidden row's printed stub command must run and succeed:\n{check}"
    );
    Ok(())
}
