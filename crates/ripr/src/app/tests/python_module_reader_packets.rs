//! The real Python diff producer must complete the existing packet route and
//! close the same module-constant gap when its suggested current pin is added.

use crate::app::gap_bound_repair::{GapStartLookup, lookup_python_gap_start};
use crate::domain::{ExposureClass, Finding, StageState};
use crate::output::agent_seam_packets::validate_agent_gap_record_packet;
use crate::output::python_repair_card::python_repair_card;
use std::fs;
use std::path::Path;

const SOURCE: &str =
    "GREETING = 'hi'\n\ndef greet(name):\n    return f'{GREETING}, {name}'\n";
const TEST: &str =
    "from hostile.settings import greet\n\ndef test_it():\n    assert 'bob' in greet('bob')\n";
const SOURCE_DIFF: &str = "diff --git a/hostile/settings.py b/hostile/settings.py\nindex 1111111..2222222 100644\n--- a/hostile/settings.py\n+++ b/hostile/settings.py\n@@ -1,4 +1,4 @@\n-GREETING = 'hello'\n+GREETING = 'hi'\n \n def greet(name):\n     return f'{GREETING}, {name}'\n";
const TEST_DIFF: &str = "diff --git a/tests/test_settings.py b/tests/test_settings.py\nindex 3333333..4444444 100644\n--- a/tests/test_settings.py\n+++ b/tests/test_settings.py\n@@ -1,4 +1,5 @@\n from hostile.settings import greet\n \n def test_it():\n     assert 'bob' in greet('bob')\n+    assert greet('bob') == 'hi, bob'\n";

fn run_check(root: &Path) -> Result<crate::CheckOutput, String> {
    let config =
        crate::config::tests_only_parse("[languages]\nenabled = [\"rust\", \"python\"]\n")?;
    crate::app::check_workspace_with_config(
        crate::CheckInput {
            root: root.to_path_buf(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        },
        &config,
    )
}

fn source_finding(output: &crate::CheckOutput) -> Result<&Finding, String> {
    let mut findings = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.file.ends_with("hostile/settings.py"));
    match (findings.next(), findings.next()) {
        (Some(finding), None) => Ok(finding),
        _ => Err("the source edit must produce exactly one finding".to_string()),
    }
}

#[test]
fn check_output_python_module_reader_starts_and_closes_the_same_gap() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-module-reader-{}-{stamp}",
        std::process::id()
    ));
    let proof = (|| -> Result<(), String> {
        fs::create_dir_all(root.join("hostile"))
            .map_err(|error| format!("create source fixture: {error}"))?;
        fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test fixture: {error}"))?;
        fs::write(root.join("hostile/settings.py"), SOURCE)
            .map_err(|error| format!("write source fixture: {error}"))?;
        fs::write(root.join("tests/test_settings.py"), TEST)
            .map_err(|error| format!("write test fixture: {error}"))?;
        fs::write(root.join("diff.patch"), SOURCE_DIFF)
            .map_err(|error| format!("write source diff: {error}"))?;
        let before = run_check(&root)?;
        let finding = source_finding(&before)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        assert_eq!(finding.ripr.reach.state, StageState::Yes);
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
        let card = python_repair_card(finding).ok_or("weak reader repair card missing")?;
        assert_eq!(card.missing_discriminator, "greet('bob') == \"hi, bob\"");
        assert_eq!(
            card.suggested_test_node_id.as_deref(),
            Some("tests/test_settings.py::test_it")
        );
        let gap = finding
            .canonical_gap
            .as_ref()
            .ok_or("weak reader gap identity missing")?
            .id
            .clone();
        let json = crate::render_check(&before, &crate::OutputFormat::Json)?;
        let start =
            lookup_python_gap_start(&before.findings, &json, &root.to_string_lossy(), &gap);
        let mut record = match start {
            GapStartLookup::Ready(record) => record,
            GapStartLookup::NotReady { reason } => return Err(reason),
            GapStartLookup::NoSuchGap => return Err("weak reader gap not found".to_string()),
        };
        validate_agent_gap_record_packet(&record)?;
        // Packet completeness remains the shared validator's decision.
        record.verification_commands.clear();
        assert!(validate_agent_gap_record_packet(&record).is_err());

        // Append the proposed exact current pin. The real adapter marks this
        // test edited in the diff; its preceding membership holds on both sides.
        fs::write(
            root.join("tests/test_settings.py"),
            format!("{TEST}    assert greet('bob') == 'hi, bob'\n"),
        )
        .map_err(|error| format!("write exact current pin: {error}"))?;
        fs::write(root.join("diff.patch"), format!("{SOURCE_DIFF}{TEST_DIFF}"))
            .map_err(|error| format!("write source and test diff: {error}"))?;
        let after = run_check(&root)?;
        let finding = source_finding(&after)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert_eq!(
            finding
                .canonical_gap
                .as_ref()
                .ok_or("exact reader gap identity missing")?
                .id,
            gap
        );
        assert!(python_repair_card(finding).is_none());
        let json = crate::render_check(&after, &crate::OutputFormat::Json)?;
        assert!(matches!(
            lookup_python_gap_start(&after.findings, &json, &root.to_string_lossy(), &gap),
            GapStartLookup::NotReady { .. }
        ));
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove source fixture {}: {error}", root.display()));
    proof?;
    cleanup
}
