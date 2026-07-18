use serde_json::json;

pub(crate) fn issue_intake(args: &[String]) -> Result<(), String> {
    let mut issue_number = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--issue" && let Some(val) = iter.next() {
            issue_number = val.parse::<u32>().ok();
        }
    }

    let Some(issue_number) = issue_number else {
        return Err("usage: cargo xtask issue-intake --issue <number>".to_string());
    };

    let gh_output = crate::run::run_output("gh", &["issue", "view", &issue_number.to_string(), "--json", "title,labels,body", "--repo", "EffortlessMetrics/ripr-swarm"])
        .map_err(|e| format!("failed to run gh issue view: {}", e))?;

    let parsed: serde_json::Value = serde_json::from_str(&gh_output)
        .map_err(|e| format!("failed to parse gh issue view output: {}", e))?;

    let title = parsed["title"].as_str().unwrap_or("").to_string();
    let body = parsed["body"].as_str().unwrap_or("").to_string();
    let body_excerpt = if body.len() > 1000 {
        format!("{}...", &body[..1000])
    } else {
        body
    };
    
    let labels: Vec<String> = parsed["labels"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|l| l["name"].as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    let packet = json!({
        "schema_version": "1.0",
        "issue_number": issue_number,
        "title": title,
        "labels": labels,
        "body_excerpt": body_excerpt,
        "initial_readiness": "unknown"
    });

    let packet_json = serde_json::to_string_pretty(&packet)
        .map_err(|e| format!("failed to serialize intake packet: {}", e))?;

    let out_path = std::path::PathBuf::from(format!("target/ripr/reports/issue-intake-{}.json", issue_number));
    super::write_parented_file(&out_path, "issue intake packet", packet_json.as_bytes())?;

    println!("wrote {}", out_path.display());
    Ok(())
}

