use crate::domain::Finding;

/// Outcome of resolving one selector against the finding set (#8166 Slice B).
///
/// Selection is fail-closed: a selector that names several findings is
/// `Ambiguous`, never a silent first match, and a selector that names none
/// is `NoMatch`. Both refusals are usage-class at the CLI (prose-only,
/// exit 2), matching the existing selector-miss contract.
pub(in crate::app) enum FindingSelection<'a> {
    Unique(&'a Finding),
    NoMatch,
    Ambiguous(Vec<&'a Finding>),
}

/// Resolve `selector` against `findings`: an exact finding ID, an exact
/// canonical gap identity, or a `file:line` location. One finding matched
/// through several spellings counts once; distinct findings matched through
/// any spelling make the selector ambiguous.
pub(in crate::app) fn select_finding<'a>(
    findings: &'a [Finding],
    selector: &str,
) -> FindingSelection<'a> {
    let matches: Vec<&Finding> = findings
        .iter()
        .filter(|finding| {
            finding.id == selector
                || finding
                    .canonical_gap
                    .as_ref()
                    .is_some_and(|gap| gap.id == selector)
                || selector_matches_location(selector, finding)
        })
        .collect();
    match matches.len() {
        0 => FindingSelection::NoMatch,
        1 => FindingSelection::Unique(matches[0]),
        _ => FindingSelection::Ambiguous(matches),
    }
}

/// Whether `selector` has the shape of a seam ID: exactly 16 lowercase hex
/// characters. Only seam-shaped selectors pay for the reverse seam lookup;
/// every other miss keeps the direct selection result.
pub(in crate::app) fn is_seam_id_selector(selector: &str) -> bool {
    selector.len() == 16
        && selector
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

/// Resolve a seam ID against the finding set through the shared
/// seam-to-finding binding (#8166 residual): the seam named by `seam_id`
/// binds at most one finding via
/// [`finding_for_seam`](super::repair_card_handoff::finding_for_seam), whose
/// producer branch already refuses sibling collisions. A seam naming no
/// finding in this set (stale for the selected diff), or no such seam at
/// all, is `NoMatch` — never a manufactured finding.
pub(in crate::app) fn select_finding_for_seam<'a>(
    findings: &'a [Finding],
    classified: &[crate::analysis::ClassifiedSeam],
    seam_id: &str,
) -> FindingSelection<'a> {
    let Some(entry) = classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == seam_id)
    else {
        return FindingSelection::NoMatch;
    };
    let canonical_gap_id =
        crate::analysis::canonical_gap::canonical_gap_identity(entry).map(|identity| identity.id);
    match super::repair_card_handoff::finding_for_seam(
        findings,
        entry,
        canonical_gap_id.as_deref(),
        classified,
    ) {
        Some(finding) => FindingSelection::Unique(finding),
        None => FindingSelection::NoMatch,
    }
}

/// [`select_finding`] plus the seam-ID reverse lookup for a fresh run
/// (#8166 residual): a direct hit keeps the existing result; only a
/// seam-shaped miss inventories the root's classified seams and binds the
/// named seam back to its finding. An unreadable or truncated inventory
/// leaves the miss standing rather than guessing.
pub(in crate::app) fn select_finding_with_seam_reverse<'a>(
    findings: &'a [Finding],
    root: &std::path::Path,
    config: &crate::config::RiprConfig,
    selector: &str,
) -> FindingSelection<'a> {
    let direct = select_finding(findings, selector);
    if !matches!(direct, FindingSelection::NoMatch) || !is_seam_id_selector(selector) {
        return direct;
    }
    let Ok((classified, limit_info)) =
        crate::analysis::inventory_classified_seams_at_with_config(root, config)
    else {
        return FindingSelection::NoMatch;
    };
    if let Some(info) = &limit_info
        && info.analyzed < info.total
    {
        return FindingSelection::NoMatch;
    }
    select_finding_for_seam(findings, &classified, selector)
}

/// Add location syntax guidance only after selection misses. Finding IDs are
/// opaque: this helper never rejects a matched ID or changes locator matching.
pub(in crate::app) fn location_selector_needs_syntax_hint(selector: &str) -> bool {
    if selector.starts_with("probe:")
        || selector.starts_with("repo-probe:")
        || selector.starts_with("gap:")
    {
        return false;
    }
    let Some((file, line)) = selector.rsplit_once(':') else {
        return selector.contains(['/', '\\']);
    };
    let looks_like_location = file.is_empty()
        || file.contains(['/', '\\', '.'])
        || selector.chars().all(|character| character == ':');
    looks_like_location
        && (file.is_empty()
            || !line
                .parse::<usize>()
                .is_ok_and(|number| number > 0 && line == number.to_string()))
}

pub(in crate::app) fn selector_matches_location(selector: &str, finding: &Finding) -> bool {
    // Match the spelling renderers show (stable text, %XX for invalid
    // bytes), not a lossy re-render that a raw-byte path could never
    // type into (#3609).
    let file = crate::analysis::stable_path_text(&finding.probe.location.file);
    selector_matches_file_line(file.as_ref(), finding.probe.location.line, selector)
}

/// Resolve a `file:line` locator against a stored finding location.
///
/// Match the natural path a user sees, not the exact internal string. The
/// stored path carries a `.\`/`./` prefix and platform separators (e.g.
/// `.\crates/ripr/src/lib.rs`); normalize both sides, then accept an exact
/// match or a path-suffix match in either direction so that `src/lib.rs:8` and
/// `crates/ripr/src/lib.rs:8` both resolve.
pub(in crate::app) fn selector_matches_file_line(
    stored_file: &str,
    stored_line: usize,
    selector: &str,
) -> bool {
    let Some((selector_file, selector_line)) = selector.rsplit_once(':') else {
        return false;
    };
    if selector_line != stored_line.to_string() {
        return false;
    }
    let stored = normalize_locator_path(stored_file);
    let wanted = normalize_locator_path(selector_file);
    if wanted.is_empty() || stored.is_empty() {
        return false;
    }
    stored == wanted
        || stored.ends_with(&format!("/{wanted}"))
        || wanted.ends_with(&format!("/{stored}"))
}

/// Normalize a path for locator matching: convert `\` to `/`, drop a leading
/// `./`, and trim a trailing `/`. Lets the path a user copies from `check`
/// output (or types naturally) match the stored form regardless of platform
/// separators or the leading-current-directory prefix.
fn normalize_locator_path(path: &str) -> String {
    let forward = path.replace('\\', "/");
    let trimmed = forward.strip_prefix("./").unwrap_or(forward.as_str());
    trimmed.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        FindingSelection, location_selector_needs_syntax_hint, normalize_locator_path,
        select_finding, selector_matches_file_line,
    };
    use crate::domain::{
        ActivationEvidence, DeltaKind, ExposureClass, Finding, FindingCanonicalGap, Probe,
        ProbeFamily, ProbeId, RevealEvidence, RiprEvidence, SourceLocation, StageEvidence,
        SymbolId,
    };

    fn stage() -> StageEvidence {
        StageEvidence::new(
            crate::domain::StageState::Yes,
            crate::domain::Confidence::Medium,
            "selector test",
        )
    }

    fn finding(id: &str, line: usize, gap: Option<&str>) -> Finding {
        Finding {
            id: id.to_string(),
            canonical_gap: gap.map(|gap_id| FindingCanonicalGap {
                id: gap_id.to_string(),
                language: "rust".to_string(),
                file: "src/lib.rs".to_string(),
                owner: "add".to_string(),
                behavior_kind: "predicate".to_string(),
                probe_kind: "predicate".to_string(),
                normalized_discriminator: "x".to_string(),
            }),
            probe: Probe {
                id: ProbeId(id.to_string()),
                location: SourceLocation::new("src/lib.rs", line, 1),
                owner: Some(SymbolId("crate::add".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "x > 0".to_string(),
                expected_sinks: vec![],
                required_oracles: vec![],
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(),
                infect: stage(),
                propagate: stage(),
                reveal: RevealEvidence {
                    observe: stage(),
                    discriminate: stage(),
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

    #[test]
    fn unique_probe_and_gap_and_location_selectors_resolve_one_finding() {
        let findings = vec![
            finding("probe:src_lib.rs:predicate:aaaa", 3, Some("gap:rust:one")),
            finding("probe:src_lib.rs:predicate:bbbb", 9, Some("gap:rust:two")),
        ];
        for (selector, expected) in [
            ("probe:src_lib.rs:predicate:aaaa", 0usize),
            ("gap:rust:one", 0),
            ("src/lib.rs:9", 1),
        ] {
            assert!(
                matches!(
                    select_finding(&findings, selector),
                    FindingSelection::Unique(selected) if selected.id == findings[expected].id
                ),
                "unique selector {selector} must resolve exactly its finding"
            );
        }
    }

    /// #8166 residual: a seam ID resolves back to the one finding bound to
    /// that seam; a seam naming no finding in the set, and a seam-shaped ID
    /// naming no seam, stay `NoMatch`.
    #[test]
    fn seam_id_selector_resolves_through_the_seam_binding() {
        use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
        use crate::analysis::test_grip_evidence::TestGripEvidence;
        use crate::analysis::{ClassifiedSeam, canonical_gap::canonical_gap_identity};
        use crate::domain::{Confidence, StageState};

        let seam = RepoSeam::new(
            "src/lib.rs",
            "src/lib.rs::add",
            SeamKind::PredicateBoundary,
            30,
            3,
            "x > 0",
            RequiredDiscriminator::BoundaryValue {
                description: "x > 0".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let stage = |state: StageState| StageEvidence::new(state, Confidence::Medium, "t");
        let entry = ClassifiedSeam {
            evidence: TestGripEvidence {
                seam_id: seam.id().clone(),
                related_tests: Vec::new(),
                reach: stage(StageState::Yes),
                activate: stage(StageState::Yes),
                propagate: stage(StageState::Yes),
                observe: stage(StageState::Yes),
                discriminate: stage(StageState::Weak),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                new_test_target: None,
                statically_contradicted_related_tests: 0,
            },
            seam,
            class: crate::analysis::seams::SeamGripClass::WeaklyGripped,
        };
        let seam_id = entry.seam.id().as_str().to_string();
        let identity = canonical_gap_identity(&entry);
        assert!(identity.is_some(), "eligible seam must have a gap identity");
        let Some(identity) = identity else {
            return;
        };
        let mut bound = finding("probe:src_lib.rs:predicate:aaaa", 3, None);
        bound.canonical_gap = Some(crate::domain::FindingCanonicalGap {
            id: identity.id.clone(),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: entry.seam.owner().to_string(),
            behavior_kind: "predicate".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "x".to_string(),
        });
        let findings = vec![bound];
        let classified = [entry];
        assert!(
            matches!(
                super::select_finding_for_seam(&findings, &classified, &seam_id),
                FindingSelection::Unique(selected) if selected.id == findings[0].id
            ),
            "the seam ID must resolve to its bound finding"
        );
        // Stale for this diff: the seam exists, no finding binds to it.
        assert!(matches!(
            super::select_finding_for_seam(&[], &classified, &seam_id),
            FindingSelection::NoMatch
        ));
        // No such seam at all.
        assert!(matches!(
            super::select_finding_for_seam(&findings, &classified, "0123456789abcdef"),
            FindingSelection::NoMatch
        ));
        assert!(super::is_seam_id_selector("0123456789abcdef"));
        assert!(!super::is_seam_id_selector(
            "probe:src_lib.rs:predicate:aaaa"
        ));
    }

    #[test]
    fn unmatched_selector_is_no_match() {
        let findings = vec![finding("probe:src_lib.rs:predicate:aaaa", 3, None)];
        assert!(matches!(
            select_finding(&findings, "0123456789abcdef"),
            FindingSelection::NoMatch
        ));
        assert!(matches!(
            select_finding(&findings, "src/lib.rs:99"),
            FindingSelection::NoMatch
        ));
    }

    #[test]
    fn shared_location_and_shared_gap_are_ambiguous_never_first() {
        let findings = vec![
            finding(
                "probe:src_lib.rs:predicate:aaaa",
                3,
                Some("gap:rust:shared"),
            ),
            finding(
                "probe:src_lib.rs:predicate:bbbb",
                3,
                Some("gap:rust:shared"),
            ),
        ];
        for selector in ["src/lib.rs:3", "gap:rust:shared"] {
            assert!(
                matches!(
                    select_finding(&findings, selector),
                    FindingSelection::Ambiguous(candidates) if candidates.len() == 2
                ),
                "shared selector {selector} must refuse as ambiguous"
            );
        }
    }

    #[test]
    fn ambiguous_message_bounds_the_candidate_list() {
        let findings: Vec<Finding> = (0..10)
            .map(|index| finding(&format!("probe:src_lib.rs:predicate:{index:04}"), 3, None))
            .collect();
        assert!(
            matches!(
                select_finding(&findings, "src/lib.rs:3"),
                FindingSelection::Ambiguous(candidates) if candidates.len() == 10
            ),
            "ten findings at one location must be ambiguous"
        );
        let FindingSelection::Ambiguous(candidates) = select_finding(&findings, "src/lib.rs:3")
        else {
            return;
        };
        let message = crate::app::explain::ambiguous_finding_match(
            "src/lib.rs:3",
            &candidates,
            "ripr check --json",
        );
        assert!(message.contains("matched 10 findings"), "{message}");
        assert!(message.contains("(and 2 more)"), "{message}");
    }

    // Stored form mirrors real `check` output: a `.\` prefix and forward slashes.
    const STORED: &str = ".\\crates/ripr/examples/sample/src/lib.rs";

    #[test]
    fn exact_internal_path_still_matches() {
        // Backward compatible: the verbatim stored string keeps working.
        assert!(selector_matches_file_line(
            STORED,
            8,
            ".\\crates/ripr/examples/sample/src/lib.rs:8"
        ));
    }

    #[test]
    fn natural_full_path_matches() {
        assert!(selector_matches_file_line(
            STORED,
            8,
            "crates/ripr/examples/sample/src/lib.rs:8"
        ));
    }

    #[test]
    fn natural_suffix_matches() {
        assert!(selector_matches_file_line(STORED, 8, "src/lib.rs:8"));
    }

    #[test]
    fn pasted_windows_separators_match() {
        assert!(selector_matches_file_line(
            STORED,
            8,
            "crates\\ripr\\examples\\sample\\src\\lib.rs:8"
        ));
    }

    #[test]
    fn line_must_match() {
        assert!(!selector_matches_file_line(STORED, 8, "src/lib.rs:9"));
        assert!(selector_matches_file_line(STORED, 8, "src/lib.rs:8"));
    }

    #[test]
    fn unrelated_file_does_not_match() {
        assert!(!selector_matches_file_line(STORED, 8, "src/other.rs:8"));
    }

    #[test]
    fn malformed_selector_does_not_match() {
        // No line component.
        assert!(!selector_matches_file_line(STORED, 8, "src/lib.rs"));
        // Empty file component.
        assert!(!selector_matches_file_line(STORED, 8, ":8"));
    }

    #[test]
    fn malformed_location_misses_get_syntax_guidance_without_classifying_ids() {
        for selector in [
            ":::",
            ":12",
            "src/lib.rs",
            "src/lib.rs:abc",
            "lib.rs:0",
            "src\\lib.rs:+2",
            "src/lib.rs:02",
        ] {
            assert!(location_selector_needs_syntax_hint(selector), "{selector}");
        }
        for selector in [
            "src/lib.rs:12",
            "lib.rs:12",
            "repo\\lib.rs:12",
            "path:with:colon.rs:12",
            "probe:src/lib.rs:predicate:missing",
            "repo-probe:src_lib.rs:error_path:3bf8c64c",
            "repo-probe:src_lib.rs:error_path:3bf8c64c.2",
            "missing-finding-id",
        ] {
            assert!(!location_selector_needs_syntax_hint(selector), "{selector}");
        }
    }

    #[test]
    fn normalize_strips_prefix_and_separators() {
        assert_eq!(normalize_locator_path(".\\a/b/c.rs"), "a/b/c.rs");
        assert_eq!(normalize_locator_path("./a/b/c.rs"), "a/b/c.rs");
        assert_eq!(normalize_locator_path("a\\b\\c.rs"), "a/b/c.rs");
        assert_eq!(normalize_locator_path("a/b/c.rs/"), "a/b/c.rs");
    }
}
