//! Public-API controls for an integer whole-body constant tail whose owner
//! a `cfg` gate may swap for a twin (#15104).
//!
//! `fn ready() -> i32 { 1 }` changed to `{ 2 }` with a test pinning
//! `ready() == 2` reads `weakly_exposed` with propagation credited: the
//! tail is the owner's return. When the owner carries a `cfg` gate, sits in
//! a gated module, or has a same-path `cfg` twin, the test may run the twin
//! instead of the changed body, so the legacy constant-tail edge must not
//! credit propagation. Ungated owners, including one beside an unrelated
//! `cfg(test)` module or a same-named function on another path, keep the
//! credit.

use ripr::domain::StageState;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const TESTS: &str = "
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_is_two() {
        assert_eq!(ready(), 2);
    }
}
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str, extra: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-constant-tail-cfg-twin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"constant-tail\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
            .map_err(|error| format!("write diff failed: {error}"))?;
        for (path, text) in extra {
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))?;
        }
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        // Keep a failing run's crate for inspection.
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// `src/lib.rs` as `items` then the shared test module, and a diff that
/// replaces the tail `1` with `2` on `line` (1-based) of `items`.
fn tail_finding(items: &str, line: usize) -> Result<Finding, String> {
    tail_finding_with(items, line, &[])
}

/// [`tail_finding`] with `extra` workspace files beside `src/lib.rs`.
fn tail_finding_with(items: &str, line: usize, extra: &[(&str, &str)]) -> Result<Finding, String> {
    let lines: Vec<&str> = items.lines().collect();
    let changed = lines
        .get(line - 1)
        .ok_or_else(|| format!("premise: line {line} exists in {items:?}"))?;
    if changed.trim() != "2" {
        return Err(format!(
            "premise: line {line} is the tail `2`, got {changed:?}"
        ));
    }
    let old_changed = changed.replace('2', "1");
    let total = lines.len();
    let mut patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{total} +1,{total} @@\n"
    );
    for (index, text) in lines.iter().enumerate() {
        if index + 1 == line {
            patch.push_str(&format!("-{old_changed}\n+{text}\n"));
        } else {
            patch.push_str(&format!(" {text}\n"));
        }
    }
    let lib = format!("{items}\n{TESTS}");
    let findings = TempRepo::create(&lib, &patch, extra)?.findings()?;
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.line == line
            && finding.probe.expression.trim() == "2"
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one return_value finding for `2` on line {line}, got {other:?} in {findings:?}"
        )),
    }
}

/// The changed tail keeps its exposure and direct-return credit.
fn assert_credited(items: &str, line: usize) -> Result<(), String> {
    let finding = tail_finding(items, line)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.ripr.propagate.state, StageState::Yes, "{finding:?}");
    Ok(())
}

/// The changed tail reads no stronger than `propagation_unknown`: no
/// exposure credit, and propagation unknown for the `cfg` gate.
fn assert_not_credited(items: &str, line: usize) -> Result<(), String> {
    let finding = tail_finding(items, line)?;
    assert!(
        !matches!(
            finding.class,
            ExposureClass::Exposed | ExposureClass::WeaklyExposed
        ),
        "{finding:?}"
    );
    assert_eq!(
        finding.ripr.propagate.state,
        StageState::Unknown,
        "{finding:?}"
    );
    assert!(
        finding.ripr.propagate.summary.contains("cfg_gated_owner"),
        "{finding:?}"
    );
    // Reach stands, so the refusal is the propagation one.
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{finding:?}");
    assert_eq!(
        finding.class,
        ExposureClass::PropagationUnknown,
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn a_plain_integer_constant_tail_keeps_its_credit() -> Result<(), String> {
    assert_credited("pub fn ready() -> i32 {\n    2\n}", 2)
}

#[test]
fn an_unrelated_cfg_test_module_keeps_the_credit() -> Result<(), String> {
    assert_credited(
        "pub fn ready() -> i32 {\n    2\n}\n\n#[cfg(test)]\nmod fake {\n    pub fn stub() -> i32 {\n        2\n    }\n}",
        2,
    )
}

#[test]
fn a_same_named_function_on_another_path_keeps_the_credit() -> Result<(), String> {
    assert_credited(
        "pub fn ready() -> i32 {\n    2\n}\n\npub mod other {\n    pub fn ready() -> i32 {\n        5\n    }\n}",
        2,
    )
}

/// #15104: the changed body compiles only outside tests; the test runs the
/// `cfg(test)` twin, which returns `2` on both sides.
#[test]
fn a_function_level_cfg_twin_is_not_credited() -> Result<(), String> {
    assert_not_credited(
        "#[cfg(not(test))]\npub fn ready() -> i32 {\n    2\n}\n\n#[cfg(test)]\npub fn ready() -> i32 {\n    2\n}",
        3,
    )
}

/// Platform twins: the second one changes. Either may be the one a test
/// runs, so a gate on the owner refuses whichever twin changed.
#[test]
fn the_second_platform_twin_changing_is_not_credited() -> Result<(), String> {
    assert_not_credited(
        "#[cfg(unix)]\npub fn ready() -> i32 {\n    3\n}\n\n#[cfg(not(unix))]\npub fn ready() -> i32 {\n    2\n}",
        8,
    )
}

/// The twins are whole `cfg`-gated modules re-exported under one name.
#[test]
fn a_cfg_module_twin_is_not_credited() -> Result<(), String> {
    assert_not_credited(
        "#[cfg(not(test))]\nmod m {\n    pub fn ready() -> i32 {\n        2\n    }\n}\n\n#[cfg(test)]\nmod m {\n    pub fn ready() -> i32 {\n        2\n    }\n}\n\npub use m::ready;",
        4,
    )
}

/// A `cfg_attr` on the owner may change it too.
#[test]
fn a_cfg_attr_on_the_owner_is_not_credited() -> Result<(), String> {
    assert_not_credited(
        "#[cfg_attr(test, inline(never))]\npub fn ready() -> i32 {\n    2\n}",
        3,
    )
}

/// A workspace file the parser refuses still reaches the module-declaration
/// reading when it may declare a module (`mod/* gap */real;`), so the
/// owner's chain cannot be read and the tail is not credited. The same
/// declaration in a file that parses leaves the ungated owner its credit.
#[test]
fn an_unparseable_declaring_file_withholds_the_credit() -> Result<(), String> {
    let items = "pub fn ready() -> i32 {\n    2\n}";
    let readable = [
        ("src/other.rs", "mod/* gap */real;\n"),
        ("src/real.rs", "pub fn real() {}\n"),
    ];
    let finding = tail_finding_with(items, 2, &readable)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.ripr.propagate.state, StageState::Yes, "{finding:?}");
    let unreadable = [
        ("src/other.rs", "mod/* gap */real;\nfn broken( {\n"),
        ("src/real.rs", "pub fn real() {}\n"),
    ];
    let finding = tail_finding_with(items, 2, &unreadable)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(
        finding.ripr.propagate.state,
        StageState::Unknown,
        "{finding:?}"
    );
    assert!(
        finding.ripr.propagate.summary.contains("cfg_gated_owner"),
        "{finding:?}"
    );
    Ok(())
}
