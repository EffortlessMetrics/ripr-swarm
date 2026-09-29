//! Source-state subject stamp for editor-consumed gap artifacts (#4544).
//!
//! A gap decision ledger or actionable-gaps report names files and lines in
//! the workspace it was computed from. Nothing about a line number says which
//! revision it belongs to, so after a branch switch, edit, or commit an editor
//! consumer could place branch A's gap at `anchor.file:anchor.line` on branch
//! B's file and present it as current. The producer therefore stamps the
//! artifact with a `source_subject`: the content digest of every workspace
//! file its records make claims about. Consumers recompute those digests from
//! the current workspace; a mismatch, a deleted file, or a missing stamp means
//! the artifact no longer describes the files in front of the reader.
//!
//! The stamp shape is shared by the Rust ledger writer, the xtask
//! actionable-gaps writer (which writes the same shape), and the LSP
//! validator in `lsp::gap_artifacts`, which is the only consumer authority.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Component, Path};

/// The only digest algorithm a `source_subject` stamp may declare.
pub(crate) const SOURCE_SUBJECT_DIGEST_ALGORITHM: &str = "sha256";

/// Artifact-level source-state identity: one entry per workspace file the
/// artifact's records name, sorted by path.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct GapSourceSubject {
    pub(crate) digest_algorithm: String,
    pub(crate) files: Vec<GapSourceSubjectFile>,
}

/// One stamped workspace file. `digest` is `sha256:<hex>` of the file bytes
/// when the artifact was written, or `null` when the named file was absent.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct GapSourceSubjectFile {
    pub(crate) path: String,
    pub(crate) digest: Option<String>,
}

/// The result of comparing a stamp with the current workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceSubjectCheck {
    /// Every stamped file still has its stamped content.
    Current,
    /// A stamped file changed, was deleted, or appeared since the artifact was
    /// written. Carries the repo-relative path of the first differing file.
    Stale(String),
    /// The artifact cannot be matched to the current workspace: the stamp is
    /// missing, malformed, uses an unknown digest, omits a file the records
    /// name, or a stamped file cannot be read.
    Unverifiable(&'static str),
}

/// `sha256:<hex>` of one workspace file, `Ok(None)` when it does not exist.
pub(crate) fn source_file_digest(root: &Path, relative: &str) -> Result<Option<String>, String> {
    match std::fs::read(root.join(relative)) {
        Ok(bytes) => Ok(Some(format!("sha256:{:x}", Sha256::digest(bytes)))),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("read source subject file {relative} failed: {err}")),
    }
}

/// Normalize a record path to the repo-relative spelling the stamp uses. A
/// `path::test_name` selector keeps its file part. Absolute paths inside
/// `root` are made relative; anything outside the workspace, traversing, or
/// blank yields `None`.
pub(crate) fn subject_relative_path(root: &Path, raw: &str) -> Option<String> {
    let file = raw.split_once("::").map_or(raw, |(file, _)| file).trim();
    if file.is_empty() || file.contains('\n') || file.contains('\r') {
        return None;
    }
    let normalized = file.replace('\\', "/");
    let path = Path::new(&normalized);
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// Files a gap decision ledger record makes claims about: the anchor the
/// diagnostic is placed on and the repair route's target and related test.
pub(crate) fn gap_record_subject_paths(root: &Path, record: &Value) -> BTreeSet<String> {
    subject_paths(
        root,
        record,
        &[
            &["anchor", "file"],
            &["repair_route", "target_file"],
            &["repair_route", "related_test"],
        ],
    )
}

/// Files an actionable-gaps packet anchors its claim on: the changed source
/// file, the primary anchor file the editor places the gap on, and the related
/// test file whose missing discriminator the packet describes.
pub(crate) fn actionable_packet_subject_paths(root: &Path, packet: &Value) -> BTreeSet<String> {
    subject_paths(
        root,
        packet,
        &[
            &["source_file"],
            &["primary_anchor", "file"],
            &["related_test_or_observer", "file"],
        ],
    )
}

fn subject_paths(root: &Path, value: &Value, fields: &[&[&str]]) -> BTreeSet<String> {
    fields
        .iter()
        .filter_map(|path| {
            let mut current = value;
            for key in *path {
                current = current.get(*key)?;
            }
            current.as_str()
        })
        .filter_map(|raw| subject_relative_path(root, raw))
        .collect()
}

/// Stamp the given repo-relative files with their current content digests.
pub(crate) fn stamp_source_subject(
    root: &Path,
    paths: &BTreeSet<String>,
) -> Result<GapSourceSubject, String> {
    let files = paths
        .iter()
        .map(|path| {
            Ok(GapSourceSubjectFile {
                path: path.clone(),
                digest: source_file_digest(root, path)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(GapSourceSubject {
        digest_algorithm: SOURCE_SUBJECT_DIGEST_ALGORITHM.to_string(),
        files,
    })
}

/// Compare an artifact's `source_subject` with the current workspace.
/// `required` is every file the artifact's records name; each must be
/// stamped, so a stamp cannot vouch for a subset of the claim.
pub(crate) fn check_source_subject(
    root: &Path,
    stamp: Option<&Value>,
    required: &BTreeSet<String>,
) -> SourceSubjectCheck {
    let Some(stamp) = stamp else {
        return SourceSubjectCheck::Unverifiable("source_subject_missing");
    };
    let Ok(subject) = serde_json::from_value::<GapSourceSubject>(stamp.clone()) else {
        return SourceSubjectCheck::Unverifiable("source_subject_malformed");
    };
    if subject.digest_algorithm != SOURCE_SUBJECT_DIGEST_ALGORITHM {
        return SourceSubjectCheck::Unverifiable("source_subject_unsupported_digest");
    }
    let mut stamped = BTreeSet::new();
    for file in &subject.files {
        if subject_relative_path(root, &file.path).as_deref() != Some(file.path.as_str()) {
            return SourceSubjectCheck::Unverifiable("source_subject_malformed");
        }
        stamped.insert(file.path.as_str());
    }
    if required.iter().any(|path| !stamped.contains(path.as_str())) {
        return SourceSubjectCheck::Unverifiable("source_subject_incomplete");
    }
    for file in &subject.files {
        match source_file_digest(root, &file.path) {
            Ok(current) if current == file.digest => {}
            Ok(_) => return SourceSubjectCheck::Stale(file.path.clone()),
            Err(_) => return SourceSubjectCheck::Unverifiable("source_subject_unreadable"),
        }
    }
    SourceSubjectCheck::Current
}

/// Test support: stamp a hand-built ledger or actionable-gaps value against
/// `root` exactly as the producers do, so fixtures that exercise other
/// validation rules carry a current `source_subject`.
#[cfg(test)]
pub(crate) fn with_source_subject_for_test(root: &Path, mut artifact: Value) -> Value {
    let mut paths = BTreeSet::new();
    for key in ["records", "gap_records"] {
        if let Some(records) = artifact.get(key).and_then(Value::as_array) {
            for record in records {
                paths.extend(gap_record_subject_paths(root, record));
            }
        }
    }
    if let Some(packets) = artifact.get("packets").and_then(Value::as_array) {
        for packet in packets {
            paths.extend(actionable_packet_subject_paths(root, packet));
        }
    }
    if let (Ok(stamp), Some(object)) = (
        stamp_source_subject(root, &paths)
            .and_then(|stamp| serde_json::to_value(stamp).map_err(|err| err.to_string())),
        artifact.as_object_mut(),
    ) {
        object.insert("source_subject".to_string(), stamp);
    }
    artifact
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> Result<std::path::PathBuf, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("system time before epoch: {err}"))?;
        let root = std::env::temp_dir().join(format!(
            "ripr-gap-source-subject-{label}-{}-{}",
            std::process::id(),
            now.as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir: {err}"))?;
        Ok(root)
    }

    fn stamp_value(root: &Path, paths: &BTreeSet<String>) -> Result<Value, String> {
        serde_json::to_value(stamp_source_subject(root, paths)?).map_err(|err| err.to_string())
    }

    #[test]
    fn subject_relative_path_normalizes_selectors_and_rejects_escape() {
        let root = Path::new("/repo");
        assert_eq!(
            subject_relative_path(root, "tests/pricing.rs::discount_threshold"),
            Some("tests/pricing.rs".to_string())
        );
        assert_eq!(
            subject_relative_path(root, "./src\\lib.rs"),
            Some("src/lib.rs".to_string())
        );
        assert_eq!(
            subject_relative_path(root, "/repo/src/lib.rs"),
            Some("src/lib.rs".to_string())
        );
        assert_eq!(subject_relative_path(root, "/elsewhere/src/lib.rs"), None);
        assert_eq!(subject_relative_path(root, "../src/lib.rs"), None);
        assert_eq!(subject_relative_path(root, "  "), None);
    }

    #[test]
    fn source_subject_stays_current_until_a_stamped_file_changes() -> Result<(), String> {
        let root = temp_root("edit")?;
        std::fs::write(root.join("src/lib.rs"), "fn a() {}\n").map_err(|err| err.to_string())?;
        let paths = BTreeSet::from(["src/lib.rs".to_string()]);
        let stamp = stamp_value(&root, &paths)?;
        assert_eq!(
            check_source_subject(&root, Some(&stamp), &paths),
            SourceSubjectCheck::Current
        );
        std::fs::write(root.join("src/lib.rs"), "fn b() {}\n").map_err(|err| err.to_string())?;
        assert_eq!(
            check_source_subject(&root, Some(&stamp), &paths),
            SourceSubjectCheck::Stale("src/lib.rs".to_string())
        );
        std::fs::remove_file(root.join("src/lib.rs")).map_err(|err| err.to_string())?;
        assert_eq!(
            check_source_subject(&root, Some(&stamp), &paths),
            SourceSubjectCheck::Stale("src/lib.rs".to_string())
        );
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn source_subject_absent_file_stamp_goes_stale_when_the_file_appears() -> Result<(), String> {
        let root = temp_root("appear")?;
        let paths = BTreeSet::from(["src/new.rs".to_string()]);
        let stamp = stamp_value(&root, &paths)?;
        assert_eq!(stamp["files"][0]["digest"], Value::Null);
        assert_eq!(
            check_source_subject(&root, Some(&stamp), &paths),
            SourceSubjectCheck::Current
        );
        std::fs::write(root.join("src/new.rs"), "fn c() {}\n").map_err(|err| err.to_string())?;
        assert_eq!(
            check_source_subject(&root, Some(&stamp), &paths),
            SourceSubjectCheck::Stale("src/new.rs".to_string())
        );
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn source_subject_missing_malformed_or_incomplete_is_unverifiable() -> Result<(), String> {
        let root = temp_root("unverifiable")?;
        std::fs::write(root.join("src/lib.rs"), "fn a() {}\n").map_err(|err| err.to_string())?;
        let paths = BTreeSet::from(["src/lib.rs".to_string()]);
        assert_eq!(
            check_source_subject(&root, None, &paths),
            SourceSubjectCheck::Unverifiable("source_subject_missing")
        );
        assert_eq!(
            check_source_subject(&root, Some(&json!({"files": "x"})), &paths),
            SourceSubjectCheck::Unverifiable("source_subject_malformed")
        );
        assert_eq!(
            check_source_subject(
                &root,
                Some(&json!({"digest_algorithm": "md5", "files": []})),
                &paths
            ),
            SourceSubjectCheck::Unverifiable("source_subject_unsupported_digest")
        );
        assert_eq!(
            check_source_subject(
                &root,
                Some(&json!({"digest_algorithm": "sha256", "files": []})),
                &paths
            ),
            SourceSubjectCheck::Unverifiable("source_subject_incomplete")
        );
        assert_eq!(
            check_source_subject(
                &root,
                Some(&json!({
                    "digest_algorithm": "sha256",
                    "files": [{"path": "../outside.rs", "digest": null}]
                })),
                &BTreeSet::new()
            ),
            SourceSubjectCheck::Unverifiable("source_subject_malformed")
        );
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn subject_paths_name_the_anchor_route_and_packet_files() {
        let root = Path::new("/repo");
        let record = json!({
            "anchor": {"file": "src/pricing.rs", "line": 42},
            "repair_route": {
                "target_file": "tests/pricing.rs",
                "related_test": "tests/pricing.rs::discount_threshold"
            }
        });
        assert_eq!(
            gap_record_subject_paths(root, &record),
            BTreeSet::from(["src/pricing.rs".to_string(), "tests/pricing.rs".to_string()])
        );
        let packet = json!({
            "source_file": "src/pricing.rs",
            "primary_anchor": {"file": "src/lib.rs", "line": 3}
        });
        assert_eq!(
            actionable_packet_subject_paths(root, &packet),
            BTreeSet::from(["src/lib.rs".to_string(), "src/pricing.rs".to_string()])
        );
    }
}
