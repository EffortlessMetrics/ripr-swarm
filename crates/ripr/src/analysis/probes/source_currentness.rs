use super::super::diff::{ChangedFile, ChangedLine};
use super::super::rust_index::{RustIndex, find_owner_function};
use super::ids::normalize_expression;
use crate::domain::{
    FindingSourceIdentity, FindingSourceResolution, Probe, SourceCurrentness, SymbolId,
};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub(super) fn resolve_file_probes(
    root: &Path,
    changed: &ChangedFile,
    index: &RustIndex,
    probes: &mut [Probe],
) {
    let candidate_path = root.join(&changed.path);
    let candidate = match fs::read_to_string(&candidate_path) {
        Ok(text) => CandidateSource::Present(text),
        Err(error) if error.kind() == ErrorKind::NotFound => CandidateSource::Missing,
        Err(_) => CandidateSource::Unreadable,
    };

    for probe in probes {
        probe.location.source_resolution =
            resolve_probe(root, changed, index, probe, &candidate);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CandidateSource {
    Present(String),
    Missing,
    Unreadable,
}

fn resolve_probe(
    root: &Path,
    changed: &ChangedFile,
    index: &RustIndex,
    probe: &Probe,
    candidate: &CandidateSource,
) -> FindingSourceResolution {
    let base = base_identity(root, changed, probe);
    let candidate_path = root.join(&changed.path);
    let candidate_expression = probe
        .after
        .as_deref()
        .unwrap_or(probe.expression.as_str());

    match (&probe.after, candidate) {
        (Some(_), CandidateSource::Present(text)) => {
            let matches = candidate_matches(text, candidate_expression, changed, index);
            if let Some(current) = matches
                .iter()
                .find(|current| {
                    current.start_line == probe.location.line
                        && owner_is_consistent(probe.owner.as_ref(), current.owner.as_ref())
                })
                .cloned()
            {
                return FindingSourceResolution::candidate_current(
                    current.into_identity(candidate_path),
                    base,
                );
            }

            let matching = matches
                .into_iter()
                .filter(|current| {
                    owner_is_consistent(probe.owner.as_ref(), current.owner.as_ref())
                })
                .collect::<Vec<_>>();
            if matching.len() == 1 {
                return FindingSourceResolution::moved_or_renamed(
                    matching
                        .into_iter()
                        .next()
                        .map(|current| current.into_identity(candidate_path)),
                    base,
                );
            }

            FindingSourceResolution::unresolved(None, base)
        }
        (Some(_), CandidateSource::Missing | CandidateSource::Unreadable) => {
            FindingSourceResolution::unresolved(None, base)
        }
        (None, CandidateSource::Missing) => base
            .map(FindingSourceResolution::base_deleted)
            .unwrap_or_default(),
        (None, CandidateSource::Unreadable) => {
            FindingSourceResolution::unresolved(None, base)
        }
        (None, CandidateSource::Present(text)) => {
            let matching = candidate_matches(text, candidate_expression, changed, index)
                .into_iter()
                .filter(|current| {
                    owner_is_consistent(probe.owner.as_ref(), current.owner.as_ref())
                })
                .collect::<Vec<_>>();

            match matching.len() {
                0 => base
                    .map(FindingSourceResolution::base_deleted)
                    .unwrap_or_default(),
                1 => FindingSourceResolution::moved_or_renamed(
                    matching
                        .into_iter()
                        .next()
                        .map(|current| current.into_identity(candidate_path)),
                    base,
                ),
                _ => FindingSourceResolution::unresolved(None, base),
            }
        }
    }
}

fn base_identity(
    root: &Path,
    changed: &ChangedFile,
    probe: &Probe,
) -> Option<FindingSourceIdentity> {
    let before = probe.before.as_deref()?;
    let removed = matching_removed_line(changed, probe.location.line, before)?;
    let normalized = normalize_expression(before);
    if normalized.is_empty() {
        return None;
    }
    let end_line = removed
        .line
        .saturating_add(before.lines().count().saturating_sub(1));

    Some(FindingSourceIdentity::new(
        root.join(&changed.path),
        removed.line,
        end_line,
        normalized,
        probe.owner.clone(),
    ))
}

fn matching_removed_line<'a>(
    changed: &'a ChangedFile,
    candidate_line: usize,
    before: &str,
) -> Option<&'a ChangedLine> {
    let normalized = normalize_expression(before);
    let matching = changed
        .removed_lines
        .iter()
        .filter(|line| normalize_expression(line.text.trim()) == normalized)
        .collect::<Vec<_>>();
    let exact = matching
        .iter()
        .copied()
        .filter(|line| line.new_side_line == candidate_line)
        .collect::<Vec<_>>();

    if exact.len() == 1 {
        return exact.into_iter().next();
    }
    if matching.len() == 1 {
        return matching.into_iter().next();
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CandidateMatch {
    start_line: usize,
    end_line: usize,
    normalized_expression: String,
    owner: Option<SymbolId>,
}

impl CandidateMatch {
    fn into_identity(self, file: impl Into<std::path::PathBuf>) -> FindingSourceIdentity {
        FindingSourceIdentity::new(
            file,
            self.start_line,
            self.end_line,
            self.normalized_expression,
            self.owner,
        )
    }
}

fn candidate_matches(
    candidate: &str,
    expression: &str,
    changed: &ChangedFile,
    index: &RustIndex,
) -> Vec<CandidateMatch> {
    let normalized = normalize_expression(expression);
    if normalized.is_empty() {
        return Vec::new();
    }

    let lines = candidate.lines().collect::<Vec<_>>();
    let window_len = expression.lines().count().max(1);
    if window_len > lines.len() {
        return Vec::new();
    }

    lines
        .windows(window_len)
        .enumerate()
        .filter_map(|(offset, window)| {
            let candidate_expression = normalize_expression(&window.join("\n"));
            if candidate_expression != normalized {
                return None;
            }
            let start_line = offset + 1;
            let end_line = start_line + window_len - 1;
            let owner = find_owner_function(index, &changed.path, start_line)
                .map(|function| function.id.clone());
            Some(CandidateMatch {
                start_line,
                end_line,
                normalized_expression: candidate_expression,
                owner,
            })
        })
        .collect()
}

fn owner_is_consistent(expected: Option<&SymbolId>, actual: Option<&SymbolId>) -> bool {
    expected.is_none() || expected == actual
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        DeltaKind, ProbeFamily, ProbeId, SourceLocation,
    };
    use std::path::PathBuf;

    fn changed(
        added: Vec<(usize, usize, &str)>,
        removed: Vec<(usize, usize, &str)>,
    ) -> ChangedFile {
        ChangedFile {
            path: PathBuf::from("src/lib.rs"),
            added_lines: added
                .into_iter()
                .map(|(line, new_side_line, text)| ChangedLine {
                    line,
                    new_side_line,
                    text: text.to_string(),
                })
                .collect(),
            removed_lines: removed
                .into_iter()
                .map(|(line, new_side_line, text)| ChangedLine {
                    line,
                    new_side_line,
                    text: text.to_string(),
                })
                .collect(),
        }
    }

    fn probe(
        line: usize,
        expression: &str,
        before: Option<&str>,
        after: Option<&str>,
    ) -> Probe {
        Probe {
            id: ProbeId(format!("probe:src_lib.rs:{line}")),
            location: SourceLocation::new("workspace/src/lib.rs", line, 1),
            owner: None,
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: before.map(str::to_string),
            after: after.map(str::to_string),
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    #[test]
    fn modified_candidate_expression_is_candidate_current() {
        let changed = changed(
            vec![(3, 3, "return total + tax;")],
            vec![(3, 3, "return total;")],
        );
        let probe = probe(
            3,
            "return total + tax;",
            Some("return total;"),
            Some("return total + tax;"),
        );

        let resolution = resolve_probe(
            Path::new("workspace"),
            &changed,
            &RustIndex::default(),
            &probe,
            &CandidateSource::Present(
                "fn total() {\n    let tax = 1;\nreturn total + tax;\n}".to_string(),
            ),
        );

        assert_eq!(
            resolution.currentness,
            SourceCurrentness::CandidateCurrent
        );
        assert_eq!(resolution.candidate.as_ref().map(|source| source.start_line), Some(3));
        assert_eq!(resolution.base.as_ref().map(|source| source.start_line), Some(3));
    }

    #[test]
    fn deleted_tail_retains_old_line_without_candidate_target() {
        let changed = changed(Vec::new(), vec![(29, 13, "return legacy;")]);
        let probe = probe(13, "return legacy;", Some("return legacy;"), None);

        let resolution = resolve_probe(
            Path::new("workspace"),
            &changed,
            &RustIndex::default(),
            &probe,
            &CandidateSource::Present("fn current() {\n    return current;\n}".to_string()),
        );

        assert_eq!(resolution.currentness, SourceCurrentness::BaseDeleted);
        assert!(resolution.candidate.is_none());
        assert_eq!(resolution.base.as_ref().map(|source| source.start_line), Some(29));
    }

    #[test]
    fn whole_file_delete_is_base_deleted() {
        let changed = changed(Vec::new(), vec![(1, 1, "fn removed() {}")]);
        let probe = probe(1, "fn removed() {}", Some("fn removed() {}"), None);

        let resolution = resolve_probe(
            Path::new("workspace"),
            &changed,
            &RustIndex::default(),
            &probe,
            &CandidateSource::Missing,
        );

        assert_eq!(resolution.currentness, SourceCurrentness::BaseDeleted);
        assert!(resolution.candidate.is_none());
        assert_eq!(resolution.base.as_ref().map(|source| source.start_line), Some(1));
    }

    #[test]
    fn reused_coordinate_with_different_expression_does_not_become_candidate_current() {
        let changed = changed(Vec::new(), vec![(29, 3, "return legacy;")]);
        let probe = probe(3, "return legacy;", Some("return legacy;"), None);

        let resolution = resolve_probe(
            Path::new("workspace"),
            &changed,
            &RustIndex::default(),
            &probe,
            &CandidateSource::Present("fn current() {\n    let value = 1;\nreturn current;\n}".to_string()),
        );

        assert_eq!(resolution.currentness, SourceCurrentness::BaseDeleted);
        assert!(resolution.candidate.is_none());
    }

    #[test]
    fn unique_candidate_match_at_a_new_range_is_moved() {
        let changed = changed(Vec::new(), vec![(2, 2, "return value;")]);
        let probe = probe(2, "return value;", Some("return value;"), None);

        let resolution = resolve_probe(
            Path::new("workspace"),
            &changed,
            &RustIndex::default(),
            &probe,
            &CandidateSource::Present(
                "fn current() {\n    let value = 1;\n    value += 1;\n    value += 1;\nreturn value;\n}"
                    .to_string(),
            ),
        );

        assert_eq!(
            resolution.currentness,
            SourceCurrentness::MovedOrRenamed
        );
        assert_eq!(resolution.candidate.as_ref().map(|source| source.start_line), Some(5));
        assert_eq!(resolution.base.as_ref().map(|source| source.start_line), Some(2));
    }

    #[test]
    fn ambiguous_repeated_candidate_expression_is_unresolved() {
        let changed = changed(Vec::new(), vec![(2, 2, "return value;")]);
        let probe = probe(2, "return value;", Some("return value;"), None);

        let resolution = resolve_probe(
            Path::new("workspace"),
            &changed,
            &RustIndex::default(),
            &probe,
            &CandidateSource::Present(
                "fn one() {\nreturn value;\n}\nfn two() {\nreturn value;\n}".to_string(),
            ),
        );

        assert_eq!(
            resolution.currentness,
            SourceCurrentness::UnresolvedSubject
        );
        assert!(resolution.candidate.is_none());
        assert!(resolution.base.is_some());
    }

    #[test]
    fn mixed_diff_keeps_candidate_and_deleted_resolutions_separate() {
        let changed = changed(
            vec![(2, 2, "return current;")],
            vec![(8, 3, "return legacy;")],
        );
        let mut probes = vec![
            probe(2, "return current;", None, Some("return current;")),
            probe(3, "return legacy;", Some("return legacy;"), None),
        ];
        let candidate = CandidateSource::Present(
            "fn current() {\nreturn current;\n}".to_string(),
        );

        for probe in &mut probes {
            probe.location.source_resolution = resolve_probe(
                Path::new("workspace"),
                &changed,
                &RustIndex::default(),
                probe,
                &candidate,
            );
        }

        assert_eq!(
            probes[0].location.source_resolution.currentness,
            SourceCurrentness::CandidateCurrent
        );
        assert_eq!(
            probes[1].location.source_resolution.currentness,
            SourceCurrentness::BaseDeleted
        );
    }
}
