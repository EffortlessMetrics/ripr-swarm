use crate::domain::{OracleKind, OracleStrength, SymbolId};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WorkspaceRootAuthority {
    pub(crate) root: PathBuf,
    pub(crate) workspace_identity: String,
    pub(crate) files: BTreeMap<PathBuf, WorkspaceFileAuthority>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WorkspaceFileAuthority {
    pub(crate) source_digest: String,
    pub(crate) package_identity: String,
    pub(crate) valid: bool,
}

impl WorkspaceRootAuthority {
    pub(crate) fn from_index(root: &Path, files: &BTreeMap<PathBuf, FileFacts>) -> Self {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut authorities = BTreeMap::new();
        for (relative, facts) in files {
            let valid = is_relative_without_parent(relative)
                && canonical_root
                    .join(relative)
                    .canonicalize()
                    .map(|path| path.starts_with(&canonical_root))
                    .unwrap_or(false);
            authorities.insert(
                relative.clone(),
                WorkspaceFileAuthority {
                    source_digest: source_digest(facts.source.as_bytes()),
                    package_identity: package_identity(&canonical_root, relative),
                    valid,
                },
            );
        }
        let mut canonical = String::new();
        for (path, authority) in &authorities {
            canonical.push_str(&path.to_string_lossy().replace('\\', "/"));
            canonical.push('\0');
            canonical.push_str(&authority.source_digest);
            canonical.push('\0');
            canonical.push_str(&authority.package_identity);
            canonical.push('\n');
        }
        Self {
            root: canonical_root,
            workspace_identity: source_digest(canonical.as_bytes()),
            files: authorities,
        }
    }

    pub(crate) fn validates_target(
        &self,
        test_file: &Path,
        seam_file: &Path,
        source: &str,
    ) -> bool {
        let Some(file) = self.files.get(test_file) else {
            return false;
        };
        let Some(seam) = self.files.get(seam_file) else {
            return false;
        };
        if !file.valid || !seam.valid || file.package_identity != seam.package_identity {
            return false;
        }
        let full = self.root.join(test_file);
        let Ok(current) = std::fs::read(&full) else {
            return false;
        };
        current == source.as_bytes() && source_digest(&current) == file.source_digest
    }
}

fn source_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{digest:x}")
}

fn is_relative_without_parent(path: &Path) -> bool {
    !path.is_absolute()
        && path.components().all(|component| {
            !matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn package_identity(root: &Path, relative: &Path) -> String {
    let mut cursor = root.join(relative).parent().map(Path::to_path_buf);
    while let Some(directory) = cursor {
        let manifest = directory.join("Cargo.toml");
        if let Ok(bytes) = std::fs::read(&manifest) {
            let relative_manifest = manifest
                .strip_prefix(root)
                .unwrap_or(manifest.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            return format!("{relative_manifest}:{}", source_digest(&bytes));
        }
        if directory == root {
            break;
        }
        cursor = directory.parent().map(Path::to_path_buf);
    }
    let containing = relative.parent().unwrap_or_else(|| Path::new("."));
    format!(
        "directory:{}",
        containing.to_string_lossy().replace('\\', "/")
    )
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RustIndex {
    pub files: BTreeMap<PathBuf, FileFacts>,
    pub tests: Vec<TestFact>,
    pub functions: Vec<FunctionFact>,
    #[serde(default)]
    pub(crate) workspace_authority: Option<WorkspaceRootAuthority>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileFacts {
    pub path: PathBuf,
    pub functions: Vec<FunctionFact>,
    pub tests: Vec<TestFact>,
    pub calls: Vec<CallFact>,
    pub returns: Vec<ReturnFact>,
    pub literals: Vec<LiteralFact>,
    pub probe_shapes: Vec<ProbeShapeFact>,
    /// True when parser-backed syntax failed and lexical fallback produced these facts.
    /// Lexical fallback intentionally emits no probe shapes and may under-credit repo seams.
    pub used_lexical_fallback: bool,
    /// Original file source text. Held so `analysis/value-extraction-v2`
    /// can scan for top-level `const`/`static` declarations without
    /// re-reading the file at evidence-build time. Not part of any
    /// cached envelope (the cache stores `ClassifiedSeam` only).
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FunctionFact {
    pub id: SymbolId,
    pub name: String,
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub body: String,
    pub calls: Vec<CallFact>,
    pub returns: Vec<ReturnFact>,
    pub literals: Vec<LiteralFact>,
    pub is_test: bool,
    /// Attribute syntax lines (e.g., `#[rstest]`, `#[case(100, 100)]`,
    /// `#[test]`) captured from the AST `attrs()` iterator. Used by
    /// `analysis/value-extraction-v2` to read rstest case parameters
    /// without re-reading the file. The lexical fallback path
    /// populates this as empty.
    pub attrs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TestFact {
    pub name: String,
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub body: String,
    pub calls: Vec<CallFact>,
    pub assertions: Vec<OracleFact>,
    pub literals: Vec<LiteralFact>,
    /// Attribute syntax lines on the test fn. Mirrors
    /// `FunctionFact.attrs`. Carries `#[rstest]` and `#[case(...)]` for
    /// case-driven tests so value resolution can map case literals to
    /// test parameters.
    pub attrs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OracleFact {
    pub line: usize,
    pub text: String,
    pub kind: OracleKind,
    pub strength: OracleStrength,
    pub observed_tokens: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CallFact {
    pub line: usize,
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReturnFact {
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LiteralFact {
    pub line: usize,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProbeShapeFact {
    pub start_line: usize,
    pub end_line: usize,
    /// Byte offset of the shape's start within the source file. Populated
    /// by the parser-backed summarizer; the lexical fallback emits no
    /// probe shapes at all, so this stays accurate.
    pub start_byte: usize,
    pub kind: String,
    pub text: String,
}

pub type FunctionSummary = FunctionFact;
pub type TestSummary = TestFact;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_index_default_has_empty_fact_sets() {
        let index = RustIndex::default();
        assert!(index.files.is_empty());
        assert!(index.tests.is_empty());
        assert!(index.functions.is_empty());
    }

    #[test]
    fn file_facts_default_has_empty_collections() {
        let facts = FileFacts::default();
        assert!(facts.path.as_os_str().is_empty());
        assert!(facts.functions.is_empty());
        assert!(facts.tests.is_empty());
        assert!(facts.calls.is_empty());
        assert!(facts.returns.is_empty());
        assert!(facts.literals.is_empty());
        assert!(facts.probe_shapes.is_empty());
        assert!(!facts.used_lexical_fallback);
    }

    #[test]
    fn fact_types_clone_and_compare_equal_for_simple_samples() {
        let call = CallFact {
            line: 1,
            name: "test_fn".to_string(),
            text: "test_fn()".to_string(),
        };
        let call_cloned = call.clone();
        assert_eq!(call, call_cloned);

        let ret = ReturnFact {
            line: 2,
            text: "return Ok(())".to_string(),
        };
        let ret_cloned = ret.clone();
        assert_eq!(ret, ret_cloned);

        let lit = LiteralFact {
            line: 3,
            value: "42".to_string(),
        };
        let lit_cloned = lit.clone();
        assert_eq!(lit, lit_cloned);
    }

    #[test]
    fn probe_shape_fact_preserves_span_kind_text_and_start_byte() {
        let shape = ProbeShapeFact {
            start_line: 10,
            end_line: 12,
            start_byte: 256,
            kind: "predicate".to_string(),
            text: "x > 0".to_string(),
        };
        assert_eq!(shape.start_line, 10);
        assert_eq!(shape.end_line, 12);
        assert_eq!(shape.start_byte, 256);
        assert_eq!(shape.kind, "predicate");
        assert_eq!(shape.text, "x > 0");
    }
}
