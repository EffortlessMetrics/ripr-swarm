//! A refused output location must keep the requested machine-output contract.
use super::{
    DeclaredHunkScratch, init_producer_fixture_repo, spawn_command, unique_temp_workspace,
};
use serde_json::Value;
use std::{fs, path::Path, process::Output};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn check(root: &Path, format: &str) -> Result<Output, std::io::Error> {
    // Reuse the harness's 30-second owned-process deadline and stream drains.
    // An empty cache override keeps ambient configuration from waiving the guard.
    spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &["check", "--format", format, "--quiet"],
        &[("RIPR_CACHE_DIR", "")],
        None,
        Some(b""),
    )
}

#[test]
fn repo_exposure_containment_failure_keeps_the_refusal_document() -> TestResult {
    for relative in ["target", "target/ripr"] {
        let directory = unique_temp_workspace("containment-refusal");
        let _cleanup = DeclaredHunkScratch(directory.clone());
        let root = directory.join("checkout");
        let outside = directory.join("outside");
        init_producer_fixture_repo(&root)?;
        fs::create_dir_all(&outside)?;
        fs::write(outside.join("sentinel"), "outside data")?;
        let link = root.join(relative);
        fs::create_dir_all(link.parent().ok_or("output parent missing")?)?;
        std::os::unix::fs::symlink(&outside, &link)?;

        for format in ["json", "repo-exposure-json"] {
            let output = check(&root, format)?;
            let stderr = String::from_utf8(output.stderr)?;
            assert_eq!(output.status.code(), Some(2), "{format}: {stderr}");
            assert!(stderr.contains("refusing to write ripr output"), "{stderr}");
            assert!(
                stderr.contains("resolves outside the workspace"),
                "{stderr}"
            );
            // Parse the whole stream: empty output, two documents and a partial
            // success report are all rejected, despite sharing failure exit 2.
            let report: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
                format!("{format}/{relative}: invalid refusal JSON: {error}; stderr: {stderr}")
            })?;
            let limitation = if format == "json" {
                assert_eq!(
                    report["analysis_scope"]["downstream_consumable"], false,
                    "{report}"
                );
                assert_eq!(report["findings"], serde_json::json!([]), "{report}");
                &report["run_limitations"][0]
            } else {
                assert_eq!(report["schema_version"], "0.5", "{report}");
                assert_eq!(report["scope"], "repo", "{report}");
                assert_eq!(report["run_status"], "refused", "{report}");
                assert_eq!(report["seams"], serde_json::json!([]), "{report}");
                assert_eq!(report["metrics"]["seams_total"], 0, "{report}");
                &report["limitations"][0]
            };
            // This is an operational failure, not a source finding or a
            // malformed configuration: preserve the existing typed fallback.
            assert_eq!(limitation["category"], "analysis_failed", "{report}");
            assert_eq!(limitation["repair_route"], "analysis/failure", "{report}");
            let message = limitation["message"].as_str().ok_or("missing diagnostic")?;
            assert!(
                stderr.contains(message),
                "stdout/stderr disagree: {report}, {stderr}"
            );
            assert_eq!(fs::read_link(&link)?, outside);
            assert_eq!(
                fs::read_to_string(outside.join("sentinel"))?,
                "outside data"
            );
            assert_eq!(
                fs::read_dir(&outside)?.count(),
                1,
                "refusal wrote outside root"
            );
        }

        // The same nonempty source is analyzable once the test-owned link is
        // replaced with a contained directory; refusing every run cannot pass.
        fs::remove_file(&link)?;
        fs::create_dir(&link)?;
        let success = check(&root, "repo-exposure-json")?;
        assert_eq!(success.status.code(), Some(0), "{success:?}");
        let report: Value = serde_json::from_slice(&success.stdout)?;
        assert!(
            report["seams"]
                .as_array()
                .is_some_and(|seams| !seams.is_empty()),
            "{report}"
        );
        assert_ne!(report["run_status"], "refused", "{report}");
        assert_eq!(
            fs::read_to_string(outside.join("sentinel"))?,
            "outside data"
        );
        assert_eq!(fs::read_dir(&outside)?.count(), 1);
    }
    Ok(())
}
