//! Count and digest of raw Git path-record inventories (#4006 item 6).
//!
//! The digest is over NUL-terminated decoded path bytes, not LF-joined text:
//! a Git path may contain a newline but never a NUL, so the record grammar
//! stays unambiguous for names such as `a\nb`.

use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PathInventory {
    pub(crate) path_record_count: usize,
    pub(crate) path_record_sha256: String,
    pub(crate) path_record_recipe: String,
}

/// Count and digest a decoded path inventory, sorted bytewise and
/// deduplicated here so the digest does not depend on caller order. `source` names the Git command (and any set
/// operation) that produced the records, so the retained recipe is exact.
/// Omitting, adding or splitting one path changes the count and digest.
pub(crate) fn path_inventory(paths: &[String], source: &str) -> PathInventory {
    let mut sorted = paths.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut hasher = Sha256::new();
    for path in &sorted {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
    }
    PathInventory {
        path_record_count: sorted.len(),
        path_record_sha256: format!("sha256:{:x}", hasher.finalize()),
        path_record_recipe: format!(
            "{source}; decoded UTF-8 paths sorted bytewise and deduplicated; each path followed by one NUL byte; SHA-256"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn newline_names_do_not_collide_with_split_names() {
        let joined = path_inventory(&paths(&["a\nb", "c"]), "test");
        let split = path_inventory(&paths(&["a", "b\nc"]), "test");
        assert_eq!(joined.path_record_count, split.path_record_count);
        assert_ne!(joined.path_record_sha256, split.path_record_sha256);
    }

    #[test]
    fn digest_is_order_independent_and_deduplicated() {
        let one = path_inventory(&paths(&["b", "a", "a"]), "test");
        let two = path_inventory(&paths(&["a", "b"]), "test");
        assert_eq!(one.path_record_count, 2);
        assert_eq!(one.path_record_sha256, two.path_record_sha256);
    }

    #[test]
    fn serialized_shape_is_pinned() -> Result<(), String> {
        let value = serde_json::to_value(path_inventory(&paths(&["a"]), "cmd"))
            .map_err(|error| error.to_string())?;
        let mut keys: Vec<&str> = value
            .as_object()
            .ok_or("not an object")?
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "path_record_count",
                "path_record_recipe",
                "path_record_sha256"
            ]
        );
        Ok(())
    }
}
