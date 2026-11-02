//! Complete-packet finding projection (#3221).
//!
//! Indexes packet facts, builds evidence paths, and renders common `Finding`
//! values. Partial packets stay fail-closed in classification until #3222.

use super::super::boundary_input::BoundaryInput;
use super::super::identity;
use super::super::static_limit;
use super::super::*;
use super::dispositions::{
    behavior_hint_to_probe_family, perl_oracle_kind_to_domain, perl_oracle_strength_to_domain,
    perl_relation_to_domain,
};
use super::sink_alignment::{perl_row_observation_unconfirmed, sink_aligned_observation};
use crate::analysis::diff::ChangedFile;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn packet_to_findings(packet: &PerlFactPacket) -> Vec<crate::domain::Finding> {
    packet_to_findings_with_currentness(packet, &BTreeMap::new(), |_, _| {
        crate::domain::SourceCurrentness::UnresolvedSubject
    })
}

/// The one-based new-file line of each packet change, settled against the
/// diff ripr itself parsed.
///
/// When `coordinate_line_basis` is declared, packet range lines are converted
/// to one-based diff coordinates before matching added lines. When it is
/// `unspecified`, each change's first line is read both ways against the diff,
/// and a reading counts only when exactly one of the two is an added line. One
/// producer writes the whole packet, so every settled change must agree on the
/// basis: if two disagree, no change gets a line. A change left out keeps its
/// finding on the owner's declaration, as before. This only places the finding;
/// named-owner probe identity and canonical gap identity do not use it.
pub(crate) fn change_lines_from_diff(
    packet: &PerlFactPacket,
    changed_files: &[ChangedFile],
) -> BTreeMap<String, usize> {
    match packet.coordinate_line_basis {
        CoordinateLineBasis::Unspecified => {
            change_lines_from_diff_unspecified(packet, changed_files)
        }
        CoordinateLineBasis::ZeroBased | CoordinateLineBasis::OneBased => {
            change_lines_from_diff_declared(packet, changed_files)
        }
    }
}

fn change_lines_from_diff_declared(
    packet: &PerlFactPacket,
    changed_files: &[ChangedFile],
) -> BTreeMap<String, usize> {
    let mut lines = BTreeMap::new();
    for change in &packet.changes {
        let Some(file) = packet.file(&change.file_id) else {
            continue;
        };
        let path = normalize_repo_relative(&file.path);
        let Some(diff_file) = changed_files
            .iter()
            .find(|diff_file| normalize_repo_relative(&diff_file.path.to_string_lossy()) == path)
        else {
            continue;
        };
        let added: BTreeSet<usize> = diff_file.added_lines.iter().map(|l| l.line).collect();
        let Some(one_based) = packet_line_to_one_based_diff_line(
            packet.coordinate_line_basis,
            change.range.start_line,
        ) else {
            continue;
        };
        if added.contains(&one_based) {
            lines.insert(change.change_id.clone(), one_based);
        }
    }
    lines
}

fn change_lines_from_diff_unspecified(
    packet: &PerlFactPacket,
    changed_files: &[ChangedFile],
) -> BTreeMap<String, usize> {
    // The flag is the basis the line was settled on: `true` for zero-based.
    let mut settled: Vec<(String, usize, bool)> = Vec::new();
    for change in &packet.changes {
        let Some(file) = packet.file(&change.file_id) else {
            continue;
        };
        let path = normalize_repo_relative(&file.path);
        let Some(diff_file) = changed_files
            .iter()
            .find(|diff_file| normalize_repo_relative(&diff_file.path.to_string_lossy()) == path)
        else {
            continue;
        };
        let added: BTreeSet<usize> = diff_file.added_lines.iter().map(|l| l.line).collect();
        let one_based = change.range.start_line;
        let Some(zero_based) = change.range.start_line.checked_add(1) else {
            continue;
        };
        match (added.contains(&one_based), added.contains(&zero_based)) {
            (true, false) => settled.push((change.change_id.clone(), one_based, false)),
            (false, true) => settled.push((change.change_id.clone(), zero_based, true)),
            _ => {}
        }
    }
    let bases: BTreeSet<bool> = settled.iter().map(|(_, _, zero)| *zero).collect();
    if bases.len() > 1 {
        return BTreeMap::new();
    }
    settled
        .into_iter()
        .map(|(change_id, line, _)| (change_id, line))
        .collect()
}

fn packet_line_to_one_based_diff_line(
    basis: CoordinateLineBasis,
    packet_line: usize,
) -> Option<usize> {
    match basis {
        CoordinateLineBasis::OneBased => Some(packet_line),
        CoordinateLineBasis::ZeroBased => packet_line.checked_add(1),
        CoordinateLineBasis::Unspecified => None,
    }
}

fn one_based_range_inclusive(
    basis: CoordinateLineBasis,
    range: &RangeFact,
) -> std::ops::RangeInclusive<usize> {
    let end_packet = range.end_line.max(range.start_line);
    match basis {
        CoordinateLineBasis::OneBased | CoordinateLineBasis::Unspecified => {
            range.start_line..=end_packet
        }
        CoordinateLineBasis::ZeroBased => {
            let start = range.start_line.saturating_add(1);
            let end = end_packet.saturating_add(1);
            start..=end
        }
    }
}

fn diff_line_covered_by_packet_range(
    basis: CoordinateLineBasis,
    range: &RangeFact,
    diff_line_one_based: usize,
) -> bool {
    match basis {
        CoordinateLineBasis::Unspecified => {
            let end_packet = range.end_line.max(range.start_line);
            range.start_line <= diff_line_one_based
                && diff_line_one_based <= end_packet.saturating_add(1)
        }
        _ => one_based_range_inclusive(basis, range).contains(&diff_line_one_based),
    }
}

/// #6586: a Perl change is candidate-current only when the consumer observed
/// it: the file is on disk under the analysis root after resolving symlinks
/// (so ingestion verified its digest against the packet) and the diff adds a
/// line inside the change's range whose text matches the source at that line.
/// Anything else, including fixture-only packets, stale diffs and changes the
/// diff does not touch, stays the explicit unknown.
pub(crate) fn perl_change_currentness(
    root: &std::path::Path,
    changed_files: &[ChangedFile],
    file: &FileFact,
    change: &ChangeFact,
    coordinate_line_basis: CoordinateLineBasis,
) -> crate::domain::SourceCurrentness {
    let lines = one_based_range_inclusive(coordinate_line_basis, &change.range);
    perl_lines_currentness(root, changed_files, file, lines)
}

/// Whether the diff's added line within `lines` of `file` is the line on disk
/// under `root` (see [`perl_change_currentness`]).
pub(crate) fn perl_lines_currentness(
    root: &std::path::Path,
    changed_files: &[ChangedFile],
    file: &FileFact,
    lines: std::ops::RangeInclusive<usize>,
) -> crate::domain::SourceCurrentness {
    use crate::domain::SourceCurrentness;
    // Resolve symlinks before trusting the on-disk source: a packet path whose
    // directory is a symlink out of the root is not a source ripr observed
    // under the analysis root.
    let (Ok(canonical_root), Ok(source)) =
        (root.canonicalize(), root.join(&file.path).canonicalize())
    else {
        return SourceCurrentness::UnresolvedSubject;
    };
    if !source.starts_with(&canonical_root) || !source.is_file() {
        return SourceCurrentness::UnresolvedSubject;
    }
    // The diff's added line must also exist in the digest-verified source at
    // that coordinate with the same text, so a stale or foreign diff cannot
    // promote a finding to a candidate edit target.
    let Ok(text) = std::fs::read_to_string(&source) else {
        return SourceCurrentness::UnresolvedSubject;
    };
    let source_lines = text.lines().collect::<Vec<_>>();
    let source_line = |line: usize| {
        let text = source_lines.get(line.checked_sub(1)?)?;
        Some(if line == 1 {
            text.trim_start_matches('\u{feff}')
        } else {
            text
        })
    };
    let added_in_range = changed_files
        .iter()
        .filter(|changed| changed.path == std::path::Path::new(&file.path))
        .flat_map(|changed| &changed.added_lines)
        .any(|added| {
            lines.contains(&added.line)
                && source_line(added.line) == Some(added.text.trim_end_matches('\r'))
        });
    if added_in_range {
        SourceCurrentness::CandidateCurrent
    } else {
        SourceCurrentness::UnresolvedSubject
    }
}

/// RIPR-SPEC-0235 rule 21 (#9420): an added line of a source file that no
/// packet change or owner covers, in a file whose owner-less dynamic
/// boundaries name a static limit, reads `static_unknown` with that limit.
/// The producer emits no change for top-level code such as the table a glob
/// assignment turns into subs, so without this the line is silent, and silence
/// reads as "nothing to check".
pub(crate) fn unowned_change_findings(
    packet: &PerlFactPacket,
    changed_files: &[ChangedFile],
    root: &std::path::Path,
) -> Vec<crate::domain::Finding> {
    use crate::domain::{
        ActivationEvidence, Confidence as RiprConfidence, DeltaKind, ExposureClass,
        LanguageId as DomainLanguageId, LanguageStatus, Probe, ProbeFamily, ProbeId,
        RevealEvidence, RiprEvidence, SourceLocation, StageEvidence, StageState,
    };

    let mut findings = Vec::new();
    for diff_file in changed_files {
        let diff_path = normalize_repo_relative(&diff_file.path.to_string_lossy());
        let Some(file) = packet.files.iter().find(|file| {
            file.role.contains(&FileRole::Source)
                && normalize_repo_relative(&file.path) == diff_path
        }) else {
            continue;
        };
        let Some(limit) = static_limit::file_scope_kind(packet, &file.file_id) else {
            continue;
        };
        for added in &diff_file.added_lines {
            let text = added.text.trim();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            let line = added.line;
            let covers = |range: &RangeFact| {
                diff_line_covered_by_packet_range(packet.coordinate_line_basis, range, line)
            };
            let covered = packet
                .changes
                .iter()
                .filter(|change| change.file_id == file.file_id)
                .any(|change| covers(&change.range))
                || packet
                    .owners
                    .iter()
                    .filter(|owner| {
                        owner.file_id == file.file_id && owner.kind != OwnerKind::Package
                    })
                    .any(|owner| covers(&owner.range));
            if covered {
                continue;
            }
            let probe_id = format!("probe:{}:{line}:perl-unowned", file.path);
            let unknown = StageEvidence::new(
                StageState::Unknown,
                RiprConfidence::Low,
                "No Perl fact covers this changed line",
            );
            findings.push(crate::domain::Finding {
                analysis_scope: crate::domain::AnalysisScope::Diff,
                confidence_basis: Vec::new(),
                id: probe_id.clone(),
                canonical_gap: None,
                probe: Probe {
                    id: ProbeId(probe_id),
                    location: SourceLocation::new(std::path::PathBuf::from(&file.path), line, 1),
                    owner: None,
                    family: ProbeFamily::StaticUnknown,
                    delta: DeltaKind::Value,
                    before: None,
                    after: None,
                    expression: "unowned_change".to_string(),
                    expected_sinks: Vec::new(),
                    required_oracles: Vec::new(),
                },
                class: ExposureClass::StaticUnknown,
                ripr: RiprEvidence {
                    reach: unknown.clone(),
                    infect: unknown.clone(),
                    propagate: unknown.clone(),
                    reveal: RevealEvidence {
                        observe: unknown.clone(),
                        discriminate: unknown,
                    },
                },
                confidence: 0.5,
                evidence: vec![
                    "perl_unowned_change: no change or owner fact covers this line, and the file holds dynamic boundaries".to_string(),
                ],
                missing: Vec::new(),
                flow_sinks: Vec::new(),
                activation: ActivationEvidence {
                    observed_values: Vec::new(),
                    missing_discriminators: Vec::new(),
                },
                stop_reasons: Vec::new(),
                related_tests_matched_total: None,
                related_tests: Vec::new(),
                recommended_next_step: Some(
                    "This line is top-level code in a file ripr cannot model statically (generated subs, symbol-table writes or string eval), and the fact producer attributed no owner to it. If a test calls what it defines and asserts the result, no change is needed; otherwise add one."
                        .to_string(),
                ),
                language: Some(DomainLanguageId::Perl),
                language_status: Some(LanguageStatus::Preview),
                owner_kind: None,
                static_limit_kind: Some(limit),
                changed_sink: None,
                observed_sink: None,
                oracle_alignment: None,
                alignment_reason: None,
                source_currentness: perl_lines_currentness(root, changed_files, file, line..=line),
            });
        }
    }
    findings
}
pub(crate) fn packet_to_findings_with_currentness(
    packet: &PerlFactPacket,
    change_lines: &BTreeMap<String, usize>,
    currentness: impl Fn(&FileFact, &ChangeFact) -> crate::domain::SourceCurrentness,
) -> Vec<crate::domain::Finding> {
    use crate::domain::{
        ActivationEvidence, Confidence as RiprConfidence, DeltaKind, ExposureClass,
        FindingCanonicalGap, LanguageId as DomainLanguageId, LanguageStatus,
        MissingDiscriminatorFact, OracleKind as DomainOracleKind,
        OracleStrength as DomainOracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest,
        RevealEvidence, RiprEvidence, SourceLocation, StageEvidence, StageState, SymbolId,
    };

    let mut findings = Vec::new();
    // Assign over the complete packet before projection/output filtering, so
    // hiding a sibling cannot silently change another finding's ordinal.
    let probe_ids = identity::probe_ids(packet);

    for change in &packet.changes {
        // Find the owning entity.
        let Some(owner) = packet.owner(&change.owner_id) else {
            continue;
        };
        // Find the source file for the probe's location and projected facts.
        let Some(file) = packet.file(&change.file_id) else {
            continue;
        };

        // Resolve related-test evidence through the packet helper. This is the
        // key H1 fix: each evidence carries the *test* file path (resolved via
        // `test.file_id`), the per-test verify command, and the relation kind.
        let related_evidence = packet.related_test_evidence_for_change(&change.change_id);

        // Keep blocking disposition separate from the strongest shared taxonomy
        // label earned by the packet. Operational v1 boundaries still fail
        // closed, but they no longer masquerade as dynamic dispatch.
        let static_limit_projection = static_limit::for_change(packet, change, &related_evidence);
        // Two distinct gates: `blocks_class` caps the exposure CLASS
        // (semantic dynamic-dispatch evidence only), while `blocks` gates
        // repair-shaped output (canonical gap + suggestions) — operational
        // limitations such as `partial_emitter` must suppress repair
        // guidance even when they do not mask an earned class (#3583 review).
        let class_blocked = static_limit_projection.blocks_class;
        let actionability_blocked = static_limit_projection.blocks;

        // Build the projected RelatedTests from the packet evidence. The test
        // FILE comes from `ev.test_path` (resolved from test.file_id), never
        // from the production `file.path`. The test LINE is re-resolved from
        // the TestFact (PerlRelatedTestEvidence intentionally carries no
        // range), so the projection sees the real assertion location.
        let mut related: Vec<RelatedTest> = related_evidence
            .iter()
            .map(|ev| {
                let test_line = packet
                    .test(&ev.test_id)
                    .map(|test| test.range.start_line)
                    .unwrap_or(0);
                let oracle = ev.oracle_id.as_deref().and_then(|oid| packet.oracle(oid));
                let (perl_relation_reason, perl_relation_confidence) =
                    perl_relation_to_domain(ev.relation_kind);
                RelatedTest {
                    name: ev.test_name.clone(),
                    file: std::path::PathBuf::from(&ev.test_path),
                    line: test_line,
                    oracle: oracle.and_then(|o| o.expression.clone()),
                    oracle_kind: oracle
                        .map(|o| perl_oracle_kind_to_domain(o.kind))
                        .unwrap_or(DomainOracleKind::Unknown),
                    oracle_strength: ev
                        .oracle_strength
                        .map(perl_oracle_strength_to_domain)
                        .unwrap_or(DomainOracleStrength::Unknown),
                    relation_reason: perl_relation_reason,
                    relation_confidence: perl_relation_confidence,
                    miss: None,
                    miss_support: None,
                    role: crate::domain::RelatedTestRole::Contributing,
                }
            })
            .collect();

        // Determine exposure class — conservative-or-stricter.
        //
        // H2 (Campaign 31): sink alignment is now computable. When a
        // DirectOwnerCall relation links a strong-exact oracle whose
        // `observed_sink` aligns to the change's `changed_observable`, the
        // change is ALREADY OBSERVED → `Exposed` (the maintainer end-state
        // outcome #2: "no test is needed"). This is the discrimination-vs-
        // discrimination distinction made concrete: owner-target identity alone is
        // NOT observation (the false-exposed family); the oracle must observe
        // the *specific changed sink*.
        //
        // Otherwise: start from the packet's relation/oracle-aware classifier,
        // then apply the relation-kind gate the packet classifier omits:
        // advisory kinds can never promote past ReachableUnrevealed. We never
        // *raise* the class beyond what the packet computes except for the
        // established-sink-alignment Exposed promotion; we downgrade advisory
        // relations and override to StaticUnknown on a blocking boundary.
        let sink_aligned_evidence = sink_aligned_observation(&related_evidence, change, packet);
        // #8352: a `partial` packet may simply not have emitted the relation
        // that would link a test to this change, and `file_proximity` is
        // co-location, not a link. With no relation other than
        // `file_proximity`, neither
        // "no static path" nor "reachable but unrevealed" is an assessed
        // result, so the class is unknown rather than a coverage claim.
        let partial_without_change_link = packet.packet_status == PacketStatus::Partial
            && related_evidence
                .iter()
                .all(|ev| ev.relation_kind == RelationKind::FileProximity);
        let class = if class_blocked || partial_without_change_link {
            ExposureClass::StaticUnknown
        } else if related.is_empty() {
            ExposureClass::NoStaticPath
        } else if sink_aligned_evidence
            .as_ref()
            .is_some_and(|aligned| aligned.boundary_input == BoundaryInput::Unestablished)
        {
            // RIPR-SPEC-0235 rule 22 (#8979): equal sink text on a changed
            // ordering comparison names the predicate, not an input that
            // crosses its threshold. Unknown, never `exposed`.
            ExposureClass::StaticUnknown
        } else if sink_aligned_evidence.is_some() {
            // H2: established sink alignment → already observed. The strongest
            // honest claim in the conservative taxonomy. No repair gap is
            // needed because a discriminator already exists.
            ExposureClass::Exposed
        } else {
            let packet_class = packet.classify_change_from_related_tests(&change.change_id);
            // Downgrade advisory-only relations: a WeaklyExposed class is
            // kept only when some relation's own bound evidence earned it
            // through an owner call shape (#6583). `DirectOwnerCall` earns
            // it directly; a reachable `MethodReceiver` or `HelperCall`
            // earns the same weak credit when its own oracle is the bound,
            // owner-targeted strong oracle (RIPR-SPEC-0235 examples 13 and
            // 14) — `classify_related_relation` already requires exactly
            // that binding, so a wrong-receiver, weak-oracle, or
            // unreachable relation never reaches WeaklyExposed. Every other
            // relation kind is capped at ReachableUnrevealed. Sink-aligned
            // `Exposed` stays direct-only in `sink_aligned_observation`.
            if packet_class == ExposureClass::WeaklyExposed
                && !related_evidence.iter().any(|ev| {
                    ev.class == ExposureClass::WeaklyExposed
                        && ev.relation_kind.supports_weak_oracle_credit()
                })
            {
                ExposureClass::ReachableUnrevealed
            } else {
                packet_class
            }
        };
        // RIPR-SPEC-0235 rule 20 (#9416): a direct owner call no oracle is
        // linked to, in a test that also asserts through a code reference
        // (`is($c->(), 5)`), may be observed through a closure the owner
        // returned. The packet carries no relation from the oracle to the
        // owner, so this is unknown, not a gap.
        let coderef_unresolved = class == ExposureClass::ReachableUnrevealed
            && related_evidence.iter().any(|ev| {
                ev.class == ExposureClass::ReachableUnrevealed
                    && ev.relation_kind.supports_positive_reachability()
                    && ev.reachability_hint == ReachabilityHint::Reachable
                    && ev.oracle_id.is_none()
                    && packet.test_asserts_through_code_reference(&ev.test_id)
            });
        let class = if coderef_unresolved {
            ExposureClass::StaticUnknown
        } else {
            class
        };
        let is_already_observed = class == ExposureClass::Exposed;
        let predicate_input_unestablished = class == ExposureClass::StaticUnknown
            && !class_blocked
            && !partial_without_change_link
            && sink_aligned_evidence
                .as_ref()
                .is_some_and(|aligned| aligned.boundary_input == BoundaryInput::Unestablished);
        let observable_unavailable = class == ExposureClass::StaticUnknown
            && !class_blocked
            && !partial_without_change_link
            && !coderef_unresolved
            && packet.observable_declared_absent(&change.change_id);
        let pinned_error_unresolved = class == ExposureClass::StaticUnknown
            && !class_blocked
            && packet.grades_oracle_matchers()
            && related_evidence.iter().any(|ev| {
                ev.class == ExposureClass::StaticUnknown
                    && ev.reachability_hint == ReachabilityHint::Reachable
                    && ev.oracle_strength == Some(OracleStrength::StrongExact)
                    && ev.oracle_shape.as_deref() == Some("exception_observer")
            });
        let broad_oracle_unresolved = class == ExposureClass::StaticUnknown
            && !class_blocked
            && related_evidence.iter().any(|ev| {
                ev.class == ExposureClass::StaticUnknown
                    && ev.reachability_hint == ReachabilityHint::Reachable
                    && ev.oracle_strength == Some(OracleStrength::WeakBroad)
            });
        // RIPR-SPEC-0224 / #5498: on a weakly exposed finding from a
        // complete, unblocked packet (a concrete discriminator is not
        // required), a row whose own direct, strong exact, owner-targeted
        // oracle earned the weak exposure but establishes no sink alignment
        // says observation is unconfirmed.
        // `related` maps `related_evidence` one to one and in order. Every
        // other row keeps no miss: advisory, weak and unknown evidence is not
        // an established per-test defect, and the finding-wide discriminator
        // has no test identity to lend a row.
        let packet_admitted = packet.packet_status == PacketStatus::Complete
            && !class_blocked
            && !actionability_blocked;
        if class == ExposureClass::WeaklyExposed && packet_admitted {
            for (row, ev) in related.iter_mut().zip(&related_evidence) {
                if perl_row_observation_unconfirmed(packet, change, ev) {
                    row.miss = Some(crate::domain::RelatedTestMiss::ObservationUnconfirmed);
                }
            }
        }

        // Concrete discriminator gate (H1). A canonical repair gap requires a
        // concrete, packet-provided discriminator. SPEC-0064 documents the
        // `missing_discriminator` change field as the canonical channel
        // (#6829); the `discriminator:` prefix on `changed_text_digest`
        // remains accepted for compatibility with packets written before the
        // field was specified. Generic enum labels are not actionable gaps.
        let concrete_discriminator = concrete_change_discriminator(change);
        let has_concrete_discriminator = concrete_discriminator.is_some();

        // Advice must name an established direct call, not whichever advisory
        // row the producer listed first (#6734). Prefer admitted exact evidence;
        // stable identities break ties without raising any evidence strength.
        let suggested_test = related_evidence
            .iter()
            .filter(|ev| {
                ev.relation_kind.supports_positive_reachability()
                    && ev.reachability_hint == ReachabilityHint::Reachable
            })
            .min_by_key(|ev| {
                (
                    ev.class != ExposureClass::WeaklyExposed,
                    &ev.test_id,
                    &ev.relation_id,
                )
            });

        // Use the selected eligible oracle's assertion shape (the one actually
        // linked to this change), not a generic behavior-hint default.
        let selected_assertion_shape = suggested_test
            .and_then(|selected| {
                let oracle = packet.oracle(selected.oracle_id.as_deref()?)?;
                (oracle.test_id == selected.test_id
                    && oracle.target_owner_id.as_deref() == Some(change.owner_id.as_str()))
                .then(|| oracle.kind.assertion_shape().to_string())
            })
            .unwrap_or_else(|| change.behavior_hint.default_assertion_shape().to_string());

        // Canonical gap: only when a concrete discriminator exists, the packet
        // can form a stable identity, the change is NOT already observed, and
        // no dynamic boundary/limitation blocks static actionability. Static
        // limits stay fail-closed: they may explain why classification is
        // unknown, but they must not produce repair-shaped gap identities.
        let canonical_gap: Option<FindingCanonicalGap> = if has_concrete_discriminator
            && !is_already_observed
            && !actionability_blocked
            && class != ExposureClass::StaticUnknown
        {
            packet
                .canonical_gap_identity_for_change_with_assertion_shape(
                    &change.change_id,
                    &selected_assertion_shape,
                )
                .map(|gap| FindingCanonicalGap {
                    id: gap.id,
                    language: "perl".to_string(),
                    file: file.path.clone(),
                    owner: owner.name.clone().unwrap_or_default(),
                    behavior_kind: gap.behavior_kind,
                    probe_kind: gap.assertion_shape,
                    normalized_discriminator: gap.missing_discriminator,
                })
        } else {
            None
        };

        // Build evidence strings (perl_*: keys the projection reads).
        //
        // H2: when the change is ALREADY OBSERVED (established sink alignment),
        // emit `perl_already_discriminated:` explaining why no test is needed
        // (maintainer end-state outcome #2) and SUPPRESS the repair-gap fields
        // (perl_suggested_test_location / perl_suggested_assertion) — telling a
        // maintainer to ADD a test when one already discriminates the change
        // would be the cardinal sin.
        let mut evidence: Vec<String> = Vec::new();
        evidence.push(format!(
            "perl_repair_kind: {}",
            change.behavior_hint.repair_kind().unwrap_or("unknown")
        ));
        evidence.push(format!(
            "perl_coordinate_line_basis: {}",
            packet.coordinate_line_basis.evidence_label()
        ));
        if change.provenance_refs.is_empty() {
            evidence.push(
                "perl_change_provenance_refs: absent (producer linked no provenance for this change)"
                    .to_string(),
            );
        } else {
            evidence.push(format!(
                "perl_change_provenance_refs: {}",
                change.provenance_refs.join(",")
            ));
        }
        evidence.push(format!(
            "perl_target_test_shape: {}",
            change.behavior_hint.default_assertion_shape()
        ));
        if static_limit_projection.missing_test_runner {
            evidence.push(
                "perl_missing_test_runner: the related Perl test runner is unavailable; static sink alignment does not verify execution"
                    .to_string(),
            );
        }
        if is_already_observed && let Some(aligned) = sink_aligned_evidence.as_ref() {
            // The sink-aligned evidence explains the observation: which test +
            // oracle observes which changed sink. This is the "already-observed
            // evidence explaining why no test is needed" (goal outcome #2).
            evidence.push(format!(
                "perl_already_discriminated: {} observes changed sink `{}` via {}",
                aligned.test_name, aligned.observed_sink, aligned.oracle_shape
            ));
        } else if !is_already_observed
            && !actionability_blocked
            && class != ExposureClass::StaticUnknown
            && has_concrete_discriminator
            && let Some(selected) = suggested_test
        {
            evidence.push(format!(
                "perl_suggested_test_location: {}::{}",
                selected.test_path, selected.test_name
            ));
            evidence.push(format!(
                "perl_suggested_assertion: {}",
                change.behavior_hint.default_missing_discriminator()
            ));
        }
        // Verification belongs to the same selected test as the location.
        // A missing command cannot be replaced by another row's command.
        if let Some(selected) = suggested_test
            && let Some(argv) = selected.verify_command.as_ref()
            && !argv.is_empty()
        {
            evidence.push(format!("perl_verify_command: {}", argv.join(" ")));
        }

        // Missing discriminator: only when concrete AND not already observed.
        // An already-observed change has NO missing discriminator — it is
        // discriminated. An informational finding (no concrete discriminator)
        // carries no missing-discriminator entry either.
        let missing_discriminators: Vec<MissingDiscriminatorFact> = if is_already_observed {
            Vec::new()
        } else {
            concrete_discriminator
                .clone()
                .map(|value| MissingDiscriminatorFact {
                    value,
                    reason: format!(
                        "changed Perl {} at {} lacks a concrete discriminator",
                        change.behavior_hint.as_str(),
                        file.path
                    ),
                    flow_sink: None,
                })
                .into_iter()
                .collect()
        };

        // Build the Finding.
        let Some(probe_id) = probe_ids.get(change.change_id.as_str()).cloned() else {
            continue;
        };
        let probe = Probe {
            id: ProbeId(probe_id.clone()),
            location: match change_lines.get(&change.change_id) {
                Some(&line) => SourceLocation::new(std::path::PathBuf::from(&file.path), line, 1),
                None => SourceLocation::new(
                    std::path::PathBuf::from(&file.path),
                    owner.range.start_line,
                    owner.range.start_column,
                ),
            },
            owner: owner
                .name
                .as_ref()
                .map(|n| SymbolId(format!("perl:{}::{}", file.path, n))),
            family: behavior_hint_to_probe_family(change.behavior_hint),
            delta: DeltaKind::Value,
            before: None,
            after: None,
            expression: change.behavior_hint.as_str().to_string(),
            expected_sinks: change
                .behavior_hint
                .repair_kind()
                .map(|_| selected_assertion_shape.clone())
                .into_iter()
                .collect(),
            required_oracles: vec![selected_assertion_shape.clone()],
        };

        let reach = StageEvidence::new(
            if related.is_empty() {
                StageState::No
            } else {
                StageState::Yes
            },
            RiprConfidence::Medium,
            "Perl fact packet relation evidence",
        );
        let unknown = StageEvidence::new(
            StageState::Unknown,
            RiprConfidence::Low,
            "Perl preview adapter does not model infection/propagation",
        );

        findings.push(crate::domain::Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: probe_id,
            canonical_gap,
            probe,
            class,
            ripr: RiprEvidence {
                reach: reach.clone(),
                infect: unknown.clone(),
                propagate: unknown,
                reveal: RevealEvidence {
                    observe: reach,
                    // #6584: an exposed finding was credited by a sink-aligned
                    // strong exact oracle, which is the discriminator; saying
                    // it is missing contradicts the class.
                    discriminate: if is_already_observed {
                        StageEvidence::new(
                            StageState::Yes,
                            RiprConfidence::Medium,
                            "Sink-aligned strong exact oracle from packet",
                        )
                    } else {
                        StageEvidence::new(
                            StageState::Weak,
                            RiprConfidence::Medium,
                            "Missing discriminator from packet",
                        )
                    },
                },
            },
            confidence: 0.5,
            evidence,
            missing: if is_already_observed {
                Vec::new()
            } else {
                concrete_discriminator.clone().into_iter().collect()
            },
            flow_sinks: Vec::new(),
            activation: ActivationEvidence {
                observed_values: Vec::new(),
                missing_discriminators,
            },
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: related,
            recommended_next_step: Some(if static_limit_projection.missing_test_runner {
                if is_already_observed {
                    "Make the related Perl test runner available and run the sink-aligned test to verify the static observation."
                        .to_string()
                } else {
                    "Make the related Perl test runner available, then add a focused assertion for the changed behavior and verify it."
                        .to_string()
                }
            } else if predicate_input_unestablished {
                // RIPR-SPEC-0235 rule 22: say why this is unknown and what
                // settles it, instead of crediting an input that may sit far
                // from the threshold.
                "A related Perl test asserts the value of the changed comparison, but ripr cannot establish that its owner call passes the threshold value (the packet carries no fact saying where the inputs fall), so it cannot tell whether moving the threshold would fail it. If it asserts a value on each side of the boundary, no change is needed; otherwise add one."
                    .to_string()
            } else if coderef_unresolved {
                // RIPR-SPEC-0235 rule 20: say why this is unknown and what
                // settles it, instead of asking for an assertion that may exist.
                "A related Perl test calls this code and asserts through a code reference (`$code->()`), so ripr cannot tell whether that assertion pins the changed behavior. If it asserts the value a returned closure produces, no change is needed; otherwise add one."
                    .to_string()
            } else if broad_oracle_unresolved {
                // RIPR-SPEC-0235 rule 16: say why this is unknown and what
                // settles it, instead of asking for an assertion that may exist.
                "A related Perl test reaches this code with an assertion the fact producer grades broad, so ripr cannot tell whether it pins the changed behavior. If it asserts the specific message, class or value, no change is needed; otherwise tighten it."
                    .to_string()
            } else if observable_unavailable {
                // RIPR-SPEC-0235 rule 18: say why this is unknown instead of
                // asking for an assertion that may already exist.
                "The fact producer found no observable for this change, so ripr cannot tell whether a related Perl test's assertion pins it. If an existing assertion checks the changed value, no change is needed; otherwise add one."
                    .to_string()
            } else if pinned_error_unresolved {
                // RIPR-SPEC-0235 rule 16: the assertion pins the raised error,
                // and static facts do not say whether it pins the boundary.
                "A related Perl test asserts the error this code raises through a class or message matcher, so ripr cannot tell whether it pins the changed boundary. If it fails when the boundary moves, no change is needed; otherwise add a case on each side of it."
                    .to_string()
            } else if is_already_observed {
                // H2: the change is already discriminated by an existing test.
                // No new test is needed; this is maintainer end-state outcome #2.
                "No test change needed — an existing test already observes the \
                 changed behavior (sink aligned)."
                    .to_string()
            } else {
                "Add a focused Perl assertion that pins the changed behavior.".to_string()
            }),
            language: Some(DomainLanguageId::Perl),
            language_status: Some(LanguageStatus::Preview),
            owner_kind: None,
            static_limit_kind: static_limit_projection.kind.or(
                coderef_unresolved.then_some(crate::domain::StaticLimitKind::DynamicDispatch),
            ),
            // changed_sink uses the concrete discriminator when present;
            // otherwise the behavior-hint label (advisory only, since no
            // canonical gap is attached).
            changed_sink: Some(concrete_discriminator.clone().unwrap_or_else(|| {
                change
                    .behavior_hint
                    .default_missing_discriminator()
                    .to_string()
            })),
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            // #6586: resolved by the caller from the observed diff; without one
            // the disposition stays the explicit unknown (#3280).
            source_currentness: currentness(file, change),
        });
    }

    findings
}
