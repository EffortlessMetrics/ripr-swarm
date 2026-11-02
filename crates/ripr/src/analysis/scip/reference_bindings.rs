//! Admitted SCIP reference-to-definition bindings for relation consumers (#8116).

use super::admission::{ScipAdmissionError, admit_scip_index};
use super::generated::scip::{Index, Occurrence, SymbolRole};
use prost::Message;
use std::collections::BTreeMap;
use std::path::Path;

/// Read-only lookup built from one admitted SCIP index (#8116).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScipReferenceBindings {
    project_root_relative: String,
    /// SCIP symbol for each local definition keyed by workspace-relative path
    /// and the item's simple name (`expect_response`, etc.).
    owner_definition_symbols: BTreeMap<(String, String), String>,
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
        let index_bytes =
            std::fs::read(index_path).map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
        let index = Index::decode(index_bytes.as_slice())
            .map_err(|error| ScipAdmissionError::IndexDecode(error.to_string()))?;
        let manifest = super::admission::read_manifest(manifest_path)?;
        Ok(Self::from_index(&manifest.project_root_relative, &index))
    }

    fn from_index(project_root_relative: &str, index: &Index) -> Self {
        let mut owner_definition_symbols = BTreeMap::new();
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
                            .insert((relative_path.clone(), name), occurrence.symbol.clone());
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
    pub(crate) fn call_resolves_to_owner(
        &self,
        workspace_root: &Path,
        test_file: &Path,
        call_line: usize,
        callee: &str,
        owner_file: &Path,
        owner_name: &str,
    ) -> bool {
        let Some(owner_symbol) =
            self.owner_definition_symbol(workspace_root, owner_file, owner_name)
        else {
            return false;
        };
        let Some(test_path) = self.relative_path(workspace_root, test_file) else {
            return false;
        };
        let line = i32::try_from(call_line.saturating_sub(1)).unwrap_or(i32::MAX);
        self.reference_symbols.iter().any(
            |((path, reference_line, _), symbol)| {
                path.as_str() == test_path
                    && *reference_line == line
                    && **symbol == owner_symbol
                    && simple_symbol_name(symbol).is_some_and(|name| name == callee)
            },
        )
    }

    fn owner_definition_symbol(
        &self,
        workspace_root: &Path,
        owner_file: &Path,
        owner_name: &str,
    ) -> Option<String> {
        let path = self.relative_path(workspace_root, owner_file)?;
        self.owner_definition_symbols
            .get(&(path, owner_name.to_string()))
            .cloned()
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
    let tail = symbol.split(' ').next_back()?.trim_end_matches('#');
    let name = tail.split('.').next()?.trim_end_matches("()");
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
            ],
            ..Index::default()
        };
        let bindings = ScipReferenceBindings::from_index(".", &index);
        assert!(bindings.call_resolves_to_owner(
            &project,
            &project.join("crates/beta/tests/protocol.rs"),
            1,
            "expect_response",
            &project.join("crates/alpha/src/lib.rs"),
            "expect_response",
        ));
        assert!(!bindings.call_resolves_to_owner(
            &project,
            &project.join("crates/beta/tests/protocol.rs"),
            1,
            "expect_response",
            &project.join("crates/alpha/src/lib.rs"),
            "other_name",
        ));
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
