use super::{DiagnosticWitness, Finding, MissingDiscriminatorFact, RelatedTest, ValueFact};

#[derive(Clone, Debug, PartialEq)]
pub struct ContextPacket {
    pub version: &'static str,
    pub tool: &'static str,
    pub canonical_gap_id: Option<String>,
    pub probe: ContextPacketProbe,
    /// Which revision owns the packet finding's source (#3281): agent
    /// consumers must not treat `base_deleted` / `moved_or_renamed` /
    /// `unresolved_subject` packets as candidate edit targets.
    pub source_currentness: &'static str,
    /// The finding's exposure classification, the same `ExposureClass::as_str`
    /// value `ripr check --json` findings carry (#8743). The five `ripr`
    /// stage states below are documented as underdetermined — a `yes`
    /// discriminate grade does not mean a discriminator exists — so the
    /// packet states the verdict outright instead of making the consumer
    /// derive it.
    pub classification: &'static str,
    /// The finding's configured severity, the same `ConfigSeverity::as_str`
    /// value `ripr check --json` findings carry. `from_finding` seeds the
    /// class profile default; renderers holding a `RiprConfig` re-resolve it
    /// through `SeverityConfig::for_exposure` so a user override applies here
    /// exactly as it does on the check surface.
    pub severity: &'static str,
    /// The finding's confidence score (`0.0..=1.0`), wire-identical to the
    /// `confidence` float on the check finding (#8743).
    pub confidence: f32,
    pub ripr: ContextPacketRipr,
    /// Matched related-test rows before the packet's display window, using
    /// the finding's conservative retained-count fallback for legacy data.
    pub related_tests_total: usize,
    pub related_tests: Vec<RelatedTest>,
    pub observed_values: Vec<ValueFact>,
    pub missing_discriminators: Vec<MissingDiscriminatorFact>,
    pub witness: Option<DiagnosticWitness>,
    pub missing: Vec<String>,
    pub stop_reasons: Vec<String>,
    pub recommended_next_step: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextPacketProbe {
    pub id: String,
    pub family: String,
    pub delta: String,
    pub file: String,
    pub line: usize,
    /// Observed call shape of the function owning the changed line (the same
    /// producer `ripr agent stub` reads), e.g. `greet(name: &str) -> String`.
    /// `None` when the finding's recorded source does not own the analyzed
    /// root's checkout (non-`candidate_current`), when the source is missing
    /// or unparsed, when no function covers the changed line, or when the
    /// shape is not honestly nameable (#7796). Observed data only — no
    /// assertion value is synthesized.
    pub owner_signature: Option<String>,
    pub changed_expression: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextPacketRipr {
    pub reach: String,
    pub infect: String,
    pub propagate: String,
    pub observe: String,
    pub discriminate: String,
}

impl ContextPacket {
    /// `root` is the analyzed workspace root: the packet's location renders
    /// through the shared finding-location owner (#5996) so every surface
    /// names the finding's file with one string.
    pub fn from_finding(
        finding: &Finding,
        max_related_tests: usize,
        stop_reasons: Vec<String>,
        root: &std::path::Path,
    ) -> Self {
        Self {
            // 1.1 (#8743): `classification` / `severity` / `confidence` join
            // the packet so an agent holding the packet alone can state the
            // verdict without re-parsing `check --json`.
            version: "1.1",
            tool: "ripr",
            canonical_gap_id: finding.canonical_gap.as_ref().map(|gap| gap.id.clone()),
            probe: ContextPacketProbe {
                id: finding.probe.id.0.clone(),
                family: finding.probe.family.as_str().to_string(),
                delta: finding.probe.delta.as_str().to_string(),
                file: crate::analysis::finding_location_text(root, &finding.probe.location.file),
                line: finding.probe.location.line,
                // #7796 review: the signature is read from the analyzed
                // root's on-disk source, so it is projected only when the
                // finding's recorded source still owns that checkout. A
                // base-deleted/moved/unresolved finding, or a run whose
                // analyzed tree is not the checkout, keeps `None` rather
                // than naming a function that does not belong to the
                // finding.
                owner_signature: match finding.source_currentness {
                    crate::domain::SourceCurrentness::CandidateCurrent => {
                        crate::analysis::owner_call_shape_at_line(
                            root,
                            &finding.probe.location.file,
                            finding.probe.location.line,
                        )
                    }
                    _ => None,
                },
                changed_expression: finding.probe.expression.clone(),
            },
            source_currentness: finding.source_currentness.as_str(),
            classification: finding.class.as_str(),
            severity: finding.class.severity(),
            confidence: finding.confidence,
            ripr: ContextPacketRipr {
                reach: finding.ripr.reach.state.as_str().to_string(),
                infect: finding.ripr.infect.state.as_str().to_string(),
                propagate: finding.ripr.propagate.state.as_str().to_string(),
                observe: finding.ripr.reveal.observe.state.as_str().to_string(),
                discriminate: finding.ripr.reveal.discriminate.state.as_str().to_string(),
            },
            related_tests_total: finding.related_tests_total(),
            related_tests: finding
                .related_tests
                .iter()
                .take(max_related_tests)
                .cloned()
                .collect(),
            observed_values: finding.activation.observed_values.clone(),
            missing_discriminators: finding.activation.missing_discriminators.clone(),
            witness: DiagnosticWitness::from_finding(finding),
            missing: finding.missing.clone(),
            stop_reasons,
            recommended_next_step: finding.recommended_next_step.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, ExposureClass, Finding, FindingCanonicalGap,
        MissingDiscriminatorFact, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId,
        RevealEvidence, RiprEvidence, SourceLocation, StageEvidence, StageState, SymbolId,
        ValueContext, ValueFact,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn context_packet_from_finding_carries_probe_and_ripr_shape() {
        let packet = ContextPacket::from_finding(
            &sample_finding(),
            5,
            vec!["missing related test".to_string()],
            Path::new("."),
        );

        assert_eq!(packet.version, "1.1");
        assert_eq!(packet.tool, "ripr");
        assert_eq!(packet.canonical_gap_id, None);
        assert_eq!(packet.probe.id, "probe:src_lib_rs:9:predicate");
        assert_eq!(packet.probe.family, "predicate");
        assert_eq!(packet.probe.delta, "control");
        assert_eq!(packet.probe.file, "src/lib.rs");
        assert_eq!(packet.probe.line, 9);
        assert_eq!(packet.probe.changed_expression, "x >= 0");
        assert_eq!(packet.ripr.reach, "yes");
        assert_eq!(packet.ripr.infect, "yes");
        assert_eq!(packet.ripr.propagate, "yes");
        assert_eq!(packet.ripr.observe, "yes");
        assert_eq!(packet.ripr.discriminate, "yes");
        assert_eq!(packet.stop_reasons, vec!["missing related test"]);
    }

    #[test]
    fn context_packet_from_finding_limits_related_tests_and_copies_evidence() {
        let mut finding = sample_finding();
        finding.related_tests = vec![related("t1"), related("t2"), related("t3")];
        finding.activation.observed_values.push(ValueFact {
            line: 11,
            text: "assert_eq!(x, 1)".to_string(),
            value: "1".to_string(),
            context: ValueContext::AssertionArgument,
        });
        finding
            .activation
            .missing_discriminators
            .push(MissingDiscriminatorFact {
                value: "x == 0".to_string(),
                reason: "boundary not asserted".to_string(),
                flow_sink: None,
            });
        finding.missing.push("exact boundary assertion".to_string());
        finding.recommended_next_step = Some("add exact boundary assertion".to_string());

        let packet = ContextPacket::from_finding(&finding, 2, Vec::new(), Path::new("."));

        assert_eq!(packet.related_tests.len(), 2);
        assert_eq!(packet.related_tests[0].name, "t1");
        assert_eq!(packet.related_tests[1].name, "t2");
        assert_eq!(packet.observed_values, finding.activation.observed_values);
        assert_eq!(
            packet.missing_discriminators,
            finding.activation.missing_discriminators
        );
        assert_eq!(packet.missing, vec!["exact boundary assertion"]);
        assert_eq!(
            packet.recommended_next_step.as_deref(),
            Some("add exact boundary assertion")
        );
    }

    #[test]
    fn context_packet_from_finding_carries_canonical_gap_id() {
        let mut finding = sample_finding();
        finding.canonical_gap = Some(FindingCanonicalGap {
            id: "gap:python:src/pricing.py:discount:predicate_boundary:predicate:amount>=threshold"
                .to_string(),
            language: "python".to_string(),
            file: "src/pricing.py".to_string(),
            owner: "discount".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "amount>=threshold".to_string(),
        });

        let packet = ContextPacket::from_finding(&finding, 2, Vec::new(), Path::new("."));

        assert_eq!(
            packet.canonical_gap_id.as_deref(),
            Some(
                "gap:python:src/pricing.py:discount:predicate_boundary:predicate:amount>=threshold"
            )
        );
    }

    /// #5996: a session whose analyzed root is an absolute verbatim Windows
    /// path (the MCP server's root spelling) must render the packet location
    /// as the same workspace-relative string every other surface emits, not
    /// the raw `//?/` producer join.
    #[test]
    fn context_packet_renders_a_verbatim_root_location_workspace_relatively() {
        let mut finding = sample_finding();
        let verbatim_root = PathBuf::from(r"\\?\F:\repo");
        finding.probe.location =
            SourceLocation::new(verbatim_root.join("src").join("main.rs"), 4, 1);

        let packet = ContextPacket::from_finding(&finding, 2, Vec::new(), &verbatim_root);

        assert_eq!(packet.probe.file, "./src/main.rs");
    }

    #[test]
    fn context_packet_projects_owner_signature_when_source_names_the_call_shape()
    -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-context-packet-signature-{}",
            std::process::id()
        ));
        let src = root.join("src");
        std::fs::create_dir_all(&src).map_err(|err| err.to_string())?;
        std::fs::write(
            src.join("lib.rs"),
            "pub fn greet(name: &str) -> String {\n    format!(\"HELLO, {}!\", name.to_uppercase())\n}\n",
        )
        .map_err(|err| err.to_string())?;
        let mut finding = sample_finding();
        finding.probe.location = SourceLocation::new("src/lib.rs", 1, 1);

        let packet = ContextPacket::from_finding(&finding, 2, Vec::new(), &root);

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            packet.probe.owner_signature.as_deref(),
            Some("greet(name: &str) -> String")
        );
        Ok(())
    }

    #[test]
    fn context_packet_declines_owner_signature_when_source_is_missing() -> Result<(), String> {
        let mut finding = sample_finding();
        finding.probe.location = SourceLocation::new("src/does-not-exist.rs", 4, 1);

        let packet = ContextPacket::from_finding(&finding, 2, Vec::new(), Path::new("."));

        assert_eq!(packet.probe.owner_signature, None);
        Ok(())
    }

    #[test]
    fn context_packet_declines_owner_signature_when_source_is_not_candidate_current() {
        let root = std::env::temp_dir().join(format!(
            "ripr-context-packet-currentness-{}",
            std::process::id()
        ));
        let src = root.join("src");
        let _ = std::fs::create_dir_all(&src);
        let _ = std::fs::write(
            src.join("lib.rs"),
            "pub fn greet(name: &str) -> String {\n    format!(\"HELLO, {}!\", name.to_uppercase())\n}\n",
        );
        let mut finding = sample_finding();
        finding.probe.location = SourceLocation::new("src/lib.rs", 1, 1);
        finding.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;

        let packet = ContextPacket::from_finding(&finding, 2, Vec::new(), &root);

        let _ = std::fs::remove_dir_all(&root);
        // The on-disk file does exist and would name a shape, but the
        // finding's recorded source was deleted on the candidate side: the
        // packet must not present a surviving function as the owner.
        assert_eq!(packet.probe.owner_signature, None);
    }

    fn sample_finding() -> Finding {
        Finding {
            id: "probe:src_lib_rs:9:predicate".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:src_lib_rs:9:predicate".to_string()),
                location: SourceLocation::new("src/lib.rs", 9, 1),
                owner: Some(SymbolId("crate::sample".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: Some("x > 0".to_string()),
                after: Some("x >= 0".to_string()),
                expression: "x >= 0".to_string(),
                expected_sinks: vec![],
                required_oracles: vec![],
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage("Reachable"),
                infect: stage("Possible infection"),
                propagate: stage("Possible propagation"),
                reveal: RevealEvidence {
                    observe: stage("Observed"),
                    discriminate: stage("Discriminated"),
                },
            },
            confidence: 0.5,
            evidence: vec![],
            missing: vec![],
            flow_sinks: vec![],
            activation: ActivationEvidence::default(),
            stop_reasons: vec![],
            related_tests_matched_total: None,
            related_tests: vec![],
            recommended_next_step: None,
            language: None,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    fn related(name: &str) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/sample.rs"),
            line: 7,
            oracle: Some("assert_eq!(value, 1)".to_string()),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
        }
    }

    fn stage(summary: &str) -> StageEvidence {
        StageEvidence::new(StageState::Yes, Confidence::Medium, summary)
    }
}
