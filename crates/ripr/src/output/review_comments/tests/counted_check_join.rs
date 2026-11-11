use super::*;

fn assert_unjoinable(value: &Value) -> Result<usize, String> {
    assert_eq!(value["counted_check_join"]["schema_version"], 1);
    assert_eq!(value["counted_check_join"]["status"], "unjoinable");
    assert_eq!(value["counted_check_join"]["finding_ids"], json!([]));
    assert_eq!(value["counted_check_join"]["analysis_binding"], Value::Null);
    let mut count = 0;
    for key in ["comments", "summary_only", "suppressed"] {
        for card in value[key].as_array().ok_or("missing card array")? {
            assert_eq!(
                card["counted_check_join"],
                json!({
                    "schema_version": 1,
                    "status": "unjoinable",
                    "finding_ids": [],
                    "analysis_binding": null,
                    "reason": "producer_mapping_unavailable",
                })
            );
            count += 1;
        }
    }
    Ok(count)
}

#[test]
fn seam_and_ledger_renderers_disclose_unjoinable_on_every_route() -> Result<(), String> {
    let seams = (1..=12).map(|i| classified(i * 10)).collect::<Vec<_>>();
    let lines = seams
        .iter()
        .map(|s| AgentBriefLine::new("src/pricing.rs", s.seam.display_line()))
        .collect();
    let value = render_value(&AgentBriefResolvedWorkingSet::base("main", lines), &seams)?;
    assert!(!value["comments"].as_array().ok_or("inline")?.is_empty());
    assert!(
        !value["summary_only"]
            .as_array()
            .ok_or("summary")?
            .is_empty()
    );
    assert!(
        !value["suppressed"]
            .as_array()
            .ok_or("suppressed")?
            .is_empty()
    );
    assert_eq!(assert_unjoinable(&value)?, 12);

    let records = crate::output::gap_decision_ledger::parse_gap_records_json(
        gap_record_review_comments_fixture(),
    )?;
    let ledger: Value = serde_json::from_str(&render_gap_record_review_comments_json(
        Path::new("."),
        "main",
        "HEAD",
        &Mode::Draft,
        "ledger.json",
        &records,
    )?)
    .map_err(|e| e.to_string())?;
    assert!(assert_unjoinable(&ledger)? > 0);

    let limited = [cross_language_classified_with_external_observer()];
    let working_set =
        AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("test/js/web/fetch/blob.test.ts")]);
    let value = render_value(&working_set, &limited)?;
    assert!(assert_unjoinable(&value)? > 0);
    for card in all_cards(&value) {
        assert_eq!(card["gap_state"], "static_limitation");
        assert!(card["suggested_test"].is_object());
    }
    Ok(())
}

/// Replays retained field-preserving projections, not a new hosted analysis.
#[test]
fn hosted_16619_projection_never_assigns_three_probes_to_ten_cards() -> Result<(), String> {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/counted-check-join/hosted-16619-counted-raw-gaps.json"
    ))
    .map_err(|e| e.to_string())?;
    let mut guidance: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/counted-check-join/hosted-16619-static-limitations.json"
    ))
    .map_err(|e| e.to_string())?;
    let findings = raw["findings"].as_array().ok_or("raw findings")?;
    assert_eq!(findings.len(), 3);
    let ids = findings
        .iter()
        .map(|f| f["id"].as_str().ok_or("finding id"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    assert_eq!(ids.len(), 3);
    assert_eq!(raw["summary"]["no_static_path"], 3);
    assert_eq!(
        guidance["comments"].as_array().ok_or("inline cards")?.len(),
        3
    );
    assert_eq!(
        guidance["summary_only"]
            .as_array()
            .ok_or("summary cards")?
            .len(),
        7
    );
    let original = guidance.clone();
    assert_eq!(
        all_cards(&guidance)
            .iter()
            .filter(|c| c["gap_state"] == "static_limitation")
            .count(),
        9
    );
    assert_eq!(
        all_cards(&guidance)
            .iter()
            .filter(|c| c["gap_state"] == "actionable")
            .count(),
        1
    );
    for card in all_cards(&guidance) {
        assert!(!ids.contains(card["seam_id"].as_str().ok_or("seam id")?));
    }
    disclose_counted_check_join(&mut guidance);
    assert_eq!(assert_unjoinable(&guidance)?, 10);
    // Disclosing missing provenance must not normalize away actionability,
    // generic test suggestions, placement, identities, or the captured subject.
    guidance
        .as_object_mut()
        .ok_or("document")?
        .remove("counted_check_join");
    for key in ["comments", "summary_only", "suppressed"] {
        for card in guidance[key].as_array_mut().ok_or("cards")? {
            card.as_object_mut()
                .ok_or("card")?
                .remove("counted_check_join");
        }
    }
    assert_eq!(guidance, original);
    Ok(())
}

#[test]
fn matching_or_conflicting_labels_cannot_upgrade_the_unjoinable_contract() -> Result<(), String> {
    for status in ["advisory", "incomplete", "error"] {
        let mut document = json!({
            "status": status, "head": "same-head", "base": "same-base",
            "comments": [{"seam_id": "probe:one", "gap_state": "static_limitation"}],
            "summary_only": [{"seam_id": "probe:one", "gap_state": "actionable"}],
            "suppressed": [{"gap_id": "probe:one", "reason": "suppression"}],
        });
        // Deliberately adversarial labels only: this does not claim to validate
        // stale inputs or duplicate IDs against an actual check invocation.
        disclose_counted_check_join(&mut document);
        assert_eq!(assert_unjoinable(&document)?, 3);
    }
    Ok(())
}
