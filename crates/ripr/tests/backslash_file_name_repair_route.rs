//! #15908: on Unix a backslash is a file-name byte. A library root named
//! `src/a\b.rs` is one file inside `src/`, so its finding must carry that
//! file's own path (`src/a%5Cb.rs` in stable path text), not `src/a/b.rs`, and
//! the repair route must start, complete and still refuse an edit to it. The
//! edit cage must also observe an edit to any tracked file with such a name.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const LIB_OLD: &str = "pub fn discount(total: u32) -> u32 {\n    if total > 100 {\n        total - 10\n    } else {\n        total\n    }\n}\n";

const WEAK_TEST: &str = "use pricing::discount;\n\n#[test]\nfn big_orders_get_discount() {\n    assert_eq!(discount(200), 190);\n}\n";

const BOUNDARY_TEST: &str =
    "\n#[test]\nfn discount_boundary_discriminator() {\n    assert_eq!(discount(100), 90);\n}\n";

/// One file named `a\b.rs` inside `src/`.
const BACKSLASH_LIB: &str = "src/a\\b.rs";
/// The same file in stable path text.
const BACKSLASH_LIB_TEXT: &str = "src/a%5Cb.rs";
const TEST_PATH: &str = "tests/pricing.rs";
/// A tracked file the repair never names, also with a backslash in its name.
const NOTES_PATH: &str = "notes/a\\b.md";

struct Repo(PathBuf);

impl Repo {
    /// A `pricing` crate whose library root is `lib_path`, with one weak
    /// integration test and a tracked `notes/a\b.md`; the boundary change is
    /// committed on `feature`.
    fn create(lib_path: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-backslash-file-name-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        repo.write(
            "Cargo.toml",
            &format!(
                "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = {}\n",
                toml_string(lib_path)
            ),
        )?;
        repo.write(lib_path, LIB_OLD)?;
        repo.write(TEST_PATH, WEAK_TEST)?;
        repo.write(NOTES_PATH, "notes\n")?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        repo.git(&["branch", "-M", "main"])?;
        repo.git(&["checkout", "-qb", "feature"])?;
        repo.write(lib_path, &LIB_OLD.replace("total > 100", "total >= 100"))?;
        repo.git(&["commit", "-qam", "change"])?;
        Ok(repo)
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
    fn start(&self) -> Result<String, String> {
        let check = self.check()?;
        let id = check_start_id(&check)
            .ok_or_else(|| format!("check must offer a repair start:\n{check}"))?;
        let started = self.ripr(&["repair", id, "--root", "."])?;
        let screen = text(&started);
        if !started.status.success() {
            return Err(format!("repair must start:\n{screen}"));
        }
        let surface = format!("Files you may change: {TEST_PATH}");
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

/// A TOML basic string for `text`.
fn toml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[test]
fn backslash_named_library_file_keeps_its_own_path_and_completes_a_repair() -> Result<(), String> {
    let repo = Repo::create(BACKSLASH_LIB)?;
    let check = repo.check()?;
    let mut failures = Vec::new();
    if !check.contains(&format!("File: ./{BACKSLASH_LIB_TEXT}:2")) {
        failures.push("check must report the finding in the file's own path");
    }
    if !check.contains(&format!("gap:rust:{BACKSLASH_LIB_TEXT}:discount:")) {
        failures.push("the gap id must carry the file's own path");
    }
    if check.contains("src/a/b.rs") {
        failures.push("check must not name `src/a/b.rs`, a file that does not exist");
    }
    if !failures.is_empty() {
        return Err(format!("{}\n{check}", failures.join("\n")));
    }
    let attempt = repo.start()?;
    repo.append(TEST_PATH, BOUNDARY_TEST)?;
    let (finished, screen) = repo.finish(&attempt)?;
    if !finished
        || !screen.contains(&format!(
            "edit-cage verdict for attempt `{attempt}`: Compliant"
        ))
        || !screen.contains("weak -> exposed")
    {
        return Err(format!(
            "the boundary test must be a compliant edit that moves the gap:\n{screen}"
        ));
    }
    Ok(())
}

#[test]
fn attempt_refuses_an_edit_to_the_backslash_named_library_file() -> Result<(), String> {
    let repo = Repo::create(BACKSLASH_LIB)?;
    let attempt = repo.start()?;
    repo.append(TEST_PATH, BOUNDARY_TEST)?;
    repo.append(BACKSLASH_LIB, "// touched\n")?;
    let (finished, screen) = repo.finish(&attempt)?;
    if finished
        || !screen.contains(&format!(
            "edit-cage verdict for attempt `{attempt}`: Violated"
        ))
        || !screen.contains(&format!("{BACKSLASH_LIB} (ForbiddenPath)"))
    {
        return Err(format!(
            "a production edit must violate the cage by its own path:\n{screen}"
        ));
    }
    Ok(())
}

#[test]
fn cage_observes_an_edit_to_an_unrelated_backslash_named_file() -> Result<(), String> {
    let repo = Repo::create("src/lib.rs")?;
    let attempt = repo.start()?;
    repo.append(TEST_PATH, BOUNDARY_TEST)?;
    repo.append(NOTES_PATH, "touched\n")?;
    let (finished, screen) = repo.finish(&attempt)?;
    if finished
        || !screen.contains(&format!(
            "edit-cage verdict for attempt `{attempt}`: Violated"
        ))
        || !screen.contains(NOTES_PATH)
    {
        return Err(format!(
            "an edit outside the allowed test file must violate the cage:\n{screen}"
        ));
    }
    Ok(())
}
