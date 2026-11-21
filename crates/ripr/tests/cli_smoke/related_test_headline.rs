//! Public headline projections and independent preview-card grades.

use super::run_ripr;

fn run_fixture(fixture: &str, command: &str, options: &[&str]) -> Result<String, String> {
    let case = format!("{}/../../fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
    let root = format!("{case}/input");
    let diff = format!("{case}/diff.patch");
    let mut args = vec![command, "--root", &root, "--diff", &diff];
    args.extend_from_slice(options);
    String::from_utf8(run_ripr(&args).stdout).map_err(|error| error.to_string())
}

/// Pin the public one-row projection to independent source facts. Both
/// Python constants bind the changed boundary only in test_at. The two
/// heuristic fixtures never call the changed owner.
#[cfg(feature = "lang-python")]
#[test]
fn public_packet_pairs_boundaries_and_discloses_heuristic_rows() -> Result<(), String> {
    for (fixture, observer) in [
        (
            "python_module_constant_boundary_rival_control",
            Some("test_at"),
        ),
        (
            "python_module_constant_boundary_test_local_control",
            Some("test_at"),
        ),
        ("python_related_test_name_similarity", None),
        ("javascript_js_preview", None),
    ] {
        let report: serde_json::Value =
            serde_json::from_str(&run_fixture(fixture, "check", &["--format", "json"])?)
                .map_err(|e| e.to_string())?;
        let finding = &report["findings"][0];
        let id = finding["id"].as_str().ok_or("missing fixture finding")?;
        let packet: serde_json::Value = serde_json::from_str(&run_fixture(
            fixture,
            "context",
            &["--at", id, "--max-related-tests", "1", "--json"],
        )?)
        .map_err(|e| e.to_string())?;
        let human = run_fixture(fixture, "check", &["--format", "human"])?;
        let agent = run_fixture(fixture, "explain", &["--format", "agent", id])?;
        let rows = packet["related_tests"]
            .as_array()
            .ok_or("missing packet rows")?;
        assert_eq!(rows.len(), 1, "{fixture}");
        if let Some(name) = observer {
            assert_eq!(rows[0]["name"], name, "{fixture}: {packet}");
            assert!(
                human
                    .lines()
                    .any(|l| l.trim_start().starts_with("Related test") && l.contains(name)),
                "{fixture}: {human}"
            );
            assert!(
                agent
                    .lines()
                    .any(|l| l.starts_with("t1=") && l.contains(name)),
                "{fixture}: {agent}"
            );
            assert_eq!(finding["oracle_strength"], "strong");
        } else {
            let message = "only name or file matches were retained";
            assert!(human.contains(message), "{fixture}: {human}");
            assert!(
                packet["missing"].as_array().is_some_and(|xs| xs
                    .iter()
                    .any(|s| s.as_str().is_some_and(|x| x.contains(message)))),
                "{fixture}: {packet}"
            );
            assert!(
                !agent.lines().any(|l| l.starts_with("t1=")),
                "{fixture}: {agent}"
            );
            assert_eq!(finding["oracle_strength"], "none");
        }
    }
    Ok(())
}

/// These preview audit cases retain their independently graded advisory card
/// even though the producer discloses only a heuristic test link.
#[test]
fn heuristic_preview_preserves_its_raw_test_and_repair_card_grade() -> Result<(), String> {
    for (fixture, probe_id, name, kind, strength) in [
        (
            "typescript_awaited_promise_reject",
            "probe:src_profile.ts:typescript_preview:9da98202",
            "rejects missing profile id broadly",
            "broad_error",
            "weak",
        ),
        (
            "typescript_probe_facts",
            "probe:src_notify.js:typescript_preview:2f0e6fff",
            "notify smoke",
            "smoke_only",
            "smoke",
        ),
        (
            "typescript_mocked_module_limit",
            "probe:src_discount.ts:typescript_preview:2396aec1",
            "at threshold discounts",
            "exact_value",
            "strong",
        ),
        (
            "javascript_js_preview",
            "probe:src_discount.js:typescript_preview:8152c8c0",
            "at threshold discounts",
            "exact_value",
            "strong",
        ),
        (
            "typescript_static_limit_taxonomy",
            "probe:src_dispatch.ts:typescript_preview:dd0f8848",
            "dispatch smoke",
            "smoke_only",
            "smoke",
        ),
    ] {
        let report: serde_json::Value =
            serde_json::from_str(&run_fixture(fixture, "check", &["--format", "json"])?)
                .map_err(|error| error.to_string())?;
        let finding = report["findings"]
            .as_array()
            .and_then(|fs| fs.iter().find(|f| f["id"] == probe_id))
            .ok_or_else(|| format!("missing {probe_id} in {fixture}"))?;
        assert_eq!(finding["oracle_kind"], "unknown", "{fixture}");
        assert_eq!(finding["oracle_strength"], "none", "{fixture}");
        let row = finding["related_tests"]
            .as_array()
            .and_then(|rs| rs.iter().find(|r| r["name"] == name))
            .ok_or_else(|| format!("missing {name} in {fixture}"))?;
        for graded in [row, &finding["typescript_preview_card"]] {
            assert_eq!(graded["oracle_kind"], kind, "{fixture}");
            assert_eq!(graded["oracle_strength"], strength, "{fixture}");
        }
    }
    Ok(())
}
