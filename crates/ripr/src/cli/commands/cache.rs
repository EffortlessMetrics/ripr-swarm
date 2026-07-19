use serde_json::json;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CacheStatus {
    state: &'static str,
    total_size_bytes: u64,
    entry_count: usize,
}

fn inspect_cache_dir(cache_dir: &Path) -> CacheStatus {
    let metadata = match std::fs::metadata(cache_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return CacheStatus {
                state: "not_found",
                total_size_bytes: 0,
                entry_count: 0,
            };
        }
        Err(_) => {
            return CacheStatus {
                state: "unavailable",
                total_size_bytes: 0,
                entry_count: 0,
            };
        }
    };
    if !metadata.is_dir() {
        return CacheStatus {
            state: "unavailable",
            total_size_bytes: 0,
            entry_count: 0,
        };
    }

    let mut total_size_bytes = 0u64;
    let mut entry_count = 0usize;
    let mut partially_readable = false;
    let mut stack = vec![cache_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            partially_readable = true;
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                partially_readable = true;
                continue;
            };
            let Ok(metadata) = entry.metadata() else {
                partially_readable = true;
                continue;
            };
            let file_type = metadata.file_type();
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                total_size_bytes = total_size_bytes.saturating_add(metadata.len());
                entry_count = entry_count.saturating_add(1);
            }
        }
    }

    CacheStatus {
        state: if partially_readable { "partial" } else { "ok" },
        total_size_bytes,
        entry_count,
    }
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: ripr cache status [--json]");
        return Ok(());
    }

    let Some((subcommand, rest)) = args.split_first() else {
        return Err("cache requires subcommand `status`".to_string());
    };

    if subcommand != "status" {
        return Err(format!(
            "unknown cache subcommand {subcommand:?}; expected `status`"
        ));
    }

    let is_json = rest.iter().any(|arg| arg == "--json");
    let current_dir =
        std::env::current_dir().map_err(|e| format!("failed to get current dir: {}", e))?;
    let cache_dir = current_dir.join("target").join("ripr").join("cache");
    let cache_dir_str = cache_dir.display().to_string();

    let status = inspect_cache_dir(&cache_dir);

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "cache_dir": cache_dir_str,
                "status": status.state,
                "total_size_bytes": status.total_size_bytes,
                "entry_count": status.entry_count
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        println!("Cache dir: {}", cache_dir_str);
        println!("Status: {}", status.state);
        println!("Total size: {} bytes", status.total_size_bytes);
        println!("Entries: {}", status.entry_count);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("ripr-cache-{label}-{nonce}"))
    }

    #[test]
    fn missing_cache_is_reported_without_fabricated_counts() -> Result<(), String> {
        let path = temp_dir("missing");
        let status = inspect_cache_dir(&path);
        let expected = CacheStatus {
            state: "not_found",
            total_size_bytes: 0,
            entry_count: 0,
        };
        if status != expected {
            return Err(format!("expected {expected:?}, got {status:?}"));
        }
        Ok(())
    }

    #[test]
    fn nested_cache_files_are_counted_and_marked_ok() -> Result<(), String> {
        let root = temp_dir("nested");
        fs::create_dir_all(root.join("nested")).map_err(|error| error.to_string())?;
        fs::write(root.join("one"), b"123").map_err(|error| error.to_string())?;
        fs::write(root.join("nested").join("two"), b"4567").map_err(|error| error.to_string())?;

        let status = inspect_cache_dir(&root);
        let cleanup = fs::remove_dir_all(&root).map_err(|error| error.to_string());
        cleanup?;
        if status.state != "ok" {
            return Err(format!("expected ok status, got {:?}", status.state));
        }
        if status.total_size_bytes != 7 {
            return Err(format!("expected 7 bytes, got {}", status.total_size_bytes));
        }
        if status.entry_count != 2 {
            return Err(format!("expected 2 entries, got {}", status.entry_count));
        }
        Ok(())
    }

    #[test]
    fn cache_file_is_not_treated_as_a_directory() -> Result<(), String> {
        let path = temp_dir("file");
        fs::write(&path, b"not a cache directory").map_err(|error| error.to_string())?;
        let status = inspect_cache_dir(&path);
        let cleanup = fs::remove_file(&path).map_err(|error| error.to_string());
        cleanup?;
        if status.state != "unavailable" {
            return Err(format!(
                "expected unavailable status, got {:?}",
                status.state
            ));
        }
        Ok(())
    }
}
