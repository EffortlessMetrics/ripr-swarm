use super::*;
use crate::analysis::classify_seam;
use crate::analysis::repair_route::{RepairPacketIneligibility, repair_packet_eligibility};
use crate::analysis::test_grip_evidence::evidence_for_seams;
use crate::app::agent_brief::{
    AgentBriefPolicy, AgentBriefResolvedWorkingSet, select_agent_brief_seams,
};
use crate::config::RiprConfig;
use crate::domain::StaticLimitKind;
use crate::output::evidence_record::{
    evidence_record_for, evidence_record_json_value, is_static_limited, static_limitations_for,
};
use crate::output::pilot::ranking::{
    actionable_in_change, actionable_in_owner, actionable_total, routable_total,
};

fn panic_predicate(extra_test: &str) -> Result<ClassifiedSeam, String> {
    let path = PathBuf::from("src/lib.rs");
    let source = format!(
        "pub fn div(a: i32, b: i32) -> i32 {{
    if b == 1 {{
        panic!(\"zero\");
    }}
    a / b
}}
#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    #[should_panic]
    fn rejects_zero() {{ div(1, 0); }}
    {extra_test}
}}
"
    );
    let facts = RaRustSyntaxAdapter.summarize_file(&path, &source)?;
    assert!(!facts.used_lexical_fallback);
    let mut index = RustIndex::default();
    index.extend_functions(facts.functions.iter().cloned());
    index.extend_tests(facts.tests.iter().cloned());
    index.insert_file_only(path.clone(), facts);
    let seams = inventory_seams_from_index(&[path], &index);
    let mut predicates = seams
        .into_iter()
        .filter(|seam| seam.kind() == SeamKind::PredicateBoundary && seam.display_line() == 2);
    let seam = predicates.next().ok_or("missing div predicate seam")?;
    assert!(
        predicates.next().is_none(),
        "expected exactly one div predicate"
    );
    let mut evidence = evidence_for_seams(std::slice::from_ref(&seam), &index);
    let evidence = evidence.pop().ok_or("missing div predicate evidence")?;
    let class = classify_seam(&seam, &evidence);
    Ok(ClassifiedSeam {
        seam,
        evidence,
        class,
    })
}

/// Isolate the consumer decision with concrete, same-class evidence. The marker
/// summary comes from the real producer; the CLI snapshot-policy case separately
/// proves that the full analyzer can produce concrete stages with this limit.
#[test]
fn named_panic_limit_with_concrete_stages_withholds_pilot_and_brief() -> Result<(), String> {
    use StageState::{No, Yes};

    let mut ordinary = panic_predicate("")?;
    assert!(
        ordinary
            .evidence
            .names_static_limit(StaticLimitKind::RustPanicOutcomeUnresolved)
    );
    let named_summary = ordinary.evidence.discriminate.summary.clone();
    ordinary.evidence.reach = stage(Yes);
    ordinary.evidence.activate = stage(Yes);
    ordinary.evidence.propagate = stage(Yes);
    ordinary.evidence.observe = stage(No);
    ordinary.evidence.discriminate = stage(No);
    ordinary.class = classify_seam(&ordinary.seam, &ordinary.evidence);
    let mut limited = ordinary.clone();
    limited.evidence.discriminate.summary = named_summary;

    let config = RiprConfig::default();
    let working_set = AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/lib.rs")]);
    for (entry, withheld) in [(ordinary, false), (limited, true)] {
        assert_eq!(entry.class, SeamGripClass::ReachableUnrevealed);
        assert_eq!(classify_seam(&entry.seam, &entry.evidence), entry.class);
        assert_eq!(is_static_limited(&entry), withheld);
        let limits = static_limitations_for(&entry);
        if withheld {
            assert_eq!(limits.len(), 1);
            assert_eq!(
                limits[0].category,
                StaticLimitKind::RustPanicOutcomeUnresolved.as_str()
            );
            assert_eq!(limits[0].state, "no");
            assert_eq!(limits[0].reason, entry.evidence.discriminate.summary);
        } else {
            // Missing repair-target provenance is independent of this marker.
            let category = StaticLimitKind::RustPanicOutcomeUnresolved.as_str();
            assert!(limits.iter().all(|limit| limit.category != category));
        }
        let entries = [entry];
        assert_eq!(
            top_actionable_seams(&entries, 5, None).len(),
            usize::from(!withheld)
        );
        let brief = select_agent_brief_seams(
            &entries,
            &working_set,
            3,
            AgentBriefPolicy::from_config(&config),
        );
        assert_eq!(brief.top_seams.len(), usize::from(!withheld));
        assert_eq!(brief.omitted_by_policy, usize::from(withheld));
    }
    Ok(())
}

#[test]
fn named_panic_limit_withholds_pilot_and_brief_without_reclassifying() -> Result<(), String> {
    let entry = panic_predicate("")?;
    assert_eq!(entry.class, SeamGripClass::ReachableUnrevealed);
    assert!(
        entry
            .evidence
            .names_static_limit(StaticLimitKind::RustPanicOutcomeUnresolved)
    );
    // This real parser path also has unresolved propagation. The controlled
    // consumer test above and the CLI snapshot-policy case cover concrete stages;
    // here the named reason must survive alongside the other limitation.
    assert_eq!(entry.evidence.propagate.state, StageState::Unknown);
    assert_eq!(entry.evidence.discriminate.state, StageState::No);
    assert!(!entry.evidence.missing_discriminators.is_empty());
    assert_eq!(
        repair_packet_eligibility(&entry).ineligibility,
        Some(RepairPacketIneligibility::NamedStaticLimitation)
    );
    let original = serde_json::to_value(&entry).map_err(|error| error.to_string())?;
    let entries = [entry];
    let change = changed("src/lib.rs", 2);
    assert!(change.touches(&entries[0]));
    assert!(top_actionable_seams(&entries, 5, Some(&change)).is_empty());
    assert_eq!(actionable_total(&entries), 0);
    assert_eq!(routable_total(&entries), 0);
    assert_eq!(actionable_in_change(&entries, &change), 0);
    assert_eq!(actionable_in_owner(&entries, &entries[0]), 0);
    assert_eq!(withheld_static_limitations(&entries), 1);

    let artifacts = pilot_artifacts();
    let context = pilot_context(&artifacts);
    let pilot: serde_json::Value =
        serde_json::from_str(&render_pilot_summary_json(&entries, context))
            .map_err(|error| error.to_string())?;
    assert_eq!(pilot["actionable_seams_total"], 0);
    assert_eq!(pilot["withheld_static_limitations_total"], 1);
    assert_eq!(pilot["top_actionable_seams"], serde_json::json!([]));
    for command in [
        "repair_command",
        "after_snapshot_command",
        "outcome_command",
    ] {
        assert!(pilot["next"][command].is_null(), "{command}: {pilot}");
    }
    let markdown = render_pilot_summary_md(&entries, context);
    assert!(
        markdown.contains("None ranked: 1 spot were withheld"),
        "{markdown}"
    );
    assert!(
        markdown.contains("this is not a clean result"),
        "{markdown}"
    );
    let terminal = render_pilot_terminal(&entries, context);
    assert!(
        terminal.contains("none ranked: 1 spot were withheld"),
        "{terminal}"
    );

    let config = RiprConfig::default();
    for working_set in [
        AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/lib.rs")]),
        AgentBriefResolvedWorkingSet::seam_id(entries[0].seam.id().as_str()),
    ] {
        let brief = select_agent_brief_seams(
            &entries,
            &working_set,
            3,
            AgentBriefPolicy::from_config(&config),
        );
        assert!(brief.top_seams.is_empty());
        assert!(
            brief.warnings.iter().any(|warning| {
                warning.contains("cannot tell whether the changed code causes the expected panic")
            }),
            "{:?}",
            brief.warnings
        );
        assert_eq!(
            brief.omitted_by_policy,
            usize::from(working_set.seam_id.is_none())
        );
    }

    let record = evidence_record_json_value(&evidence_record_for(&entries[0], None));
    assert_eq!(record["grip_class"], "reachable_unrevealed");
    assert_eq!(record["actionability"]["class"], "static_limitation");
    assert_eq!(
        record["canonical_item"]["canonical_item_kind"],
        "limitation"
    );
    let limits = record["static_limitations"]
        .as_array()
        .ok_or("static_limitations must be an array")?;
    let named: Vec<_> = limits
        .iter()
        .filter(|limit| limit["category"] == "rust_panic_outcome_unresolved")
        .collect();
    assert_eq!(named.len(), 1, "{record}");
    assert_eq!(named[0]["stage"], "discriminate");
    assert_eq!(named[0]["state"], "no");
    assert_eq!(named[0]["reason"], entries[0].evidence.discriminate.summary);
    assert_eq!(
        named[0]["repair_route"],
        "analysis/oracle-semantics-audit-fixes"
    );
    assert_eq!(
        record["canonical_item"]["static_limitations"],
        record["static_limitations"]
    );
    assert_eq!(
        record["recommendation"]["action"],
        "inspect_static_limitation"
    );
    for command in ["repair_command", "verify_command", "receipt_command"] {
        assert!(
            record["canonical_item"][command].is_null(),
            "{command}: {record}"
        );
    }
    assert_eq!(
        serde_json::to_value(&entries[0]).map_err(|error| error.to_string())?,
        original,
        "selection and projection preserve the producer evidence"
    );
    Ok(())
}

#[test]
fn weak_asserting_should_panic_control_stays_actionable() -> Result<(), String> {
    let entry = panic_predicate("#[test] fn positive() { assert!(div(4, 2) > 0); }")?;
    assert_eq!(entry.class, SeamGripClass::WeaklyGripped);
    assert!(
        !entry
            .evidence
            .names_static_limit(StaticLimitKind::RustPanicOutcomeUnresolved)
    );
    let entries = [entry];
    let ranked = top_actionable_seams(&entries, 5, None);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].seam.id(), entries[0].seam.id());
    assert_eq!(actionable_total(&entries), 1);
    assert_eq!(withheld_static_limitations(&entries), 0);
    let config = RiprConfig::default();
    let working_set = AgentBriefResolvedWorkingSet::files(vec![PathBuf::from("src/lib.rs")]);
    let brief = select_agent_brief_seams(
        &entries,
        &working_set,
        3,
        AgentBriefPolicy::from_config(&config),
    );
    assert_eq!(brief.top_seams.len(), 1);
    assert_eq!(brief.top_seams[0].seam.seam.id(), entries[0].seam.id());
    assert_eq!(brief.omitted_by_policy, 0);
    Ok(())
}

#[test]
fn source_mentions_do_not_manufacture_a_panic_limitation() -> Result<(), String> {
    let limit = StaticLimitKind::RustPanicOutcomeUnresolved;
    let mut ordinary = panic_predicate("#[test] fn ok() { assert!(div(4, 2) > 0); }")?;
    assert_eq!(ordinary.class, SeamGripClass::WeaklyGripped);
    let config = RiprConfig::default();
    let working_set = AgentBriefResolvedWorkingSet::seam_id(ordinary.seam.id().as_str());
    for summary in [
        limit.as_str().to_string(),
        format!("{}: source text in another stage", limit.as_str()),
    ] {
        for stage in [
            &mut ordinary.evidence.reach,
            &mut ordinary.evidence.activate,
            &mut ordinary.evidence.propagate,
            &mut ordinary.evidence.observe,
        ] {
            stage.summary.clone_from(&summary);
        }
        for discrimination in [
            "a weak assertion".to_string(),
            format!("source identifier `{}`", limit.as_str()),
            format!("source text mentions {}: text", limit.as_str()),
        ] {
            ordinary.evidence.discriminate.summary = discrimination;
            assert!(!ordinary.evidence.names_static_limit(limit));
            assert!(!is_static_limited(&ordinary));
            assert!(
                static_limitations_for(&ordinary)
                    .iter()
                    .all(|row| row.category != limit.as_str())
            );
            assert_ne!(
                repair_packet_eligibility(&ordinary).ineligibility,
                Some(RepairPacketIneligibility::NamedStaticLimitation)
            );
            let entries = [ordinary.clone()];
            assert_eq!(top_actionable_seams(&entries, 5, None).len(), 1);
            let brief = select_agent_brief_seams(
                &entries,
                &working_set,
                3,
                AgentBriefPolicy::from_config(&config),
            );
            assert_eq!(brief.top_seams.len(), 1);
            assert!(
                brief
                    .warnings
                    .iter()
                    .all(|warning| !warning.contains("expected panic"))
            );
        }
    }
    // An independently unknown stage still has a limitation, but an incidental
    // source mention cannot give that row the panic producer's category.
    ordinary.evidence.reach.state = StageState::Unknown;
    assert!(is_static_limited(&ordinary));
    assert!(
        static_limitations_for(&ordinary)
            .iter()
            .all(|row| row.category != limit.as_str())
    );
    Ok(())
}
