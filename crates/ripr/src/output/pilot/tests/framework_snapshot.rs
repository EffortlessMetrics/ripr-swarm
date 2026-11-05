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

const LIMIT: StaticLimitKind = StaticLimitKind::RustFrameworkOracleUnresolved;

fn field_snapshot() -> Result<ClassifiedSeam, String> {
    let path = PathBuf::from("src/lib.rs");
    let source = "pub struct Record { pub items: usize }
pub fn make_record(items: &[u32]) -> Record {
    Record {
        items: items.iter().count(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn record_snapshot() {
        let record = make_record(&[1, 2]);
        let settings = insta::Settings::clone_current();
        let _guard = settings.bind_to_scope();
        insta::assert_json_snapshot!(record);
    }
}
";
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    assert!(!facts.used_lexical_fallback);
    let mut index = RustIndex::default();
    index.extend_functions(facts.functions.iter().cloned());
    index.extend_tests(facts.tests.iter().cloned());
    index.insert_file_only(path.clone(), facts);
    let fields: Vec<_> = inventory_seams_from_index(&[path], &index)
        .into_iter()
        .filter(|seam| seam.kind() == SeamKind::FieldConstruction && seam.display_line() == 4)
        .collect();
    assert_eq!(fields.len(), 1, "one intended field seam");
    let seam = fields.into_iter().next().ok_or("missing items seam")?;
    let mut evidence = evidence_for_seams(std::slice::from_ref(&seam), &index);
    let evidence = evidence.pop().ok_or("missing items evidence")?;
    Ok(ClassifiedSeam {
        class: classify_seam(&seam, &evidence),
        seam,
        evidence,
    })
}

#[test]
fn a_named_field_snapshot_limit_alone_withholds_pilot_brief_and_repair() -> Result<(), String> {
    let limited = field_snapshot()?;
    assert_eq!(limited.class, SeamGripClass::WeaklyGripped);
    assert!(limited.evidence.names_static_limit(LIMIT));
    for (stage, state) in [
        (&limited.evidence.reach, StageState::Yes),
        (&limited.evidence.activate, StageState::Yes),
        (&limited.evidence.propagate, StageState::Yes),
        (&limited.evidence.observe, StageState::Yes),
        (&limited.evidence.discriminate, StageState::Weak),
    ] {
        assert_eq!(stage.state, state, "{stage:?}");
    }
    let mut ordinary = limited.clone();
    ordinary.evidence.discriminate.summary = ordinary
        .evidence
        .discriminate
        .summary
        .split_once("; Original discrimination: ")
        .map(|(_, original)| original)
        .ok_or("missing original discrimination summary")?
        .to_string();
    assert!(!ordinary.evidence.names_static_limit(LIMIT));
    assert_eq!(ordinary.class, limited.class);
    assert_eq!(
        classify_seam(&ordinary.seam, &ordinary.evidence),
        limited.class
    );

    let config = RiprConfig::default();
    for (entry, withheld) in [(ordinary, false), (limited, true)] {
        let original = serde_json::to_value(&entry).map_err(|error| error.to_string())?;
        assert_eq!(is_static_limited(&entry), withheld);
        assert_eq!(
            repair_packet_eligibility(&entry).ineligibility
                == Some(RepairPacketIneligibility::NamedStaticLimitation),
            withheld
        );
        let limits = static_limitations_for(&entry);
        let named: Vec<_> = limits
            .iter()
            .filter(|limit| limit.category == LIMIT.as_str())
            .collect();
        assert_eq!(named.len(), usize::from(withheld));
        if withheld {
            assert_eq!(named[0].state, "weak");
            assert_eq!(named[0].reason, entry.evidence.discriminate.summary);
            assert_eq!(named[0].repair_route, "analysis/oracle-semantics-audit-fixes");
        }
        let entries = [entry];
        assert_eq!(
            top_actionable_seams(&entries, 5, None).len(),
            usize::from(!withheld)
        );
        assert_eq!(withheld_static_limitations(&entries), usize::from(withheld));
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
            assert_eq!(brief.top_seams.len(), usize::from(!withheld));
            if withheld {
                assert!(
                    brief.warnings.iter().any(|warning| {
                        warning.contains(
                            "cannot tell whether the framework oracle notices the changed field",
                        )
                    }),
                    "{:?}",
                    brief.warnings
                );
            }
        }
        let record = evidence_record_json_value(&evidence_record_for(&entries[0], None));
        assert_eq!(record["grip_class"], "weakly_gripped");
        if withheld {
            assert_eq!(record["actionability"]["class"], "static_limitation");
            assert_eq!(
                record["actionability"]["reason"],
                "whether the framework oracle notices the changed field is unresolved"
            );
            assert_eq!(record["canonical_item"]["canonical_item_kind"], "limitation");
            assert_eq!(record["recommendation"]["action"], "inspect_static_limitation");
            for command in ["repair_command", "verify_command", "receipt_command"] {
                assert!(record["canonical_item"][command].is_null(), "{record}");
            }
        }
        assert_eq!(
            serde_json::to_value(&entries[0]).map_err(|error| error.to_string())?,
            original,
            "consumers preserve inspection evidence"
        );
    }
    Ok(())
}

#[test]
fn source_mentions_in_other_stages_do_not_create_framework_limitations() -> Result<(), String> {
    let mut ordinary = field_snapshot()?;
    ordinary.evidence.discriminate.summary = "a weak assertion".to_string();
    for summary in [
        LIMIT.as_str().to_string(),
        format!("{}: source text in another stage", LIMIT.as_str()),
    ] {
        for stage in [
            &mut ordinary.evidence.reach,
            &mut ordinary.evidence.activate,
            &mut ordinary.evidence.propagate,
            &mut ordinary.evidence.observe,
        ] {
            stage.summary.clone_from(&summary);
        }
        assert!(!ordinary.evidence.names_static_limit(LIMIT));
        assert!(!is_static_limited(&ordinary));
        assert!(
            static_limitations_for(&ordinary)
                .iter()
                .all(|limit| limit.category != LIMIT.as_str())
        );
        assert_ne!(
            repair_packet_eligibility(&ordinary).ineligibility,
            Some(RepairPacketIneligibility::NamedStaticLimitation)
        );
        assert_eq!(top_actionable_seams(&[ordinary.clone()], 5, None).len(), 1);
    }
    ordinary.evidence.discriminate.summary = format!("source identifier `{}`", LIMIT.as_str());
    assert!(!ordinary.evidence.names_static_limit(LIMIT));
    assert!(!is_static_limited(&ordinary));
    assert!(
        static_limitations_for(&ordinary)
            .iter()
            .all(|limit| limit.category != LIMIT.as_str())
    );
    Ok(())
}
