//! Typed custody of the ledger's range-touched role (#4006).
//! This is the union of per-record paths, never a net source/swarm diff.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathCustody {
    pub(crate) range_touched: RangeTouched,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RangeTouched {
    repository: String,
    base_sha: String,
    cut_sha: String,
    cut_tree_sha: String,
    path_record_count: u64,
    path_record_sha256: String,
    path_record_recipe: String,
    denominator_packet_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathComparison {
    status: String,
    expected: PathCustody,
    observed: PathCustody,
}

impl PathCustody {
    /// Inputs are the already native-admitted manifest and retained #2768 raw
    /// packet. A candidate-provided outer preflight digest cannot replace them.
    pub(crate) fn compare(
        &self,
        manifest: &Value,
        denominator: &[u8],
        denominator_sha256: &str,
        candidate_sha: &str,
        candidate_tree: &str,
    ) -> Result<PathComparison, String> {
        if format!("{:x}", Sha256::digest(denominator)) != denominator_sha256 {
            return Err("path custody denominator raw digest differs".into());
        }
        let packet: Value = serde_json::from_slice(denominator)
            .map_err(|error| format!("path custody denominator JSON: {error}"))?;
        let ledger = packet
            .get("ledger")
            .filter(|row| row.is_object())
            .unwrap_or(&packet);
        let range = &manifest["range"];
        let base = range["last_integrated_swarm_parent"]
            .as_str()
            .ok_or("path custody manifest range base missing")?;
        let recipe = super::path_inventory::path_inventory(
            &[],
            "release-ledger capture: union of per-record changed paths across base..cut",
        )
        .path_record_recipe;
        if ledger["source"]["base_sha"] != base
            || ledger["source"]["cut_sha"] != candidate_sha
            || ledger["source"]["cut_tree_sha"] != candidate_tree
        {
            return Err("path custody denominator base/cut/tree differs".into());
        }
        let inventory = &ledger["path_inventories"]["range_touched"];
        let count = inventory["path_record_count"]
            .as_u64()
            .ok_or("path custody range_touched missing count; missing is not empty")?;
        let hash = inventory["path_record_sha256"]
            .as_str()
            .ok_or("path custody range_touched missing digest")?;
        let hex = hash.strip_prefix("sha256:").unwrap_or_default();
        let empty = format!("sha256:{:x}", Sha256::digest([]));
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || (count == 0) != (hash == empty)
            || inventory["path_record_recipe"] != recipe
            || range["path_record_count"] != count
            || range["path_record_sha256"] != hex
            || range["path_record_recipe"] != recipe
        {
            return Err("path custody manifest/ledger count, digest or recipe differs".into());
        }
        let expected = Self {
            range_touched: RangeTouched {
                repository: "EffortlessMetrics/ripr-swarm".into(),
                base_sha: base.into(),
                cut_sha: candidate_sha.into(),
                cut_tree_sha: candidate_tree.into(),
                path_record_count: count,
                path_record_sha256: hash.into(),
                path_record_recipe: recipe,
                denominator_packet_sha256: denominator_sha256.into(),
            },
        };
        if *self != expected {
            return Err(
                "qualification path custody differs from selected #2768 range_touched".into(),
            );
        }
        Ok(PathComparison {
            status: "accepted".into(),
            expected,
            observed: self.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_inventory_is_explicit_and_raw_denominator_tamper_refuses() -> Result<(), String> {
        let base = "1".repeat(40);
        let sha = "2".repeat(40);
        let tree = "3".repeat(40);
        let empty = format!("{:x}", Sha256::digest([]));
        let recipe = super::super::path_inventory::path_inventory(
            &[],
            "release-ledger capture: union of per-record changed paths across base..cut",
        )
        .path_record_recipe;
        let manifest = json!({"range":{"last_integrated_swarm_parent":base,
            "path_record_count":0,"path_record_sha256":empty,"path_record_recipe":recipe}});
        let mut ledger = json!({"source":{"base_sha":base,"cut_sha":sha,"cut_tree_sha":tree},
            "path_inventories":{"range_touched":{"path_record_count":0,
                "path_record_sha256":format!("sha256:{empty}"),"path_record_recipe":recipe}}});
        let bytes = serde_json::to_vec(&ledger).map_err(|error| error.to_string())?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let custody: PathCustody = serde_json::from_value(json!({"range_touched":{
            "repository":"EffortlessMetrics/ripr-swarm","base_sha":base,"cut_sha":sha,"cut_tree_sha":tree,
            "path_record_count":0,"path_record_sha256":format!("sha256:{empty}"),"path_record_recipe":recipe,
            "denominator_packet_sha256":hash}})).map_err(|error| error.to_string())?;
        custody.compare(&manifest, &bytes, &hash, &sha, &tree)?;
        assert!(
            custody
                .compare(&manifest, b"malformed", &hash, &sha, &tree)
                .is_err()
        );
        let unsupported = b"\xff";
        let unsupported_hash = format!("{:x}", Sha256::digest(unsupported));
        let error = custody
            .compare(&manifest, unsupported, &unsupported_hash, &sha, &tree)
            .err()
            .ok_or("accepted non-UTF-8 denominator as empty")?;
        assert!(error.contains("denominator JSON"));
        for (pointer, replacement) in [
            ("/source/base_sha", json!("4".repeat(40))),
            ("/source/cut_sha", json!("4".repeat(40))),
            ("/source/cut_tree_sha", json!("4".repeat(40))),
            (
                "/path_inventories/range_touched/path_record_count",
                json!(1),
            ),
            (
                "/path_inventories/range_touched/path_record_recipe",
                json!("LF"),
            ),
            ("/path_inventories/range_touched", Value::Null),
        ] {
            let original = ledger.clone();
            *ledger
                .pointer_mut(pointer)
                .ok_or("fixture pointer missing")? = replacement;
            let altered = serde_json::to_vec(&ledger).map_err(|error| error.to_string())?;
            let altered_hash = format!("{:x}", Sha256::digest(&altered));
            assert!(
                custody
                    .compare(&manifest, &altered, &altered_hash, &sha, &tree)
                    .is_err(),
                "accepted {pointer}"
            );
            ledger = original;
        }
        Ok(())
    }
}
