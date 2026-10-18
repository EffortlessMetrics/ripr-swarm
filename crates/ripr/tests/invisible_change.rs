//! End-to-end witness for a changed return expression that no related test
//! input can tell apart from its baseline (#8659): an exact assertion on the
//! owner is not a discriminator for a change its literal inputs never reach.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
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

/// `lib` is the changed `src/lib.rs` whose line 2 replaced `old`; `test` is
/// the integration test.
fn findings(lib: &str, old: &str, test: &str) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-invisible-change-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"invisible\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", lib)?;
    write("tests/it.rs", test)?;
    let lines = lib.lines().collect::<Vec<_>>();
    let new = lines
        .get(1)
        .ok_or_else(|| "lib has a second line".to_string())?;
    let context = |range: std::ops::Range<usize>| {
        range
            .filter_map(|index| lines.get(index))
            .map(|line| format!(" {line}\n"))
            .collect::<String>()
    };
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n{}-{old}\n+{new}\n{}",
        context(0..1),
        context(2..lines.len()),
        count = lines.len()
    );
    write("diff.patch", &diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .collect())
}

fn return_value(findings: &[Finding]) -> Result<&Finding, String> {
    findings
        .iter()
        .find(|finding| finding.probe.family.as_str() == "return_value")
        .ok_or_else(|| format!("no return_value finding in {findings:?}"))
}

const POW: &str = "pub fn backoff_ms(attempt: u32, base: u64) -> u64 {\n    base * 3u64.pow(attempt)\n}\n\npub fn tail() {}\n";
const POW_OLD: &str = "    base * 2u64.pow(attempt)";
const MIN: &str =
    "pub fn capped(attempt: u32) -> u32 {\n    attempt.min(6)\n}\n\npub fn tail() {}\n";
const MIN_OLD: &str = "    attempt.min(5)";

#[test]
fn an_input_where_pow_bases_agree_does_not_expose_the_change() -> Result<(), String> {
    // `3^0 == 2^0`: the only asserted input cannot tell the versions apart.
    let findings = findings(
        POW,
        POW_OLD,
        "use invisible::backoff_ms;\n\n#[test]\nfn first() {\n    assert_eq!(backoff_ms(0, 100), 100);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn an_input_below_the_cap_does_not_expose_a_raised_cap() -> Result<(), String> {
    let findings = findings(
        MIN,
        MIN_OLD,
        "use invisible::capped;\n\n#[test]\nfn low() {\n    assert_eq!(capped(3), 3);\n}\n",
    )?;
    assert_ne!(return_value(&findings)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_second_input_that_separates_the_versions_keeps_the_credit() -> Result<(), String> {
    // Control: `backoff_ms(1, 100)` is 300 changed and 200 before.
    let findings = findings(
        POW,
        POW_OLD,
        "use invisible::backoff_ms;\n\n#[test]\nfn first() {\n    assert_eq!(backoff_ms(0, 100), 100);\n}\n\n#[test]\nfn second() {\n    assert_eq!(backoff_ms(1, 100), 300);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert!(
        !finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn an_input_that_hits_the_cap_keeps_the_credit() -> Result<(), String> {
    // Control: `capped(9)` is 6 changed and 5 before.
    let findings = findings(
        MIN,
        MIN_OLD,
        "use invisible::capped;\n\n#[test]\nfn high() {\n    assert_eq!(capped(9), 6);\n}\n",
    )?;
    assert!(
        !return_value(&findings)?
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value")
    );
    Ok(())
}

#[test]
fn a_computed_argument_keeps_the_credit() -> Result<(), String> {
    // Control: `next()` is not a literal, so nothing is folded or claimed.
    let findings = findings(
        MIN,
        MIN_OLD,
        "use invisible::capped;\n\nfn next() -> u32 {\n    3\n}\n\n#[test]\nfn computed() {\n    assert_eq!(capped(next()), 3);\n}\n",
    )?;
    assert!(
        !return_value(&findings)?
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value")
    );
    Ok(())
}
