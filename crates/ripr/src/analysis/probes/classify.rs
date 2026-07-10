use super::super::rust_index::RustIndex;
use super::family::family_for_probe_shape;
use crate::domain::ProbeFamily;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedSyntaxProbe {
    pub family: ProbeFamily,
    pub expression: String,
}

pub fn classify_changed_syntax(
    index: &RustIndex,
    file: &Path,
    line: usize,
    changed_text: &str,
) -> Option<Vec<ClassifiedSyntaxProbe>> {
    let facts = index.files.get(file)?;
    let mut families = facts
        .probe_shapes
        .iter()
        .filter(|shape| {
            shape.start_line <= line
                && line <= shape.end_line
                && shape_contains_changed_text(&shape.text, changed_text)
        })
        .filter_map(|shape| {
            family_for_probe_shape(&shape.kind).map(|family| ClassifiedSyntaxProbe {
                family,
                expression: shape.text.clone(),
            })
        })
        .collect::<Vec<_>>();
    if families.is_empty() {
        return None;
    }
    families.sort_by(|a, b| {
        a.family
            .as_str()
            .cmp(b.family.as_str())
            .then(a.expression.len().cmp(&b.expression.len()))
            .then(a.expression.cmp(&b.expression))
    });
    families.dedup_by(|a, b| a.family == b.family);
    Some(families)
}

fn shape_contains_changed_text(shape_text: &str, changed_text: &str) -> bool {
    let changed = changed_text
        .trim()
        .trim_end_matches(';')
        .trim_end_matches(',');
    if changed.is_empty() {
        return false;
    }
    let shape = shape_text.trim();
    shape.contains(changed) || changed.contains(shape)
}

pub fn should_ignore_changed_line(text: &str) -> bool {
    text.is_empty()
        || text.starts_with("//")
        || text.starts_with("use ")
        || text.starts_with("pub use ")
        || text.starts_with("mod ")
        || text.starts_with("#")
}

#[cfg(test)]
mod tests {
    use super::super::super::rust_index::{
        FileFacts, PROBE_SHAPE_ERROR_PATH, PROBE_SHAPE_PREDICATE, ProbeShapeFact, RustIndex,
    };
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn classify_functions_are_callable() {
        assert!(should_ignore_changed_line("// comment"));
        assert!(!should_ignore_changed_line("let x = 5;"));
    }

    #[test]
    fn classify_changed_syntax_uses_matching_probe_shape() {
        let path = PathBuf::from("src/lib.rs");
        let index = RustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    probe_shapes: vec![
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: 0,
                            kind: PROBE_SHAPE_PREDICATE.to_string(),
                            text: "if amount >= threshold {".to_string(),
                        },
                        ProbeShapeFact {
                            start_line: 7,
                            end_line: 7,
                            start_byte: 20,
                            kind: PROBE_SHAPE_ERROR_PATH.to_string(),
                            text: "Err(AuthError::Revoked)".to_string(),
                        },
                    ],
                    ..FileFacts::default()
                },
            )]),
            ..RustIndex::default()
        };

        let families = classify_changed_syntax(&index, &path, 3, "amount >= threshold;");
        assert_eq!(
            families,
            Some(vec![ClassifiedSyntaxProbe {
                family: ProbeFamily::Predicate,
                expression: "if amount >= threshold {".to_string(),
            }])
        );
    }

    #[test]
    fn classify_changed_syntax_returns_none_without_matching_shape() {
        let path = PathBuf::from("src/lib.rs");
        let index = RustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    probe_shapes: vec![ProbeShapeFact {
                        start_line: 3,
                        end_line: 3,
                        start_byte: 0,
                        kind: PROBE_SHAPE_PREDICATE.to_string(),
                        text: "if amount >= threshold {".to_string(),
                    }],
                    ..FileFacts::default()
                },
            )]),
            ..RustIndex::default()
        };

        let families = classify_changed_syntax(&index, &path, 4, "return total");
        assert_eq!(families, None);
    }

    #[test]
    fn classify_changed_syntax_keeps_one_most_specific_shape_per_family() -> Result<(), String> {
        let path = PathBuf::from("src/lib.rs");
        let index = RustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    probe_shapes: vec![
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: 20,
                            kind: crate::analysis::rust_index::PROBE_SHAPE_CALL_DELETION
                                .to_string(),
                            text: "inner()".to_string(),
                        },
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: 14,
                            kind: crate::analysis::rust_index::PROBE_SHAPE_CALL_DELETION
                                .to_string(),
                            text: "outer(inner())".to_string(),
                        },
                    ],
                    ..FileFacts::default()
                },
            )]),
            ..RustIndex::default()
        };

        let classified = classify_changed_syntax(&index, &path, 3, "inner()")
            .ok_or_else(|| "expected nested call classification".to_string())?;
        if classified
            != vec![ClassifiedSyntaxProbe {
                family: ProbeFamily::CallDeletion,
                expression: "inner()".to_string(),
            }]
        {
            return Err(format!(
                "unexpected nested call classification: {classified:?}"
            ));
        }
        Ok(())
    }
}
