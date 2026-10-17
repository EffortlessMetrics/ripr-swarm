use super::*;
#[test]
fn timeout_json_retry_preserves_format_and_unknown_analysis() -> Result<(), String> {
    let root = if cfg!(windows) { "C:/pilot" } else { "/pilot" };
    let packet = PathBuf::from(format!("{root}/packet"));
    let artifacts = PilotArtifacts {
        repo_exposure_json: packet.join("repo-exposure.json"),
        repo_exposure_md: packet.join("repo-exposure.md"),
        agent_seam_packets_json: packet.join("agent-seam-packets.json"),
        pilot_summary_json: packet.join("pilot-summary.json"),
        pilot_summary_md: packet.join("pilot-summary.md"),
    };
    let context = PilotSummaryContext {
        root: Path::new(root),
        timeout_ms: 100,
        ..pilot_context(&artifacts)
    };
    let document: serde_json::Value =
        serde_json::from_str(&render_pilot_timeout_summary_json(context))
            .map_err(|error| error.to_string())?;
    assert_eq!(document["schema_version"], "0.4");
    assert_eq!(document["status"], "partial");
    assert_eq!(document["reason"], "timeout");
    assert_eq!(document["timeout_ms"], 100);
    assert_eq!(document["completed_phases"], serde_json::json!([]));
    assert_eq!(
        document.get("actionable_seams_total"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(
        document.get("withheld_static_limitations_total"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(document["top_actionable_seams"], serde_json::json!([]));
    assert_eq!(
        document["outputs_written"],
        serde_json::json!(["pilot_summary_json", "pilot_summary_md"])
    );
    assert_eq!(
        document["next"]["retry_command"],
        format!(
            "ripr pilot --root {root} --out {root}/packet --mode draft --max-seams 5 --timeout-ms 120000 --format json"
        )
    );
    let human_retry = format!(
        "ripr pilot --root {root} --out {root}/packet --mode draft --max-seams 5 --timeout-ms 120000"
    );
    let markdown = render_pilot_timeout_summary_md(context);
    assert!(
        markdown.contains(&format!("```bash\n{human_retry}\n```")),
        "{markdown}"
    );
    let terminal = render_pilot_timeout_terminal(context);
    assert!(
        terminal.lines().any(|line| line.trim() == human_retry),
        "{terminal}"
    );
    assert!(!markdown.contains("--format json"), "{markdown}");
    assert!(!terminal.contains("--format json"), "{terminal}");
    Ok(())
}
