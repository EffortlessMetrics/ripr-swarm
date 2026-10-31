//! Gap-ledger badge rendering and the first-run legend marker of `ripr check`:
//! the success path of `--gap-ledger` for every `repo-badge-*` format, and the
//! containment rule that keeps the legend marker inside the workspace.

use super::super::tests::unique_command_test_dir;
use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Result<Self, String> {
        let path = unique_command_test_dir(label);
        std::fs::create_dir_all(&path).map_err(|err| err.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One gap counted by the strict `ripr` badge and one counted only by `ripr+`.
const LEDGER: &str = r#"{
  "gap_records": [
    {
      "gap_id": "gap:repo:strict",
      "source_currentness": "candidate_current",
      "kind": "MissingBoundaryAssertion",
      "language": "rust",
      "language_status": "stable",
      "scope": "repo_scoped",
      "gap_state": "reintroduced",
      "policy_state": "reintroduced",
      "repairability": "repairable",
      "projection_eligibility": {
        "ripr_zero_count": {"eligible": true, "reason": "repo_policy_targeted_unresolved_gap"},
        "ripr_plus_count": {"eligible": true, "reason": "broader_repo_advisory_gap"}
      }
    },
    {
      "gap_id": "gap:repo:broader",
      "source_currentness": "candidate_current",
      "kind": "MissingValueAssertion",
      "language": "rust",
      "language_status": "stable",
      "scope": "repo_scoped",
      "gap_state": "reintroduced",
      "policy_state": "reintroduced",
      "repairability": "repairable",
      "projection_eligibility": {
        "ripr_zero_count": {"eligible": false, "reason": "broader_repo_advisory_gap"},
        "ripr_plus_count": {"eligible": true, "reason": "broader_repo_advisory_gap"}
      }
    }
  ]
}"#;

fn render(ledger: &Path, format: OutputFormat) -> Result<String, String> {
    render_check_gap_ledger_badge(ledger, &format, &RiprConfig::default())
}

#[test]
fn gap_ledger_badge_selects_kind_and_wire_shape_from_the_format() -> Result<(), String> {
    let dir = TempDir::new("gap-ledger-formats")?;
    let ledger = dir.0.join("ledger.json");
    std::fs::write(&ledger, LEDGER).map_err(|err| err.to_string())?;

    let ripr = render(&ledger, OutputFormat::RepoBadgeJson)?;
    let plus = render(&ledger, OutputFormat::RepoBadgePlusJson)?;
    let ripr_shields = render(&ledger, OutputFormat::RepoBadgeShields)?;
    let plus_shields = render(&ledger, OutputFormat::RepoBadgePlusShields)?;

    // The strict badge counts one gap and the plus badge counts both: only the
    // format choice differs between these calls over the same ledger.
    if ripr == plus {
        return Err("ripr and ripr+ formats must not render the same badge".to_string());
    }
    if !ripr_shields.contains("\"message\": \"1 actionable\"") {
        return Err(format!("ripr shields must show 1 gap, got {ripr_shields}"));
    }
    if !plus_shields.contains("\"message\": \"2 actionable\"") {
        return Err(format!(
            "ripr+ shields must show 2 gaps, got {plus_shields}"
        ));
    }
    // Shields output is the endpoint schema, native output is not.
    if !ripr_shields.contains("schemaVersion") || ripr.contains("schemaVersion") {
        return Err("shields and native wire shapes must be distinct".to_string());
    }
    Ok(())
}

#[test]
fn gap_ledger_badge_refuses_a_non_badge_format_even_with_a_valid_ledger() -> Result<(), String> {
    let dir = TempDir::new("gap-ledger-format-refusal")?;
    let ledger = dir.0.join("ledger.json");
    std::fs::write(&ledger, LEDGER).map_err(|err| err.to_string())?;
    // Single-change negative control: the same ledger renders under a badge format.
    render(&ledger, OutputFormat::RepoBadgeJson)?;
    match render(&ledger, OutputFormat::Json) {
        Err(message) if message.contains("only supported with repo-badge-* formats") => Ok(()),
        other => Err(format!("a non-badge format must be refused, got {other:?}")),
    }
}

#[test]
fn legend_marker_lives_under_the_workspace_output_tree_and_is_written_once() -> Result<(), String> {
    let dir = TempDir::new("legend-marker")?;
    let Some(marker) = human_legend_marker(&dir.0) else {
        return Err("a plain workspace must get a legend marker path".to_string());
    };
    if marker != dir.0.join("target/ripr/legend-seen") {
        return Err(format!("unexpected marker path {}", marker.display()));
    }
    if marker.exists() {
        return Err("choosing the marker path must not create it".to_string());
    }
    record_human_legend_seen(&dir.0, &marker);
    let written = std::fs::read_to_string(&marker).map_err(|err| err.to_string())?;
    if written != "1\n" {
        return Err(format!("unexpected marker contents {written:?}"));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn legend_marker_is_refused_when_the_output_tree_links_outside_the_workspace() -> Result<(), String>
{
    let workspace = TempDir::new("legend-marker-escape")?;
    let outside = TempDir::new("legend-marker-outside")?;
    std::os::unix::fs::symlink(&outside.0, workspace.0.join("target"))
        .map_err(|err| err.to_string())?;
    if human_legend_marker(&workspace.0).is_some() {
        return Err("a target link leaving the workspace must give no marker".to_string());
    }
    // Writing is refused as well, even when the caller names a marker directly.
    let escaped = workspace.0.join("target/ripr/legend-seen");
    record_human_legend_seen(&workspace.0, &escaped);
    if outside.0.join("ripr/legend-seen").exists() {
        return Err("the marker must not be written through the escaping link".to_string());
    }
    // Negative control: with the link removed, the same call yields a marker.
    std::fs::remove_file(workspace.0.join("target")).map_err(|err| err.to_string())?;
    if human_legend_marker(&workspace.0).is_none() {
        return Err("a contained output tree must get a marker".to_string());
    }
    Ok(())
}
