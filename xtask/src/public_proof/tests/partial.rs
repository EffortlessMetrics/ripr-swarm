use super::*;
use serde_json::json;

fn edit_metric(
    receipts: &mut Receipts,
    id: &str,
    edit: impl FnOnce(&mut Value),
) -> Result<(), String> {
    let metric = receipts
        .dx
        .get_mut("metrics")
        .and_then(Value::as_array_mut)
        .ok_or("missing DX metrics")?
        .iter_mut()
        .find(|metric| metric["id"] == id)
        .ok_or_else(|| format!("missing metric {id}"))?;
    edit(metric);
    Ok(())
}

fn rendered_scoreboard(bar: Bar) -> String {
    let mut page = Page(String::new());
    scoreboard(&mut page, &[bar]);
    page.0
}

#[test]
fn qualifying_partial_bar_keeps_its_number_but_not_a_fully_met_count() -> Result<(), String> {
    let mut receipts = load(&workspace_root())?;
    edit_metric(&mut receipts, "paste.unsafe_commands", |metric| {
        assert_eq!(metric["value"], json!(0.0));
        assert_eq!(metric["status"], "meets_target");
        metric["partial"] = json!(true);
    })?;
    let bar = bar_named(&receipts, "paste.unsafe_commands")?;
    assert_eq!(bar.value, Some(0.0));
    assert!(
        bar.status == Status::Meets,
        "preserve the producer's target judgment"
    );
    assert_eq!(bar_row(&bar)[4], "partially measured");
    let page = rendered_scoreboard(bar);
    assert!(
        page.contains("ripr meets 0, partially measured 1, is below the bar on 0"),
        "{page}"
    );
    assert!(
        page.contains("| Board | Meets | Partial | Below | Not measured | Failed |"),
        "{page}"
    );
    assert!(
        page.contains("| Pasted commands | 0 | 1 | 0 | 0 | 0 |"),
        "{page}"
    );
    Ok(())
}

#[test]
fn partial_below_and_failed_bars_keep_their_stronger_status() -> Result<(), String> {
    for (producer_status, label, tally) in [
        (
            "below_target",
            "below the bar (partially measured)",
            "| Pasted commands | 0 | 1 | 0 | 0 |",
        ),
        (
            "failed",
            "instrument failed (partially measured)",
            "| Pasted commands | 0 | 0 | 0 | 1 |",
        ),
    ] {
        let mut receipts = load(&workspace_root())?;
        edit_metric(&mut receipts, "paste.unsafe_commands", |metric| {
            metric["value"] = json!(0.0);
            metric["status"] = json!(producer_status);
            metric["partial"] = json!(true);
        })?;
        let bar = bar_named(&receipts, "paste.unsafe_commands")?;
        assert_eq!(bar.value, Some(0.0));
        let row = bar_row(&bar);
        assert_eq!(row[4], label);
        if producer_status == "below_target" {
            assert!(bar.status == Status::Below);
            assert!(row[2].starts_with("**"), "retain below-target emphasis");
        } else {
            assert!(bar.status == Status::Failed);
        }
        let page = rendered_scoreboard(bar);
        assert!(page.contains(tally), "{page}");
        assert!(
            !page.contains("| Partial |"),
            "do not double-count below or failed bars"
        );
    }
    Ok(())
}

#[test]
fn public_partial_coverage_comes_from_the_selected_lane_only() -> Result<(), String> {
    for (placeholder_partial, lane_partial) in [(false, true), (true, false)] {
        let mut receipts = load(&workspace_root())?;
        edit_metric(&mut receipts, "corpus.not_analyzed", |metric| {
            metric["value"] = Value::Null;
            metric["status"] = json!("not_measured");
            metric["partial"] = json!(placeholder_partial);
        })?;
        receipts.corpus_lane = Some(LaneReceipt {
            path: CORPUS_FULL_BASELINE.to_string(),
            value: json!({"metrics": [{
                "id": "corpus.not_analyzed", "board": "corpus", "title": "Lane completeness",
                "unit": "repos", "target": 0.0, "direction": "lower_is_better",
                "value": 0.0, "status": "meets_target", "partial": lane_partial,
            }]}),
        });
        let bar = bar_named(&receipts, "corpus.not_analyzed")?;
        assert_eq!(bar.value, Some(0.0));
        assert_eq!(
            bar.basis, CORPUS_FULL_BASELINE,
            "the lane must actually supply the bar"
        );
        assert_eq!(
            bar_row(&bar)[4],
            if lane_partial {
                "partially measured"
            } else {
                "meets the bar"
            }
        );
        let page = rendered_scoreboard(bar);
        if lane_partial {
            assert!(page.contains("| corpus | 0 | 1 | 0 | 0 | 0 |"), "{page}");
        } else {
            assert!(page.contains("| corpus | 1 | 0 | 0 | 0 |"), "{page}");
            assert!(!page.contains("partially measured"), "{page}");
        }
    }
    Ok(())
}

#[test]
fn unused_partial_source_cannot_downgrade_an_independent_derivation() -> Result<(), String> {
    let mut receipts = load(&workspace_root())?;
    edit_metric(&mut receipts, "trust.gap_claim_agreement", |metric| {
        // Keep this unused source numeric so removing the derivation guard
        // would incorrectly carry its partial coverage into the chosen bar.
        metric["value"] = json!(0.25);
        metric["status"] = json!("below_target");
        metric["partial"] = json!(true);
        metric["source"] = json!("unused DX source");
    })?;
    let bar = bar_named(&receipts, "trust.gap_claim_agreement")?;
    assert_eq!(
        bar.value,
        Some(1.0),
        "the retained mutation receipt supplies this rate"
    );
    assert!(bar.basis.contains("scored;"), "{}", bar.basis);
    assert_eq!(bar_row(&bar)[4], "meets the bar");
    let page = rendered_scoreboard(bar);
    assert!(page.contains("| Trust | 1 | 0 | 0 | 0 |"), "{page}");
    assert!(!page.contains("partially measured"), "{page}");
    Ok(())
}

#[test]
fn legacy_and_unmeasured_receipts_keep_their_existing_labels() -> Result<(), String> {
    let mut receipts = load(&workspace_root())?;
    edit_metric(&mut receipts, "paste.unsafe_commands", |metric| {
        if let Some(object) = metric.as_object_mut() {
            let _ = object.remove("partial");
        }
        assert!(metric.get("partial").is_none());
    })?;
    let bar = bar_named(&receipts, "paste.unsafe_commands")?;
    assert_eq!(bar_row(&bar)[4], "meets the bar");
    let page = rendered_scoreboard(bar);
    assert!(
        page.contains("| Pasted commands | 1 | 0 | 0 | 0 |"),
        "{page}"
    );
    assert!(!page.contains("| Partial |"), "{page}");

    edit_metric(&mut receipts, "paste.unsafe_commands", |metric| {
        metric["value"] = Value::Null;
        metric["status"] = json!("not_measured");
        metric["partial"] = json!(true);
    })?;
    let bar = bar_named(&receipts, "paste.unsafe_commands")?;
    assert_eq!(bar.value, None);
    assert_eq!(bar_row(&bar)[4], "not measured");
    let page = rendered_scoreboard(bar);
    assert!(
        page.contains("| Pasted commands | 0 | 0 | 1 | 0 |"),
        "{page}"
    );
    assert!(!page.contains("partially measured"), "{page}");
    Ok(())
}
