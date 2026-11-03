//! #5341 control 10: public projections read shared `Arc<RelatedTestGrip>` backing.

#[cfg(test)]
mod tests {
    use crate::analysis::ClassifiedSeam;
    use crate::analysis::seams::{
        ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind,
    };
    use crate::analysis::test_grip_evidence::{
        RelatedTestGrip, RelationConfidence, RelationReason, TestGripEvidence, TestTargetEvidence,
    };
    use crate::config::RiprConfig;
    use crate::domain::{Confidence, OracleKind, OracleStrength, StageEvidence, StageState};
    use crate::output::agent_seam_packets::render_agent_seam_packet_json;
    use crate::output::evidence_record::evidence_record_for;
    use crate::output::repo_exposure::render_repo_exposure_json;
    use crate::output::sarif::render_repo_seams_sarif;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    fn sample_grip(test_name: &str) -> RelatedTestGrip {
        RelatedTestGrip {
            test_name: test_name.to_string(),
            file: PathBuf::from("tests/pricing_tests.rs"),
            line: 12,
            test_target: Some(TestTargetEvidence::fixture(
                test_name,
                Path::new("tests/pricing_tests.rs"),
                12,
            )),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            evidence_summary: "exact returned value assertion".to_string(),
            relation_reason: RelationReason::DirectOwnerCall,
            relation_confidence: RelationConfidence::High,
        }
    }

    fn classified_seam(line: usize, related: Arc<RelatedTestGrip>) -> ClassifiedSeam {
        let seam = RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            line * 10,
            line,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount == discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        )
        .with_owner_call(crate::analysis::seams::OwnerCallShape::Free);
        let seam_id = seam.id().clone();
        ClassifiedSeam {
            seam,
            class: SeamGripClass::WeaklyGripped,
            evidence: TestGripEvidence {
                seam_id,
                related_tests: vec![related],
                reach: stage(StageState::Yes, "owner is reached"),
                activate: stage(StageState::Yes, "activation observed"),
                propagate: stage(StageState::Yes, "return value flow"),
                observe: stage(StageState::Yes, "assertion observes output"),
                discriminate: stage(StageState::Weak, "boundary missing"),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                new_test_target: None,
                statically_contradicted_related_tests: 0,
            },
        }
    }

    fn assert_shared_backing_unchanged(
        left: &ClassifiedSeam,
        right: &ClassifiedSeam,
        expected: *const RelatedTestGrip,
    ) -> Result<(), String> {
        if !Arc::ptr_eq(
            &left.evidence.related_tests[0],
            &right.evidence.related_tests[0],
        ) {
            return Err("consumers must read the same shared backing across seams".into());
        }
        let left_ptr = Arc::as_ptr(&left.evidence.related_tests[0]);
        let right_ptr = Arc::as_ptr(&right.evidence.related_tests[0]);
        if left_ptr != expected || right_ptr != expected {
            return Err("consumer projection must not re-intern related-test records".into());
        }
        Ok(())
    }

    #[test]
    fn shared_grip_consumer_matrix_projects_without_reinterning() -> Result<(), String> {
        let shared = Arc::new(sample_grip("boundary_case"));
        let seam_a = classified_seam(1, Arc::clone(&shared));
        let seam_b = classified_seam(2, Arc::clone(&shared));
        let expected = Arc::as_ptr(&shared);
        assert_shared_backing_unchanged(&seam_a, &seam_b, expected)?;

        let _record = evidence_record_for(&seam_a, None);
        assert_shared_backing_unchanged(&seam_a, &seam_b, expected)?;

        let packet = render_agent_seam_packet_json(&seam_a);
        if packet.is_empty() {
            return Err("agent seam packet projection must not be empty".into());
        }
        assert_shared_backing_unchanged(&seam_a, &seam_b, expected)?;

        let exposure =
            render_repo_exposure_json(&[seam_a.clone(), seam_b.clone()], None, None, None);
        if !exposure.contains("boundary_case") {
            return Err("repo exposure JSON must name the shared related test".into());
        }
        assert_shared_backing_unchanged(&seam_a, &seam_b, expected)?;

        let sarif = render_repo_seams_sarif(
            &[seam_a.clone(), seam_b.clone()],
            None,
            &RiprConfig::default(),
        );
        if sarif.is_empty() {
            return Err("SARIF projection must not be empty".into());
        }
        assert_shared_backing_unchanged(&seam_a, &seam_b, expected)?;

        Ok(())
    }
}
