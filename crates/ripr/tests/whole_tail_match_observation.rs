//! #5500: whole-tail header observation never supplies individual-arm selection.
#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "#[derive(Debug)]
struct Lease { timeout_sec: u64, settled_elapsed_ms: Option<u128> }
fn new_lease(timeout_sec: u64) -> Lease {
    Lease {
        timeout_sec,
        settled_elapsed_ms: None,
    }
}
fn charge(lease: &Lease) -> u64 {
    match lease.settled_elapsed_ms {
        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),
        None => lease.timeout_sec,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Lease { new_lease(600) }
    #[test]
    fn charge_uses_elapsed_and_retains_legacy_reservation() {
        let mut lease = fixture();
        assert_eq!(charge(&lease), 600);
        lease.settled_elapsed_ms = Some(1001);
        assert_eq!(charge(&lease), 2);
    }
    #[test]
    fn unrelated_owner_with_same_none_literal() {
        let value: Option<u32> = None;
        assert_eq!(value, None);
    }
}
";

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str, diff: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-whole-tail-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        )));
        std::fs::create_dir_all(fixture.0.join("src"))?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname='field_selector'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        )?;
        std::fs::write(fixture.0.join("src/lib.rs"), source)?;
        std::fs::write(fixture.0.join("input.diff"), diff)?;
        Ok(fixture)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("input.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn added_diff(source: &str) -> String {
    format!("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -8,0 +9,{} @@\n{}", source.lines().count() - 8,
        source.lines().skip(8).map(|line| format!("+{line}\n")).collect::<String>())
}

#[test]
fn added_documented_owner_header_observes_its_whole_return() -> Result<(), Box<dyn std::error::Error>> {
    for visibility in ["", "pub ", "pub(crate) "] {
        let source = SOURCE.replace("fn charge(", &format!("/// Charge elapsed time.\n/// Retain reservations.\n{visibility}fn charge("));
        let output = Fixture::new(&source, &added_diff(&source))?.check()?;
        let headers = output.findings.iter().filter(|finding| finding.probe.family == ripr::ProbeFamily::MatchArm
            && finding.probe.expression == "match lease.settled_elapsed_ms").collect::<Vec<_>>();
        assert_eq!(headers.len(), 1, "one exact header subject: {visibility}");
        assert_eq!(headers[0].class, ExposureClass::Exposed, "{visibility}: {}", headers[0].id);
    }
    Ok(())
}
