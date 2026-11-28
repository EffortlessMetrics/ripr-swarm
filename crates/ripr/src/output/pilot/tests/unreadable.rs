use super::*;

fn unreadable_limitations() -> Result<Vec<AnalysisLimitation>, String> {
    Ok(vec![
        AnalysisLimitation::new(
            AnalysisLimitationKind::LanguageScopeUnsupported,
            AnalysisStage::LanguageAdapter,
            AnalysisRecovery::new(
                AnalysisRecoveryKind::Retry,
                "Restore read access, then re-run.",
            )?,
        )
        .with_path("tests/locked")?
        .with_affected_items(1)?
        .with_detail("unchanged Rust path could not be read")?,
    ])
}

/// #13624 review: a seam limit sets `before_snapshot_command`, but an
/// unreadable skip withholds the whole snapshot loop, so no half of it leaks.
#[test]
fn unreadable_skip_withholds_the_before_snapshot_command_too() -> Result<(), String> {
    let artifacts = pilot_artifacts();
    let entries = [route_ready_entry(&["tests/pricing.rs"])];
    let limit = crate::analysis::SeamLimitInfo {
        analyzed: 3,
        total: 9,
        source: crate::analysis::SeamLimitSource::Default,
    };
    let mut context = pilot_context(&artifacts);
    context.seam_limit = Some(&limit);
    // Control: the cut alone emits the before snapshot command.
    let control: serde_json::Value =
        serde_json::from_str(&render_pilot_summary_json(&entries, context))
            .map_err(|error| error.to_string())?;
    assert!(
        control["next"]["before_snapshot_command"].is_string(),
        "{control}"
    );
    let limitations = unreadable_limitations()?;
    context.inventory_evidence_limitations = &limitations;
    let json = render_pilot_summary_json(&entries, context);
    let summary: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    assert!(
        summary["next"]["after_snapshot_command"].is_null(),
        "{json}"
    );
    assert!(
        summary["next"].get("before_snapshot_command").is_none(),
        "{json}"
    );
    Ok(())
}

/// #13624 review: with the ranking emptied by an unreadable skip, the
/// current-change reason names the unreadable evidence, not "already gripped".
#[test]
fn unreadable_skip_explains_an_unranked_change_as_unreadable_evidence() -> Result<(), String> {
    let artifacts = pilot_artifacts();
    let entries = [classified_with(
        SeamGripClass::WeaklyGripped,
        "src/other.rs",
        3,
        Vec::new(),
        Vec::new(),
    )];
    let change = changed("src/other.rs", 3).with_seams_counted(&entries, None);
    let limitations = unreadable_limitations()?;
    let mut context = pilot_context(&artifacts);
    context.current_change = Some(&change);
    context.inventory_evidence_limitations = &limitations;
    let terminal = render_pilot_terminal(&entries, context);
    let md = render_pilot_summary_md(&entries, context);
    for (name, text) in [("terminal", &terminal), ("markdown", &md)] {
        assert!(
            text.contains("Pilot ranks no gap for this change because 1 unchanged Rust path(s)"),
            "{name}\n{text}"
        );
        assert!(!text.contains("already gripped"), "{name}\n{text}");
    }
    Ok(())
}
