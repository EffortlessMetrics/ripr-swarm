use std::path::{Component, Path, PathBuf};
use tower_lsp_server::ls_types::Uri;

mod percent_codec {
    pub(super) fn decode_uri_path(path: &str) -> Option<String> {
        let bytes = path.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' {
                let high = hex_value(*bytes.get(index + 1)?)?;
                let low = hex_value(*bytes.get(index + 2)?)?;
                decoded.push((high << 4) | low);
                index += 3;
            } else {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
        String::from_utf8(decoded).ok()
    }

    pub(super) fn encode_uri_path(path: &str) -> String {
        let mut encoded = String::new();
        for byte in path.bytes() {
            match byte {
                b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'/'
                | b':' => encoded.push(byte as char),
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        encoded
    }

    fn hex_value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
}

mod windows_paths {
    pub(super) fn is_windows_drive_uri_path(path: &str) -> bool {
        let bytes = path.as_bytes();
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic()
    }

    pub(super) fn is_windows_drive_path(path: &str) -> bool {
        let bytes = path.as_bytes();
        bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
    }
}

pub(super) fn file_uri_for_path(path: &Path) -> Result<Uri, String> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let encoded = encode_uri_path(&normalized);
    let uri = if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    };
    uri.parse()
        .map_err(|err| format!("failed to build LSP file URI for {}: {err}", path.display()))
}

pub(super) fn path_from_file_uri(uri: &Uri) -> Option<PathBuf> {
    normalized_file_uri_path(uri).map(PathBuf::from)
}

/// Prove that a projected path stays inside the selected workspace root.
/// Existing paths are canonicalized so symlink/junction escapes are rejected;
/// missing paths fall back to normalized lexical containment for diagnostics
/// and command payloads that refer to a future file.
pub(super) fn path_is_within_root(root: &Path, path: &Path) -> bool {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let root = canonical_or_normalized(root);
    let candidate = canonical_or_normalized(&candidate);
    paths_equal_or_below(&root, &candidate)
}

pub(super) fn file_uri_is_within_root(root: &Path, uri: &Uri) -> bool {
    path_from_file_uri(uri).is_some_and(|path| path_is_within_root(root, &path))
}

pub(super) fn file_uris_match(left: &Uri, right: &Uri) -> bool {
    if left == right {
        return true;
    }
    let Some(left_path) = normalized_file_uri_path(left) else {
        return false;
    };
    let Some(right_path) = normalized_file_uri_path(right) else {
        return false;
    };
    if windows_paths::is_windows_drive_path(&left_path)
        && windows_paths::is_windows_drive_path(&right_path)
    {
        return left_path.eq_ignore_ascii_case(&right_path);
    }
    left_path == right_path
}

fn normalized_file_uri_path(uri: &Uri) -> Option<String> {
    let raw = uri.as_str();
    let path = raw.strip_prefix("file://")?;
    let decoded = percent_codec::decode_uri_path(path)?;
    let path = if windows_paths::is_windows_drive_uri_path(&decoded) {
        decoded[1..].to_string()
    } else {
        decoded
    };
    Some(path.replace('\\', "/"))
}

/// Render a path with forward slashes for LSP display (diagnostic messages,
/// hover text, context packets). Replaces native OS separators so Windows
/// backslash paths display consistently. Previously duplicated as
/// `display_lsp_path` in both `backend.rs` and `diagnostics.rs`.
pub(super) fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Resolve a path to an absolute PathBuf, joining to `root` when relative.
/// Previously duplicated as `absolute_context_path` in `backend.rs` and
/// `absolute_path` in `diagnostics.rs` (byte-identical bodies, different names).
pub(super) fn absolute_join(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Byte cap for report artifacts read by the long-running LSP server, checked
/// from file metadata before reading. Real repo-exposure artifacts for large
/// repositories can reach tens of megabytes; 256 MiB is far above any
/// legitimate artifact while still failing closed on an unbounded input.
/// Mirrors the CLI's `MAX_AGENT_VERIFY_SNAPSHOT_BYTES` (#2921).
pub(super) const MAX_LSP_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

/// Outcome of a capped artifact read. Callers must distinguish an absent
/// artifact — a normal state for deferred analysis output, where falling back
/// or returning no data is honest — from a present-but-unusable artifact
/// (oversize, unreadable, or non-UTF-8), which requires a typed degradation
/// instead of a silent `None`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum CappedArtifactRead {
    Contents(String),
    Missing,
    Unusable,
}

/// Read a repo-controlled report artifact with a metadata pre-check and a
/// byte cap, failing closed. Follows `read_agent_verify_snapshot`'s shape.
pub(super) fn read_artifact_capped(path: &Path) -> CappedArtifactRead {
    read_artifact_capped_with_limit(path, MAX_LSP_ARTIFACT_BYTES)
}

/// `limit` is a parameter so tests can exercise the cap without materializing
/// 256 MiB; production callers use [`read_artifact_capped`].
pub(super) fn read_artifact_capped_with_limit(path: &Path, limit: u64) -> CappedArtifactRead {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return CappedArtifactRead::Missing;
        }
        Err(_) => return CappedArtifactRead::Unusable,
    };
    if metadata.len() > limit {
        return CappedArtifactRead::Unusable;
    }
    match std::fs::read_to_string(path) {
        Ok(contents) => CappedArtifactRead::Contents(contents),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => CappedArtifactRead::Missing,
        Err(_) => CappedArtifactRead::Unusable,
    }
}

fn canonical_or_normalized(path: &Path) -> PathBuf {
    canonicalize_with_missing_tail(path).unwrap_or_else(|| normalize_path(path))
}

fn canonicalize_with_missing_tail(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        if let Ok(mut canonical) = current.canonicalize() {
            for component in missing.iter().rev() {
                canonical.push(component);
            }
            return Some(canonical);
        }

        let component = current.file_name()?.to_os_string();
        missing.push(component);
        if !current.pop() {
            return None;
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn paths_equal_or_below(root: &Path, candidate: &Path) -> bool {
    if cfg!(windows) {
        let root_components = root
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>();
        let candidate_components = candidate
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>();
        candidate_components.len() >= root_components.len()
            && candidate_components[..root_components.len()] == root_components[..]
    } else {
        candidate == root || candidate.starts_with(root)
    }
}

pub(super) fn encode_uri_path(path: &str) -> String {
    percent_codec::encode_uri_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_uri(value: &str) -> Result<Uri, String> {
        value
            .parse()
            .map_err(|err| format!("failed to parse test URI {value}: {err}"))
    }

    #[test]
    fn file_uri_for_path_percent_encodes_spaces_and_symbols() -> Result<(), String> {
        let uri = file_uri_for_path(Path::new("/tmp/ripr fixtures/a#b?.rs"))?;

        assert_eq!(uri.as_str(), "file:///tmp/ripr%20fixtures/a%23b%3F.rs");
        assert_eq!(
            path_from_file_uri(&uri).ok_or("expected decoded path")?,
            PathBuf::from("/tmp/ripr fixtures/a#b?.rs")
        );
        Ok(())
    }

    #[test]
    fn file_uri_for_path_percent_encodes_unicode_relative_paths() -> Result<(), String> {
        let uri = file_uri_for_path(Path::new("workspace/ripr/src/cafe_menu.rs"))?;
        assert_eq!(uri.as_str(), "file:///workspace/ripr/src/cafe_menu.rs");

        let uri = file_uri_for_path(Path::new("workspace/ripr/src/café.rs"))?;
        assert_eq!(uri.as_str(), "file:///workspace/ripr/src/caf%C3%A9.rs");
        Ok(())
    }

    #[test]
    fn invalid_percent_encoding_is_not_a_file_path() -> Result<(), String> {
        let uri = parse_uri("file:///tmp/%FF.rs")?;

        assert_eq!(path_from_file_uri(&uri), None);
        Ok(())
    }

    #[test]
    fn path_from_file_uri_rejects_non_file_scheme() -> Result<(), String> {
        let uri = parse_uri("https://example.test/src.rs")?;

        assert_eq!(path_from_file_uri(&uri), None);
        Ok(())
    }

    #[test]
    fn file_uris_match_normalizes_percent_encoded_separators() -> Result<(), String> {
        let encoded_separator = parse_uri("file:///workspace/ripr/src%2Flib.rs")?;
        let literal_separator = parse_uri("file:///workspace/ripr/src/lib.rs")?;

        assert!(file_uris_match(&encoded_separator, &literal_separator));
        Ok(())
    }

    #[test]
    fn windows_drive_file_uris_match_case_insensitively() -> Result<(), String> {
        let upper = parse_uri("file:///C:/Work/Ripr/src/lib.rs")?;
        let lower = parse_uri("file:///c:/Work/Ripr/src/lib.rs")?;

        assert!(file_uris_match(&upper, &lower));
        Ok(())
    }

    #[test]
    fn file_uris_match_keeps_non_windows_paths_case_sensitive() -> Result<(), String> {
        let upper = parse_uri("file:///workspace/ripr/src/Lib.rs")?;
        let lower = parse_uri("file:///workspace/ripr/src/lib.rs")?;

        assert!(!file_uris_match(&upper, &lower));
        Ok(())
    }

    #[test]
    fn path_is_within_root_rejects_traversal_and_foreign_absolute_paths() {
        let root = Path::new("/workspace/ripr");
        assert!(path_is_within_root(root, Path::new("src/lib.rs")));
        assert!(path_is_within_root(
            root,
            Path::new("/workspace/ripr/src/lib.rs")
        ));
        assert!(!path_is_within_root(root, Path::new("../outside.rs")));
        assert!(!path_is_within_root(root, Path::new("/workspace/other.rs")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_drive_root_containment_uses_path_components() {
        let separator = std::path::MAIN_SEPARATOR;
        let root = PathBuf::from(format!("C:{separator}"));
        let lower_case_child = PathBuf::from(format!("c:{separator}workspace{separator}src.rs"));
        let workspace = PathBuf::from(format!("C:{separator}workspace"));
        let workspace_sibling =
            PathBuf::from(format!("C:{separator}workspace-sibling{separator}src.rs"));
        assert!(paths_equal_or_below(&root, &lower_case_child));
        let c_child = PathBuf::from(format!(
            "C:{separator}workspace{separator}src{separator}lib.rs"
        ));
        let d_child = PathBuf::from(format!(
            "D:{separator}workspace{separator}src{separator}lib.rs"
        ));
        assert!(paths_equal_or_below(&root, &c_child));
        assert!(paths_equal_or_below(&root, &root));
        assert!(!paths_equal_or_below(&workspace, &workspace_sibling));
        assert!(!paths_equal_or_below(&root, &d_child));
    }

    #[test]
    fn path_is_within_root_rejects_missing_leaf_under_symlink_ancestor() -> Result<(), String> {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-uri-root-{suffix}"));
        let outside = std::env::temp_dir().join(format!("ripr-uri-outside-{suffix}"));
        std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        std::fs::create_dir_all(&outside).map_err(|err| err.to_string())?;
        let link = root.join("linked");

        #[cfg(unix)]
        let link_result = std::os::unix::fs::symlink(&outside, &link);
        #[cfg(windows)]
        let link_result = std::os::windows::fs::symlink_dir(&outside, &link);
        if let Err(err) = link_result {
            eprintln!("skipping symlink containment test: {err}");
            let _ = std::fs::remove_dir_all(&root);
            let _ = std::fs::remove_dir_all(&outside);
            return Ok(());
        }

        assert!(!path_is_within_root(&root, Path::new("linked/missing.rs")));
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
        std::fs::remove_dir_all(&outside).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[test]
    fn file_uri_is_within_root_rejects_non_file_and_foreign_uris() -> Result<(), String> {
        let root = Path::new("/workspace/ripr");
        let inside = parse_uri("file:///workspace/ripr/src/lib.rs")?;
        let outside = parse_uri("file:///workspace/other/src/lib.rs")?;
        let foreign = parse_uri("https://example.test/src/lib.rs")?;
        assert!(file_uri_is_within_root(root, &inside));
        assert!(!file_uri_is_within_root(root, &outside));
        assert!(!file_uri_is_within_root(root, &foreign));
        Ok(())
    }

    #[test]
    fn read_artifact_capped_reads_small_file_and_reports_missing() -> Result<(), String> {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ripr-uri-capped-read-{suffix}"));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let path = dir.join("artifact.json");
        std::fs::write(&path, "{\"records\":[]}").map_err(|err| err.to_string())?;

        assert_eq!(
            read_artifact_capped(&path),
            CappedArtifactRead::Contents("{\"records\":[]}".to_string())
        );
        assert_eq!(
            read_artifact_capped(&dir.join("absent.json")),
            CappedArtifactRead::Missing
        );

        std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[test]
    fn read_artifact_capped_with_limit_rejects_oversize_and_non_utf8() -> Result<(), String> {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ripr-uri-capped-limit-{suffix}"));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let path = dir.join("artifact.json");
        std::fs::write(&path, "abcd").map_err(|err| err.to_string())?;

        // Four bytes is under any real cap but over a 3-byte test limit.
        assert_eq!(
            read_artifact_capped_with_limit(&path, 3),
            CappedArtifactRead::Unusable
        );
        assert_eq!(
            read_artifact_capped_with_limit(&path, 4),
            CappedArtifactRead::Contents("abcd".to_string())
        );

        let binary_path = dir.join("artifact.bin");
        std::fs::write(&binary_path, [0xff, 0xfe, 0xfd]).map_err(|err| err.to_string())?;
        assert_eq!(
            read_artifact_capped_with_limit(&binary_path, 1024),
            CappedArtifactRead::Unusable
        );

        std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(())
    }
}
