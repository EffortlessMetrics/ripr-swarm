//! Development publishes source and evidence without allocating Actions jobs.
//!
//! The two legacy label subscribers, required caller's main cache writer and
//! every-main cut observer are bootstrap remainders under #4838, not a claim that queue
//! cutover has happened. Keep their admission narrow until their replacement is
//! verified. A job-level `if` cannot make a subscribed event workflow-free.

use std::collections::BTreeMap;

const CALLER: &str = ".github/workflows/routed-rust.yml";
const CONFLICT_NOTICE: &str = ".github/workflows/merge-ready-conflict-notice.yml";
const LEGACY_CUT_WATCH: &str = ".github/workflows/hard-cut-watch.yml";
const MAIN_HEALTH: &str = ".github/workflows/main-health-sentinel.yml";

// Inspected existing deep-proof and maintenance authorities. This preparation
// preserves their named purposes and exact cadences; it grants no generic timer
// exception or permission to rename periodic main qualification.
const SCHEDULED_AUTHORITIES: &[(&str, &str)] = &[
    ("droid-security-scan.yml", "0 8 * * 1"),
    ("dx-scoreboard.yml", "17 7 * * *"),
    ("first-run-install.yml", "41 7 * * *"),
    ("fuzz.yml", "17 5 * * 0"),
    ("issue-staleness-watchdog.yml", "17 6 * * 1"),
    ("kani.yml", "41 4 * * *"),
    ("macos-advisory.yml", "53 5 * * *"),
    ("pilot-ranking.yml", "29 6 * * *"),
    ("pr-staleness-watchdog.yml", "*/30 * * * *"),
    ("printed-command-paste.yml", "43 5 * * *"),
    ("public-proof.yml", "41 7 * * *"),
    ("rust-corpus.yml", "43 6 * * *"),
    ("scratch-gc.yml", "17,47 * * * *"),
    ("security.yml", "17 6 * * *"),
    ("size-scaling.yml", "41 7 * * *"),
    ("source-of-truth.yml", "37 6 * * 1"),
    ("windows-advisory.yml", "17 5 * * *"),
];

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
/// required qualification route. #4838 removes all four bootstrap edges only
/// after admission, queue/cut enforcement and cache continuity work.
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
                "schedule" => path != MAIN_HEALTH && SCHEDULED_AUTHORITIES.iter().any(|(file, cron)| {
                    path == format!(".github/workflows/{file}")
                        && (configuration == [format!("- cron: '{cron}'")]
                            || configuration == [format!("- cron: \"{cron}\"")])
                }),
                "merge_group" => {
                    path == CALLER && configuration == ["types: [checks_requested]"]
                }
                "pull_request" => path == CALLER && configuration == ["types: [labeled]"],
                "pull_request_target" => {
                    path == CONFLICT_NOTICE && configuration == ["types: [labeled]"]
                }
                "push" => {
                    (path == CALLER && configuration == ["branches: [main, master]"])
                        // This observer still allocates on every main commit.
                        // Retain cut protection during bootstrap, then retire
                        // its trigger after the protected G/cut join replaces it.
                        || (path == LEGACY_CUT_WATCH && configuration == ["branches: [main]"])
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
            (LEGACY_CUT_WATCH, "push", "    branches: [main]\n"),
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
    fn scheduled_exceptions_are_named_and_cannot_expand_or_hide_a_main_timer() {
        for path in [
            ".github/workflows/renamed-main-health.yml",
            ".github/workflows/unclassified.yml",
        ] {
            assert!(
                !violations(path, &workflow("schedule", "    - cron: '9,39 * * * *'\n")).is_empty()
            );
        }
        for (file, cron) in SCHEDULED_AUTHORITIES {
            let path = format!(".github/workflows/{file}");
            let fields = format!("    - cron: '{cron}'\n");
            assert!(violations(&path, &workflow("schedule", &fields)).is_empty());
            assert!(
                !violations(&path, &workflow("schedule", "    - cron: '* * * * *'\n")).is_empty()
            );
            let expanded = format!("{fields}    - cron: '9,39 * * * *'\n");
            assert!(!violations(&path, &workflow("schedule", &expanded)).is_empty());
        }
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
