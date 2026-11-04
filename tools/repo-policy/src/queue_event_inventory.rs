//! Conservative inventory of workflow event subscriptions for queue cutover.
//!
//! This reports subscriptions, not job execution, path applicability, or proof
//! coverage. Unsupported YAML forms are errors rather than evidence of quiet.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Serialize)]
struct WorkflowEvents {
    path: String,
    sha256: String,
    events: Vec<String>,
}

fn event_names(source: &str) -> Result<Vec<String>, String> {
    let mut found = false;
    let mut inside = false;
    let mut events = BTreeSet::new();
    for raw in source.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            if inside {
                inside = false;
            }
            if line == "on:" {
                if found {
                    return Err("duplicate on mapping".into());
                }
                found = true;
                inside = true;
            } else if line.starts_with("on:")
                || line.starts_with("'on':")
                || line.starts_with("\"on\":")
            {
                return Err("unsupported on declaration".into());
            }
            continue;
        }
        if !inside {
            continue;
        }
        if line.starts_with('\t') || line.starts_with(" \t") {
            return Err("tab indentation in on mapping".into());
        }
        if line.starts_with("  ") && !line.starts_with("   ") {
            let event = line[2..]
                .strip_suffix(':')
                .ok_or("unsupported event declaration")?;
            if event.is_empty()
                || !event
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err("unsupported event name".into());
            }
            if !events.insert(event.to_string()) {
                return Err(format!("duplicate event {event}"));
            }
        }
    }
    if !found || events.is_empty() {
        return Err("missing or empty on mapping".into());
    }
    Ok(events.into_iter().collect())
}

fn inventory(root: &Path) -> Result<Vec<WorkflowEvents>, String> {
    let directory = root.join(".github/workflows");
    let mut workflows = Vec::new();
    let entries = fs::read_dir(&directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err(format!("{} is not a regular workflow file", path.display()));
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let events = event_names(text).map_err(|e| format!("{}: {e}", path.display()))?;
        workflows.push(WorkflowEvents {
            path: path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/"),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            events,
        });
    }
    if workflows.is_empty() {
        return Err("no workflow files found".into());
    }
    workflows.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(workflows)
}

fn require_no_pr_events(workflows: &[WorkflowEvents]) -> Result<(), String> {
    if workflows.iter().any(|w| {
        w.events
            .iter()
            .any(|e| e == "pull_request" || e == "pull_request_target")
    }) {
        return Err("PR event subscriptions remain; inspect the inventory and classify non-CI workflows separately".into());
    }
    Ok(())
}

fn require_no_development_events(workflows: &[WorkflowEvents]) -> Result<(), String> {
    require_no_pr_events(workflows)?;
    if workflows
        .iter()
        .any(|w| w.events.iter().any(|e| e == "push"))
    {
        return Err(
            "push subscriptions remain; this strict check includes default-branch pushes".into(),
        );
    }
    Ok(())
}

pub fn command() -> Result<(), String> {
    let args = std::env::args().skip(2).collect::<Vec<_>>();
    let scope = match args.as_slice() {
        [] => 0,
        [arg] if arg == "--require-no-pr-events" => 1,
        [arg] if arg == "--require-no-pr-or-push-events" => 2,
        _ => return Err("usage: cargo policy queue-event-inventory [--require-no-pr-events|--require-no-pr-or-push-events]".into()),
    };
    let workflows = inventory(Path::new("."))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&workflows).map_err(|e| e.to_string())?
    );
    match scope {
        1 => require_no_pr_events(&workflows),
        2 => require_no_development_events(&workflows),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{event_names, inventory, require_no_development_events, require_no_pr_events};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reports_subscriptions_not_job_guards_or_comment_text() -> Result<(), String> {
        let source = "name: fixture\non:\n  pull_request:\n    paths: [src/**]\n  workflow_dispatch:\njobs:\n  run:\n    if: false\n    run: |\n      pull_request_target:\n";
        assert_eq!(event_names(source)?, ["pull_request", "workflow_dispatch"]);
        assert_eq!(
            event_names("on:\n  pull_request_target:\n    types: [labeled]\n")?,
            ["pull_request_target"]
        );
        Ok(())
    }

    #[test]
    fn inventory_discovers_files_and_rejects_an_unreadable_contract() -> Result<(), String> {
        let root = Scratch(std::env::temp_dir().join(format!(
            "ripr-queue-inventory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        let directory = root.0.join(".github/workflows");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        assert!(inventory(&root.0).is_err(), "empty inventory must not pass");
        fs::write(
            directory.join("a.yml"),
            "on:\n  merge_group:\n    types: [checks_requested]\n",
        )
        .map_err(|e| e.to_string())?;
        require_no_pr_events(&inventory(&root.0)?)?;
        fs::write(directory.join("b.yaml"), "on:\n  pull_request:\n").map_err(|e| e.to_string())?;
        let result = inventory(&root.0)?;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, ".github/workflows/a.yml");
        assert_eq!(result[1].events, ["pull_request"]);
        assert!(
            require_no_pr_events(&result).is_err(),
            "PR subscription is not quiet"
        );
        assert!(result.iter().all(|row| row.sha256.len() == 64));
        fs::write(
            directory.join("b.yaml"),
            "on:\n  push:\n    branches: [main]\n",
        )
        .map_err(|e| e.to_string())?;
        let push = inventory(&root.0)?;
        require_no_pr_events(&push)?;
        assert!(
            require_no_development_events(&push).is_err(),
            "strict mode includes main push"
        );
        fs::write(directory.join("b.yaml"), "on: [pull_request]\n").map_err(|e| e.to_string())?;
        assert!(
            inventory(&root.0).is_err(),
            "unsupported YAML must block the whole inventory"
        );
        Ok(())
    }

    #[test]
    fn unsupported_or_ambiguous_events_fail_closed() {
        for source in [
            "",
            "on: [pull_request]",
            "on:\n  pull_request: {}\n",
            "on:\n  pull_request:\n  pull_request:\n",
            "on:\n\tpull_request:\n",
            "'on':\n  push:\n",
            "on:\n  # empty\n",
        ] {
            assert!(event_names(source).is_err(), "accepted {source:?}");
        }
    }
}
