//! Controls for an inherent receiver blocked only by an unverifiable assertion
//! macro (RIPR-SPEC-0352).
//!
//! Steven's linked-A / generated-B workspace has three members. `a` is
//! `[lib] test = false` and holds `Widget` with a `#[cfg(test)]` test that
//! asserts through `std::assert_eq!`. `fakestd` exports its own `assert_eq!`.
//! `b` renames `std` to `fakestd` and mounts `a`'s file through a
//! build-script `include!`, so `cargo test --workspace` runs the test only in
//! `b`, with the fake macro, and passes before and after the mutation.
//!
//! ripr cannot place a file in a Cargo target while an `include!` is
//! unresolved, so it cannot say which `assert_eq!` the test sees. The owner
//! call resolves if the macro is the standard one, so the finding is a named
//! limitation, not a gap. Nothing is credited: both consumers keep
//! `weak_token_substring`/`same_test_file`. Each control below changes one
//! thing about that shape and keeps the gap.
//!
//! `--mode ready` is required: draft mode does not index `b`.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const LIMIT: &str = "rust_inherent_receiver_unestablished";
const MARK: &str = "pub fn read(&self)->u32{4}";

/// Where `b` mounts `a`'s `shared.rs`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// A build-script `include!(concat!(env!("OUT_DIR"), ..))`: unresolved.
    Generated,
    /// A literal `#[path]` to the same file: placement is available and `std`
    /// is bound to `fakestd` in `b`.
    Resolved,
    /// No `b` and no `fakestd`: the standard macro, placement available.
    PlainStd,
}

struct Workspace {
    root: PathBuf,
    diff: PathBuf,
    /// The line of the mutated `read` body.
    owner_line: u64,
}

/// What surrounds the test statement in `shared.rs`.
#[derive(Clone, Copy, Default)]
struct Shape<'a> {
    /// Attribute lines before `#[test]`.
    attrs: &'a str,
    /// Items above `Widget` in the same file.
    prelude: &'a str,
}

fn shared_source(shape: Shape<'_>, test_statement: &str, read_body: &str) -> String {
    let Shape { attrs, prelude } = shape;
    format!(
        "{prelude}pub struct Widget;
pub struct Other;
impl Widget {{
    pub fn new()->Self{{Widget}}
    pub fn read(&self)->u32{{{read_body}}}
}}
impl Other {{
    pub fn new()->Self{{Other}}
    pub fn read(&self)->u32{{4}}
}}
#[cfg(test)]
mod tests {{
    use super::*;
    {attrs}#[test]
    fn reads(){{ {test_statement} }}
}}
"
    )
}

/// A unified diff covering the whole file, one changed line.
fn whole_file_diff(path: &str, before: &str, after: &str) -> String {
    let before: Vec<&str> = before.lines().collect();
    let after: Vec<&str> = after.lines().collect();
    let mut body = String::new();
    for (old, new) in before.iter().zip(&after) {
        if old == new {
            body.push_str(&format!(" {old}\n"));
        } else {
            body.push_str(&format!("-{old}\n+{new}\n"));
        }
    }
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{n} +1,{n} @@\n{body}",
        n = before.len()
    )
}

impl Workspace {
    fn create(placement: Placement, test_statement: &str) -> Result<Self, String> {
        Self::create_shaped(placement, Shape::default(), test_statement)
    }

    fn create_shaped(
        placement: Placement,
        shape: Shape<'_>,
        test_statement: &str,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-receiver-unestablished-{}-{}",
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
        let package = |name: &str, extra: &str| {
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n{extra}"
            )
        };
        let members = match placement {
            Placement::PlainStd => "[\"a\"]",
            _ => "[\"a\", \"b\", \"fakestd\"]",
        };
        write(
            "Cargo.toml",
            &format!("[workspace]\nmembers = {members}\nresolver = \"2\"\n"),
        )?;
        write(
            "a/Cargo.toml",
            &package("a", "\n[lib]\ntest = false\ndoctest = false\n"),
        )?;
        write("a/src/lib.rs", "mod shared;\n")?;
        let before = shared_source(shape, test_statement, "4");
        let after = shared_source(shape, test_statement, "5");
        if !before.contains(MARK) {
            return Err("premise: the mutated line is present".to_string());
        }
        write("a/src/shared.rs", &after)?;
        if placement != Placement::PlainStd {
            write("fakestd/Cargo.toml", &package("fakestd", ""))?;
            write(
                "fakestd/src/lib.rs",
                "#[macro_export]
macro_rules! assert_eq {
    ($a:expr, $b:expr) => {{
        let _ = &$a;
        let _ = &$b;
    }};
}
",
            )?;
            let build = if placement == Placement::Generated {
                "build = \"build.rs\"\n"
            } else {
                ""
            };
            write(
                "b/Cargo.toml",
                &package(
                    "b",
                    &format!(
                        "{build}\n[dependencies]\nstd = {{ package = \"fakestd\", path = \"../fakestd\" }}\n"
                    ),
                ),
            )?;
            if placement == Placement::Generated {
                write(
                    "b/src/lib.rs",
                    "#![no_std]\ninclude!(concat!(env!(\"OUT_DIR\"), \"/plain.rs\"));\n",
                )?;
                write(
                    "b/build.rs",
                    "use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var(\"OUT_DIR\").expect(\"OUT_DIR\"));
    let shared = PathBuf::from(std::env::var(\"CARGO_MANIFEST_DIR\").expect(\"manifest dir\"))
        .join(\"../a/src/shared.rs\");
    std::fs::write(
        out.join(\"plain.rs\"),
        format!(\"#[path = {:?}] mod shared;\\n\", shared.display().to_string()),
    )
    .expect(\"write plain.rs\");
}
",
                )?;
            } else {
                write(
                    "b/src/lib.rs",
                    "#![no_std]\n#[path = \"../../a/src/shared.rs\"]\nmod shared;\n",
                )?;
            }
        }
        let diff = root.join("change.diff");
        write(
            "change.diff",
            &whole_file_diff("a/src/shared.rs", &before, &after),
        )?;
        Ok(Self {
            root,
            diff,
            owner_line: 5 + shape.prelude.lines().count() as u64,
        })
    }

    fn run(&self, diff: bool) -> Result<Value, String> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .args(["check", "--root"])
            .arg(&self.root)
            .args(["--mode", "ready", "--quiet", "--format"])
            .arg(if diff { "json" } else { "repo-exposure-json" })
            .env("RIPR_CACHE_DIR", self.root.join("cache"));
        if diff {
            command.arg("--diff").arg(&self.diff);
        }
        let output = command
            .output()
            .map_err(|error| format!("run ripr failed: {error}"))?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(format!(
                "instrument: ripr exited {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("instrument: stdout is not JSON: {error}"))
    }

    /// The one finding of the mutated `Widget::read` line.
    fn finding(&self) -> Result<Value, String> {
        let document = self.run(true)?;
        let findings = document
            .get("findings")
            .and_then(Value::as_array)
            .ok_or_else(|| "instrument: no findings array".to_string())?;
        let mut on_owner = findings.iter().filter(|finding| {
            finding.pointer("/probe/line").and_then(Value::as_u64) == Some(self.owner_line)
        });
        match (on_owner.next(), on_owner.next()) {
            (Some(finding), None) => Ok(finding.clone()),
            other => Err(format!(
                "premise: one finding on the owner line, got {other:?}"
            )),
        }
    }

    /// The repo-exposure seam of `Widget::read`.
    fn seam(&self) -> Result<Value, String> {
        let document = self.run(false)?;
        let seams = document
            .get("seams")
            .and_then(Value::as_array)
            .ok_or_else(|| "instrument: no seams array".to_string())?;
        let mut found = seams.iter().filter(|seam| {
            seam.get("owner")
                .and_then(Value::as_str)
                .is_some_and(|owner| owner.ends_with("impl Widget::read"))
        });
        match (found.next(), found.next()) {
            (Some(seam), None) => Ok(seam.clone()),
            other => Err(format!("premise: one `Widget::read` seam, got {other:?}")),
        }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string at {pointer} in {value}"))
}

fn limit(finding: &Value) -> Option<&str> {
    finding.get("static_limit_kind").and_then(Value::as_str)
}

fn relations(value: &Value) -> Vec<String> {
    value
        .get("related_tests")
        .and_then(Value::as_array)
        .map(|tests| {
            tests
                .iter()
                .filter_map(|test| test.get("relation_reason").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn is_gap(finding: &Value) -> Result<bool, String> {
    Ok(matches!(
        text(finding, "/classification")?,
        "weakly_exposed" | "reachable_unrevealed"
    ))
}

/// The statement a test body asserts through `std::assert_eq!`.
const CONTROL: &str = "std::assert_eq!(Widget::new().read(), 4);";

/// Premise: the shape is a gap when nothing about the macro is in doubt, and
/// the control is the same file behind an unresolved include.
#[test]
fn an_unplaced_standard_assertion_over_the_owner_call_is_a_named_limitation() -> Result<(), String>
{
    let control = Workspace::create(Placement::Generated, CONTROL)?.finding()?;
    assert_eq!(text(&control, "/classification")?, "static_unknown");
    assert_eq!(limit(&control), Some(LIMIT), "{control}");
    assert_eq!(
        control
            .get("stop_reasons")
            .and_then(Value::as_array)
            .map(|reasons| reasons.iter().filter_map(Value::as_str).collect::<Vec<_>>()),
        Some(vec!["gap_evidence_unresolved"])
    );
    // Nothing is credited: the relation stays a name match.
    assert_eq!(
        relations(&control),
        vec!["weak_token_substring".to_string()]
    );
    assert!(
        !control
            .get("evidence")
            .and_then(Value::as_array)
            .is_some_and(|lines| lines
                .iter()
                .filter_map(Value::as_str)
                .any(|line| line.starts_with("receiver_unestablished:"))),
        "the classifier marker must not reach the output: {control}"
    );
    Ok(())
}

/// The repository consumer already reads an unknown for this cause and credits
/// nothing: `opaque`, a name-only `same_test_file` relation, not actionable.
#[test]
fn the_repository_consumer_credits_nothing_for_the_unplaced_call() -> Result<(), String> {
    let seam = Workspace::create(Placement::Generated, CONTROL)?.seam()?;
    assert_eq!(text(&seam, "/grip_class")?, "opaque", "{seam}");
    assert!(
        !text(&seam, "/evidence_record/actionability/class")?.starts_with("actionable_"),
        "{seam}"
    );
    assert!(
        !relations(&seam).iter().any(|r| r == "direct_owner_call"),
        "{seam}"
    );
    Ok(())
}

/// A receiver that would not resolve even with the macro trusted keeps the
/// gap: the rule lifts only the macro gate, never the receiver check.
#[test]
fn a_receiver_that_does_not_resolve_even_with_the_macro_trusted_keeps_its_gap() -> Result<(), String>
{
    for (label, statement) in [
        ("wrong type", "std::assert_eq!(Other::new().read(), 4);"),
        (
            "message-only operand",
            "std::assert_eq!(1u32 + 3, 4, \"{}\", Widget::new().read());",
        ),
        (
            "rebound local",
            "let w = Widget::new(); let w = Other::new(); std::assert_eq!(w.read(), 4);",
        ),
        (
            "unrelated local",
            "let w = Other::new(); std::assert_eq!(w.read(), 4);",
        ),
    ] {
        let finding = Workspace::create(Placement::Generated, statement)?.finding()?;
        assert_ne!(limit(&finding), Some(LIMIT), "{label}: {finding}");
        assert!(is_gap(&finding)?, "{label}: {finding}");
    }
    Ok(())
}

/// With placement available, a rebound standard root is a real source-level
/// refusal and keeps its gap; a plain standard macro is still credited.
#[test]
fn available_placement_keeps_the_rebound_root_gap_and_the_plain_credit() -> Result<(), String> {
    let rebound = Workspace::create(Placement::Resolved, CONTROL)?.finding()?;
    assert_ne!(limit(&rebound), Some(LIMIT), "{rebound}");
    assert!(is_gap(&rebound)?, "{rebound}");
    let plain = Workspace::create(Placement::PlainStd, CONTROL)?.finding()?;
    assert_ne!(limit(&plain), Some(LIMIT), "{plain}");
    assert_eq!(
        relations(&plain),
        vec!["direct_owner_call".to_string()],
        "premise: the plain standard macro is credited: {plain}"
    );
    Ok(())
}

/// The statement must be a gap with no limit; the label names the shape.
fn assert_keeps_gap(label: &str, finding: &Value) -> Result<(), String> {
    assert_ne!(limit(finding), Some(LIMIT), "{label}: {finding}");
    assert!(is_gap(finding)?, "{label}: {finding}");
    Ok(())
}

/// A weak oracle is a gap whether or not the macro can be trusted, so the
/// withhold must not hide it: `std::assert!(w.read() > 0)` reads
/// `weakly_exposed`/`direct_owner_call` with placement available.
#[test]
fn a_weak_oracle_keeps_its_gap_even_though_the_macro_is_unverifiable() -> Result<(), String> {
    for (label, statement) in [
        (
            "relational assert",
            "std::assert!(Widget::new().read() > 0);",
        ),
        (
            "assert_ne against a literal",
            "std::assert_ne!(Widget::new().read(), 0);",
        ),
        (
            "weak assertion over the owner next to an unrelated strong one",
            "std::assert_eq!(1u32 + 3, 4); std::assert!(Widget::new().read() > 0);",
        ),
        (
            "strong assertion on another type next to a weak one over the owner",
            "std::assert_eq!(Other::new().read(), 4); std::assert!(Widget::new().read() > 0);",
        ),
    ] {
        let finding = Workspace::create(Placement::Generated, statement)?.finding()?;
        assert_keeps_gap(label, &finding)?;
    }
    Ok(())
}

/// An unrelated weak assertion elsewhere in the test does not hide that the
/// strong one over the owner is only blocked by the macro.
#[test]
fn an_unrelated_weak_assertion_does_not_lift_the_withhold() -> Result<(), String> {
    let statement = "std::assert!(true); std::assert_eq!(Widget::new().read(), 4);";
    let finding = Workspace::create(Placement::Generated, statement)?.finding()?;
    assert_eq!(limit(&finding), Some(LIMIT), "{finding}");
    Ok(())
}

/// Only the placement-dependent part of the macro check is lifted. A visible
/// `macro_rules! assert_eq` above `Widget` replaces the bare macro, whatever
/// the workspace placement, so the assertion is not the standard one and the
/// gap stays.
#[test]
fn a_visible_macro_that_replaces_the_bare_assertion_keeps_its_gap() -> Result<(), String> {
    let shape = Shape {
        attrs: "",
        prelude: "macro_rules! assert_eq { ($($t:tt)*) => {}; }\n",
    };
    let finding = Workspace::create_shaped(
        Placement::Generated,
        shape,
        "assert_eq!(Widget::new().read(), 4);",
    )?
    .finding()?;
    assert_keeps_gap("bare macro shadowed in the file", &finding)
}

/// A call that resolves with the macro gate on (it is outside any assertion)
/// is already credited as a direct call; nothing is blocked by the macro, so
/// the rule must not claim it. `read` is a local, so the assertion text names
/// the owner while the call is not inside the macro.
#[test]
fn a_call_outside_the_assertion_is_not_blocked_by_the_macro() -> Result<(), String> {
    let statement = "let read = Widget::new().read(); std::assert_eq!(read, 4);";
    let finding = Workspace::create(Placement::Generated, statement)?.finding()?;
    assert_keeps_gap("call outside the assertion", &finding)?;
    assert_eq!(
        relations(&finding),
        vec!["direct_owner_call".to_string()],
        "premise: the call is credited without the macro: {finding}"
    );
    Ok(())
}

/// An `#[ignore]` test never runs in the default suite, so it cannot be what
/// stands between the finding and a gap.
#[test]
fn an_ignored_test_does_not_trigger_the_withhold() -> Result<(), String> {
    let shape = Shape {
        attrs: "#[ignore]\n    ",
        prelude: "",
    };
    let finding = Workspace::create_shaped(Placement::Generated, shape, CONTROL)?.finding()?;
    assert_keeps_gap("ignored test", &finding)
}

/// A same-named type in another crate is not the owner: its test is never one
/// of the finding's related tests, and the withhold must not look at it.
#[test]
fn a_same_named_type_in_another_crate_does_not_trigger_the_withhold() -> Result<(), String> {
    let workspace = Workspace::create(
        Placement::Generated,
        "std::assert_eq!(Other::new().read(), 4);",
    )?;
    let write = |path: &str, text: &str| -> Result<(), String> {
        let file = workspace.root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(file, text).map_err(|error| error.to_string())
    };
    write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"a\", \"b\", \"fakestd\", \"c\"]\nresolver = \"2\"\n",
    )?;
    write(
        "c/Cargo.toml",
        "[package]\nname = \"c\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )?;
    write(
        "c/src/lib.rs",
        "pub struct Widget;
impl Widget {
    pub fn new() -> Self { Widget }
    pub fn read(&self) -> u32 { 4 }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_c() { std::assert_eq!(Widget::new().read(), 4); }
}
",
    )?;
    let finding = workspace.finding()?;
    assert_keeps_gap("same-named type in another crate", &finding)
}

impl Workspace {
    fn write_file(&self, path: &str, text: &str) -> Result<(), String> {
        let file = self.root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(file, text).map_err(|error| error.to_string())
    }
}

/// A bare `assert_eq!` is decided by name. A site in the test's own package
/// (here a non-exported `macro_rules! assert_eq` in another module of the same
/// crate, ahead of `shared` by `#[macro_use]`) reaches it whatever the
/// placement, so that refusal is not placement-dependent and the gap stays.
#[test]
fn an_own_crate_macro_in_another_module_keeps_the_bare_assertion_gap() -> Result<(), String> {
    let workspace =
        Workspace::create(Placement::Generated, "assert_eq!(Widget::new().read(), 4);")?;
    workspace.write_file("a/src/lib.rs", "#[macro_use]\nmod noop;\nmod shared;\n")?;
    workspace.write_file(
        "a/src/noop.rs",
        "macro_rules! assert_eq { ($($t:tt)*) => {}; }\n",
    )?;
    let finding = workspace.finding()?;
    assert_keeps_gap("own-crate macro in another module", &finding)
}

/// The serde shape: another crate re-exports `core::*` (so `assert_eq!` is
/// ambiguous workspace-wide) and the test crate asserts with a bare
/// `assert_eq!`. That crate's file reaches the test only if mounted into its
/// target, which unavailable placement cannot establish, so the bare assertion
/// is a named limitation. The other crate here is `fakestd`, which also exports
/// an `assert_eq!` of its own.
#[test]
fn another_crates_glob_reexport_is_part_of_the_unplaced_doubt() -> Result<(), String> {
    let workspace =
        Workspace::create(Placement::Generated, "assert_eq!(Widget::new().read(), 4);")?;
    workspace.write_file(
        "Cargo.toml",
        "[workspace]\nmembers = [\"a\", \"b\", \"fakestd\", \"g\"]\nresolver = \"2\"\n",
    )?;
    workspace.write_file(
        "g/Cargo.toml",
        "[package]\nname = \"g\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )?;
    workspace.write_file("g/src/lib.rs", "#![no_std]\npub use core::*;\n")?;
    let finding = workspace.finding()?;
    assert_eq!(
        limit(&finding),
        Some(LIMIT),
        "another crate's glob and exported macro are placement-dependent doubt: {finding}"
    );
    Ok(())
}
