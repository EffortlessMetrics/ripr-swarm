use super::{probe_command, run_owned_stdin_probe, unique_temp_workspace};
use std::process::Stdio;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Compare literal source-cell boundaries through the CLI, without deriving
/// expectations from the renderer's width or segmentation helpers.
fn assert_unicode_source(name: &str, payload: &str, expected_after: &str) -> TestResult {
    let root = unique_temp_workspace(name);
    let result: TestResult = (|| {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"source_unicode\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )?;
        let source_line = format!("\"{payload}\"");
        std::fs::write(
            root.join("src/lib.rs"),
            format!("pub fn label() -> &'static str {{\n{source_line}\n}}\n"),
        )?;
        let diff = root.join("change.patch");
        std::fs::write(
            &diff,
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn label() -> &'static str {{\n-\"old\"\n+{source_line}\n }}\n",
            ),
        )?;
        let root_arg = root.to_str().ok_or("fixture path must be UTF-8")?;
        let diff_arg = diff.to_str().ok_or("diff path must be UTF-8")?;
        let mut command = probe_command(env!("CARGO_BIN_EXE_ripr"));
        command
            .args([
                "check",
                "--root",
                root_arg,
                "--diff",
                diff_arg,
                "--format",
                "human-full",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_owned_stdin_probe(command, &[], Duration::from_secs(30))?;
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rendered = String::from_utf8(output.stdout)?;
        let changed = rendered
            .split_once("\nChanged\n")
            .ok_or("CLI must analyze a source finding")?
            .1
            .split_once("\nProbe\n")
            .ok_or("finding must carry a Probe section")?
            .0;
        assert_eq!(
            changed,
            format!("  before: \"old\"\n{expected_after}"),
            "{name}: wrong literal source-cell boundary"
        );
        let after = changed
            .split_once("  after:  ")
            .ok_or("after field missing")?
            .1;
        let reassembled: String = after
            .lines()
            .enumerate()
            .map(|(index, line)| {
                if index == 0 {
                    Ok(line)
                } else {
                    line.strip_prefix("          ")
                        .ok_or("continuation indent missing")
                }
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("");
        assert_eq!(
            reassembled, source_line,
            "{name}: source bytes lost or changed"
        );
        // Exercise the two other fields through the public finding renderer.
        // Their expected boundaries come from the same literal test inputs,
        // never from a production width/segmentation calculation.
        let analysis = ripr::check_workspace(ripr::CheckInput {
            root: root.clone(),
            diff_file: Some(diff.clone()),
            ..ripr::CheckInput::default()
        })?;
        let mut finding = analysis
            .findings
            .first()
            .ok_or("API must analyze the source finding")?
            .clone();
        finding.probe.before = Some(source_line.clone());
        finding.probe.after = None;
        finding.probe.expression = source_line.clone();
        let rendered = ripr::output::human::render_finding(&finding);
        let changed = rendered
            .split_once("\nChanged\n")
            .ok_or("API Changed section missing")?
            .1
            .split_once("\nProbe\n")
            .ok_or("API Probe section missing")?
            .0;
        let expected_value = expected_after
            .strip_prefix("  after:  ")
            .ok_or("literal expectation label missing")?;
        assert_eq!(
            changed,
            format!("  before: {expected_value}  expr:   {expected_value}"),
            "{name}: before/expr fields must use the same literal boundaries"
        );
        let oversized = format!("क{}", "्क".repeat(170));
        for (value, expected) in [
            (String::new(), "  after:  \n".to_string()),
            (
                "short\n\nlast\n".to_string(),
                "  after:  short\n\nlast\n\n".to_string(),
            ),
            (
                "x".repeat(171),
                format!("  after:  {}\n          x\n", "x".repeat(170)),
            ),
            (
                "\t".repeat(171),
                format!("  after:  {}\n          \t\n", "\t".repeat(170)),
            ),
            (
                format!("{oversized}Z"),
                format!("  after:  {oversized}\n          Z\n"),
            ),
        ] {
            finding.probe.before = None;
            finding.probe.after = Some(value);
            let rendered = ripr::output::human::render_finding(&finding);
            let changed = rendered
                .split_once("\nChanged\n")
                .ok_or("control Changed section missing")?
                .1
                .split_once("\nProbe\n")
                .ok_or("control Probe section missing")?
                .0;
            assert_eq!(changed, expected, "{name}: boundary/progress control");
        }
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root);
    result?;
    cleanup?;
    Ok(())
}

#[test]
fn human_full_source_unicode_wide() -> TestResult {
    assert_unicode_source(
        "source-unicode-wide",
        &"界".repeat(180),
        &format!(
            "  after:  \"{}\n          {}\n          {}\"\n",
            "界".repeat(84),
            "界".repeat(85),
            "界".repeat(11),
        ),
    )
}

#[test]
fn human_full_source_unicode_combining() -> TestResult {
    assert_unicode_source(
        "source-unicode-combining",
        &format!("{}e\u{301}Z", "a".repeat(168)),
        &format!("  after:  \"{}e\u{301}\n          Z\"\n", "a".repeat(168)),
    )
}

#[test]
fn human_full_source_unicode_emoji() -> TestResult {
    assert_unicode_source(
        "source-unicode-emoji",
        &format!("{}👩‍🔬Z", "a".repeat(167)),
        &format!("  after:  \"{}👩‍🔬\n          Z\"\n", "a".repeat(167)),
    )
}

#[test]
fn human_full_source_unicode_ascii_control() -> TestResult {
    assert_unicode_source(
        "source-unicode-ascii",
        &"a".repeat(180),
        &format!(
            "  after:  \"{}\n          {}\"\n",
            "a".repeat(169),
            "a".repeat(11),
        ),
    )
}
