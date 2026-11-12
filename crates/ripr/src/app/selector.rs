use crate::domain::Finding;

/// Outcome of resolving one selector against the finding set (#8166 Slice B,
/// #8689).
///
/// Selection is fail-closed: a selector that names several findings is
/// `Ambiguous`, never a silent first match, and a selector that names none
/// is `NoMatch`. A lookup that could not check — an unreadable or
/// truncated seam inventory — is `Undecided`, never a clean miss. All
/// refusals are usage-class at the CLI (prose-only, exit 2), matching the
/// existing selector-miss contract.
pub(in crate::app) enum FindingSelection<'a> {
    Unique(&'a Finding),
    NoMatch,
    /// The lookup could not establish a match or a miss (#8689): the seam
    /// inventory the reverse lookup needs was unreadable or budget-
    /// truncated. Rendering must name the limitation instead of claiming a
    /// clean miss the lookup did not establish.
    Undecided(SelectorLimitation),
    Ambiguous(Vec<&'a Finding>),
}

/// Why a seam-reverse lookup could not decide (#8689). Both arms mean the
/// lookup checked nothing, so the refusal must disclose the state rather
/// than render a plain miss.
pub(in crate::app) enum SelectorLimitation {
    /// The seam inventory could not be read at all.
    InventoryUnreadable,
    /// The seam inventory was budget-truncated: `analyzed` of `total` seams.
    InventoryTruncated { analyzed: usize, total: usize },
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

/// Whether `selector` has the shape of the seam-derived hash-form canonical
/// gap ID `ripr rerun --json` and `ripr repo-exposure` JSON emit:
/// `gap:` followed by exactly 16 lowercase hex characters (#8688). The
/// producer form `ripr check` prints (`gap:rust:...`) is longer and names a
/// colon-separated body, so this shape check cannot confuse the two.
pub(in crate::app) fn is_gap_hash_selector(selector: &str) -> bool {
    let Some(hash) = selector.strip_prefix("gap:") else {
        return false;
    };
    hash.len() == 16
        && hash
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

/// Resolve the seam-derived hash-form canonical gap ID (`gap:<16hex>`,
/// #8688) against the finding set through the same seam-to-finding binding
/// as the seam-ID reverse lookup. The hash form is minted from the seam's
/// owner, kind, flow sink, missing discriminator and assertion shape, so it
/// is read from the seam side: every headline-eligible seam whose
/// [`canonical_gap_identity`] recomputes to the selector is bound back to
/// its finding. Several seams can group under one gap; distinct bound
/// findings make the selector ambiguous, never a silent first match, and a
/// gap no seam carries is `NoMatch`.
pub(in crate::app) fn select_finding_for_gap_hash<'a>(
    findings: &'a [Finding],
    classified: &[crate::analysis::ClassifiedSeam],
    selector: &str,
) -> FindingSelection<'a> {
    let mut bound: Vec<&Finding> = Vec::new();
    for entry in classified.iter().filter(|entry| {
        crate::analysis::canonical_gap::canonical_gap_identity(entry)
            .is_some_and(|identity| identity.id == selector)
    }) {
        let canonical_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(entry)
            .map(|identity| identity.id);
        let Some(finding) = super::repair_card_handoff::finding_for_seam(
            findings,
            entry,
            canonical_gap_id.as_deref(),
            classified,
        ) else {
            continue;
        };
        if !bound.contains(&finding) {
            bound.push(finding);
        }
    }
    match bound.len() {
        0 => FindingSelection::NoMatch,
        1 => FindingSelection::Unique(bound[0]),
        _ => FindingSelection::Ambiguous(bound),
    }
}

/// [`select_finding`] plus the seam-side reverse lookups for a fresh run
/// (#8166 residual, #8688): a direct hit keeps the existing result; only a
/// miss shaped like a seam ID or a hash-form canonical gap ID inventories
/// the root's classified seams and binds the named identity back to its
/// finding. An unreadable or truncated inventory is `Undecided` rather than
/// a miss (#8689): the lookup checked nothing, so claiming "no finding
/// matched" would overstate what was established.
pub(in crate::app) fn select_finding_with_seam_reverse<'a>(
    findings: &'a [Finding],
    root: &std::path::Path,
    config: &crate::config::RiprConfig,
    selector: &str,
) -> FindingSelection<'a> {
    let direct = select_finding(findings, selector);
    if !matches!(direct, FindingSelection::NoMatch) {
        return direct;
    }
    let seam_lookup = if is_seam_id_selector(selector) {
        true
    } else if is_gap_hash_selector(selector) {
        false
    } else {
        return direct;
    };
    let (classified, limit_info) = match super::task_first::inventory_for_repair(root, config) {
        Ok(inventory) => inventory,
        Err(_error) => {
            return FindingSelection::Undecided(SelectorLimitation::InventoryUnreadable);
        }
    };
    if let Some(info) = &limit_info
        && info.is_truncated()
    {
        return FindingSelection::Undecided(SelectorLimitation::InventoryTruncated {
            analyzed: info.analyzed,
            total: info.total,
        });
    }
    if seam_lookup {
        select_finding_for_seam(findings, &classified, selector)
    } else {
        select_finding_for_gap_hash(findings, &classified, selector)
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

/// Where the findings of this scope are, for a `file:line` selector that
/// matched none (#14384): the lines that do hold findings in the named file,
/// or, when the file holds none, the files that do. `None` for a selector
/// that is not location-shaped. The listing comes from the analyzed findings
/// only; the typed selector is never echoed into a command.
/// `analysis_incomplete` is set by a fresh run whose producer outcome or
/// language run did not complete or left files unanalyzed (#14406): an empty scope or a file
/// with no finding is then not evidence that none exist there.
pub(in crate::app) fn location_miss_hint(
    findings: &[Finding],
    root: &std::path::Path,
    selector: &str,
    analysis_incomplete: bool,
) -> Option<String> {
    const MAX_LISTED: usize = 5;
    if ["probe:", "repo-probe:", "gap:", "seam:"]
        .iter()
        .any(|prefix| selector.starts_with(prefix))
    {
        return None;
    }
    let (selector_file, selector_line) = selector.rsplit_once(':')?;
    if selector_line.parse::<usize>().is_err() {
        return None;
    }
    // `file:line:column` splits as file `file:line`; matching is per line.
    let (selector_file, column_given) = match selector_file.rsplit_once(':') {
        Some((file, line)) if !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()) => {
            (file, true)
        }
        _ => (selector_file, false),
    };
    let wanted = normalize_locator_path(selector_file);
    if wanted.is_empty() || !(wanted.contains(['/', '.'])) {
        return None;
    }
    let column_note = if column_given {
        " Selectors are file:line; drop the column."
    } else {
        ""
    };
    let incomplete_note = if analysis_incomplete {
        " Analysis of this scope was incomplete, so a finding there may be missing."
    } else {
        ""
    };
    if findings.is_empty() {
        if analysis_incomplete {
            return Some(format!(
                " No findings were reported in this scope.{incomplete_note}{column_note}"
            ));
        }
        return Some(format!(
            " This scope has no findings, so no location can match.{column_note}"
        ));
    }
    // Keyed by file so a suffix selector (`lib.rs`) that matches several
    // files never mixes their lines under one name.
    let mut same_file =
        std::collections::BTreeMap::<String, std::collections::BTreeSet<usize>>::new();
    let mut other_files = std::collections::BTreeSet::<String>::new();
    for finding in findings {
        let file = crate::analysis::stable_path_text(&finding.probe.location.file);
        let stored = normalize_locator_path(file.as_ref());
        let matches = stored == wanted
            || stored.ends_with(&format!("/{wanted}"))
            || wanted.ends_with(&format!("/{stored}"));
        // Shown root-relative through the shared location projector: a
        // fresh run stores absolute paths (verbatim `\\?\` ones on Windows),
        // and the suffix match accepts the relative form the user types.
        let stored = normalize_locator_path(&crate::analysis::finding_location_text(
            root,
            &finding.probe.location.file,
        ));
        if matches {
            same_file
                .entry(stored)
                .or_default()
                .insert(finding.probe.location.line);
        } else {
            other_files.insert(stored);
        }
    }
    // An exact path match leads; otherwise the first matching file.
    if let Some((file, lines)) = same_file
        .get_key_value(&wanted)
        .or_else(|| same_file.iter().next())
    {
        // Suggest only a selector that resolves to exactly one finding: a
        // line shared by several findings (or by another matching file)
        // would refuse as ambiguous on retry.
        let suggestion = lines
            .iter()
            .map(|line| format!("{file}:{line}"))
            .find(|candidate| {
                matches!(
                    select_finding(findings, candidate),
                    FindingSelection::Unique(_)
                )
            });
        let suggestion = match suggestion {
            Some(candidate) => format!("select one as {candidate}"),
            None => "every listed line holds several findings, so select one by its probe id"
                .to_string(),
        };
        let listed = lines
            .iter()
            .take(MAX_LISTED)
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let more = lines.len().saturating_sub(MAX_LISTED);
        let more = if more > 0 {
            format!(" (and {more} more)")
        } else {
            String::new()
        };
        let noun = if lines.len() == 1 { "line" } else { "lines" };
        let others = match same_file.len().saturating_sub(1) {
            0 => String::new(),
            1 => " 1 other file matching that path also has findings.".to_string(),
            n => format!(" {n} other files matching that path also have findings."),
        };
        return Some(format!(
            " Findings in {file} are on {noun} {listed}{more}; {suggestion}.{others}{column_note}"
        ));
    }
    let files = other_files
        .iter()
        .take(MAX_LISTED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let more = other_files.len().saturating_sub(MAX_LISTED);
    let more = if more > 0 {
        format!(" (and {more} more)")
    } else {
        String::new()
    };
    if analysis_incomplete {
        return Some(format!(
            " No finding was reported in {wanted:?}; findings are in {files}{more}.{incomplete_note}{column_note}"
        ));
    }
    Some(format!(
        " No finding in this scope is in {wanted:?}; findings are in {files}{more}.{column_note}"
    ))
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
        FindingSelection, location_miss_hint, location_selector_needs_syntax_hint,
        normalize_locator_path, select_finding, selector_matches_file_line,
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
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
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

    /// #8688: the seam-derived hash-form canonical gap ID (`gap:<16hex>`)
    /// that `ripr rerun --json` and repo-exposure JSON carry resolves
    /// through the seam binding to the same finding the seam ID resolves
    /// to; a hash no seam carries stays `NoMatch`, and the producer form
    /// `ripr check` prints is not mistaken for the hash shape.
    #[test]
    fn gap_hash_selector_resolves_through_the_seam_binding() {
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
        let identity = canonical_gap_identity(&entry);
        assert!(identity.is_some(), "eligible seam must have a gap identity");
        let Some(identity) = identity else {
            return;
        };
        assert!(
            super::is_gap_hash_selector(&identity.id),
            "the minted id is the hash form: {}",
            identity.id
        );
        assert!(!super::is_gap_hash_selector(
            "gap:rust:src/lib.rs:add:predicate_boundary:predicate:x==y"
        ));
        assert!(!super::is_gap_hash_selector("gap:rust"));
        assert!(!super::is_gap_hash_selector("0123456789abcdef"));
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
                super::select_finding_for_gap_hash(&findings, &classified, &identity.id),
                FindingSelection::Unique(selected) if selected.id == findings[0].id
            ),
            "the hash-form gap ID must resolve to its bound finding"
        );
        // A hash no seam carries names nothing (clean miss is established).
        assert!(matches!(
            super::select_finding_for_gap_hash(&findings, &classified, "gap:0000000000000000"),
            FindingSelection::NoMatch
        ));
    }

    /// #14384: a location miss names where the findings are instead of
    /// only pointing at a separate listing run.
    #[test]
    fn location_miss_hint_names_lines_files_and_the_column_form() {
        let findings = vec![
            finding("probe:a", 3, None),
            finding("probe:b", 3, None),
            finding("probe:c", 9, None),
        ];
        assert_eq!(
            location_miss_hint(
                &findings,
                std::path::Path::new(""),
                "src/lib.rs:9999",
                false
            )
            .as_deref(),
            Some(" Findings in src/lib.rs are on lines 3, 9; select one as src/lib.rs:9.")
        );
        assert_eq!(
            location_miss_hint(
                &findings,
                std::path::Path::new(""),
                "./src\\lib.rs:1:2",
                false
            )
            .as_deref(),
            Some(
                " Findings in src/lib.rs are on lines 3, 9; select one as src/lib.rs:9. Selectors are file:line; drop the column."
            )
        );
        assert_eq!(
            location_miss_hint(&findings, std::path::Path::new(""), "nofile.rs:1", false)
                .as_deref(),
            Some(" No finding in this scope is in \"nofile.rs\"; findings are in src/lib.rs.")
        );
        assert_eq!(
            location_miss_hint(&[], std::path::Path::new(""), "src/lib.rs:1", false).as_deref(),
            Some(" This scope has no findings, so no location can match.")
        );
        // A suffix that matches several files never mixes their lines: the
        // first file's lines are listed and the rest are counted.
        let mut other = finding("probe:d", 2, None);
        other.probe.location = SourceLocation::new("crates/b/src/lib.rs", 2, 1);
        let mut third = finding("probe:e", 7, None);
        third.probe.location = SourceLocation::new("crates/a/src/lib.rs", 7, 1);
        let spread = vec![finding("probe:a", 3, None), other, third];
        assert_eq!(
            location_miss_hint(&spread, std::path::Path::new(""), "lib.rs:99", false).as_deref(),
            Some(
                " Findings in crates/a/src/lib.rs are on line 7; select one as crates/a/src/lib.rs:7. 2 other files matching that path also have findings."
            )
        );
        assert_eq!(
            location_miss_hint(&spread, std::path::Path::new(""), "src/lib.rs:99", false)
                .as_deref(),
            Some(
                " Findings in src/lib.rs are on line 3; select one as src/lib.rs:3. 2 other files matching that path also have findings."
            )
        );
        // A fresh run stores absolute paths; the hint shows them
        // root-relative, which the suffix match resolves.
        let mut absolute = finding("probe:f", 4, None);
        absolute.probe.location = SourceLocation::new("/work/repo/src/lib.rs", 4, 1);
        assert_eq!(
            location_miss_hint(
                &[absolute],
                std::path::Path::new("/work/repo"),
                "src/lib.rs:1",
                false
            )
            .as_deref(),
            Some(" Findings in src/lib.rs are on line 4; select one as src/lib.rs:4.")
        );
        // #14406: an incomplete fresh run never reads as an all-clear for
        // an empty scope or a file with no reported finding.
        assert_eq!(
            location_miss_hint(&[], std::path::Path::new(""), "src/lib.rs:1", true).as_deref(),
            Some(
                " No findings were reported in this scope. Analysis of this scope was incomplete, so a finding there may be missing."
            )
        );
        assert_eq!(
            location_miss_hint(&findings, std::path::Path::new(""), "nofile.rs:1:4", true)
                .as_deref(),
            Some(
                " No finding was reported in \"nofile.rs\"; findings are in src/lib.rs. Analysis of this scope was incomplete, so a finding there may be missing. Selectors are file:line; drop the column."
            )
        );
        // A file with findings lists them the same way either way.
        assert_eq!(
            location_miss_hint(&findings, std::path::Path::new(""), "src/lib.rs:9999", true),
            location_miss_hint(
                &findings,
                std::path::Path::new(""),
                "src/lib.rs:9999",
                false
            )
        );
        // A Windows verbatim finding path under a plain-drive root is shown
        // root-relative, like every other rendered location.
        #[cfg(windows)]
        {
            let drive = "F:";
            let root = std::path::PathBuf::from(format!(r"{drive}\repo"));
            let mut verbatim = finding("probe:w", 8, None);
            verbatim.probe.location =
                SourceLocation::new(format!(r"\\?\{drive}\repo\src\lib.rs"), 8, 1);
            assert_eq!(
                location_miss_hint(&[verbatim], &root, "src/lib.rs:99", false).as_deref(),
                Some(" Findings in src/lib.rs are on line 8; select one as src/lib.rs:8.")
            );
        }
        // Line 3 holds two findings, so `src/lib.rs:3` would refuse as
        // ambiguous; the suggestion is the line that resolves uniquely.
        assert!(matches!(
            select_finding(&findings, "src/lib.rs:3"),
            FindingSelection::Ambiguous(_)
        ));
        assert!(matches!(
            select_finding(&findings, "src/lib.rs:9"),
            FindingSelection::Unique(_)
        ));
        // When every line is shared, no location selector is suggested.
        let shared = vec![finding("probe:a", 3, None), finding("probe:b", 3, None)];
        assert_eq!(
            location_miss_hint(&shared, std::path::Path::new(""), "src/lib.rs:9999", false)
                .as_deref(),
            Some(
                " Findings in src/lib.rs are on line 3; every listed line holds several findings, so select one by its probe id."
            )
        );
        // Not location-shaped: ids and bare words get no location hint.
        for selector in [
            "probe:missing",
            "probe:src/lib.rs:3",
            "gap:rust:x",
            "nocolon",
            "src/lib.rs:abc",
            // Path guard boundaries: no `/` or `.`, and an empty file part.
            "rust:12",
            ":12",
            "./:12",
        ] {
            assert_eq!(
                location_miss_hint(&findings, std::path::Path::new(""), selector, false),
                None,
                "{selector}"
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
