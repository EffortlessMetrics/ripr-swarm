//! Development publishes source and evidence without allocating Actions jobs.
//!
//! The two legacy label subscribers and the required caller's main cache writer
//! are an explicit bootstrap remainder under #4838, not an assertion that queue
//! cutover has happened. Keep their admission narrow until their replacement is
//! verified. A job-level `if` cannot make a subscribed event workflow-free.

use std::collections::BTreeMap;

const CALLER: &str = ".github/workflows/routed-rust.yml";
const CONFLICT_NOTICE: &str = ".github/workflows/merge-ready-conflict-notice.yml";
const RELEASE_WATCH: &str = ".github/workflows/hard-cut-watch.yml";
const MAIN_HEALTH: &str = ".github/workflows/main-health-sentinel.yml";

/// Read the repository's explicit block-style event mapping. Unsupported YAML
/// forms fail closed rather than making aliases, quoted keys or flow mappings
/// invisible to the event policy. Job inputs and shell text are not events.
pub fn event_declarations(source: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let mut declarations = BTreeMap::new();
    let mut active = false;
    let mut found = false;
    let mut current: Option<String> = None;
    for raw in source.lines() {
        let line = raw.split(" #").next().unwrap_or(raw).trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') {
            active = false;
            current = None;
            // Obfuscated or duplicate YAML keys must not let a later `on :`,
            // quoted key, or alias replace the literal mapping we inspected.
            let key = line.split_once(':').map(|(key, _)| key);
            if !matches!(
                key,
                Some(
                    "name"
                        | "run-name"
                        | "on"
                        | "permissions"
                        | "env"
                        | "defaults"
                        | "concurrency"
                        | "jobs"
                )
            ) {
                return Err("workflow root keys must use static unquoted names".into());
            }
            if line == "on:" {
                if found {
                    return Err("duplicate on mapping".into());
                }
                found = true;
                active = true;
            } else if line.starts_with("on:")
                || line.starts_with("'on':")
                || line.starts_with("\"on\":")
            {
                return Err("events must use the explicit block-style on mapping".into());
            }
            continue;
        }
        if !active {
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("   ") {
            let name = line
                .trim()
                .strip_suffix(':')
                .ok_or_else(|| "events must use explicit two-space block-style keys".to_string())?;
            if name.is_empty() || !name.bytes().all(|c| c.is_ascii_lowercase() || c == b'_') {
                return Err("event key is not a static unquoted name".into());
            }
            if declarations.insert(name.to_owned(), Vec::new()).is_some() {
                return Err(format!("duplicate event {name}"));
            }
            current = Some(name.to_owned());
        } else if line.starts_with("    ") {
            let name = current
                .as_ref()
                .ok_or_else(|| "event configuration is not under a declared event".to_string())?;
            declarations
                .get_mut(name)
                .ok_or_else(|| "event configuration has no owner".to_string())?
                .push(line.trim().to_owned());
        } else {
            return Err("event indentation is not auditable".into());
        }
    }
    if !found || declarations.is_empty() {
        return Err("workflow has no explicit nonempty on mapping".into());
    }
    Ok(declarations)
}

/// Enforce the source migration perimeter without weakening the still-installed
/// required qualification route. #4838 removes the three bootstrap edges only
/// after truthful admission, queue-only enforcement and cache continuity work.
pub fn violations(path: &str, source: &str) -> Vec<String> {
    let events = match event_declarations(source) {
        Ok(events) => events,
        Err(error) => return vec![format!("{path}: workflow lifecycle: {error}")],
    };
    events
        .into_iter()
        .filter_map(|(event, configuration)| {
            let allowed = match event.as_str() {
                "workflow_call" | "workflow_dispatch" => true,
                // Existing deep-proof and maintenance purposes are distinct
                // from periodic main qualification. The ordinary health timer
                // is specifically retired; incident proof remains explicit.
                "schedule" => path != MAIN_HEALTH,
                "merge_group" => {
                    path == CALLER && configuration == ["types: [checks_requested]"]
                }
                "pull_request" => path == CALLER && configuration == ["types: [labeled]"],
                "pull_request_target" => {
                    path == CONFLICT_NOTICE && configuration == ["types: [labeled]"]
                }
                "push" => {
                    (path == CALLER && configuration == ["branches: [main, master]"])
                        // Release-cut observation has no product qualification.
                        || (path == RELEASE_WATCH && configuration == ["branches: [main]"])
                }
                _ => false,
            };
            (!allowed).then(|| {
                format!(
                    "{path}: `{event}` starts Actions outside the retained qualification/maintenance authorities; development checks and review belong in the owning session, not skipped-job wrappers (#4984, #4842)"
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow(event: &str, fields: &str) -> String {
        format!(
            "name: fixture\non:\n  {event}:\n{fields}permissions: {{}}\njobs:\n  work:\n    if: false\n    runs-on: ubuntu-latest\n    steps:\n      - run: ':'\n"
        )
    }

    #[test]
    fn every_development_event_is_refused_even_when_all_jobs_skip() {
        for event in [
            "pull_request",
            "pull_request_target",
            "pull_request_review",
            "pull_request_review_comment",
            "issue_comment",
            "issues",
            "push",
            "workflow_run",
        ] {
            assert_eq!(
                violations(".github/workflows/advisory.yml", &workflow(event, "")).len(),
                1,
                "{event} must not hide behind a cheap or skipped job"
            );
        }
    }

    #[test]
    fn bootstrap_exceptions_cannot_expand_or_move_to_another_file() {
        for (path, event, fields) in [
            (CALLER, "pull_request", "    types: [labeled]\n"),
            (CALLER, "push", "    branches: [main, master]\n"),
            (
                CONFLICT_NOTICE,
                "pull_request_target",
                "    types: [labeled]\n",
            ),
        ] {
            let source = workflow(event, fields);
            assert!(violations(path, &source).is_empty());
            assert!(!violations(".github/workflows/copy.yml", &source).is_empty());
            assert!(!violations(path, &workflow(event, "")).is_empty());
            let expanded = format!("{fields}    paths: ['**']\n");
            assert!(!violations(path, &workflow(event, &expanded)).is_empty());
        }
        for action in [
            "opened",
            "synchronize",
            "reopened",
            "ready_for_review",
            "edited",
            "unlabeled",
        ] {
            let source = workflow("pull_request", &format!("    types: [labeled, {action}]\n"));
            assert!(!violations(CALLER, &source).is_empty(), "{action}");
        }
    }

    #[test]
    fn queue_requires_the_actual_check_request_event() {
        assert!(
            violations(
                CALLER,
                &workflow("merge_group", "    types: [checks_requested]\n")
            )
            .is_empty()
        );
        assert!(!violations(CALLER, &workflow("merge_group", "")).is_empty());
        assert!(
            !violations(CALLER, &workflow("merge_group", "    types: [destroyed]\n")).is_empty()
        );
    }

    #[test]
    fn explicit_proof_survives_but_health_timer_does_not() {
        assert!(violations(MAIN_HEALTH, &workflow("workflow_dispatch", "")).is_empty());
        assert!(
            !violations(
                MAIN_HEALTH,
                &workflow("schedule", "    - cron: '9,39 * * * *'\n")
            )
            .is_empty()
        );
        assert!(
            violations(
                ".github/workflows/security.yml",
                &workflow("schedule", "    - cron: '17 6 * * *'\n")
            )
            .is_empty()
        );
    }

    #[test]
    fn ambiguous_yaml_does_not_hide_an_event() {
        for header in [
            "on: [pull_request]",
            "on: push",
            "on: {pull_request: {}}",
            "on: *events",
            "'on':",
            "\"on\":",
            "on:\n  'pull_request':",
            "on:\n  pull_request: {}",
            "on:\n    pull_request:",
            "on:\n  workflow_dispatch:\non:\n  pull_request:",
            "on:\n  workflow_dispatch:\n  workflow_dispatch:",
            "on:\n  workflow_dispatch:\non :\n  pull_request:",
            "on:\n  workflow_dispatch:\n\"o\\u006e\":\n  pull_request:",
            "on:\n  workflow_dispatch:\n&events on:\n  pull_request:",
        ] {
            assert!(!violations(CALLER, header).is_empty(), "{header}");
        }
    }

    #[test]
    fn nested_input_or_script_cannot_impersonate_an_event() {
        let source = "# on: push\non:\n  workflow_dispatch:\n    inputs:\n      pull_request:\n        type: string\njobs:\n  job:\n    steps:\n      - run: |\n          on:\n            pull_request:\n";
        assert_eq!(
            event_declarations(source).map(|v| v.into_keys().collect::<Vec<_>>()),
            Ok(vec!["workflow_dispatch".into()])
        );
        assert!(violations(CALLER, source).is_empty());
    }

    #[test]
    fn live_repository_keeps_development_outside_actions() -> Result<(), String> {
        let root = crate::test_support::workspace_root()?;
        let mut findings = Vec::new();
        let mut count = 0;
        for path in crate::collect_files(&root.join(".github/workflows"))? {
            if !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("yml" | "yaml")
            ) {
                continue;
            }
            count += 1;
            let relative = path.strip_prefix(&root).map_err(|e| e.to_string())?;
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            findings.extend(violations(&crate::normalize_path(relative), &text));
        }
        assert!(count > 0, "must inspect real workflows");
        assert!(
            findings.is_empty(),
            "{} lifecycle violations:\n{}",
            findings.len(),
            findings.join("\n")
        );
        Ok(())
    }
}
