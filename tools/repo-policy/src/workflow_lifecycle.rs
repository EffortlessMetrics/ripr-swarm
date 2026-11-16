//! Development publishes source and evidence without allocating Actions jobs.
//!
//! The protected qualification has one automatic authority:
//! `merge_group.checks_requested` in `routed-rust.yml`. Explicit reusable/manual
//! calls and the named maintenance schedules below remain separate authorities.

use std::collections::BTreeMap;

const CALLER: &str = ".github/workflows/routed-rust.yml";
const MAIN_HEALTH: &str = ".github/workflows/main-health-sentinel.yml";

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
    let mut leading_marker_allowed = true;
    let mut current: Option<String> = None;
    for raw in source.lines() {
        let line = raw.split(" #").next().unwrap_or(raw).trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line == "---" && leading_marker_allowed {
            leading_marker_allowed = false;
            continue;
        }
        leading_marker_allowed = false;
        if !line.starts_with(' ') {
            active = false;
            current = None;
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
            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            {
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

/// Enforce one automatic development root. Job-level conditions are irrelevant:
/// subscribing to a PR or push event already allocates an Actions workflow run.
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
                "schedule" => {
                    (path == MAIN_HEALTH
                        && (configuration == ["- cron: '9,39 * * * *'"]
                            || configuration == ["- cron: \"9,39 * * * *\""]))
                        || SCHEDULED_AUTHORITIES.iter().any(|(file, cron)| {
                            path == format!(".github/workflows/{file}")
                                && (configuration == [format!("- cron: '{cron}'")]
                                    || configuration == [format!("- cron: \"{cron}\"")])
                        })
                }
                "merge_group" => {
                    path == CALLER && configuration == ["types: [checks_requested]"]
                }
                _ => false,
            };
            (!allowed).then(|| {
                let schedule_hint = if event == "schedule" {
                    "; retained schedules require their exact declared cadence"
                } else {
                    ""
                };
                format!(
                    "{path}: `{event}` starts Actions outside the queue/manual/maintenance authorities; development qualification belongs to merge_group.checks_requested{schedule_hint}"
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
    fn every_non_queue_development_event_is_refused_even_when_jobs_skip() {
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
            assert_eq!(violations(CALLER, &workflow(event, "")).len(), 1, "{event}");
        }
    }

    #[test]
    fn queue_requires_the_exact_caller_and_check_request_event() {
        let source = workflow("merge_group", "    types: [checks_requested]\n");
        assert!(violations(CALLER, &source).is_empty());
        assert!(!violations(".github/workflows/copy.yml", &source).is_empty());
        assert!(!violations(CALLER, &workflow("merge_group", "")).is_empty());
        assert!(!violations(
            CALLER,
            &workflow("merge_group", "    types: [destroyed]\n")
        )
        .is_empty());
    }

    #[test]
    fn manual_and_reusable_authorities_remain_explicit() {
        assert!(violations(CALLER, &workflow("workflow_dispatch", "")).is_empty());
        assert!(violations(
            ".github/workflows/ci.yml",
            &workflow("workflow_call", "")
        )
        .is_empty());
    }

    #[test]
    fn retained_schedules_are_named_and_exact() {
        let health = workflow("schedule", "    - cron: '9,39 * * * *'\n");
        assert!(violations(MAIN_HEALTH, &health).is_empty());
        assert!(!violations(".github/workflows/renamed.yml", &health).is_empty());
        for (file, cron) in SCHEDULED_AUTHORITIES {
            let path = format!(".github/workflows/{file}");
            let fields = format!("    - cron: '{cron}'\n");
            assert!(violations(&path, &workflow("schedule", &fields)).is_empty());
            assert!(!violations(&path, &workflow("schedule", "    - cron: '* * * * *'\n")).is_empty());
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
        ] {
            assert!(!violations(CALLER, header).is_empty(), "{header}");
        }
    }

    #[test]
    fn current_repository_workflows_obey_the_lifecycle_boundary() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root");
        let directory = root.join(".github/workflows");
        for entry in std::fs::read_dir(directory).expect("workflow directory") {
            let path = entry.expect("workflow entry").path();
            if !matches!(path.extension().and_then(|value| value.to_str()), Some("yml" | "yaml")) {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("workflow source");
            let relative = path.strip_prefix(root).expect("relative workflow");
            let normalized = relative.to_string_lossy().replace('\\', "/");
            let findings = violations(&normalized, &source);
            assert!(findings.is_empty(), "{normalized}: {findings:?}");
        }
    }
}
