use super::*;

/// Pin argv and redirect spelling independently of the rendering helpers.
/// These are the default packet paths, not a backslash-free explicit --out.
#[test]
fn pilot_followups_preserve_native_default_artifacts() {
    for (root_text, prefix, suffix) in [
        ("/tmp/team\\repo", "'/tmp/team\\repo", "'"),
        ("/tmp/team's\\repo", r"'/tmp/team'\''s\repo", "'"),
        ("/tmp/ordinary-repo", "/tmp/ordinary-repo", ""),
    ] {
        let root = Path::new(root_text);
        let out = root.join("target/ripr/pilot");
        let artifacts = PilotArtifacts {
            repo_exposure_json: out.join("repo-exposure.json"),
            repo_exposure_md: out.join("repo-exposure.md"),
            agent_seam_packets_json: out.join("agent-seam-packets.json"),
            pilot_summary_json: out.join("pilot-summary.json"),
            pilot_summary_md: out.join("pilot-summary.md"),
        };
        let context = PilotSummaryContext {
            root,
            timeout_ms: 1,
            ..pilot_context(&artifacts)
        };
        let commands = commands::PilotCommands::new(context);
        let quoted_root = format!("{prefix}{suffix}");
        let quoted_out = format!("{prefix}/target/ripr/pilot{suffix}");
        let before = format!("{prefix}/target/ripr/pilot/repo-exposure.json{suffix}");
        let after = format!("{prefix}/target/ripr/pilot/after.repo-exposure.json{suffix}");
        assert_eq!(
            commands.retry,
            format!(
                "ripr pilot --root {quoted_root} --out {quoted_out} --mode draft --max-seams 5 --timeout-ms 120000"
            )
        );
        assert_eq!(
            commands.after_snapshot,
            format!(
                "ripr check --root {quoted_root} --mode draft --format repo-exposure-json > {after}"
            )
        );
        assert_eq!(
            commands.outcome,
            format!("ripr outcome --before {before} --after {after}")
        );
        assert_eq!(
            commands::python_card_first_pr_command(root),
            format!("ripr first-pr --root {quoted_root}")
        );
    }
}
