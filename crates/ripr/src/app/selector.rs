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
fn selector_matches_file_line(stored_file: &str, stored_line: usize, selector: &str) -> bool {
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
