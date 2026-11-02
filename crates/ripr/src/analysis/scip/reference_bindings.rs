//! Admitted SCIP reference-to-definition bindings for relation consumers (#8116).

use super::admission::{ScipAdmissionError, admit_scip_index, read_admitted_index_bytes};
use super::generated::scip::{Index, Occurrence, SymbolRole};
use prost::Message;
use std::collections::BTreeMap;
use std::path::Path;

/// Read-only lookup built from one admitted SCIP index (#8116).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScipReferenceBindings {
    project_root_relative: String,
    /// SCIP definition symbols keyed by workspace-relative path and terminal name.
    owner_definition_symbols: BTreeMap<(String, String), Vec<String>>,
    /// Reference occurrences keyed by `(relative_path, line, start_character)`.
    reference_symbols: BTreeMap<(String, i32, i32), String>,
}

impl ScipReferenceBindings {
    pub(crate) fn from_admitted_index(
        workspace_root: &Path,
        index_path: &Path,
        manifest_path: &Path,
    ) -> Result<Self, ScipAdmissionError> {
        let _admitted = admit_scip_index(workspace_root, index_path, manifest_path)?;
        let index_bytes = read_admitted_index_bytes(index_path)?;
        let index = Index::decode(index_bytes.as_slice())
            .map_err(|error| ScipAdmissionError::IndexDecode(error.to_string()))?;
        let manifest = super::admission::read_manifest(manifest_path)?;
        Ok(Self::from_index(&manifest.project_root_relative, &index))
    }

    fn from_index(project_root_relative: &str, index: &Index) -> Self {
        let mut owner_definition_symbols: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut reference_symbols = BTreeMap::new();
        for document in &index.documents {
            let relative_path = normalize_relative_path(&document.relative_path);
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
                    reference_symbols.insert(
                        (relative_path.clone(), line, start),
                        occurrence.symbol.clone(),
                    );
                }
            }
        }
        Self {
            project_root_relative: project_root_relative.to_string(),
            owner_definition_symbols,
            reference_symbols,
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
        let line = i32::try_from(call_line.saturating_sub(1)).unwrap_or(i32::MAX);
        let matching_references = self
            .reference_symbols
            .iter()
            .filter(|((path, reference_line, _), symbol)| {
                path.as_str() == test_path
                    && *reference_line == line
                    && **symbol == owner_symbol
                    && simple_symbol_name(symbol).is_some_and(|name| name == callee)
            })
            .count();
        matching_references == 1
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
            "expect_response",
            &project.join("crates/alpha/src/lib.rs"),
            "expect_response",
            1,
        ));
        assert!(!bindings.call_resolves_to_owner(
            &project,
            &project.join("crates/beta/tests/protocol.rs"),
            1,
            "expect_response",
            &project.join("crates/alpha/src/lib.rs"),
            "other_name",
            1,
        ));
        assert!(!bindings.call_resolves_to_owner(
            &project,
            &project.join("crates/beta/tests/protocol.rs"),
            1,
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
                "score",
                &project.join("crates/alpha/src/lib.rs"),
                "score",
                1,
            ),
            "duplicate same-name owners in one file must fail closed"
        );
        Ok(())
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
