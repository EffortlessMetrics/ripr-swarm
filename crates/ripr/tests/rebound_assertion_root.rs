//! A crate-qualified assertion whose root the test's manifest binds to
//! another package is not a known assertion (#10150, RIPR-SPEC-0197).
//!
//! `std = { package = "fakestd" }` in a `#![no_std]` crate, or
//! `core = { package = "fakecore" }` in an ordinary one, makes
//! `std::assert_eq!` / `core::assert_eq!` whatever that package exports. Here
//! the package's `assert_eq!` discards its tokens, so the test calls nothing
//! and a mutated `Widget::new` survives `cargo test`. The constructor seam must
//! not read `strongly_gripped` from that assertion. The same file and test
//! without the rename keep the exact-value credit, and a rename under another
//! key leaves the standard root alone.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const FAKE_ASSERT: &str =
    "#![no_std]\n#[macro_export]\nmacro_rules! assert_eq { ($($t:tt)*) => { () }; }\n";

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// One package `a` holding `Widget` and a test `reads` whose only
    /// statement is `assertion`, plus a token-discarding package `fake`.
    fn create(no_std: bool, dependencies: &str, assertion: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-rebound-assertion-root-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write = |path: &str, text: &str| -> Result<(), String> {
            let file = root.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(&file, text).map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"fake\"]\nresolver = \"2\"\n",
        )?;
        write(
            "fake/Cargo.toml",
            "[package]\nname = \"fake\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        write("fake/src/lib.rs", FAKE_ASSERT)?;
        write(
            "a/Cargo.toml",
            &format!(
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{dependencies}"
            ),
        )?;
        let header = if no_std { "#![no_std]\n" } else { "" };
        write(
            "a/src/lib.rs",
            &format!(
                "{header}pub struct Widget {{ size: u32 }}
impl Widget {{
    pub fn new() -> Self {{ Widget {{ size: 4 }} }}
    pub fn read(&self) -> u32 {{ self.size }}
}}
#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn reads() {{
        {assertion}
    }}
}}
"
            ),
        )?;
        Ok(Self { root })
    }

    fn constructor_seam(&self) -> Result<Value, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.root)
            .args([
                "--mode",
                "ready",
                "--quiet",
                "--format",
                "repo-exposure-json",
            ])
            .env("RIPR_CACHE_DIR", self.root.join("cache"))
            .output()
            .map_err(|error| format!("run ripr failed: {error}"))?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(format!(
                "instrument: ripr exited {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let document: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("instrument: stdout is not JSON: {error}"))?;
        let seams = document
            .pointer("/seams")
            .and_then(Value::as_array)
            .ok_or("instrument: no seams array")?;
        seams
            .iter()
            .find(|seam| {
                seam.pointer("/owner").and_then(Value::as_str)
                    == Some("a/src/lib.rs::impl Widget::new")
                    && seam.pointer("/kind").and_then(Value::as_str) == Some("return_value")
            })
            .cloned()
            .ok_or_else(|| format!("premise: no Widget::new return seam in {seams:?}"))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// (grip class, related test `reads` oracle kind, its relation reason).
fn reading(seam: &Value) -> Result<(String, String, String), String> {
    let field = |pointer: &str| {
        seam.pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("instrument: {pointer} missing in {seam}"))
    };
    let test = seam
        .pointer("/related_tests")
        .and_then(Value::as_array)
        .and_then(|tests| {
            tests
                .iter()
                .find(|test| test.pointer("/name").and_then(Value::as_str) == Some("reads"))
        })
        .ok_or_else(|| format!("premise: test `reads` is not related to {seam}"))?;
    let test_field = |pointer: &str| {
        test.pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("instrument: {pointer} missing in {test}"))
    };
    Ok((
        field("/grip_class")?,
        test_field("/oracle_kind")?,
        test_field("/relation_reason")?,
    ))
}

#[derive(Debug)]
enum Expect {
    /// `strongly_gripped` through an `exact_value` oracle.
    Credited,
    /// `propagation_unknown` through an `unknown` oracle: the rebound root's
    /// package may or may not assert.
    Unknown,
    /// Any reading without grip credit. The renamed `pretty_assertions` root
    /// is already refused before this pass (pinned here, not changed).
    Uncredited,
}

#[test]
fn a_manifest_rebound_assertion_root_gives_the_constructor_no_exact_credit() -> Result<(), String> {
    const ASSERTION: &str = "std::assert_eq!(Widget::new().read(), 4);";
    const CORE: &str = "core::assert_eq!(Widget::new().read(), 4);";
    const PRETTY: &str = "pretty_assertions::assert_eq!(Widget::new().read(), 4);";
    // (label, no_std, [dependencies] body, assertion, expected reading)
    let cases: &[(&str, bool, &str, &str, Expect)] = &[
        (
            "no_std crate with std renamed to the fake package",
            true,
            "std = { package = \"fake\", path = \"../fake\" }\n",
            ASSERTION,
            Expect::Unknown,
        ),
        (
            "core renamed to the fake package",
            false,
            "core = { package = \"fake\", path = \"../fake\" }\n",
            CORE,
            Expect::Unknown,
        ),
        (
            "pretty_assertions renamed to the fake package",
            false,
            "pretty_assertions = { package = \"fake\", path = \"../fake\" }\n",
            PRETTY,
            Expect::Uncredited,
        ),
        (
            "plain std assertion",
            false,
            "",
            ASSERTION,
            Expect::Credited,
        ),
        ("plain core assertion", false, "", CORE, Expect::Credited),
        (
            "the fake package under another key leaves std alone",
            false,
            "other = { package = \"fake\", path = \"../fake\" }\n",
            ASSERTION,
            Expect::Credited,
        ),
    ];
    for (label, no_std, dependencies, assertion, expected) in cases {
        let workspace = Workspace::create(*no_std, dependencies, assertion)?;
        let (grip, oracle, relation) = reading(&workspace.constructor_seam()?)?;
        if relation != "direct_owner_call" {
            return Err(format!(
                "{label}: premise: `reads` calls Widget::new directly, got {relation}"
            ));
        }
        let observed = (grip.as_str(), oracle.as_str());
        let holds = match expected {
            Expect::Credited => observed == ("strongly_gripped", "exact_value"),
            Expect::Unknown => observed == ("propagation_unknown", "unknown"),
            Expect::Uncredited => !matches!(observed.0, "strongly_gripped" | "weakly_gripped"),
        };
        if !holds {
            return Err(format!("{label}: expected {expected:?}, got {observed:?}"));
        }
    }
    Ok(())
}
