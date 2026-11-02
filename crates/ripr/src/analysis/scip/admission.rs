//! Bounded admission of one caller-supplied SCIP index plus a content-bound
//! companion manifest (#8115).

use super::generated::scip::{Document, Index, TextEncoding};
use prost::Message;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Component, Path};

/// Maximum bytes read from a caller-supplied SCIP index file.
pub(crate) const MAX_SCIP_INDEX_BYTES: u64 = 32 * 1024 * 1024;

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
    ManifestParse(String),
    ManifestSchema(String),
    IndexDigestMismatch,
    IndexDecode(String),
    ProjectRootEscape,
    UnsupportedTextEncoding(TextEncoding),
    SourceDigestMismatch { path: String },
    MissingSourceFile { path: String },
    DuplicateSymbol { symbol: String },
    MalformedRange { path: String, detail: String },
}

impl ScipAdmissionError {
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::Io(message) => format!("io: {message}"),
            Self::OversizedIndex => "scip index exceeds size bound".to_string(),
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
            Self::DuplicateSymbol { symbol } => format!("duplicate symbol {symbol}"),
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
    let index_bytes = read_bounded_file(index_path, MAX_SCIP_INDEX_BYTES)?;
    verify_index_digest(&manifest, &index_bytes)?;
    let index = Index::decode(index_bytes.as_slice())
        .map_err(|error| ScipAdmissionError::IndexDecode(error.to_string()))?;
    verify_project_root(workspace_root, &manifest)?;
    verify_sources(workspace_root, &manifest)?;
    let facts = project_facts(&index, &manifest)?;
    Ok(facts)
}

fn read_manifest(path: &Path) -> Result<ScipAdmissionManifest, ScipAdmissionError> {
    let text =
        std::fs::read_to_string(path).map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
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

fn read_bounded_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, ScipAdmissionError> {
    let metadata =
        std::fs::metadata(path).map_err(|error| ScipAdmissionError::Io(error.to_string()))?;
    if metadata.len() > max_bytes {
        return Err(ScipAdmissionError::OversizedIndex);
    }
    std::fs::read(path).map_err(|error| ScipAdmissionError::Io(error.to_string()))
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
    if manifest.project_root_relative.contains('\\')
        || manifest
            .project_root_relative
            .split('/')
            .any(|segment| segment == "..")
    {
        return Err(ScipAdmissionError::ProjectRootEscape);
    }
    let joined = workspace_root.join(&manifest.project_root_relative);
    if joined
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ScipAdmissionError::ProjectRootEscape);
    }
    Ok(())
}

fn verify_sources(
    workspace_root: &Path,
    manifest: &ScipAdmissionManifest,
) -> Result<(), ScipAdmissionError> {
    for entry in &manifest.source_files {
        if entry.relative_path.contains('\\') {
            return Err(ScipAdmissionError::ProjectRootEscape);
        }
        let path = workspace_root
            .join(&manifest.project_root_relative)
            .join(&entry.relative_path);
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(ScipAdmissionError::ProjectRootEscape);
        }
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
    let mut symbol_count = 0usize;
    for document in &index.documents {
        document_count += 1;
        validate_document_ranges(document)?;
        for occurrence in &document.occurrences {
            if !occurrence.symbol.is_empty() {
                if !symbols.insert(occurrence.symbol.clone()) {
                    return Err(ScipAdmissionError::DuplicateSymbol {
                        symbol: occurrence.symbol.clone(),
                    });
                }
                symbol_count += 1;
            }
        }
    }
    let portable_identity =
        portable_fact_identity(manifest, &symbols, document_count, symbol_count);
    Ok(AdmittedScipFacts {
        portable_identity,
        index_digest: manifest.index_sha256.clone(),
        producer_name: manifest.producer.name.clone(),
        producer_version: manifest.producer.version.clone(),
        document_count,
        symbol_count,
    })
}

fn validate_document_ranges(document: &Document) -> Result<(), ScipAdmissionError> {
    for occurrence in &document.occurrences {
        for range in occurrence.range.chunks(4) {
            if range.len() != 4 {
                return Err(ScipAdmissionError::MalformedRange {
                    path: document.relative_path.clone(),
                    detail: "range length not a multiple of four".to_string(),
                });
            }
            let line = range[0];
            let character = range[1];
            if line < 0 || character < 0 {
                return Err(ScipAdmissionError::MalformedRange {
                    path: document.relative_path.clone(),
                    detail: "negative line or character".to_string(),
                });
            }
        }
    }
    Ok(())
}

fn portable_fact_identity(
    manifest: &ScipAdmissionManifest,
    symbols: &BTreeSet<String>,
    document_count: usize,
    symbol_count: usize,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest.index_sha256.as_bytes());
    hasher.update(manifest.toolchain_fingerprint.as_bytes());
    hasher.update(document_count.to_string().as_bytes());
    hasher.update(symbol_count.to_string().as_bytes());
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

    fn write_minimal_fixture(root: &Path) -> Result<(PathBuf, PathBuf), String> {
        let src_dir = root.join("crate/src");
        std::fs::create_dir_all(&src_dir).map_err(|error| error.to_string())?;
        let source = "pub fn hello() -> &'static str { \"héllo\" }\n";
        std::fs::write(src_dir.join("lib.rs"), source).map_err(|error| error.to_string())?;
        let source_digest = hex_sha256(source.as_bytes());
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
            "project_root_relative": "crate",
            "toolchain_fingerprint": "stable-fixture",
            "source_files": [
                { "relative_path": "src/lib.rs", "source_sha256": source_digest }
            ]
        });
        let manifest_path = root.join("manifest.json");
        std::fs::write(&manifest_path, manifest.to_string()).map_err(|error| error.to_string())?;
        Ok((index_path, manifest_path))
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
}
