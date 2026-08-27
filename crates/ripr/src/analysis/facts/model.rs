use crate::domain::{OracleKind, OracleStrength, SymbolId};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct WorkspaceRootAuthority {
    pub(crate) root: PathBuf,
    pub(crate) workspace_identity: String,
    pub(crate) files: BTreeMap<PathBuf, WorkspaceFileAuthority>,
    #[serde(skip, default)]
    current_files: Arc<Mutex<BTreeMap<PathBuf, (String, bool)>>>,
}

impl Clone for WorkspaceRootAuthority {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            workspace_identity: self.workspace_identity.clone(),
            files: self.files.clone(),
            current_files: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

impl PartialEq for WorkspaceRootAuthority {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && self.workspace_identity == other.workspace_identity
            && self.files == other.files
    }
}

impl Eq for WorkspaceRootAuthority {}

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
            let path_valid = is_relative_without_parent(relative)
                && canonical_root
                    .join(relative)
                    .canonicalize()
                    .map(|path| path.starts_with(&canonical_root))
                    .unwrap_or(false);
            let (package_identity, package_valid) =
                match resolve_package_identity(&canonical_root, relative) {
                    PackageIdentity::Known(identity) => (identity, true),
                    PackageIdentity::UnreadableManifest { manifest } => (
                        format!(
                            "manifest-unreadable:{}:{}",
                            relative_path(&canonical_root, &manifest),
                            relative_path(&canonical_root, &canonical_root.join(relative)),
                        ),
                        false,
                    ),
                };
            authorities.insert(
                relative.clone(),
                WorkspaceFileAuthority {
                    source_digest: source_digest(facts.source.as_bytes()),
                    package_identity,
                    valid: path_valid && package_valid,
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
            current_files: Arc::new(Mutex::new(BTreeMap::new())),
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
        let test_current = self.current_file_is_current(test_file, file);
        let seam_current = self.current_file_is_current(seam_file, seam);
        test_current && seam_current && source_digest(source.as_bytes()) == file.source_digest
    }

    fn current_file_is_current(&self, path: &Path, authority: &WorkspaceFileAuthority) -> bool {
        let fingerprint = filesystem_fingerprint(&self.root, path);
        if let Ok(cache) = self.current_files.lock() {
            if let Some((cached_fingerprint, valid)) = cache.get(path) {
                if cached_fingerprint == &fingerprint {
                    return *valid;
                }
            }
        }
        let valid = authority.valid
            && self.root.join(path).canonicalize().is_ok_and(|full| {
                full.starts_with(&self.root)
                    && std::fs::read(&full)
                        .map(|bytes| source_digest(&bytes) == authority.source_digest)
                        .unwrap_or(false)
                    && matches!(
                        resolve_package_identity(&self.root, path),
                        PackageIdentity::Known(ref identity)
                            if identity == &authority.package_identity
                    )
            });
        if let Ok(mut cache) = self.current_files.lock() {
            cache.insert(path.to_path_buf(), (fingerprint, valid));
        }
        valid
    }
}

fn filesystem_fingerprint(root: &Path, relative: &Path) -> String {
    let mut fingerprint = String::new();
    let source = root.join(relative);
    append_metadata_fingerprint(&mut fingerprint, &source);
    let mut cursor = source.parent().map(Path::to_path_buf);
    while let Some(directory) = cursor {
        append_metadata_fingerprint(&mut fingerprint, &directory.join("Cargo.toml"));
        if directory == root {
            break;
        }
        cursor = directory.parent().map(Path::to_path_buf);
    }
    fingerprint
}

fn append_metadata_fingerprint(output: &mut String, path: &Path) {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            use std::time::UNIX_EPOCH;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok());
            output.push_str(&format!(
                "{}:{}:{:?};",
                path.display(),
                metadata.len(),
                modified.map(|time| (time.as_secs(), time.subsec_nanos()))
            ));
        }
        Err(error) => output.push_str(&format!("{}:{:?};", path.display(), error.kind())),
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

#[derive(Debug, PartialEq, Eq)]
enum PackageIdentity {
    Known(String),
    UnreadableManifest { manifest: PathBuf },
}

fn resolve_package_identity(root: &Path, relative: &Path) -> PackageIdentity {
    resolve_package_identity_with_reader(root, relative, |path| std::fs::read(path))
}

fn resolve_package_identity_with_reader<F>(
    root: &Path,
    relative: &Path,
    mut read_manifest: F,
) -> PackageIdentity
where
    F: FnMut(&Path) -> std::io::Result<Vec<u8>>,
{
    let mut cursor = root.join(relative).parent().map(Path::to_path_buf);
    while let Some(directory) = cursor {
        let manifest = directory.join("Cargo.toml");
        match read_manifest(&manifest) {
            Ok(bytes) => {
                let relative_manifest = relative_path(root, &manifest);
                return PackageIdentity::Known(format!(
                    "{relative_manifest}:{}",
                    source_digest(&bytes)
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => return PackageIdentity::UnreadableManifest { manifest },
        }
        if directory == root {
            break;
        }
        cursor = directory.parent().map(Path::to_path_buf);
    }
    let containing = relative.parent().unwrap_or_else(|| Path::new("."));
    PackageIdentity::Known(format!(
        "directory:{}",
        containing.to_string_lossy().replace('\\', "/")
    ))
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RustIndex {
    pub files: BTreeMap<PathBuf, FileFacts>,
    pub tests: Vec<TestFact>,
    pub functions: Vec<FunctionFact>,
    #[serde(default)]
    pub include_parents: BTreeMap<PathBuf, PathBuf>,
    #[serde(default)]
    pub include_limitations: Vec<RustIncludeLimitation>,
    #[serde(default)]
    pub(crate) workspace_authority: Option<WorkspaceRootAuthority>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RustIncludeLimitation {
    pub parent: PathBuf,
    pub line: usize,
    pub expression: String,
    pub reason_code: String,
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
    use std::io::ErrorKind;

    #[test]
    fn rust_index_default_has_empty_fact_sets() {
        let index = RustIndex::default();
        assert!(index.files.is_empty());
        assert!(index.tests.is_empty());
        assert!(index.functions.is_empty());
        assert!(index.include_parents.is_empty());
        assert!(index.include_limitations.is_empty());
    }

    #[test]
    fn legacy_rust_index_deserializes_with_empty_include_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let index: RustIndex = serde_json::from_str(
            r#"{"files":{},"tests":[],"functions":[],"workspace_authority":null}"#,
        )?;
        assert!(index.include_parents.is_empty());
        assert!(index.include_limitations.is_empty());
        Ok(())
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

    #[test]
    fn permission_denied_manifest_is_not_treated_as_absent() {
        let root = Path::new("workspace");
        let relative = Path::new("pkg/src/lib.rs");
        let identity = resolve_package_identity_with_reader(root, relative, |manifest| {
            if manifest.ends_with(Path::new("pkg/Cargo.toml")) {
                Err(std::io::Error::from(ErrorKind::PermissionDenied))
            } else {
                Err(std::io::Error::from(ErrorKind::NotFound))
            }
        });

        assert!(matches!(
            identity,
            PackageIdentity::UnreadableManifest { manifest }
                if manifest.ends_with(Path::new("pkg/Cargo.toml"))
        ));
    }

    #[test]
    fn unreadable_manifest_invalidates_authority_entries() -> Result<(), Box<dyn std::error::Error>>
    {
        struct FixtureCleanup(PathBuf);
        impl Drop for FixtureCleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let root = std::env::temp_dir().join(format!(
            "ripr-authority-unreadable-manifest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let _cleanup = FixtureCleanup(root.clone());
        std::fs::create_dir_all(root.join("pkg/src"))?;
        std::fs::create_dir_all(root.join("pkg/tests"))?;
        // Reading a directory as Cargo.toml is a deterministic non-NotFound
        // manifest error on every supported platform.
        std::fs::create_dir(root.join("pkg/Cargo.toml"))?;

        let sources = [
            (
                PathBuf::from("pkg/src/lib.rs"),
                "pub fn source() -> i32 { 1 }\n",
            ),
            (
                PathBuf::from("pkg/tests/lib.rs"),
                "#[test]\nfn source_test() { assert_eq!(1, 1); }\n",
            ),
        ];
        for (path, source) in &sources {
            std::fs::write(root.join(path), source)?;
        }
        let files = sources
            .iter()
            .map(|(path, source)| {
                (
                    path.clone(),
                    FileFacts {
                        path: path.clone(),
                        source: (*source).to_string(),
                        ..FileFacts::default()
                    },
                )
            })
            .collect();
        let authority = WorkspaceRootAuthority::from_index(&root, &files);
        let source = Path::new("pkg/src/lib.rs");
        let test = Path::new("pkg/tests/lib.rs");
        let source_authority = authority.files.get(source).ok_or("missing source")?;
        let test_authority = authority.files.get(test).ok_or("missing test")?;

        assert!(!source_authority.valid);
        assert!(!test_authority.valid);
        assert_ne!(
            source_authority.package_identity,
            test_authority.package_identity
        );
        assert!(!authority.validates_target(source, test, "pub fn source() -> i32 { 1 }\n"));
        Ok(())
    }
    #[test]
    fn manifest_added_after_index_invalidates_target() -> Result<(), Box<dyn std::error::Error>> {
        struct FixtureCleanup(PathBuf);
        impl Drop for FixtureCleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let root = std::env::temp_dir().join(format!(
            "ripr-authority-manifest-race-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let _cleanup = FixtureCleanup(root.clone());
        std::fs::create_dir_all(root.join("pkg/src"))?;
        std::fs::create_dir_all(root.join("pkg/tests"))?;
        std::fs::write(root.join("pkg/Cargo.toml"), "[package]\nname = \"pkg\"\n")?;

        let sources = [
            (
                PathBuf::from("pkg/src/lib.rs"),
                "pub fn source() -> i32 { 1 }\n",
            ),
            (
                PathBuf::from("pkg/tests/lib.rs"),
                "#[test]\nfn source_test() { assert_eq!(1, 1); }\n",
            ),
        ];
        for (path, source) in &sources {
            std::fs::write(root.join(path), source)?;
        }
        let files = sources
            .iter()
            .map(|(path, source)| {
                (
                    path.clone(),
                    FileFacts {
                        path: path.clone(),
                        source: (*source).to_string(),
                        ..FileFacts::default()
                    },
                )
            })
            .collect();
        let authority = WorkspaceRootAuthority::from_index(&root, &files);
        std::fs::write(
            root.join("pkg/tests/Cargo.toml"),
            "[package]\nname = \"nested-tests\"\n",
        )?;

        assert!(!authority.validates_target(
            Path::new("pkg/tests/lib.rs"),
            Path::new("pkg/src/lib.rs"),
            "#[test]\nfn source_test() { assert_eq!(1, 1); }\n"
        ));
        Ok(())
    }

    #[test]
    fn source_change_invalidates_cached_currentness() -> Result<(), Box<dyn std::error::Error>> {
        struct FixtureCleanup(PathBuf);
        impl Drop for FixtureCleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let root = std::env::temp_dir().join(format!(
            "ripr-authority-cache-invalidation-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let _cleanup = FixtureCleanup(root.clone());
        std::fs::create_dir_all(root.join("pkg/src"))?;
        std::fs::create_dir_all(root.join("pkg/tests"))?;
        std::fs::write(root.join("pkg/Cargo.toml"), "[package]\nname = \"pkg\"\n")?;
        let sources = [
            (
                PathBuf::from("pkg/src/lib.rs"),
                "pub fn source() -> i32 { 1 }\n",
            ),
            (
                PathBuf::from("pkg/tests/lib.rs"),
                "#[test]\nfn source_test() { assert_eq!(1, 1); }\n",
            ),
        ];
        let files = sources
            .iter()
            .map(|(path, source)| {
                (
                    path.clone(),
                    FileFacts {
                        path: path.clone(),
                        source: (*source).to_string(),
                        ..FileFacts::default()
                    },
                )
            })
            .collect();
        let authority = WorkspaceRootAuthority::from_index(&root, &files);
        let test = Path::new("pkg/tests/lib.rs");
        let source = Path::new("pkg/src/lib.rs");
        assert!(root.join(test).is_file());
        assert!(root.join(source).is_file());
        assert!(authority.files.get(test).is_some_and(|file| file.valid));
        assert!(authority.files.get(source).is_some_and(|file| file.valid));
        assert!(
            authority.validates_target(test, source, sources[1].1),
            "authority should accept unchanged materialized files"
        );
        std::fs::write(root.join("pkg/tests/lib.rs"), "changed\n")?;
        assert!(!authority.validates_target(test, source, sources[1].1));
        Ok(())
    }
}
