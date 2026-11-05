//! Admitted SCIP reference-to-definition bindings for relation consumers (#8116).

use super::admission::{AdmittedScipSnapshot, ScipAdmissionError, admit_scip_snapshot};
use super::generated::scip::{Index, Occurrence, PositionEncoding, SymbolRole};
#[cfg(test)]
use prost::Message;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Read-only lookup built from one admitted SCIP index (#8116).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScipReferenceBindings {
    project_root_relative: String,
    /// SCIP definition symbols keyed by workspace-relative path and terminal name.
    owner_definition_symbols: BTreeMap<(String, String), Vec<String>>,
    /// Reference occurrences keyed by `(path, line, start_character, end_character)`.
    reference_symbols: BTreeMap<(String, i32, i32, i32), Vec<String>>,
    position_encodings: BTreeMap<String, i32>,
}

impl ScipReferenceBindings {
    pub(crate) fn from_admitted_index<'a>(
        workspace_root: &Path,
        index_path: &Path,
        manifest_path: &Path,
        consumed: &crate::analysis::consumed_source::ConsumedRustSources,
        retained_sources: impl IntoIterator<Item = (&'a Path, &'a str)>,
    ) -> Result<Self, ScipAdmissionError> {
        Self::from_admitted_snapshot(
            admit_scip_snapshot(workspace_root, index_path, manifest_path)?,
            consumed,
            retained_sources,
        )
    }

    fn from_admitted_snapshot<'a>(
        snapshot: AdmittedScipSnapshot,
        consumed: &crate::analysis::consumed_source::ConsumedRustSources,
        retained_sources: impl IntoIterator<Item = (&'a Path, &'a str)>,
    ) -> Result<Self, ScipAdmissionError> {
        let retained_sources = retained_sources.into_iter().collect::<Vec<_>>();
        let (index, project_root_relative) =
            snapshot.into_index_for_consumed_sources(consumed, retained_sources.iter().copied())?;
        let mut bindings = Self::from_index(&project_root_relative, &index);
        // A terminal SCIP name cannot identify one of several same-file Rust
        // owners. Count the retained declarations, including inactive cfg items,
        // rather than trusting the supplied index to enumerate every owner.
        let counts = retained_sources
            .into_iter()
            .map(|(path, source)| {
                let counts =
                    crate::analysis::syntax::parse_clean_source_file(source).map(|parsed| {
                        let mut counts = BTreeMap::<String, usize>::new();
                        for function in parsed
                            .tree()
                            .syntax()
                            .descendants()
                            .filter_map(ast::Fn::cast)
                        {
                            if let Some(name) = function.name() {
                                *counts
                                    .entry(name.text().trim_start_matches("r#").to_string())
                                    .or_default() += 1;
                            }
                        }
                        counts
                    });
                (path.to_path_buf(), counts)
            })
            .collect::<BTreeMap<_, _>>();
        bindings.owner_definition_symbols.retain(|(path, name), _| {
            let relative = Path::new(&project_root_relative)
                .join(path)
                .components()
                .filter(|part| !matches!(part, std::path::Component::CurDir))
                .collect::<PathBuf>();
            counts
                .get(&relative)
                .and_then(Option::as_ref)
                .and_then(|counts| counts.get(name))
                == Some(&1)
        });
        Ok(bindings)
    }

    fn from_index(project_root_relative: &str, index: &Index) -> Self {
        let mut owner_definition_symbols: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut reference_symbols: BTreeMap<(String, i32, i32, i32), Vec<String>> = BTreeMap::new();
        let mut position_encodings = BTreeMap::new();
        for document in &index.documents {
            let relative_path = normalize_relative_path(&document.relative_path);
            position_encodings.insert(relative_path.clone(), document.position_encoding);
            for occurrence in &document.occurrences {
                if occurrence.symbol.is_empty() {
                    continue;
                }
                let Some((line, start)) = occurrence_start(occurrence) else {
                    continue;
                };
                if is_definition(occurrence) {
                    if let Some(name) = simple_symbol_name(&occurrence.symbol) {
                        owner_definition_symbols
                            .entry((relative_path.clone(), name))
                            .or_default()
                            .push(occurrence.symbol.clone());
                    }
                } else if is_reference(occurrence) {
                    let end = match occurrence.range.as_slice() {
                        [_, _, end] => *end,
                        [_, _, end_line, end] if *end_line == line => *end,
                        _ => continue,
                    };
                    reference_symbols
                        .entry((relative_path.clone(), line, start, end))
                        .or_default()
                        .push(occurrence.symbol.clone());
                }
            }
        }
        Self {
            project_root_relative: project_root_relative.to_string(),
            owner_definition_symbols,
            reference_symbols,
            position_encodings,
        }
    }

    /// Whether a bare call to `callee` in `test_file` at `call_line` resolves
    /// through admitted SCIP to the owner's definition symbol.
    #[expect(
        clippy::too_many_arguments,
        reason = "policy:8116-scip-owner-call-site: call-site tuple matches related-test admit inputs"
    )]
    pub(crate) fn call_resolves_to_owner(
        &self,
        workspace_root: &Path,
        test_file: &Path,
        call_line: usize,
        call_source_line: &str,
        callee: &str,
        owner_file: &Path,
        owner_name: &str,
        same_line_same_name_calls: usize,
    ) -> bool {
        if same_line_same_name_calls != 1 {
            return false;
        }
        let Some(owner_symbol) =
            self.owner_definition_symbol(workspace_root, owner_file, owner_name)
        else {
            return false;
        };
        let Some(test_path) = self.relative_path(workspace_root, test_file) else {
            return false;
        };
        let Some(line) = call_line
            .checked_sub(1)
            .and_then(|line| i32::try_from(line).ok())
        else {
            return false;
        };
        let Some((call_start, call_end)) = unique_callee_span(call_source_line, callee) else {
            return false;
        };
        let encoding = self
            .position_encodings
            .get(&test_path)
            .copied()
            .unwrap_or_default();
        let mut at_call =
            self.reference_symbols
                .iter()
                .filter(|((path, reference_line, start, end), _)| {
                    path == &test_path
                        && *reference_line == line
                        && character_to_byte(call_source_line, *start, encoding) == Some(call_start)
                        && character_to_byte(call_source_line, *end, encoding) == Some(call_end)
                });
        matches!((at_call.next(), at_call.next()), (Some((_, symbols)), None)
            if symbols.len() == 1 && symbols[0] == owner_symbol)
    }

    fn owner_definition_symbol(
        &self,
        workspace_root: &Path,
        owner_file: &Path,
        owner_name: &str,
    ) -> Option<String> {
        let path = self.relative_path(workspace_root, owner_file)?;
        let symbols = self
            .owner_definition_symbols
            .get(&(path, owner_name.to_string()))?;
        match symbols.len() {
            1 => Some(symbols[0].clone()),
            _ => None,
        }
    }

    fn relative_path(&self, workspace_root: &Path, file: &Path) -> Option<String> {
        let workspace = workspace_root.canonicalize().ok()?;
        let project_root = workspace.join(&self.project_root_relative);
        let project_root = project_root.canonicalize().ok()?;
        let file = if file.is_absolute() {
            file.to_path_buf()
        } else {
            workspace.join(file)
        };
        let file = file.canonicalize().ok()?;
        file.strip_prefix(&project_root)
            .ok()
            .map(|relative| normalize_relative_path(relative.to_string_lossy().as_ref()))
    }
}

/// Call facts deliberately deduplicate names on a source line. Re-establish
/// one actual callee token before joining the occurrence, including in macros.
fn unique_callee_span(source_line: &str, callee: &str) -> Option<(usize, usize)> {
    let masked = crate::analysis::extract::mask_comments_and_strings(source_line);
    let mut calls = masked.match_indices(callee).filter_map(|(start, _)| {
        let end = start + callee.len();
        let before = masked[..start].chars().next_back();
        if before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            || !masked[end..].trim_start().starts_with('(')
        {
            return None;
        }
        Some((start, end))
    });
    match (calls.next(), calls.next()) {
        (Some(span), None) => Some(span),
        _ => None,
    }
}

fn character_to_byte(line: &str, character: i32, encoding: i32) -> Option<usize> {
    let character = usize::try_from(character).ok()?;
    match PositionEncoding::try_from(encoding).ok()? {
        PositionEncoding::Utf8CodeUnitOffsetFromLineStart => {
            line.is_char_boundary(character).then_some(character)
        }
        PositionEncoding::Utf16CodeUnitOffsetFromLineStart
        | PositionEncoding::Utf32CodeUnitOffsetFromLineStart => {
            let mut units = 0;
            for (offset, ch) in line.char_indices() {
                if units == character {
                    return Some(offset);
                }
                units += if encoding == PositionEncoding::Utf16CodeUnitOffsetFromLineStart as i32 {
                    ch.len_utf16()
                } else {
                    1
                };
            }
            (units == character).then_some(line.len())
        }
        PositionEncoding::UnspecifiedPositionEncoding => None,
    }
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn occurrence_start(occurrence: &Occurrence) -> Option<(i32, i32)> {
    match occurrence.range.len() {
        3 => Some((occurrence.range[0], occurrence.range[1])),
        4 => Some((occurrence.range[0], occurrence.range[1])),
        _ => None,
    }
}

fn is_definition(occurrence: &Occurrence) -> bool {
    occurrence.symbol_roles & (SymbolRole::Definition as i32) != 0
}

fn is_reference(occurrence: &Occurrence) -> bool {
    let roles = occurrence.symbol_roles;
    roles & (SymbolRole::Definition as i32) == 0
        && (roles == 0 || roles & (SymbolRole::ReadAccess as i32) != 0)
}

fn simple_symbol_name(symbol: &str) -> Option<String> {
    let descriptor = symbol.split(' ').next_back()?.trim_end_matches('#');
    let scoped = descriptor
        .rsplit('/')
        .next()
        .unwrap_or(descriptor)
        .rsplit('#')
        .next()
        .unwrap_or(descriptor);
    let name = scoped
        .strip_suffix("().")
        .or_else(|| scoped.strip_suffix("()"))
        .or_else(|| scoped.strip_suffix('.'))
        .unwrap_or(scoped);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::scip::generated::scip::{
        Document, Metadata, Occurrence, ProtocolVersion, TextEncoding, ToolInfo,
    };

    #[test]
    fn reference_binding_resolves_cross_package_call_to_owner_definition() -> Result<(), String> {
        let temp = tempfile_dir("bindings")?;
        let project = temp.join("workspace");
        std::fs::create_dir_all(project.join("crates/alpha/src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(project.join("crates/beta/tests")).map_err(|e| e.to_string())?;
        std::fs::write(
            project.join("crates/alpha/src/lib.rs"),
            "pub fn expect_response() -> u32 { 1 }\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            project.join("crates/beta/tests/protocol.rs"),
            "fn t() { expect_response(); }\n",
        )
        .map_err(|e| e.to_string())?;
        let owner_symbol = "scip-test crates/alpha/src/lib.rs expect_response().";
        let scoped_owner_symbol = "scip rust-analyzer cargo crate_a 0.1.0 crate_a/score().";
        let index = Index {
            metadata: Some(Metadata {
                version: ProtocolVersion::UnspecifiedProtocolVersion as i32,
                tool_info: Some(ToolInfo {
                    name: "fixture".to_string(),
                    version: "0.0.1".to_string(),
                    arguments: Vec::new(),
                }),
                project_root: "file:///fixture/".to_string(),
                text_document_encoding: TextEncoding::Utf8 as i32,
            }),
            documents: vec![
                Document {
                    relative_path: "crates/alpha/src/lib.rs".to_string(),
                    occurrences: vec![Occurrence {
                        symbol: owner_symbol.to_string(),
                        range: vec![0, 3, 17],
                        symbol_roles: SymbolRole::Definition as i32,
                        ..Occurrence::default()
                    }],
                    ..Document::default()
                },
                Document {
                    relative_path: "crates/beta/tests/protocol.rs".to_string(),
                    position_encoding: PositionEncoding::Utf8CodeUnitOffsetFromLineStart as i32,
                    occurrences: vec![Occurrence {
                        symbol: owner_symbol.to_string(),
                        range: vec![0, 9, 24],
                        symbol_roles: SymbolRole::ReadAccess as i32,
                        ..Occurrence::default()
                    }],
                    ..Document::default()
                },
                Document {
                    relative_path: "crates/alpha/src/scoped.rs".to_string(),
                    occurrences: vec![Occurrence {
                        symbol: scoped_owner_symbol.to_string(),
                        range: vec![0, 3, 8],
                        symbol_roles: SymbolRole::Definition as i32,
                        ..Occurrence::default()
                    }],
                    ..Document::default()
                },
            ],
            ..Index::default()
        };
        let bindings = ScipReferenceBindings::from_index(".", &index);
        assert_eq!(
            simple_symbol_name(scoped_owner_symbol),
            Some("score".to_string())
        );
        assert!(bindings.call_resolves_to_owner(
            &project,
            &project.join("crates/beta/tests/protocol.rs"),
            1,
            "fn t() { expect_response(); }",
            "expect_response",
            &project.join("crates/alpha/src/lib.rs"),
            "expect_response",
            1,
        ));
        assert!(!bindings.call_resolves_to_owner(
            &project,
            &project.join("crates/beta/tests/protocol.rs"),
            1,
            "fn t() { expect_response(); }",
            "expect_response",
            &project.join("crates/alpha/src/lib.rs"),
            "other_name",
            1,
        ));
        assert!(!bindings.call_resolves_to_owner(
            &project,
            &project.join("crates/beta/tests/protocol.rs"),
            1,
            "fn t() { expect_response(); }",
            "expect_response",
            &project.join("crates/alpha/src/lib.rs"),
            "expect_response",
            2,
        ));
        let duplicate_owner_index = Index {
            metadata: index.metadata.clone(),
            documents: vec![Document {
                relative_path: "crates/alpha/src/lib.rs".to_string(),
                occurrences: vec![
                    Occurrence {
                        symbol: "scip-test mod_a/score().".to_string(),
                        range: vec![0, 3, 8],
                        symbol_roles: SymbolRole::Definition as i32,
                        ..Occurrence::default()
                    },
                    Occurrence {
                        symbol: "scip-test mod_b/score().".to_string(),
                        range: vec![1, 3, 8],
                        symbol_roles: SymbolRole::Definition as i32,
                        ..Occurrence::default()
                    },
                ],
                ..Document::default()
            }],
            ..Index::default()
        };
        let ambiguous = ScipReferenceBindings::from_index(".", &duplicate_owner_index);
        assert!(
            !ambiguous.call_resolves_to_owner(
                &project,
                &project.join("crates/beta/tests/protocol.rs"),
                1,
                "fn t() { score(); }",
                "score",
                &project.join("crates/alpha/src/lib.rs"),
                "score",
                1,
            ),
            "duplicate same-name owners in one file must fail closed"
        );
        Ok(())
    }

    #[test]
    fn same_line_pointer_reference_does_not_resolve_an_unrelated_call() -> Result<(), String> {
        let project = tempfile_dir("same-line")?;
        let owner = project.join("owner.rs");
        let test = project.join("test.rs");
        std::fs::write(&owner, "pub fn score() -> u32 { 1 }").map_err(|e| e.to_string())?;
        let source = "#[test] fn t() { let keep = alpha::score; assert_eq!(score(), 1); }";
        std::fs::write(&test, source).map_err(|e| e.to_string())?;
        let pointer = source.find("alpha::score").ok_or("missing pointer")? + "alpha::".len();
        let call = source.find("score()").ok_or("missing call")?;
        let alpha = "scip rust-analyzer cargo alpha 0.1.0 alpha/score().";
        let beta = "scip rust-analyzer cargo beta 0.1.0 beta/score().";
        let mut index = Index {
            documents: vec![
                Document {
                    relative_path: "owner.rs".into(),
                    occurrences: vec![Occurrence {
                        symbol: alpha.into(),
                        range: vec![0, 7, 12],
                        symbol_roles: SymbolRole::Definition as i32,
                        ..Occurrence::default()
                    }],
                    ..Document::default()
                },
                Document {
                    relative_path: "test.rs".into(),
                    position_encoding: PositionEncoding::Utf8CodeUnitOffsetFromLineStart as i32,
                    occurrences: vec![
                        Occurrence {
                            symbol: alpha.into(),
                            range: vec![0, pointer as i32, (pointer + 5) as i32],
                            ..Occurrence::default()
                        },
                        Occurrence {
                            symbol: beta.into(),
                            range: vec![0, call as i32, (call + 5) as i32],
                            ..Occurrence::default()
                        },
                    ],
                    ..Document::default()
                },
            ],
            ..Index::default()
        };
        let mut consumed = crate::analysis::consumed_source::ConsumedRustSources::default();
        consumed.record(Path::new("owner.rs"), Some(b"pub fn score() -> u32 { 1 }"));
        consumed.record(Path::new("test.rs"), Some(source.as_bytes()));
        let retained_sources = [
            (Path::new("owner.rs"), "pub fn score() -> u32 { 1 }"),
            (Path::new("test.rs"), source),
        ];
        let resolves = |index: &Index| -> Result<bool, String> {
            use sha2::{Digest, Sha256};
            let bytes = index.encode_to_vec();
            let index_path = project.join("index.scip");
            let manifest_path = project.join("manifest.json");
            std::fs::write(&index_path, &bytes).map_err(|e| e.to_string())?;
            let source_files = ["owner.rs", "test.rs"].into_iter().map(|relative| {
                let source = std::fs::read(project.join(relative)).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({"relative_path": relative, "source_sha256": format!("{:x}", Sha256::digest(&source))}))
            }).collect::<Result<Vec<_>, String>>()?;
            let manifest = serde_json::json!({
                "schema_version": "ripr-scip-admission-manifest-v1",
                "index_sha256": format!("{:x}", Sha256::digest(&bytes)),
                "producer": {"name": "synthetic-span-regression", "version": "1"},
                "project_root_relative": ".", "source_files": source_files,
            });
            std::fs::write(&manifest_path, manifest.to_string()).map_err(|e| e.to_string())?;
            let bindings = ScipReferenceBindings::from_admitted_index(
                &project,
                &index_path,
                &manifest_path,
                &consumed,
                retained_sources,
            )
            .map_err(|error| error.reason())?;
            Ok(bindings
                .call_resolves_to_owner(&project, &test, 1, source, "score", &owner, "score", 1))
        };
        assert!(
            !resolves(&index)?,
            "a nearby alpha reference is not the beta call"
        );
        index.documents[1].occurrences[1].symbol = alpha.into();
        assert!(
            resolves(&index)?,
            "the actual call occurrence can resolve alpha"
        );
        index.documents[1].occurrences.push(Occurrence {
            symbol: beta.into(),
            range: vec![0, call as i32, (call + 5) as i32],
            ..Occurrence::default()
        });
        assert!(
            !resolves(&index)?,
            "a contradictory binding at the call stays unknown"
        );
        Ok(())
    }

    #[test]
    fn bindings_use_the_admitted_snapshot_after_index_and_manifest_replacement()
    -> Result<(), String> {
        use sha2::{Digest, Sha256};
        let project = tempfile_dir("snapshot-replacement")?;
        let source = "pub fn score() -> u32 { 1 }\n#[test] fn t() { score(); }\n";
        let file = project.join("owner.rs");
        std::fs::write(&file, source).map_err(|e| e.to_string())?;
        let line = source.lines().nth(1).ok_or("missing call line")?;
        let call = line.find("score()").ok_or("missing call")?;
        let symbol = "scip rust-analyzer cargo alpha 0.1.0 alpha/score().";
        let index = Index {
            documents: vec![Document {
                relative_path: "owner.rs".into(),
                position_encoding: PositionEncoding::Utf8CodeUnitOffsetFromLineStart as i32,
                occurrences: vec![
                    Occurrence {
                        symbol: symbol.into(),
                        range: vec![0, 7, 12],
                        symbol_roles: SymbolRole::Definition as i32,
                        ..Occurrence::default()
                    },
                    Occurrence {
                        symbol: symbol.into(),
                        range: vec![1, call as i32, (call + 5) as i32],
                        ..Occurrence::default()
                    },
                ],
                ..Document::default()
            }],
            ..Index::default()
        };
        let bytes = index.encode_to_vec();
        let index_path = project.join("index.scip");
        let manifest_path = project.join("manifest.json");
        std::fs::write(&index_path, &bytes).map_err(|e| e.to_string())?;
        let manifest = serde_json::json!({
            "schema_version":"ripr-scip-admission-manifest-v1",
            "index_sha256":format!("{:x}",Sha256::digest(&bytes)),
            "producer":{"name":"synthetic-snapshot-regression","version":"1"},
            "project_root_relative":".",
            "source_files":[{"relative_path":"owner.rs","source_sha256":format!("{:x}",Sha256::digest(source.as_bytes()))}]
        });
        std::fs::write(&manifest_path, manifest.to_string()).map_err(|e| e.to_string())?;
        // Capture successful admission before either caller-controlled file
        // changes. Binding construction must consume these owned facts only.
        let mut consumed = crate::analysis::consumed_source::ConsumedRustSources::default();
        consumed.record(Path::new("owner.rs"), Some(source.as_bytes()));
        let snapshot =
            admit_scip_snapshot(&project, &index_path, &manifest_path).map_err(|e| e.reason())?;
        let mut replacement = index;
        replacement.documents[0].occurrences[1].symbol =
            "scip rust-analyzer cargo beta 0.1.0 beta/score().".into();
        std::fs::write(&index_path, replacement.encode_to_vec()).map_err(|e| e.to_string())?;
        let mut replacement_manifest = manifest.clone();
        replacement_manifest["project_root_relative"] = serde_json::json!("..");
        std::fs::write(&manifest_path, replacement_manifest.to_string())
            .map_err(|e| e.to_string())?;
        let retained_sources = [(Path::new("owner.rs"), source)];
        let bindings =
            ScipReferenceBindings::from_admitted_snapshot(snapshot, &consumed, retained_sources)
                .map_err(|error| error.reason())?;
        assert!(
            bindings.call_resolves_to_owner(&project, &file, 2, line, "score", &file, "score", 1),
            "retained validated index/root must survive file replacement"
        );
        assert!(
            ScipReferenceBindings::from_admitted_index(
                &project,
                &index_path,
                &manifest_path,
                &consumed,
                retained_sources,
            )
            .is_err(),
            "fresh admission must reject replaced manifest/index"
        );
        // Isolate index identity: restore the original manifest, keeping the
        // replacement index. Its digest mismatch must independently reject.
        std::fs::write(&manifest_path, manifest.to_string()).map_err(|e| e.to_string())?;
        assert!(matches!(
            admit_scip_snapshot(&project, &index_path, &manifest_path),
            Err(ScipAdmissionError::IndexDigestMismatch)
        ));
        std::fs::remove_dir_all(project).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn same_file_function_names_cannot_share_definition_authority() -> Result<(), String> {
        use crate::analysis::consumed_source::ConsumedRustSources;
        use sha2::{Digest, Sha256};
        let project = tempfile_dir("same-file-owner")?;
        let call_source = "#[test] fn t() { score(); }\n";
        let call = call_source.find("score()").ok_or("missing call")?;
        let symbol = "scip rust-analyzer cargo alpha 0.1.0 alpha/active/score().";
        for duplicate in [false, true] {
            let owner = if duplicate {
                "#[cfg(any())] mod inactive { pub fn r#score() -> u32 { 1 } }\nmod active { pub fn score() -> u32 { 7 } }\npub use active::score;\n"
            } else {
                "mod active { pub fn score() -> u32 { 7 } }\npub use active::score;\n"
            };
            let definition_line = usize::from(duplicate);
            let definition_column = owner
                .lines()
                .nth(definition_line)
                .ok_or("definition line")?
                .find("score()")
                .ok_or("definition")?;
            let index = Index {
                documents: vec![
                    Document {
                        relative_path: "owner.rs".into(),
                        occurrences: vec![Occurrence {
                            symbol: symbol.into(),
                            range: vec![
                                definition_line as i32,
                                definition_column as i32,
                                (definition_column + 5) as i32,
                            ],
                            symbol_roles: SymbolRole::Definition as i32,
                            ..Occurrence::default()
                        }],
                        ..Document::default()
                    },
                    Document {
                        relative_path: "test.rs".into(),
                        position_encoding: PositionEncoding::Utf8CodeUnitOffsetFromLineStart as i32,
                        occurrences: vec![Occurrence {
                            symbol: symbol.into(),
                            range: vec![0, call as i32, (call + 5) as i32],
                            ..Occurrence::default()
                        }],
                        ..Document::default()
                    },
                ],
                ..Index::default()
            };
            let bytes = index.encode_to_vec();
            let index_path = project.join("index.scip");
            let manifest_path = project.join("manifest.json");
            std::fs::write(&index_path, &bytes).map_err(|e| e.to_string())?;
            std::fs::write(project.join("owner.rs"), owner).map_err(|e| e.to_string())?;
            std::fs::write(project.join("test.rs"), call_source).map_err(|e| e.to_string())?;
            let manifest = serde_json::json!({"schema_version":"ripr-scip-admission-manifest-v1","index_sha256":format!("{:x}",Sha256::digest(&bytes)),"producer":{"name":"same-file-definition-regression","version":"1"},"project_root_relative":".","source_files":[
                {"relative_path":"owner.rs","source_sha256":format!("{:x}",Sha256::digest(owner.as_bytes()))},
                {"relative_path":"test.rs","source_sha256":format!("{:x}",Sha256::digest(call_source.as_bytes()))}
            ]});
            std::fs::write(&manifest_path, manifest.to_string()).map_err(|e| e.to_string())?;
            let mut consumed = ConsumedRustSources::default();
            consumed.record(Path::new("owner.rs"), Some(owner.as_bytes()));
            consumed.record(Path::new("test.rs"), Some(call_source.as_bytes()));
            let bindings = ScipReferenceBindings::from_admitted_index(
                &project,
                &index_path,
                &manifest_path,
                &consumed,
                [
                    (Path::new("owner.rs"), owner),
                    (Path::new("test.rs"), call_source),
                ],
            )
            .map_err(|e| e.reason())?;
            // The public lookup has only a file/name owner key. Neither the
            // inactive nor active same-name owner may borrow this symbol.
            assert_eq!(
                bindings.call_resolves_to_owner(
                    &project,
                    &project.join("test.rs"),
                    1,
                    call_source.trim_end(),
                    "score",
                    &project.join("owner.rs"),
                    "score",
                    1
                ),
                !duplicate,
                "same-file duplicate={duplicate}"
            );
        }
        std::fs::remove_dir_all(project).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn binding_authority_requires_matching_raw_source_capture() -> Result<(), String> {
        use crate::analysis::consumed_source::ConsumedRustSources;
        use sha2::{Digest, Sha256};
        let project = tempfile_dir("captured-source")?;
        let owner = "pub fn score() -> u32 { 1 }\n";
        let old = "use beta::score;\n#[test] fn t() { score(); }\n";
        let current = old.replace("beta::score", "alpha::score");
        let call_line = old.lines().nth(1).ok_or("missing call line")?;
        assert_eq!(Some(call_line), current.lines().nth(1));
        let call = call_line.find("score()").ok_or("missing call")?;
        let symbol = "scip rust-analyzer cargo alpha 0.1.0 alpha/score().";
        let index = Index {
            documents: vec![
                Document {
                    relative_path: "owner.rs".into(),
                    occurrences: vec![Occurrence {
                        symbol: symbol.into(),
                        range: vec![0, 7, 12],
                        symbol_roles: SymbolRole::Definition as i32,
                        ..Occurrence::default()
                    }],
                    ..Document::default()
                },
                Document {
                    relative_path: "test.rs".into(),
                    position_encoding: PositionEncoding::Utf8CodeUnitOffsetFromLineStart as i32,
                    occurrences: vec![Occurrence {
                        symbol: symbol.into(),
                        range: vec![1, call as i32, (call + 5) as i32],
                        ..Occurrence::default()
                    }],
                    ..Document::default()
                },
                // Broader valid SCIP input need not force unrelated captures.
                Document {
                    relative_path: "unrelated.rs".into(),
                    ..Document::default()
                },
            ],
            ..Index::default()
        };
        let mut stale = ConsumedRustSources::default();
        stale.record(Path::new("owner.rs"), Some(owner.as_bytes()));
        stale.record(Path::new("test.rs"), Some(old.as_bytes()));
        std::fs::write(project.join("owner.rs"), owner).map_err(|e| e.to_string())?;
        std::fs::write(project.join("test.rs"), &current).map_err(|e| e.to_string())?;
        std::fs::write(project.join("unrelated.rs"), "").map_err(|e| e.to_string())?;
        let bytes = index.encode_to_vec();
        let index_path = project.join("index.scip");
        let manifest_path = project.join("manifest.json");
        std::fs::write(&index_path, &bytes).map_err(|e| e.to_string())?;
        let manifest = serde_json::json!({
            "schema_version":"ripr-scip-admission-manifest-v1",
            "index_sha256":format!("{:x}",Sha256::digest(&bytes)),
            "producer":{"name":"capture-source-regression","version":"1"},
            "project_root_relative":".",
            "source_files":[
                {"relative_path":"owner.rs","source_sha256":format!("{:x}",Sha256::digest(owner.as_bytes()))},
                {"relative_path":"test.rs","source_sha256":format!("{:x}",Sha256::digest(current.as_bytes()))},
                {"relative_path":"unrelated.rs","source_sha256":format!("{:x}",Sha256::digest(b""))}
            ]
        });
        std::fs::write(&manifest_path, manifest.to_string()).map_err(|e| e.to_string())?;
        let retained_sources = [
            (Path::new("owner.rs"), owner),
            (Path::new("test.rs"), current.as_str()),
        ];
        let admit = |consumed: &ConsumedRustSources| {
            ScipReferenceBindings::from_admitted_index(
                &project,
                &index_path,
                &manifest_path,
                consumed,
                retained_sources,
            )
        };
        assert!(
            matches!(
                admit(&stale),
                Err(ScipAdmissionError::SourceDigestMismatch { .. })
            ),
            "new import cannot supply binding authority to an older source capture"
        );
        let mut matching = ConsumedRustSources::default();
        matching.record(Path::new("owner.rs"), Some(owner.as_bytes()));
        matching.record(Path::new("test.rs"), Some(current.as_bytes()));
        let bindings = admit(&matching).map_err(|e| e.reason())?;
        let resolves = |bindings: &ScipReferenceBindings| {
            bindings.call_resolves_to_owner(
                &project,
                &project.join("test.rs"),
                2,
                call_line,
                "score",
                &project.join("owner.rs"),
                "score",
                1,
            )
        };
        assert!(
            resolves(&bindings),
            "matching capture keeps genuine binding despite unrelated broader document"
        );
        for missing in ["owner.rs", "test.rs"] {
            let mut partial = ConsumedRustSources::default();
            if missing != "owner.rs" {
                partial.record(Path::new("owner.rs"), Some(owner.as_bytes()));
            }
            if missing != "test.rs" {
                partial.record(Path::new("test.rs"), Some(current.as_bytes()));
            }
            assert!(
                !resolves(&admit(&partial).map_err(|e| e.reason())?),
                "uncaptured {missing} cannot contribute authority"
            );
        }
        let mut conflicting = matching.clone();
        conflicting.record(Path::new("test.rs"), Some(old.as_bytes()));
        assert!(
            admit(&conflicting).is_err(),
            "conflicting raw observations must refuse"
        );
        let mut unavailable = ConsumedRustSources::default();
        unavailable.record(Path::new("owner.rs"), Some(owner.as_bytes()));
        unavailable.record(Path::new("test.rs"), None);
        assert!(
            admit(&unavailable).is_err(),
            "failed capture is not an unrelated uncaptured document"
        );
        std::fs::write(project.join("test.rs"), old).map_err(|e| e.to_string())?;
        assert!(
            resolves(&bindings),
            "later mutation does not replace matching retained capture"
        );
        std::fs::remove_dir_all(project).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn normalized_coordinate_documents_cannot_supply_raw_scip_authority() -> Result<(), String> {
        use crate::analysis::consumed_source::ConsumedRustSources;
        use sha2::{Digest, Sha256};
        let project = tempfile_dir("coordinate-source")?;
        let owner = "pub fn f() -> u32 { 1 }\n";
        let plain = "use beta::f; #[test] fn unrelated() { let (_, value) = (alpha::f, f()); assert_eq!(value, 7); }";
        let alpha = "scip rust-analyzer cargo alpha 0.1.0 alpha/f().";
        let beta = "scip rust-analyzer cargo beta 0.1.0 beta/f().";
        let index_path = project.join("index.scip");
        let manifest_path = project.join("manifest.json");
        std::fs::write(project.join("owner.rs"), owner).map_err(|e| e.to_string())?;
        for encoding in [
            PositionEncoding::Utf8CodeUnitOffsetFromLineStart,
            PositionEncoding::Utf16CodeUnitOffsetFromLineStart,
            PositionEncoding::Utf32CodeUnitOffsetFromLineStart,
        ] {
            for bom in [true, false] {
                let raw = format!("{}{plain}", if bom { "\u{feff}" } else { "" });
                let pointer = raw.find("alpha::f").ok_or("missing pointer")? + "alpha::".len();
                let call = raw.find(", f()").ok_or("missing call")? + 2;
                let column = |byte: usize| -> i32 {
                    match encoding {
                        PositionEncoding::Utf8CodeUnitOffsetFromLineStart => byte as i32,
                        PositionEncoding::Utf16CodeUnitOffsetFromLineStart => {
                            raw[..byte].encode_utf16().count() as i32
                        }
                        _ => raw[..byte].chars().count() as i32,
                    }
                };
                let mut index = Index {
                    documents: vec![
                        Document {
                            relative_path: "owner.rs".into(),
                            occurrences: vec![Occurrence {
                                symbol: alpha.into(),
                                range: vec![0, 7, 8],
                                symbol_roles: SymbolRole::Definition as i32,
                                ..Occurrence::default()
                            }],
                            ..Document::default()
                        },
                        Document {
                            relative_path: "test.rs".into(),
                            position_encoding: encoding as i32,
                            occurrences: vec![
                                Occurrence {
                                    symbol: alpha.into(),
                                    range: vec![0, column(pointer), column(pointer + 1)],
                                    ..Occurrence::default()
                                },
                                Occurrence {
                                    symbol: beta.into(),
                                    range: vec![0, column(call), column(call + 1)],
                                    ..Occurrence::default()
                                },
                            ],
                            ..Document::default()
                        },
                    ],
                    ..Index::default()
                };
                std::fs::write(project.join("test.rs"), &raw).map_err(|e| e.to_string())?;
                let mut consumed = ConsumedRustSources::default();
                consumed.record(Path::new("owner.rs"), Some(owner.as_bytes()));
                consumed.record(Path::new("test.rs"), Some(raw.as_bytes()));
                let admit = |index: &Index,
                             coordinate_owner: &str,
                             coordinate_test: &str|
                 -> Result<ScipReferenceBindings, String> {
                    let bytes = index.encode_to_vec();
                    std::fs::write(&index_path, &bytes).map_err(|e| e.to_string())?;
                    let manifest = serde_json::json!({"schema_version":"ripr-scip-admission-manifest-v1",
                        "index_sha256":format!("{:x}",Sha256::digest(&bytes)),
                        "producer":{"name":"coordinate-regression","version":"1"},"project_root_relative":".",
                        "source_files":[{"relative_path":"owner.rs","source_sha256":format!("{:x}",Sha256::digest(owner.as_bytes()))},
                            {"relative_path":"test.rs","source_sha256":format!("{:x}",Sha256::digest(raw.as_bytes()))}]});
                    std::fs::write(&manifest_path, manifest.to_string())
                        .map_err(|e| e.to_string())?;
                    ScipReferenceBindings::from_admitted_index(
                        &project,
                        &index_path,
                        &manifest_path,
                        &consumed,
                        [
                            (Path::new("owner.rs"), coordinate_owner),
                            (Path::new("test.rs"), coordinate_test),
                        ],
                    )
                    .map_err(|e| e.reason())
                };
                let resolves = |bindings: &ScipReferenceBindings, line: &str| {
                    bindings.call_resolves_to_owner(
                        &project,
                        &project.join("test.rs"),
                        1,
                        line,
                        "f",
                        &project.join("owner.rs"),
                        "f",
                        1,
                    )
                };
                assert!(
                    !resolves(&admit(&index, owner, plain)?, plain),
                    "a pointer cannot supply a different call's identity; BOM={bom}, encoding={encoding:?}"
                );
                index.documents[1].occurrences[1].symbol = alpha.into();
                let matching = admit(&index, owner, &raw)?;
                assert!(
                    resolves(&matching, &raw),
                    "exact coordinate bytes preserve the actual-call positive"
                );
                if bom {
                    assert!(
                        !resolves(&admit(&index, owner, plain)?, plain),
                        "normalized test document contributes no authority"
                    );
                }
                assert!(
                    !resolves(&admit(&index, &format!("\u{feff}{owner}"), &raw)?, &raw),
                    "a mismatching definition document contributes no authority"
                );
                std::fs::write(project.join("test.rs"), "// later edit\n")
                    .map_err(|e| e.to_string())?;
                assert!(
                    resolves(&matching, &raw),
                    "later live edits cannot replace matching retained coordinate bytes"
                );
            }
        }
        std::fs::remove_dir_all(project).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn callee_spans_and_document_positions_preserve_unicode_and_refuse_ambiguity() {
        let source = "let prefix = \"🚀\"; score();";
        let start = source.find("score");
        assert_eq!(
            unique_callee_span(source, "score"),
            start.map(|start| (start, start + 5))
        );
        assert_eq!(
            unique_callee_span("alpha::score(); beta::score();", "score"),
            None
        );
        assert_eq!(
            unique_callee_span("let keep = alpha::score;", "score"),
            None
        );
        assert_eq!(
            unique_callee_span("assert_eq!(some_score(), 1);", "score"),
            None
        );
        assert_eq!(
            unique_callee_span("let s = \"score()\"; score();", "score"),
            Some((19, 24))
        );
        for (encoding, column) in [
            (PositionEncoding::Utf8CodeUnitOffsetFromLineStart, 4),
            (PositionEncoding::Utf16CodeUnitOffsetFromLineStart, 2),
            (PositionEncoding::Utf32CodeUnitOffsetFromLineStart, 1),
        ] {
            assert_eq!(
                character_to_byte("🚀score", column, encoding as i32),
                Some(4)
            );
        }
        assert_eq!(
            character_to_byte(
                "🚀score",
                1,
                PositionEncoding::Utf16CodeUnitOffsetFromLineStart as i32
            ),
            None
        );
        assert_eq!(
            character_to_byte(
                "score",
                0,
                PositionEncoding::UnspecifiedPositionEncoding as i32
            ),
            None
        );
    }

    fn tempfile_dir(label: &str) -> Result<std::path::PathBuf, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ripr-scip-bindings-{label}-{stamp}"));
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }
}
