//! Bounded admission of one caller-supplied SCIP index plus a content-bound
//! companion manifest (#8115).

use super::generated::scip::{Document, Index, TextEncoding};
use prost::Message;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Maximum bytes read from a caller-supplied SCIP index file.
pub(crate) const MAX_SCIP_INDEX_BYTES: u64 = 32 * 1024 * 1024;

/// Maximum bytes read from a companion admission manifest.
pub(crate) const MAX_SCIP_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Companion manifest schema (`ripr-scip-admission-manifest-v1`).
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct ScipAdmissionManifest {
    pub(crate) schema_version: String,
    pub(crate) index_sha256: String,
    pub(crate) producer: ScipProducerManifest,
    pub(crate) project_root_relative: String,
    #[serde(default)]
    pub(crate) toolchain_fingerprint: String,
    pub(crate) source_files: Vec<ScipSourceFileManifest>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct ScipProducerManifest {
    pub(crate) name: String,
    pub(crate) version: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct ScipSourceFileManifest {
    pub(crate) relative_path: String,
    pub(crate) source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    dead_code,
    reason = "policy:8115-scip-enrichment-status: CLI disclosure wiring lands in the next #8115 slice"
)]
pub(crate) enum ScipEnrichmentStatus {
    NotRequested,
    Disabled,
    Admitted {
        portable_identity: String,
        producer: String,
    },
    Rejected {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdmittedScipFacts {
    pub(crate) portable_identity: String,
    pub(crate) index_digest: String,
    pub(crate) producer_name: String,
    pub(crate) producer_version: String,
    pub(crate) document_count: usize,
    pub(crate) symbol_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScipAdmissionError {
    Io(String),
    OversizedIndex,
    OversizedManifest,
    ManifestParse(String),
    ManifestSchema(String),
    IndexDigestMismatch,
    IndexDecode(String),
    ProjectRootEscape,
    UnsupportedTextEncoding(TextEncoding),
    SourceDigestMismatch { path: String },
    MissingSourceFile { path: String },
    UnboundIndexDocument { path: String },
    ExtraManifestSource { path: String },
    MalformedRange { path: String, detail: String },
}

impl ScipAdmissionError {
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::Io(message) => format!("io: {message}"),
            Self::OversizedIndex => "scip index exceeds size bound".to_string(),
            Self::OversizedManifest => "scip manifest exceeds size bound".to_string(),
            Self::ManifestParse(message) => format!("manifest parse: {message}"),
            Self::ManifestSchema(message) => format!("manifest schema: {message}"),
            Self::IndexDigestMismatch => "index digest does not match manifest".to_string(),
            Self::IndexDecode(message) => format!("index decode: {message}"),
            Self::ProjectRootEscape => "project root escapes workspace".to_string(),
            Self::UnsupportedTextEncoding(encoding) => {
                format!("unsupported text encoding: {encoding:?}")
            }
            Self::SourceDigestMismatch { path } => {
                format!("source digest mismatch for {path}")
            }
            Self::MissingSourceFile { path } => format!("missing source file {path}"),
            Self::UnboundIndexDocument { path } => {
                format!("indexed document {path} lacks manifest source binding")
            }
            Self::ExtraManifestSource { path } => {
                format!("manifest source {path} is not present in the admitted index")
            }
            Self::MalformedRange { path, detail } => {
                format!("malformed range in {path}: {detail}")
            }
        }
    }
}

/// Read and admit one SCIP index when both paths are provided; otherwise
/// return `NotRequested`.
#[expect(
    dead_code,
    reason = "policy:8115-scip-enrichment-status: CLI disclosure wiring lands in the next #8115 slice"
)]
pub(crate) fn enrichment_status(
    workspace_root: &Path,
    index_path: Option<&Path>,
    manifest_path: Option<&Path>,
) -> ScipEnrichmentStatus {
    match (index_path, manifest_path) {
        (None, None) => ScipEnrichmentStatus::NotRequested,
        (Some(_), None) | (None, Some(_)) => ScipEnrichmentStatus::Rejected {
            reason: "scip index and manifest must be supplied together".to_string(),
        },
        (Some(index), Some(manifest)) => match admit_scip_index(workspace_root, index, manifest) {
            Ok(admitted) => ScipEnrichmentStatus::Admitted {
                portable_identity: admitted.portable_identity.clone(),
                producer: format!("{}@{}", admitted.producer_name, admitted.producer_version),
            },
            Err(error) => ScipEnrichmentStatus::Rejected {
                reason: error.reason(),
            },
        },
    }
}

/// Production admission entry point (#8115).
pub(crate) fn admit_scip_index(
    workspace_root: &Path,
    index_path: &Path,
    manifest_path: &Path,
) -> Result<AdmittedScipFacts, ScipAdmissionError> {
    let manifest = read_manifest(manifest_path)?;
    verify_project_root(workspace_root, &manifest)?;
    let index_bytes = read_bounded_index_file(index_path)?;
    verify_index_digest(&manifest, &index_bytes)?;
    let index = Index::decode(index_bytes.as_slice())
        .map_err(|error| ScipAdmissionError::IndexDecode(error.to_string()))?;
    verify_index_document_binding(&index, &manifest)?;
    verify_sources(workspace_root, &manifest)?;
    let facts = project_facts(&index, &manifest)?;
    Ok(facts)
}

fn read_manifest(path: &Path) -> Result<ScipAdmissionManifest, ScipAdmissionError> {
    let text = read_bounded_utf8(
        path,
        MAX_SCIP_MANIFEST_BYTES,
        ScipAdmissionError::OversizedManifest,
    )?;
    let manifest: ScipAdmissionManifest = serde_json::from_str(&text)
        .map_err(|error| ScipAdmissionError::ManifestParse(error.to_string()))?;
    if manifest.schema_version != "ripr-scip-admission-manifest-v1" {
        return Err(ScipAdmissionError::ManifestSchema(format!(
            "unsupported schema_version {}",
            manifest.schema_version
        )));
    }
    Ok(manifest)
}

fn read_bounded_utf8(
    path: &Path,
    max_bytes: u64,
    oversize: ScipAdmissionError,
) -> Result<String, ScipAdmissionError> {
    let bytes = read_bounded_file(path, max_bytes, oversize)?;
    String::from_utf8(bytes).map_err(|error| ScipAdmissionError::ManifestParse(error.to_string()))
}

fn read_bounded_file(
    path: &Path,
    max_bytes: u64,
    oversize: ScipAdmissionError,
) -> Result<Vec<u8>, ScipAdmissionError> {
    use std::io::Read;
    let mut file =
        std::fs::File::open(path).map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = file
            .read(&mut chunk)
            .map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() as u64 > max_bytes {
            return Err(oversize);
        }
    }
    Ok(buffer)
}

fn read_bounded_index_file(path: &Path) -> Result<Vec<u8>, ScipAdmissionError> {
    read_bounded_file(
        path,
        MAX_SCIP_INDEX_BYTES,
        ScipAdmissionError::OversizedIndex,
    )
}

fn verify_index_document_binding(
    index: &Index,
    manifest: &ScipAdmissionManifest,
) -> Result<(), ScipAdmissionError> {
    let mut manifest_paths = BTreeSet::new();
    for entry in &manifest.source_files {
        if !is_safe_relative_repo_path(&entry.relative_path) {
            return Err(ScipAdmissionError::ProjectRootEscape);
        }
        if !manifest_paths.insert(entry.relative_path.clone()) {
            return Err(ScipAdmissionError::ExtraManifestSource {
                path: entry.relative_path.clone(),
            });
        }
    }
    for document in &index.documents {
        if !is_safe_relative_repo_path(&document.relative_path) {
            return Err(ScipAdmissionError::ProjectRootEscape);
        }
        if !manifest_paths.remove(&document.relative_path) {
            return Err(ScipAdmissionError::UnboundIndexDocument {
                path: document.relative_path.clone(),
            });
        }
    }
    if let Some(path) = manifest_paths.into_iter().next() {
        return Err(ScipAdmissionError::ExtraManifestSource { path });
    }
    Ok(())
}

fn is_safe_relative_repo_path(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && !path.contains('\\')
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "..")
}

fn resolve_bound_source_path(
    workspace_root: &Path,
    project_root_relative: &str,
    relative_path: &str,
) -> Result<PathBuf, ScipAdmissionError> {
    if !is_safe_relative_repo_path(relative_path) {
        return Err(ScipAdmissionError::ProjectRootEscape);
    }
    let project_root = workspace_root.join(project_root_relative);
    let candidate = project_root.join(relative_path);
    let project_root = project_root
        .canonicalize()
        .map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
    let resolved =
        candidate
            .canonicalize()
            .map_err(|_error| ScipAdmissionError::MissingSourceFile {
                path: relative_path.to_string(),
            })?;
    if !resolved.starts_with(&project_root) {
        return Err(ScipAdmissionError::ProjectRootEscape);
    }
    Ok(resolved)
}

fn verify_index_digest(
    manifest: &ScipAdmissionManifest,
    index_bytes: &[u8],
) -> Result<(), ScipAdmissionError> {
    let digest = hex_sha256(index_bytes);
    if digest != manifest.index_sha256 {
        return Err(ScipAdmissionError::IndexDigestMismatch);
    }
    Ok(())
}

fn verify_project_root(
    workspace_root: &Path,
    manifest: &ScipAdmissionManifest,
) -> Result<(), ScipAdmissionError> {
    if !is_safe_relative_repo_path(&manifest.project_root_relative) {
        return Err(ScipAdmissionError::ProjectRootEscape);
    }
    let workspace = workspace_root
        .canonicalize()
        .map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
    let project_root = workspace.join(&manifest.project_root_relative);
    let project_root = project_root
        .canonicalize()
        .map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
    if !project_root.starts_with(&workspace) {
        return Err(ScipAdmissionError::ProjectRootEscape);
    }
    Ok(())
}

fn verify_sources(
    workspace_root: &Path,
    manifest: &ScipAdmissionManifest,
) -> Result<(), ScipAdmissionError> {
    for entry in &manifest.source_files {
        let path = resolve_bound_source_path(
            workspace_root,
            &manifest.project_root_relative,
            &entry.relative_path,
        )?;
        if !path.is_file() {
            return Err(ScipAdmissionError::MissingSourceFile {
                path: entry.relative_path.clone(),
            });
        }
        let bytes =
            std::fs::read(&path).map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
        if hex_sha256(&bytes) != entry.source_sha256 {
            return Err(ScipAdmissionError::SourceDigestMismatch {
                path: entry.relative_path.clone(),
            });
        }
    }
    Ok(())
}

fn project_facts(
    index: &Index,
    manifest: &ScipAdmissionManifest,
) -> Result<AdmittedScipFacts, ScipAdmissionError> {
    let encoding = index
        .metadata
        .as_ref()
        .map(|metadata| metadata.text_document_encoding())
        .unwrap_or(TextEncoding::UnspecifiedTextEncoding);
    if !matches!(
        encoding,
        TextEncoding::Utf8 | TextEncoding::UnspecifiedTextEncoding
    ) {
        return Err(ScipAdmissionError::UnsupportedTextEncoding(encoding));
    }
    let mut symbols = BTreeSet::new();
    let mut document_count = 0usize;
    let mut occurrence_count = 0usize;
    for document in &index.documents {
        document_count += 1;
        validate_document_ranges(document)?;
        for occurrence in &document.occurrences {
            if !occurrence.symbol.is_empty() {
                symbols.insert(occurrence.symbol.clone());
                occurrence_count += 1;
            }
        }
    }
    let portable_identity =
        portable_fact_identity(manifest, &symbols, document_count, occurrence_count);
    Ok(AdmittedScipFacts {
        portable_identity,
        index_digest: manifest.index_sha256.clone(),
        producer_name: manifest.producer.name.clone(),
        producer_version: manifest.producer.version.clone(),
        document_count,
        symbol_count: symbols.len(),
    })
}

fn validate_document_ranges(document: &Document) -> Result<(), ScipAdmissionError> {
    for occurrence in &document.occurrences {
        validate_occurrence_range(&document.relative_path, &occurrence.range)?;
    }
    Ok(())
}

fn validate_occurrence_range(path: &str, range: &[i32]) -> Result<(), ScipAdmissionError> {
    match range.len() {
        3 => {
            let [line, start_character, end_character] = [range[0], range[1], range[2]];
            if line < 0 || start_character < 0 || end_character < 0 {
                return Err(ScipAdmissionError::MalformedRange {
                    path: path.to_string(),
                    detail: "negative line or character".to_string(),
                });
            }
            if end_character < start_character {
                return Err(ScipAdmissionError::MalformedRange {
                    path: path.to_string(),
                    detail: "end character precedes start character".to_string(),
                });
            }
        }
        4 => {
            let [start_line, start_character, end_line, end_character] =
                [range[0], range[1], range[2], range[3]];
            if start_line < 0 || start_character < 0 || end_line < 0 || end_character < 0 {
                return Err(ScipAdmissionError::MalformedRange {
                    path: path.to_string(),
                    detail: "negative line or character".to_string(),
                });
            }
            if end_line < start_line || (end_line == start_line && end_character < start_character)
            {
                return Err(ScipAdmissionError::MalformedRange {
                    path: path.to_string(),
                    detail: "range ends before it starts".to_string(),
                });
            }
        }
        _ => {
            return Err(ScipAdmissionError::MalformedRange {
                path: path.to_string(),
                detail: "range must contain exactly three or four values".to_string(),
            });
        }
    }
    Ok(())
}

fn portable_fact_identity(
    manifest: &ScipAdmissionManifest,
    symbols: &BTreeSet<String>,
    document_count: usize,
    unique_symbol_count: usize,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest.index_sha256.as_bytes());
    hasher.update(manifest.toolchain_fingerprint.as_bytes());
    hasher.update(document_count.to_string().as_bytes());
    hasher.update(unique_symbol_count.to_string().as_bytes());
    for symbol in symbols {
        hasher.update(symbol.as_bytes());
        hasher.update([0]);
    }
    hex_sha256(&hasher.finalize())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::scip::generated::scip::{Metadata, Occurrence, ProtocolVersion, ToolInfo};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ripr-scip-{label}-{stamp}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }

    fn write_minimal_fixture_with_project_root(
        root: &Path,
        project_root_relative: &str,
        create_sources: bool,
    ) -> Result<(PathBuf, PathBuf), String> {
        let source = "pub fn hello() -> &'static str { \"héllo\" }\n";
        let source_digest = hex_sha256(source.as_bytes());
        if create_sources {
            let project_dir = root.join(project_root_relative).join("src");
            std::fs::create_dir_all(&project_dir).map_err(|error| error.to_string())?;
            std::fs::write(project_dir.join("lib.rs"), source)
                .map_err(|error| error.to_string())?;
        }
        let index = Index {
            metadata: Some(Metadata {
                version: ProtocolVersion::UnspecifiedProtocolVersion as i32,
                tool_info: Some(ToolInfo {
                    name: "fixture-scip".to_string(),
                    version: "0.0.1".to_string(),
                    arguments: Vec::new(),
                }),
                project_root: "file:///fixture/".to_string(),
                text_document_encoding: TextEncoding::Utf8 as i32,
            }),
            documents: vec![Document {
                relative_path: "src/lib.rs".to_string(),
                occurrences: vec![Occurrence {
                    symbol: "scip-test hello#".to_string(),
                    range: vec![0, 3, 0, 8],
                    ..Occurrence::default()
                }],
                ..Document::default()
            }],
            ..Index::default()
        };
        let index_bytes = index.encode_to_vec();
        let index_path = root.join("index.scip");
        std::fs::write(&index_path, &index_bytes).map_err(|error| error.to_string())?;
        let manifest = serde_json::json!({
            "schema_version": "ripr-scip-admission-manifest-v1",
            "index_sha256": hex_sha256(&index_bytes),
            "producer": { "name": "fixture-scip", "version": "0.0.1" },
            "project_root_relative": project_root_relative,
            "toolchain_fingerprint": "stable-fixture",
            "source_files": [
                { "relative_path": "src/lib.rs", "source_sha256": source_digest }
            ]
        });
        let manifest_path = root.join("manifest.json");
        std::fs::write(&manifest_path, manifest.to_string()).map_err(|error| error.to_string())?;
        Ok((index_path, manifest_path))
    }

    fn write_minimal_fixture(root: &Path) -> Result<(PathBuf, PathBuf), String> {
        write_minimal_fixture_with_project_root(root, "crate", true)
    }

    #[test]
    fn admits_matching_index_with_stable_portable_identity() -> Result<(), String> {
        let temp = temp_dir("admit")?;
        let (index, manifest) = write_minimal_fixture(&temp)?;
        let first = admit_scip_index(&temp, &index, &manifest).map_err(|error| error.reason())?;
        let second = admit_scip_index(&temp, &index, &manifest).map_err(|error| error.reason())?;
        if first.portable_identity != second.portable_identity {
            return Err("portable identity changed across identical admission".to_string());
        }
        if first.symbol_count != 1 {
            return Err(format!("expected one symbol, got {}", first.symbol_count));
        }
        Ok(())
    }

    #[test]
    fn rejects_source_swapped_manifest() -> Result<(), String> {
        let temp = temp_dir("reject-src")?;
        let (index, manifest) = write_minimal_fixture(&temp)?;
        std::fs::write(
            temp.join("crate/src/lib.rs"),
            "pub fn hello() -> i32 { 1 }\n",
        )
        .map_err(|error| error.to_string())?;
        let error = admit_scip_index(&temp, &index, &manifest)
            .err()
            .ok_or_else(|| "expected source digest mismatch".to_string())?;
        if !matches!(error, ScipAdmissionError::SourceDigestMismatch { .. }) {
            return Err(format!("unexpected error: {}", error.reason()));
        }
        Ok(())
    }

    #[test]
    fn admits_repeated_symbol_references_and_three_element_ranges() -> Result<(), String> {
        let temp = temp_dir("repeat-ref")?;
        let src_dir = temp.join("crate/src");
        std::fs::create_dir_all(&src_dir).map_err(|error| error.to_string())?;
        let source = "pub fn hello() -> i32 { hello() + 1 }\n";
        std::fs::write(src_dir.join("lib.rs"), source).map_err(|error| error.to_string())?;
        let source_digest = hex_sha256(source.as_bytes());
        let symbol = "scip-test hello#".to_string();
        let index = Index {
            metadata: Some(Metadata {
                version: ProtocolVersion::UnspecifiedProtocolVersion as i32,
                tool_info: Some(ToolInfo {
                    name: "fixture-scip".to_string(),
                    version: "0.0.1".to_string(),
                    arguments: Vec::new(),
                }),
                project_root: "file:///fixture/".to_string(),
                text_document_encoding: TextEncoding::Utf8 as i32,
            }),
            documents: vec![Document {
                relative_path: "src/lib.rs".to_string(),
                occurrences: vec![
                    Occurrence {
                        symbol: symbol.clone(),
                        range: vec![0, 3, 8],
                        ..Occurrence::default()
                    },
                    Occurrence {
                        symbol,
                        range: vec![0, 20, 25],
                        ..Occurrence::default()
                    },
                ],
                ..Document::default()
            }],
            ..Index::default()
        };
        let index_bytes = index.encode_to_vec();
        let index_path = temp.join("index.scip");
        std::fs::write(&index_path, &index_bytes).map_err(|error| error.to_string())?;
        let manifest = serde_json::json!({
            "schema_version": "ripr-scip-admission-manifest-v1",
            "index_sha256": hex_sha256(&index_bytes),
            "producer": { "name": "fixture-scip", "version": "0.0.1" },
            "project_root_relative": "crate",
            "toolchain_fingerprint": "stable-fixture",
            "source_files": [
                { "relative_path": "src/lib.rs", "source_sha256": source_digest }
            ]
        });
        let manifest_path = temp.join("manifest.json");
        std::fs::write(&manifest_path, manifest.to_string()).map_err(|error| error.to_string())?;
        let admitted =
            admit_scip_index(&temp, &index_path, &manifest_path).map_err(|error| error.reason())?;
        if admitted.symbol_count != 1 {
            return Err(format!(
                "expected one unique symbol, got {}",
                admitted.symbol_count
            ));
        }
        Ok(())
    }

    #[test]
    fn rejects_modified_index_bytes() -> Result<(), String> {
        let temp = temp_dir("reject-idx")?;
        let (index, manifest) = write_minimal_fixture(&temp)?;
        std::fs::write(&index, b"tampered").map_err(|error| error.to_string())?;
        let error = admit_scip_index(&temp, &index, &manifest)
            .err()
            .ok_or_else(|| "expected index digest mismatch".to_string())?;
        if !matches!(error, ScipAdmissionError::IndexDigestMismatch) {
            return Err(format!("unexpected error: {}", error.reason()));
        }
        Ok(())
    }

    #[test]
    fn rejects_unbound_index_document_without_manifest_source() -> Result<(), String> {
        let temp = temp_dir("unbound-doc")?;
        let (index, manifest) = write_minimal_fixture(&temp)?;
        let mut manifest_value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        manifest_value["source_files"] = serde_json::json!([]);
        std::fs::write(&manifest, manifest_value.to_string()).map_err(|e| e.to_string())?;
        let error = admit_scip_index(&temp, &index, &manifest)
            .err()
            .ok_or_else(|| "expected unbound index document".to_string())?;
        if !matches!(error, ScipAdmissionError::UnboundIndexDocument { .. }) {
            return Err(format!("unexpected error: {}", error.reason()));
        }
        Ok(())
    }

    #[test]
    fn rejects_absolute_project_root_relative() -> Result<(), String> {
        let temp = temp_dir("abs-root")?;
        let (index, manifest) = write_minimal_fixture_with_project_root(&temp, "/etc", false)?;
        let error = admit_scip_index(&temp, &index, &manifest)
            .err()
            .ok_or_else(|| "expected project root escape".to_string())?;
        if !matches!(error, ScipAdmissionError::ProjectRootEscape) {
            return Err(format!("unexpected error: {}", error.reason()));
        }
        Ok(())
    }

    #[test]
    fn rejects_parent_traversal_in_project_root_relative() -> Result<(), String> {
        let temp = temp_dir("dotdot-root")?;
        let (index, manifest) =
            write_minimal_fixture_with_project_root(&temp, "../outside", false)?;
        let error = admit_scip_index(&temp, &index, &manifest)
            .err()
            .ok_or_else(|| "expected project root escape".to_string())?;
        if !matches!(error, ScipAdmissionError::ProjectRootEscape) {
            return Err(format!("unexpected error: {}", error.reason()));
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_project_root_that_escapes_workspace() -> Result<(), String> {
        use std::os::unix::fs::symlink;
        let temp = temp_dir("symlink-root")?;
        let outside = temp_dir("outside")?;
        std::fs::create_dir_all(outside.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(outside.join("src/lib.rs"), "pub fn hello() {}\n")
            .map_err(|error| error.to_string())?;
        symlink(&outside, temp.join("escape_link")).map_err(|error| error.to_string())?;
        let (index, manifest) =
            write_minimal_fixture_with_project_root(&temp, "escape_link", true)?;
        let error = admit_scip_index(&temp, &index, &manifest)
            .err()
            .ok_or_else(|| "expected project root escape via symlink".to_string())?;
        if !matches!(error, ScipAdmissionError::ProjectRootEscape) {
            return Err(format!("unexpected error: {}", error.reason()));
        }
        Ok(())
    }
}
