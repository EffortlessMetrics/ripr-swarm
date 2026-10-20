//! Identity and inventory controls for the inherited #8954 tail-return fixture.
//! Fixture setup only: production probe selection remains the analyzer's owner.

use crate::python_judged_panel::{DiffLineKind, parse_unified_diff};
use std::path::Path;

pub(super) const FIXTURE: &str = "observation_verified_side_effect";

pub(super) fn validate(path: &Path) -> Result<(), String> {
    let source = crate::read_text_lossy(&path.join("input/src/lib.rs"))?;
    let diff = crate::read_text_lossy(&path.join("diff.patch"))?;
    validate_identity(&source, &diff).map_err(|error| {
        format!("{FIXTURE} source/diff identity: {error}; refusing fixture analysis")
    })
}

fn validate_identity(source: &str, diff: &str) -> Result<(), String> {
    // Reuse the retained hunk parser; do not infer coordinates from text search
    // or accept the offset recovery that `git apply --check` permits.
    let parsed = parse_unified_diff(diff)?;
    let [section] = parsed.sections.as_slice() else {
        return Err("expected one src/lib.rs file section".into());
    };
    if section.old_path.as_deref() != Some("src/lib.rs")
        || section.new_path.as_deref() != Some("src/lib.rs")
    {
        return Err("expected src/lib.rs on both sides".into());
    }
    let [hunk] = section.hunks.as_slice() else {
        return Err("expected one payload-change hunk".into());
    };
    if hunk.old_start != hunk.new_start {
        return Err("payload replacement must preserve old/new coordinates".into());
    }
    if hunk.added.len() != 1 || hunk.deleted.len() != 1 {
        return Err("expected one payload replacement".into());
    }
    let lines: Vec<_> = source.lines().collect();
    let mut coordinate = usize::try_from(hunk.new_start).map_err(|error| error.to_string())?;
    for line in &hunk.lines {
        if matches!(line.kind, DiffLineKind::Deleted) {
            if line.text != "    notifier.send(\"static_msg\")" {
                return Err("expected the original static payload tail call".into());
            }
            continue;
        }
        if coordinate
            .checked_sub(1)
            .and_then(|index| lines.get(index))
            .copied()
            != Some(line.text.as_str())
        {
            return Err(format!(
                "new-side line {coordinate} does not match input/src/lib.rs"
            ));
        }
        if matches!(line.kind, DiffLineKind::Added)
            && (line.text != "    notifier.send(order_id)"
                || coordinate
                    .checked_sub(2)
                    .and_then(|index| lines.get(index))
                    .copied()
                    != Some("pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {"))
        {
            return Err("expected process_order's tail-return call without a semicolon".into());
        }
        coordinate += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures")
            .join(FIXTURE)
    }

    #[test]
    fn inherited_tail_return_has_real_parser_inventory() -> Result<(), String> {
        let fixture = fixture();
        let source = crate::read_text_lossy(&fixture.join("input/src/lib.rs"))?;
        let call_line = source
            .lines()
            .position(|line| line == "    notifier.send(order_id)")
            .ok_or("missing tail-return call")?
            + 1;
        let report = check_workspace(CheckInput {
            root: fixture.join("input"),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        assert_eq!(report.findings.len(), 2, "{:#?}", report.findings);
        let families: std::collections::BTreeSet<_> = report
            .findings
            .iter()
            .map(|finding| finding.probe.family.as_str())
            .collect();
        assert_eq!(
            families,
            std::collections::BTreeSet::from(["return_value", "side_effect"])
        );
        for finding in &report.findings {
            assert_eq!(finding.probe.location.line, call_line);
            assert_eq!(finding.probe.expression.trim(), "notifier.send(order_id)");
            assert!(!finding.related_tests.is_empty());
            if finding.probe.family == ProbeFamily::SideEffect {
                assert_eq!(finding.class, ExposureClass::Exposed);
                assert!(
                    !finding
                        .evidence
                        .iter()
                        .any(|line| line.contains("observation_unverified"))
                );
            } else {
                assert_eq!(finding.class, ExposureClass::WeaklyExposed);
            }
        }
        Ok(())
    }

    #[test]
    fn inherited_tail_return_rejects_shifted_coordinates_before_analysis() -> Result<(), String> {
        let fixture = fixture();
        let source = crate::read_text_lossy(&fixture.join("input/src/lib.rs"))?;
        let aligned = crate::read_text_lossy(&fixture.join("diff.patch"))?;
        validate_identity(&source, &aligned)?;
        // Keep the basename so the real fixture-output route selects its guard.
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "8954-shifted-tail-return-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let scratch = root.join(FIXTURE);
        std::fs::create_dir_all(scratch.join("input/src")).map_err(|e| e.to_string())?;
        std::fs::write(scratch.join("input/src/lib.rs"), &source).map_err(|e| e.to_string())?;
        for shifted in [
            "@@ -15,4 +15,4 @@",
            "@@ -17,4 +17,4 @@",
            "@@ -15,4 +16,4 @@",
        ] {
            let wrong = aligned.replace("@@ -16,4 +16,4 @@", shifted);
            assert_ne!(wrong, aligned, "control must shift the actual hunk");
            std::fs::write(scratch.join("diff.patch"), wrong).map_err(|e| e.to_string())?;
            let error = super::super::run_fixture_outputs(&scratch)
                .err()
                .ok_or("shifted fixture reached analysis")?;
            assert!(error.contains("source/diff identity"), "{error}");
            assert!(error.contains("refusing fixture analysis"), "{error}");
        }
        Ok(())
    }
}
