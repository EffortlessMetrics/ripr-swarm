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
    /// `manifest_tail` follows `a`'s `[package]` table. With `support`, the
    /// test module lives in `a/shared/checks.rs`, which `a` mounts with a
    /// literal `include!`, inside a nested `shared` package whose manifest
    /// tail is `support`: the file's nearest manifest is not its crate's.
    fn create(
        no_std: bool,
        manifest_tail: &str,
        assertion: &str,
        support: Option<&str>,
    ) -> Result<Self, String> {
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
        let members = if support.is_some() {
            "[\"a\", \"a/shared\", \"fake\"]"
        } else {
            "[\"a\", \"fake\"]"
        };
        write(
            "Cargo.toml",
            &format!("[workspace]\nmembers = {members}\nresolver = \"2\"\n"),
        )?;
        write(
            "fake/Cargo.toml",
            "[package]\nname = \"fake\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        write("fake/src/lib.rs", FAKE_ASSERT)?;
        write(
            "a/Cargo.toml",
            &format!(
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n{manifest_tail}"
            ),
        )?;
        let header = if no_std { "#![no_std]\n" } else { "" };
        let tests = format!(
            "#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn reads() {{
        {assertion}
    }}
}}
"
        );
        let mount = match support {
            Some(tail) => {
                write(
                    "a/shared/Cargo.toml",
                    &format!(
                        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n{tail}"
                    ),
                )?;
                write("a/shared/src/lib.rs", "")?;
                write("a/shared/checks.rs", &tests)?;
                "include!(\"../shared/checks.rs\");\n".to_string()
            }
            None => tests,
        };
        write(
            "a/src/lib.rs",
            &format!(
                "{header}pub struct Widget {{ size: u32 }}
impl Widget {{
    pub fn new() -> Self {{ Widget {{ size: 4 }} }}
    pub fn read(&self) -> u32 {{ self.size }}
}}
{mount}"
            ),
        )?;
        Ok(Self { root })
    }

    fn patch_root_manifest(&self, patch: &str) -> Result<(), String> {
        let path = self.root.join("Cargo.toml");
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("read root manifest failed: {error}"))?;
        std::fs::write(&path, format!("{text}{patch}"))
            .map_err(|error| format!("write root manifest failed: {error}"))
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

/// (label, no_std, `a`'s manifest tail, assertion, nested `shared` package
/// tail when the test is mounted from `a/shared/checks.rs`, expected).
type Case<'a> = (&'a str, bool, String, &'a str, Option<String>, Expect);

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
    /// Only the oracle kind of `reads` on the constructor seam.
    Oracle(&'static str),
}

#[test]
fn a_manifest_rebound_assertion_root_gives_the_constructor_no_exact_credit() -> Result<(), String> {
    const ASSERTION: &str = "std::assert_eq!(Widget::new().read(), 4);";
    const CORE: &str = "core::assert_eq!(Widget::new().read(), 4);";
    const PRETTY: &str = "pretty_assertions::assert_eq!(Widget::new().read(), 4);";
    const FAKE: &str = "{ package = \"fake\", path = \"../fake\" }";
    let dependency = |table: &str, key: &str| format!("[{table}]\n{key} = {FAKE}\n");
    let nested_core =
        "[dependencies]\ncore = { package = \"fake\", path = \"../../fake\" }\n".to_string();
    let cases: Vec<Case<'_>> = vec![
        (
            "no_std crate with std renamed to the fake package",
            true,
            dependency("dependencies", "std"),
            ASSERTION,
            None,
            Expect::Unknown,
        ),
        (
            "core renamed to the fake package",
            false,
            dependency("dependencies", "core"),
            CORE,
            None,
            Expect::Unknown,
        ),
        (
            "core renamed as a dev-dependency",
            false,
            dependency("dev-dependencies", "core"),
            CORE,
            None,
            Expect::Unknown,
        ),
        (
            "pretty_assertions renamed to the fake package",
            false,
            dependency("dependencies", "pretty_assertions"),
            PRETTY,
            None,
            Expect::Uncredited,
        ),
        (
            "plain std assertion",
            false,
            String::new(),
            ASSERTION,
            None,
            Expect::Credited,
        ),
        (
            "plain core assertion",
            false,
            String::new(),
            CORE,
            None,
            Expect::Credited,
        ),
        (
            "the fake package under another key leaves std alone",
            false,
            dependency("dependencies", "other"),
            ASSERTION,
            None,
            Expect::Credited,
        ),
        (
            "a build-only core rename is not visible to the test",
            false,
            dependency("build-dependencies", "core"),
            CORE,
            None,
            Expect::Credited,
        ),
        (
            "a workspace patch alone adds no core dependency",
            false,
            String::new(),
            CORE,
            None,
            Expect::Credited,
        ),
        (
            "an included test is judged by the including package",
            false,
            String::new(),
            CORE,
            Some(nested_core),
            Expect::Oracle("exact_value"),
        ),
        (
            "an included test under a renaming including package",
            false,
            dependency("dependencies", "core"),
            CORE,
            Some(String::new()),
            Expect::Oracle("unknown"),
        ),
    ];
    for (label, no_std, manifest_tail, assertion, support, expected) in &cases {
        let workspace = Workspace::create(*no_std, manifest_tail, assertion, support.as_deref())?;
        if label.starts_with("a workspace patch") {
            workspace.patch_root_manifest("[patch.crates-io]\ncore = { path = \"fake\" }\n")?;
        }
        let (grip, oracle, relation) = reading(&workspace.constructor_seam()?)?;
        // A cross-directory include reads `assertion_target_affinity` and
        // gets no grip either way; only its oracle is judged.
        if !matches!(expected, Expect::Oracle(_)) && relation != "direct_owner_call" {
            return Err(format!(
                "{label}: premise: `reads` calls Widget::new directly, got {relation}"
            ));
        }
        let observed = (grip.as_str(), oracle.as_str());
        let holds = match expected {
            Expect::Credited => observed == ("strongly_gripped", "exact_value"),
            Expect::Unknown => observed == ("propagation_unknown", "unknown"),
            Expect::Uncredited => !matches!(observed.0, "strongly_gripped" | "weakly_gripped"),
            Expect::Oracle(kind) => observed.1 == *kind,
        };
        if !holds {
            return Err(format!("{label}: expected {expected:?}, got {observed:?}"));
        }
    }
    Ok(())
}
