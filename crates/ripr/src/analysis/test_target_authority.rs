//! Producer-owned authority for existing Rust test targets.

use super::facts::RustIndex;
use crate::domain::{SourceCurrentness, SymbolId};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TestTargetAuthority {
    pub(crate) workspace_root_identity: String,
    pub(crate) source_currentness: SourceCurrentness,
}

pub(crate) fn validate_target(
    index: &RustIndex,
    file: &Path,
    symbol_id: &SymbolId,
    start_line: usize,
    body: &str,
) -> Option<TestTargetAuthority> {
    if !is_portable_relative_path(file) || body.is_empty() {
        return None;
    }
    let facts = index.files.get(file)?;
    let body_offset = facts.source.find(body)?;
    if facts.path != file
        || facts.source[..body_offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1
            != start_line
    {
        return None;
    }
    if index
        .functions
        .iter()
        .filter(|function| function.id == *symbol_id)
        .count()
        != 1
    {
        return None;
    }
    Some(TestTargetAuthority {
        workspace_root_identity: workspace_identity(index),
        source_currentness: SourceCurrentness::CandidateCurrent,
    })
}

fn is_portable_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn workspace_identity(index: &RustIndex) -> String {
    let mut digest = Sha256::new();
    for (path, facts) in &index.files {
        digest.update(path.to_string_lossy().replace('\\', "/").as_bytes());
        digest.update([0]);
        digest.update(facts.source.as_bytes());
        digest.update([0]);
    }
    format!("sha256:{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::{FileFacts, FunctionFact};
    use std::path::PathBuf;

    fn index(path: &str, source: &str) -> RustIndex {
        let path = PathBuf::from(path);
        let function = FunctionFact {
            id: SymbolId("target".into()),
            name: "target".into(),
            file: path.clone(),
            start_line: 1,
            end_line: 1,
            body: source.into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            is_test: true,
            attrs: vec![],
        };
        RustIndex {
            files: [(
                path.clone(),
                FileFacts {
                    path,
                    functions: vec![function.clone()],
                    source: source.into(),
                    ..FileFacts::default()
                },
            )]
            .into_iter()
            .collect(),
            functions: vec![function],
            ..RustIndex::default()
        }
    }

    #[test]
    fn rejects_absolute_and_traversal_paths() {
        let index = index("tests/target.rs", "fn target() {}");
        for path in [
            PathBuf::from("/tmp/target.rs"),
            PathBuf::from("../target.rs"),
        ] {
            assert!(
                validate_target(
                    &index,
                    &path,
                    &SymbolId("target".into()),
                    1,
                    "fn target() {}"
                )
                .is_none()
            );
        }
    }

    #[test]
    fn identity_is_stable_when_root_is_relocated() {
        let first = validate_target(
            &index("tests/target.rs", "fn target() {}"),
            Path::new("tests/target.rs"),
            &SymbolId("target".into()),
            1,
            "fn target() {}",
        )
        .unwrap();
        let second = validate_target(
            &index("tests/target.rs", "fn target() {}"),
            Path::new("tests/target.rs"),
            &SymbolId("target".into()),
            1,
            "fn target() {}",
        )
        .unwrap();
        assert_eq!(
            first.workspace_root_identity,
            second.workspace_root_identity
        );
        assert_eq!(
            first.source_currentness,
            SourceCurrentness::CandidateCurrent
        );
    }
}
