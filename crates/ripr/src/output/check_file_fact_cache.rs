//! Ordinary `check --diff` projection of producer-owned Rust file-fact cache
//! stats (#4975). Verbose human stderr and `--json --verbose` only; default
//! JSON stays unchanged.

use crate::analysis::seam_cache::FileFactCacheStats;

/// One concise `-v` stderr line. `None` when the producer reported no cache work
/// (including explicit [`FileFactCacheStats::zero_work`]).
pub(crate) fn render_verbose_line(stats: &FileFactCacheStats) -> Option<String> {
    if !stats.did_work() {
        return None;
    }
    let mut line = format!(
        "rust file facts: {} reused, {} recomputed, {} stored",
        stats.hits, stats.misses, stats.stores
    );
    match stats.corrupt_ignored {
        0 => {}
        1 => line.push_str("; 1 corrupt entry ignored"),
        count => line.push_str(&format!("; {count} corrupt entries ignored")),
    }
    if !stats.invalidated_files.is_empty() {
        line.push_str(&format!(
            "; {} content-invalidated",
            stats.invalidated_files.len()
        ));
    }
    Some(line)
}

/// Append additive verbose-only JSON fields for file-fact cache telemetry.
pub(crate) fn append_verbose_json_fields(out: &mut String, stats: Option<&FileFactCacheStats>) {
    use crate::output::json::formatter::field;

    out.push_str(",\n");
    match stats {
        None => {
            out.push_str("  \"file_fact_cache\": null,\n");
            field(out, 1, "file_fact_cache_limitation", "not_observed", false);
        }
        Some(stats) if !stats.did_work() => {
            out.push_str("  \"file_fact_cache\": null,\n");
            field(out, 1, "file_fact_cache_limitation", "zero_work", false);
        }
        Some(stats) => {
            let receipt = crate::analysis::seam_inventory::file_fact_cache_receipt(stats);
            let serialized = serde_json::to_string(&receipt).unwrap_or_else(|error| {
                format!(
                    "{{\"serialization_error\":{}}}",
                    serde_json::Value::String(error.to_string())
                )
            });
            out.push_str("  \"file_fact_cache\": ");
            out.push_str(&serialized);
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::seam_cache::{FileFactCacheStats, FileFactStoreError, FileFactStoreStage};
    use std::path::PathBuf;

    #[test]
    fn verbose_line_names_reused_and_recomputed() -> Result<(), String> {
        let cold = FileFactCacheStats {
            misses: 2,
            stores: 2,
            ..FileFactCacheStats::default()
        };
        let warm = FileFactCacheStats {
            hits: 2,
            ..FileFactCacheStats::default()
        };
        let cold_line = render_verbose_line(&cold).ok_or("cold line")?;
        let warm_line = render_verbose_line(&warm).ok_or("warm line")?;
        if !cold_line.contains("0 reused, 2 recomputed") {
            return Err(format!("cold: {cold_line}"));
        }
        if !warm_line.contains("2 reused, 0 recomputed") {
            return Err(format!("warm: {warm_line}"));
        }
        Ok(())
    }

    #[test]
    fn verbose_json_carries_receipt_and_store_failures() -> Result<(), String> {
        let mut stats = FileFactCacheStats::default();
        stats.misses = 1;
        stats.stores = 1;
        stats.record_store_failure(
            PathBuf::from("src/lib.rs"),
            FileFactStoreError {
                stage: FileFactStoreStage::Write,
                message: "disk full".to_string(),
            },
        );
        let mut out = String::from("{");
        append_verbose_json_fields(&mut out, Some(&stats));
        out.push('}');
        let value: serde_json::Value =
            serde_json::from_str(&out).map_err(|error| error.to_string())?;
        if value["file_fact_cache"]["misses"] != 1 {
            return Err(format!("misses: {}", value["file_fact_cache"]["misses"]));
        }
        if value["file_fact_cache"]["store_errors"] != 1 {
            return Err(format!(
                "store_errors: {}",
                value["file_fact_cache"]["store_errors"]
            ));
        }
        Ok(())
    }
}
