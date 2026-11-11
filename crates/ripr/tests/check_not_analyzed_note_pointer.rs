//! #14152: the default `ripr check` screen never points at a note that is
//! not there.
//!
//! A changed language with no analyzer prints one plain `Not analyzed:` line.
//! For a preview language (Python, TypeScript) a `Note:` follows with the
//! `ripr.toml` block to add, and the line points at it. Rust switched off in
//! `[languages]` has no such note, so the line used to say "(see the note
//! below)" above nothing, while the agent, GitHub and `human-full` formats
//! all printed the recovery. It now prints the producer's own recovery.
//!
//! Every case here needs a preview-language adapter in the binary, so the
//! file is empty in a build without them.

#![cfg(all(feature = "lang-python", feature = "lang-typescript"))]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"np\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const OLD_RUST: &str = "pub fn over(qty: u32) -> bool {\n    qty > 10\n}\n";
const NEW_RUST: &str = "pub fn over(qty: u32) -> bool {\n    qty >= 10\n}\n";
const OLD_PYTHON: &str = "def over(qty):\n    return qty > 10\n";
const NEW_PYTHON: &str = "def over(qty):\n    return qty >= 10\n";

const POINTER: &str = "(see the note below)";
const RUST_FIX: &str = "  Fix: The effective [languages].enabled set is [\"typescript\"], which excludes rust; add \"rust\" to [languages].enabled in ripr.toml and re-run the analysis.";
const PYTHON_NOTE: &str = "Note: this diff contains 1 Python file.";

struct Repo(PathBuf);

impl Repo {
    /// A committed base with one Rust and one Python source file.
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-note-pointer-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        std::fs::create_dir_all(repo.0.join("src")).map_err(|e| e.to_string())?;
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", OLD_RUST)?;
        repo.write("over.py", OLD_PYTHON)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
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

    /// Commits the working tree as the change and returns the default human
    /// `check` screen (stdout) for that commit.
    fn check_change(&self) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--no-gpg-sign", "-m", "change"])?;
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .env("RIPR_QUIET", "1")
            .args(["check", "--base", "HEAD~1"])
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.code() != Some(0) {
            return Err(format!(
                "check must complete: {:?}\n{stdout}\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(stdout)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _cleanup = std::fs::remove_dir_all(&self.0);
    }
}

fn require(text: &str, needle: &str, wanted: bool) -> Result<(), String> {
    if text.contains(needle) == wanted {
        Ok(())
    } else {
        Err(format!(
            "expected contains({needle:?}) == {wanted} in:\n{text}"
        ))
    }
}

/// Every pointer line must have a `Note:` somewhere after it.
fn require_no_dangling_pointer(text: &str) -> Result<(), String> {
    match text.rfind(POINTER) {
        Some(at) if !text[at..].contains("\nNote: ") => {
            Err(format!("a pointer has no note after it:\n{text}"))
        }
        _ => Ok(()),
    }
}

#[test]
fn rust_switched_off_prints_its_recovery_instead_of_a_pointer() -> Result<(), String> {
    let repo = Repo::create()?;
    repo.write("ripr.toml", "[languages]\nenabled = [\"typescript\"]\n")?;
    repo.write("src/lib.rs", NEW_RUST)?;
    let screen = repo.check_change()?;
    require(&screen, "analysis incomplete", true)?;
    require(
        &screen,
        "  Not analyzed: 1 changed file(s) in a language ripr is not set up to analyze.\n",
        true,
    )?;
    require(&screen, RUST_FIX, true)?;
    require(&screen, POINTER, false)?;
    // The bounded screen still keeps the schema tokens out.
    require(&screen, "language_adapter_unavailable", false)?;
    require_no_dangling_pointer(&screen)
}

#[test]
fn a_preview_language_keeps_the_pointer_and_its_note() -> Result<(), String> {
    let repo = Repo::create()?;
    repo.write("over.py", NEW_PYTHON)?;
    let screen = repo.check_change()?;
    require(
        &screen,
        "  Not analyzed: 1 changed file(s) in a language ripr is not set up to analyze (see the note below).\n",
        true,
    )?;
    require(&screen, PYTHON_NOTE, true)?;
    require(&screen, "  Fix: ", false)?;
    require_no_dangling_pointer(&screen)
}

#[test]
fn a_mixed_diff_points_only_the_preview_language_at_the_note() -> Result<(), String> {
    let repo = Repo::create()?;
    repo.write("ripr.toml", "[languages]\nenabled = [\"typescript\"]\n")?;
    repo.write("src/lib.rs", NEW_RUST)?;
    repo.write("over.py", NEW_PYTHON)?;
    let screen = repo.check_change()?;
    require(&screen, RUST_FIX, true)?;
    require(&screen, PYTHON_NOTE, true)?;
    if screen.matches(POINTER).count() != 1 {
        return Err(format!(
            "exactly the Python limitation points at the note:\n{screen}"
        ));
    }
    require_no_dangling_pointer(&screen)
}
