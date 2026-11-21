//! #15910: a test in a `[[test]]` target the manifest declares outside
//! `tests/` (`path = "spec/pricing.rs"`) is linked by `ripr check`, so the
//! repair route must be able to name that file as the test to edit. The
//! manifest, read at the gate, is the authority: a declared path under `src/`
//! or one the manifest also roots another target at gets no start, and an
//! attempt on a declared file keeps the same confinement a `tests/` file has.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const LIB_OLD: &str = "pub fn discount(total: u32) -> u32 {\n    if total > 100 {\n        total - 10\n    } else {\n        total\n    }\n}\n";

const WEAK_TEST: &str = "use pricing::discount;\n\n#[test]\nfn big_orders_get_discount() {\n    assert_eq!(discount(200), 190);\n}\n";

const BOUNDARY_TEST: &str =
    "\n#[test]\nfn discount_boundary_discriminator() {\n    assert_eq!(discount(100), 90);\n}\n";

struct Repo(PathBuf);

impl Repo {
    /// A `pricing` crate whose one integration test sits at `test_path`,
    /// with `manifest_tail` appended to the manifest; the boundary change is
    /// committed on `feature`.
    fn create(test_path: &str, manifest_tail: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-declared-test-target-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        repo.write(
            "Cargo.toml",
            &format!(
                "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{manifest_tail}"
            ),
        )?;
        repo.write("src/lib.rs", LIB_OLD)?;
        repo.write(test_path, WEAK_TEST)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        repo.git(&["branch", "-M", "main"])?;
        repo.git(&["checkout", "-qb", "feature"])?;
        repo.write(
            "src/lib.rs",
            &LIB_OLD.replace("total > 100", "total >= 100"),
        )?;
        repo.git(&["commit", "-qam", "change"])?;
        Ok(repo)
    }

    fn declared(test_path: &str) -> Result<Self, String> {
        Self::create(
            test_path,
            &format!("\n[[test]]\nname = \"pricing\"\npath = \"{test_path}\"\n"),
        )
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn append(&self, name: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(name);
        let mut source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        source.push_str(text);
        std::fs::write(path, source).map_err(|e| e.to_string())
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

    fn ripr(&self, args: &[&str]) -> Result<Output, String> {
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())
    }

    fn check(&self) -> Result<String, String> {
        Ok(text(&self.ripr(&["check", "--base", "main"])?))
    }

    /// Starts the repair `ripr check` offers and returns the attempt id.
    fn start(&self, test_path: &str) -> Result<String, String> {
        let check = self.check()?;
        let id = check_start_id(&check)
            .ok_or_else(|| format!("check must offer a repair start:\n{check}"))?;
        let started = self.ripr(&["repair", id, "--root", "."])?;
        let screen = text(&started);
        if !started.status.success() {
            return Err(format!("repair must start:\n{screen}"));
        }
        let surface = format!("Files you may change: {test_path}");
        if !screen.contains(&surface) {
            return Err(format!("repair must name `{surface}`:\n{screen}"));
        }
        screen
            .split_whitespace()
            .find(|word| word.starts_with("repair-attempt-"))
            .map(str::to_string)
            .ok_or_else(|| format!("repair must print its attempt id:\n{screen}"))
    }

    /// `ripr continue` for `attempt`: (succeeded, text).
    fn finish(&self, attempt: &str) -> Result<(bool, String), String> {
        let output = self.ripr(&["continue", "--root", ".", "--attempt", attempt])?;
        Ok((output.status.success(), text(&output)))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The id after `ripr repair ` on the check screen, if check offers a start.
fn check_start_id(check: &str) -> Option<&str> {
    check.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("ripr repair ")?;
        let id = rest.split_whitespace().next()?;
        (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
    })
}

/// `check` links the test in `test_path` but offers no start, and bare
/// `ripr repair` starts nothing.
fn assert_linked_without_start(case: &str, repo: &Repo, test_path: &str) -> Result<(), String> {
    let check = repo.check()?;
    let mut failures = Vec::new();
    if !check.contains(&format!(
        "Related test: {test_path}:4 big_orders_get_discount"
    )) {
        failures.push(format!("{case}: check must still link the test"));
    }
    if let Some(id) = check_start_id(&check) {
        failures.push(format!("{case}: check must not offer start `{id}`"));
    }
    if !check.contains("no repair start offered: no test file ripr may edit") {
        failures.push(format!("{case}: check must say why no start is offered"));
    }
    let repair = repo.ripr(&["repair", "--root", "."])?;
    if repair.status.success() {
        failures.push(format!("{case}: repair must not start:\n{}", text(&repair)));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("{}\n{check}", failures.join("\n")))
    }
}

#[test]
fn declared_test_path_outside_tests_starts_and_completes_a_repair() -> Result<(), String> {
    for test_path in ["spec/pricing.rs", "Tests/pricing.rs"] {
        let repo = Repo::declared(test_path)?;
        let attempt = repo.start(test_path)?;
        repo.append(test_path, BOUNDARY_TEST)?;
        let (finished, screen) = repo.finish(&attempt)?;
        if !finished
            || !screen.contains(&format!(
                "edit-cage verdict for attempt `{attempt}`: Compliant"
            ))
        {
            return Err(format!(
                "{test_path}: a new test in the declared file must be a compliant edit:\n{screen}"
            ));
        }
        if !screen.contains("weak -> exposed") {
            return Err(format!(
                "{test_path}: the boundary test must move the gap:\n{screen}"
            ));
        }
    }
    Ok(())
}

#[test]
fn attempt_on_a_declared_test_file_refuses_a_production_edit() -> Result<(), String> {
    let repo = Repo::declared("spec/pricing.rs")?;
    let attempt = repo.start("spec/pricing.rs")?;
    repo.append("spec/pricing.rs", BOUNDARY_TEST)?;
    repo.append("src/lib.rs", "// touched\n")?;
    let (finished, screen) = repo.finish(&attempt)?;
    if finished
        || !screen.contains(&format!(
            "edit-cage verdict for attempt `{attempt}`: Violated"
        ))
    {
        return Err(format!(
            "a production edit must violate the cage:\n{screen}"
        ));
    }
    Ok(())
}

#[test]
fn attempt_on_a_declared_test_file_refuses_rewriting_an_existing_test() -> Result<(), String> {
    let repo = Repo::declared("spec/pricing.rs")?;
    let attempt = repo.start("spec/pricing.rs")?;
    repo.write(
        "spec/pricing.rs",
        &WEAK_TEST.replace("discount(200), 190", "discount(100), 90"),
    )?;
    let (finished, screen) = repo.finish(&attempt)?;
    if finished
        || !screen.contains("Violated")
        || !screen.contains("not a pure insertion of test functions")
    {
        return Err(format!(
            "rewriting an existing test must violate the cage:\n{screen}"
        ));
    }
    Ok(())
}

#[test]
fn declared_test_path_under_src_gets_no_start() -> Result<(), String> {
    let repo = Repo::declared("src/checks.rs")?;
    assert_linked_without_start("declared under src/", &repo, "src/checks.rs")
}

#[test]
fn test_path_that_also_roots_another_target_gets_no_start() -> Result<(), String> {
    let repo = Repo::create(
        "spec/pricing.rs",
        "\n[[test]]\nname = \"pricing\"\npath = \"spec/pricing.rs\"\n\n[[example]]\nname = \"demo\"\npath = \"spec/pricing.rs\"\n",
    )?;
    assert_linked_without_start("also an example root", &repo, "spec/pricing.rs")
}
