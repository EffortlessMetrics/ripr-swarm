use serde_json::json;

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

    if !cache_dir.exists() {
        if is_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "cache_dir": cache_dir_str,
                    "status": "not_found"
                }))
                .map_err(|e| e.to_string())?
            );
        } else {
            println!("Cache dir: {}", cache_dir_str);
            println!("Status: not_found");
        }
        return Ok(());
    }

    let mut total_size_bytes = 0u64;
    let mut entry_count = 0usize;

    let mut stack = vec![cache_dir.clone()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    let file_type = metadata.file_type();
                    if file_type.is_dir() {
                        stack.push(entry.path());
                    } else if file_type.is_file() {
                        total_size_bytes += metadata.len();
                        entry_count += 1;
                    }
                }
            }
        }
    }

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "cache_dir": cache_dir_str,
                "total_size_bytes": total_size_bytes,
                "entry_count": entry_count
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        println!("Cache dir: {}", cache_dir_str);
        println!("Total size: {} bytes", total_size_bytes);
        println!("Entries: {}", entry_count);
    }

    Ok(())
}
