//! #15933 (tokio `8bdf1e9d`): `compile_error!(..)` stops the build, so no
//! test runs past it. A changed invocation must not read as a deleted call
//! or a log side effect. It keeps one `static_unknown` finding per line that
//! names the `compile_time_invariant` stop reason, so an empty result cannot
//! pass these tests. A different macro on the same line keeps its runtime
//! reading.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const HEAD: &str = "pub fn total(items: &[u32]) -> u32 {
    items.iter().sum()
}

";

const TAIL: &str = "
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn totals() {
        assert_eq!(total(&[1, 2]), 3);
    }
}
";

/// Findings for a diff that adds `added` (whole lines) after line 4 of the
/// fixture, so the first added line is line 5.
fn findings_for_added(added: &str) -> Result<Vec<Finding>, String> {
    let count = added.lines().count();
    let plus = added
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    findings_for(
        &format!("{HEAD}{added}{TAIL}"),
        &format!(
            "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,3 +3,{} @@\n }}\n \n{plus} \n",
            3 + count
        ),
    )
}

/// Findings for `diff` applied to a crate whose `src/lib.rs` is `lib`.
fn findings_for(lib: &str, diff: &str) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-compile-error-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )),
    };
    let root = &repo.root;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    write(
        "Cargo.toml",
        "[package]\nname = \"compile-error\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", lib)?;
    write("diff.patch", diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    Ok(output.findings)
}

fn describe(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|finding| {
            format!(
                "{}:{} {:?} {:?}",
                finding.probe.family.as_str(),
                finding.probe.location.line,
                finding.probe.expression,
                finding
                    .stop_reasons
                    .iter()
                    .map(|reason| reason.as_str())
                    .collect::<Vec<_>>()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn families_on(findings: &[Finding], line: usize) -> Vec<ProbeFamily> {
    findings
        .iter()
        .filter(|finding| finding.probe.location.line == line)
        .map(|finding| finding.probe.family.clone())
        .collect()
}

#[test]
fn a_changed_compile_error_is_one_build_time_finding() -> Result<(), String> {
    let findings = findings_for_added(
        "#[cfg(all(target_os = \"emscripten\", feature = \"process\"))]\ncompile_error!(\"Feature process is not supported on emscripten.\");\n",
    )?;
    let on_line = findings
        .iter()
        .filter(|finding| finding.probe.location.line == 6)
        .collect::<Vec<_>>();
    assert_eq!(
        families_on(&findings, 6),
        vec![ProbeFamily::StaticUnknown],
        "{}",
        describe(&findings)
    );
    assert!(
        on_line[0]
            .stop_reasons
            .iter()
            .any(|reason| reason.as_str() == "compile_time_invariant"),
        "{}",
        describe(&findings)
    );
    Ok(())
}

#[test]
fn a_split_compile_error_selects_no_runtime_family_on_any_line() -> Result<(), String> {
    let findings = findings_for_added(
        "#[cfg(feature = \"process\")]\ncompile_error!(\n    \"notify(user) > limit is not supported here\"\n);\n",
    )?;
    for line in 6..=8 {
        assert!(
            families_on(&findings, line)
                .iter()
                .all(|family| *family == ProbeFamily::StaticUnknown),
            "line {line}: {}",
            describe(&findings)
        );
    }
    assert_eq!(
        families_on(&findings, 6),
        vec![ProbeFamily::StaticUnknown],
        "{}",
        describe(&findings)
    );
    Ok(())
}

/// Control: the same line with another macro, or with code after the
/// invocation, keeps a runtime family. Without it the tests above could pass
/// on a fixture that never selects one.
#[test]
fn another_macro_or_shared_line_keeps_its_runtime_family() -> Result<(), String> {
    for added in [
        "#[cfg(feature = \"process\")]\nreport_error!(\"Feature process is not supported on emscripten.\");\n",
        "#[cfg(feature = \"process\")]\ncompile_errors!(\"Feature process is not supported on emscripten.\");\n",
    ] {
        let findings = findings_for_added(added)?;
        assert!(
            families_on(&findings, 6)
                .iter()
                .any(|family| *family != ProbeFamily::StaticUnknown),
            "{added}: {}",
            describe(&findings)
        );
    }
    Ok(())
}

/// Review of #16134: a diff that changes only the message line of a split
/// invocation leaves the probe holding the message alone. Its line, not its
/// text, places it inside the invocation, so it still names the build-time
/// stop reason instead of `no_changed_rust_line`.
#[test]
fn a_changed_message_line_of_a_split_compile_error_is_build_time() -> Result<(), String> {
    let lib = format!(
        "{HEAD}#[cfg(feature = \"process\")]\ncompile_error!(\n    \"notify(user) > limit is not supported\"\n);\n{TAIL}"
    );
    let findings = findings_for(
        &lib,
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5,4 +5,4 @@\n #[cfg(feature = \"process\")]\n compile_error!(\n-    \"notify(user) > limit is not supported here\"\n+    \"notify(user) > limit is not supported\"\n );\n",
    )?;
    let on_line = findings
        .iter()
        .filter(|finding| finding.probe.location.line == 7)
        .collect::<Vec<_>>();
    assert!(!on_line.is_empty(), "{}", describe(&findings));
    for finding in &on_line {
        assert_eq!(
            finding.probe.family,
            ProbeFamily::StaticUnknown,
            "{}",
            describe(&findings)
        );
        assert!(
            finding
                .stop_reasons
                .iter()
                .any(|reason| reason.as_str() == "compile_time_invariant"),
            "{}",
            describe(&findings)
        );
    }
    Ok(())
}

/// Review of #16134: a literal after the invocation is code that shares the
/// line. Changing that returned literal keeps its runtime reading even when
/// a cfg attribute drops the macro.
#[test]
fn a_literal_sharing_the_line_keeps_its_runtime_family() -> Result<(), String> {
    let lib = "pub fn name() -> &'static str {\n    #[cfg(any())]\n    compile_error!(\"unsupported\"); \"new\"\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn named() {\n        assert_eq!(name(), \"new\");\n    }\n}\n";
    let findings = findings_for(
        lib,
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,4 +1,4 @@\n pub fn name() -> &'static str {\n     #[cfg(any())]\n-    compile_error!(\"unsupported\"); \"old\"\n+    compile_error!(\"unsupported\"); \"new\"\n }\n",
    )?;
    assert!(
        families_on(&findings, 3)
            .iter()
            .any(|family| *family != ProbeFamily::StaticUnknown),
        "{}",
        describe(&findings)
    );
    Ok(())
}

/// Review of #16134: the same holds when the invocation is split and the
/// literal sits after its closing delimiter, for a string and for a char.
#[test]
fn a_literal_after_a_split_invocation_keeps_its_runtime_family() -> Result<(), String> {
    for (ty, old, new) in [
        ("&'static str", "\"old\"", "\"new\""),
        ("char", "'o'", "'n'"),
    ] {
        let lib = format!(
            "pub fn name() -> {ty} {{\n    #[cfg(any())]\n    compile_error!(\n        \"unsupported\"\n    ); {new}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn named() {{\n        assert_eq!(name(), {new});\n    }}\n}}\n"
        );
        let findings = findings_for(
            &lib,
            &format!(
                "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,4 +3,4 @@\n     compile_error!(\n         \"unsupported\"\n-    ); {old}\n+    ); {new}\n }}\n"
            ),
        )?;
        assert!(
            families_on(&findings, 5)
                .iter()
                .any(|family| *family != ProbeFamily::StaticUnknown),
            "{ty}: {}",
            describe(&findings)
        );
    }
    Ok(())
}
