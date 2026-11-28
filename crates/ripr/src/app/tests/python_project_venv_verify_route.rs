//! The Python producer chooses the pytest verify interpreter and confidence
//! once, from static facts at the root (#13801), and every surface carries
//! that one decision: JSON (evidence, `repair_placement`,
//! `python_repair_card`), SARIF, GitHub, human, the pilot card and the MCP gap
//! evidence document. No renderer reads the filesystem.

use crate::domain::{ExposureClass, Finding};
use crate::output::pilot::{
    PilotArtifacts, PilotPythonFirstUse, PilotSummaryContext, render_pilot_summary_json,
};
use crate::output::python_repair_card::python_repair_card;
use crate::{Mode, OutputFormat};
use std::fs;
use std::path::{Path, PathBuf};

const SOURCE: &str = "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n";
const TEST: &str = "from src.discount import apply_discount\n\n\ndef test_apply_discount_smoke():\n    result = apply_discount(100, 50)\n    if result is None:\n        raise AssertionError(\"missing result\")\n";
const DIFF: &str = "diff --git a/src/discount.py b/src/discount.py\nindex 0000000..1111111 100644\n--- a/src/discount.py\n+++ b/src/discount.py\n@@ -1,4 +1,4 @@\n def apply_discount(amount, threshold):\n-    if amount > threshold:\n+    if amount >= threshold:\n         return amount - 10\n     return amount\n";
const CONFIG: &str = "[languages]\nenabled = [\"python\"]\n";
const NODE: &str = "tests/test_discount.py::test_apply_discount_smoke";

/// The project virtualenv a fixture root carries.
#[derive(Clone, Copy, Debug)]
enum Venv {
    None,
    /// `<dir>/bin/python` with pytest in site-packages.
    WithPytest(&'static str),
    /// `<dir>/bin/python` without pytest.
    WithoutPytest(&'static str),
    /// `<dir>/bin/python` whose `pyvenv.cfg` includes system site-packages.
    SystemSitePackages(&'static str),
}

struct Fixture(PathBuf);

impl Fixture {
    fn new(venv: Venv) -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("system time: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-python-venv-route-{}-{stamp}",
            std::process::id()
        ));
        let fixture = Self(root);
        let root = &fixture.0;
        let write = |file: &str, text: &str| -> Result<(), String> {
            let path = root.join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, text).map_err(|error| format!("write {file}: {error}"))
        };
        for (file, text) in [
            ("src/discount.py", SOURCE),
            ("tests/test_discount.py", TEST),
            ("ripr.toml", CONFIG),
            ("diff.patch", DIFF),
        ] {
            write(file, text)?;
        }
        let site = "lib/python3.13/site-packages";
        match venv {
            Venv::None => {}
            Venv::WithPytest(dir) => {
                write(&format!("{dir}/bin/python"), "")?;
                write(&format!("{dir}/pyvenv.cfg"), "version = 3.13.1\n")?;
                write(&format!("{dir}/{site}/pytest-9.2.0.dist-info/METADATA"), "")?;
                write(&format!("{dir}/{site}/pytest/__init__.py"), "")?;
            }
            Venv::WithoutPytest(dir) => {
                write(&format!("{dir}/bin/python"), "")?;
                write(&format!("{dir}/pyvenv.cfg"), "version = 3.13.1\n")?;
                write(&format!("{dir}/{site}/pip/__init__.py"), "")?;
            }
            Venv::SystemSitePackages(dir) => {
                write(&format!("{dir}/bin/python"), "")?;
                write(
                    &format!("{dir}/pyvenv.cfg"),
                    "include-system-site-packages = true\nversion = 3.13.1\n",
                )?;
                write(&format!("{dir}/{site}/pip/__init__.py"), "")?;
            }
        }
        Ok(fixture)
    }

    fn root(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_check(root: &Path) -> Result<crate::CheckOutput, String> {
    let config = crate::config::tests_only_parse(CONFIG)?;
    crate::app::check_workspace_with_config(
        crate::CheckInput {
            root: root.to_path_buf(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
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

fn weak_finding(output: &crate::CheckOutput) -> Result<&Finding, String> {
    let mut findings = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.file.ends_with("src/discount.py"));
    match (findings.next(), findings.next()) {
        (Some(finding), None) if finding.class == ExposureClass::WeaklyExposed => Ok(finding),
        other => Err(format!(
            "the boundary edit must produce one weakly exposed finding: {other:?}"
        )),
    }
}

/// Assert every surface carries `command` at `confidence`.
fn assert_surfaces_agree(fixture: &Fixture, command: &str, confidence: &str) -> Result<(), String> {
    let root = fixture.root();
    let output = run_check(root)?;
    let finding = weak_finding(&output)?;
    let config = crate::config::tests_only_parse(CONFIG)?;

    // Producer evidence: the one decision every surface reads.
    let suggested = format!("suggested_verify_command: {command}");
    let level = format!("suggested_verify_command_confidence: {confidence}");
    assert!(
        finding.evidence.contains(&suggested),
        "{:#?}",
        finding.evidence
    );
    assert!(finding.evidence.contains(&level), "{:#?}", finding.evidence);
    let per_test = format!("test_verify_command: {command} (test_apply_discount_smoke)");
    assert!(
        finding.evidence.contains(&per_test),
        "{:#?}",
        finding.evidence
    );

    // Python repair card (shared by JSON, SARIF, GitHub, human and pilot).
    let card = python_repair_card(finding).ok_or("the weak finding must carry a repair card")?;
    assert_eq!(card.verify_command, command);
    assert_eq!(card.verify_command_confidence, confidence);

    // JSON: evidence, placement and card.
    let json = crate::app::render_check_with_config(&output, &OutputFormat::Json, &config)?;
    let json: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("check json: {error}"))?;
    let json_finding = json["findings"]
        .as_array()
        .and_then(|findings| {
            findings.iter().find(|row| {
                row["python_repair_card"]["canonical_gap_id"].as_str()
                    == Some(card.canonical_gap_id.as_str())
            })
        })
        .ok_or("json must project the carded finding")?;
    assert_eq!(json_finding["repair_placement"]["verify_command"], command);
    assert_eq!(
        json_finding["repair_placement"]["verify_command_confidence"],
        confidence
    );
    assert_eq!(
        json_finding["python_repair_card"]["verify"]["command"],
        command
    );
    assert_eq!(
        json_finding["python_repair_card"]["verify"]["confidence"],
        confidence
    );

    // SARIF and GitHub project the same card.
    let sarif = crate::app::render_check_with_config(&output, &OutputFormat::Sarif, &config)?;
    let sarif: serde_json::Value =
        serde_json::from_str(&sarif).map_err(|error| format!("sarif: {error}"))?;
    let sarif_text = sarif.to_string();
    assert!(
        sarif_text.contains(&format!(r#""command":"{command}""#)),
        "{sarif_text}"
    );
    assert!(
        sarif_text.contains(&format!(r#""confidence":"{confidence}""#)),
        "{sarif_text}"
    );
    // Every pytest command SARIF carries names the producer's interpreter;
    // none falls back to PATH `python`.
    let interpreter = command.split(' ').next().unwrap_or_default();
    assert_eq!(
        sarif_text.matches("python -m pytest").count(),
        sarif_text
            .matches(&format!("{interpreter} -m pytest"))
            .count(),
        "{sarif_text}"
    );

    let github = crate::app::render_check_with_config(&output, &OutputFormat::Github, &config)?;
    assert!(github.contains(command), "{github}");

    // Human: the rooted Bash action runs the producer's interpreter.
    let human = crate::app::render_check_with_config(&output, &OutputFormat::Human, &config)?;
    if cfg!(windows) {
        assert!(human.contains(command), "{human}");
    } else {
        assert!(
            human.contains(&format!(" && {command}) ({confidence})")),
            "{human}"
        );
    }

    // Pilot projects the same card into its JSON summary.
    let first_use = PilotPythonFirstUse::from_check_output(&output);
    let pilot_card = first_use
        .top_repair_card
        .as_ref()
        .ok_or("pilot must select the card")?;
    assert_eq!(pilot_card.verify_command, command);
    assert_eq!(pilot_card.verify_command_confidence, confidence);
    let artifacts = PilotArtifacts {
        repo_exposure_json: PathBuf::from("target/ripr/pilot/repo-exposure.json"),
        repo_exposure_md: PathBuf::from("target/ripr/pilot/repo-exposure.md"),
        agent_seam_packets_json: PathBuf::from("target/ripr/pilot/agent-seam-packets.json"),
        pilot_summary_json: PathBuf::from("target/ripr/pilot/pilot-summary.json"),
        pilot_summary_md: PathBuf::from("target/ripr/pilot/pilot-summary.md"),
    };
    let pilot = render_pilot_summary_json(
        &[],
        PilotSummaryContext {
            root,
            mode: &Mode::Draft,
            config_path: None,
            max_seams: 5,
            timeout_ms: 30_000,
            timeout_extension_ms: 0,
            artifacts: &artifacts,
            python_first_use: Some(&first_use),
            language_routes: None,
            current_change: None,
            seam_limit: None,
            repo_exposure_byte_bound: None,
            inventory_evidence_limitations: &[],
        },
    );
    let pilot: serde_json::Value =
        serde_json::from_str(&pilot).map_err(|error| format!("pilot json: {error}"))?;
    let top = &pilot["python_first_use"]["top_repair_card"];
    assert_eq!(top["verify_command"], command, "{pilot:#}");
    assert_eq!(top["verify_command_confidence"], confidence, "{pilot:#}");

    // MCP gap evidence document carries the producer's evidence verbatim.
    let item = crate::mcp::gaps::GapItem::from_finding(finding, root)?;
    let evidence = item.evidence_core["evidence"]
        .as_array()
        .ok_or("mcp evidence must be an array")?;
    assert!(
        evidence.iter().any(|line| line == &suggested),
        "{evidence:?}"
    );
    assert!(evidence.iter().any(|line| line == &level), "{evidence:?}");
    Ok(())
}

fn venv_command(interpreter: &str) -> String {
    format!("{interpreter} -m pytest {NODE}")
}

#[test]
fn no_project_venv_keeps_path_python_at_high_on_every_surface() -> Result<(), String> {
    let fixture = Fixture::new(Venv::None)?;
    assert_surfaces_agree(&fixture, &venv_command("python"), "high")?;
    let output = run_check(fixture.root())?;
    assert!(
        !weak_finding(&output)?
            .evidence
            .iter()
            .any(|line| line.starts_with("suggested_verify_venv_pytest: ")),
        "no venv adds no venv fact"
    );
    Ok(())
}

#[test]
fn dot_venv_with_pytest_routes_its_interpreter_at_high_on_every_surface() -> Result<(), String> {
    let fixture = Fixture::new(Venv::WithPytest(".venv"))?;
    if cfg!(windows) {
        // Windows keeps the PATH interpreter (POSIX venv layout only).
        return assert_surfaces_agree(&fixture, &venv_command("python"), "high");
    }
    assert_surfaces_agree(&fixture, &venv_command(".venv/bin/python"), "high")
}

#[test]
fn plain_venv_without_pytest_is_low_on_every_surface() -> Result<(), String> {
    let fixture = Fixture::new(Venv::WithoutPytest("venv"))?;
    if cfg!(windows) {
        return assert_surfaces_agree(&fixture, &venv_command("python"), "high");
    }
    assert_surfaces_agree(&fixture, &venv_command("venv/bin/python"), "low")
}

#[test]
fn venv_with_unreadable_pytest_fact_is_medium_on_every_surface() -> Result<(), String> {
    let fixture = Fixture::new(Venv::SystemSitePackages(".venv"))?;
    if cfg!(windows) {
        return assert_surfaces_agree(&fixture, &venv_command("python"), "high");
    }
    assert_surfaces_agree(&fixture, &venv_command(".venv/bin/python"), "medium")
}
