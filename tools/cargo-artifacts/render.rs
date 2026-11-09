use super::*;

pub(super) fn render_json(options: &Options, plan: &CleanupPlan, state: &ApplyState) -> String {
    let selected = plan
        .selected
        .iter()
        .map(|entry| {
            format!(
                "{{\"name\":\"{}\",\"path\":\"{}\",\"bytes\":{},\"files\":{},\"directories\":{}}}",
                json_escape(&entry.name),
                json_escape(&entry.path.display().to_string()),
                entry.measurement.bytes,
                entry.measurement.files,
                entry.measurement.directories
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let deleted = state
        .deleted
        .iter()
        .map(|name| format!("\"{}\"", json_escape(name)))
        .collect::<Vec<_>>()
        .join(",");
    let pending = state.pending.as_ref().map_or_else(
        || "null".to_string(),
        |name| format!("\"{}\"", json_escape(name)),
    );
    let failure = state.failure.as_ref().map_or_else(
        || "null".to_string(),
        |failure| {
            format!(
                "{{\"path\":\"{}\",\"error\":\"{}\"}}",
                json_escape(&failure.path),
                json_escape(&failure.error)
            )
        },
    );
    format!(
        concat!(
            "{\n",
            "  \"schema_version\": 1,\n",
            "  \"generated_at_unix_seconds\": {},\n",
            "  \"operation\": \"{}\",\n",
            "  \"apply\": {},\n",
            "  \"status\": \"{}\",\n",
            "  \"target_directory\": \"{}\",\n",
            "  \"preserved_entry\": \"ripr\",\n",
            "  \"max_size_bytes\": {},\n",
            "  \"selected_bytes\": {},\n",
            "  \"selected_files\": {},\n",
            "  \"selected_directories\": {},\n",
            "  \"retained_ripr_bytes\": {},\n",
            "  \"over_budget_bytes\": {},\n",
            "  \"selected\": [{}],\n",
            "  \"deleted\": [{}],\n",
            "  \"pending\": {},\n",
            "  \"failure\": {}\n",
            "}\n"
        ),
        unix_seconds(),
        options.operation.as_str(),
        options.apply,
        json_escape(&state.status),
        json_escape(&plan.target_directory.display().to_string()),
        plan.max_size_bytes,
        plan.selected_measurement.bytes,
        plan.selected_measurement.files,
        plan.selected_measurement.directories,
        plan.retained_measurement.bytes,
        plan.over_budget_bytes,
        selected,
        deleted,
        pending,
        failure
    )
}

pub(super) fn render_markdown(options: &Options, plan: &CleanupPlan, state: &ApplyState) -> String {
    let pending = state.pending.as_deref().unwrap_or("none");
    let mut output = format!(
        concat!(
            "# Cargo artifact report\n\n",
            "- Operation: `{}`\n",
            "- Mode: **{}**\n",
            "- Status: **{}**\n",
            "- Target: `{}`\n",
            "- Selected: {} entries / {}\n",
            "- Preserved `target/ripr`: {}\n",
            "- Soft budget: {}\n",
            "- Over budget: {}\n",
            "- Deleted: {} entries\n",
            "- Pending outcome: `{}`\n\n"
        ),
        options.operation.as_str(),
        if options.apply { "apply" } else { "dry run" },
        state.status,
        markdown_escape(&plan.target_directory.display().to_string()),
        plan.selected.len(),
        human_bytes(plan.selected_measurement.bytes),
        human_bytes(plan.retained_measurement.bytes),
        human_bytes(plan.max_size_bytes),
        human_bytes(plan.over_budget_bytes),
        state.deleted.len(),
        markdown_escape(pending)
    );
    if !plan.selected.is_empty() {
        output.push_str("| Entry | Size | Files | Directories | State |\n");
        output.push_str("|---|---:|---:|---:|---|\n");
        for entry in &plan.selected {
            let state_text = if state.deleted.contains(&entry.name) {
                "deleted"
            } else if state.pending.as_deref() == Some(entry.name.as_str()) {
                "pending outcome"
            } else {
                "selected"
            };
            output.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                markdown_escape(&entry.name),
                human_bytes(entry.measurement.bytes),
                entry.measurement.files,
                entry.measurement.directories,
                state_text
            ));
        }
        output.push('\n');
    }
    if let Some(failure) = &state.failure {
        output.push_str(&format!(
            "Cleanup stopped at `{}`: {}\n\n",
            markdown_escape(&failure.path),
            markdown_escape(&failure.error)
        ));
    }
    if options.operation == Operation::CleanCurrent && !options.apply && !plan.selected.is_empty() {
        output.push_str("Review this exact selection, stop Cargo work in the checkout, then rerun with `--apply`.\n");
    }
    output
}

pub(super) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(super) fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control <= '\u{001f}' => {
                escaped.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

pub(super) fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|").replace('`', "\\`")
}

pub(super) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
