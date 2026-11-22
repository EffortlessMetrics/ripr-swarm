//! Source-only workflows: files the public source repository runs and swarm
//! must never run.
//!
//! A fast-forward promotion gives public `EffortlessMetrics/ripr` the swarm
//! tree byte for byte, so public's own required-check producers have to live
//! in swarm too. `policy/source-only-workflows.toml` names them. This module
//! checks the three layers that keep them inert in swarm (path, per-job
//! repository guard, and the disabled-workflow phase rule) and that every
//! context the source settings require has a ledgered producer.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::workflow_lifecycle::event_declarations;

pub const LEDGER: &str = "policy/source-only-workflows.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub schema_version: String,
    pub source_repository: String,
    pub swarm_repository: String,
    pub source_origin: String,
    pub guard: String,
    pub source_settings: String,
    pub source_settings_origin_blob: String,
    pub swarm_settings: String,
    #[serde(default)]
    pub workflow: Vec<SourceOnlyWorkflow>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceOnlyWorkflow {
    pub path: String,
    pub source_path: String,
    pub source_blob: String,
    pub required_contexts: Vec<String>,
    pub phase: String,
    pub swarm_disable_observation: String,
    #[serde(default)]
    pub source_events: BTreeMap<String, Vec<String>>,
}

pub fn parse_ledger(text: &str) -> Result<Ledger, String> {
    toml::from_str(text).map_err(|error| format!("{LEDGER}: {error}"))
}

/// Events a ledgered file may declare beyond `workflow_dispatch`. Only phase
/// `source_triggers` with a recorded swarm disable observation admits them;
/// the exact configuration lines must match so an exemption cannot widen.
pub fn admitted_source_events(entry: &SourceOnlyWorkflow) -> BTreeMap<String, Vec<String>> {
    if entry.phase == "source_triggers" && !entry.swarm_disable_observation.trim().is_empty() {
        entry.source_events.clone()
    } else {
        BTreeMap::new()
    }
}

/// Every violation for the ledger against the given workflow texts, the
/// source settings text, and the advisory CI ledgers' texts.
pub fn violations(
    ledger: &Ledger,
    workflows: &BTreeMap<String, String>,
    source_settings: Option<&str>,
    ci_ledgers: &[(&str, &str)],
) -> Vec<String> {
    let mut violations = Vec::new();
    if ledger.schema_version != "1.0" {
        violations.push(format!(
            "{LEDGER}: unsupported schema_version {:?}",
            ledger.schema_version
        ));
    }
    if ledger.source_repository == ledger.swarm_repository {
        violations.push(format!(
            "{LEDGER}: source and swarm repositories must differ"
        ));
    }
    let expected_guard_start = format!("(github.repository == '{}' ", ledger.source_repository);
    if !ledger.guard.starts_with(&expected_guard_start) {
        violations.push(format!(
            "{LEDGER}: guard must start with `{expected_guard_start}` so it names the source repository"
        ));
    }
    for (field, value) in [
        ("source_origin", &ledger.source_origin),
        (
            "source_settings_origin_blob",
            &ledger.source_settings_origin_blob,
        ),
    ] {
        if !is_full_sha(value) {
            violations.push(format!("{LEDGER}: {field} must be a full lowercase SHA"));
        }
    }
    if ledger.source_settings != ".github/settings.yml" {
        violations.push(format!(
            "{LEDGER}: source_settings must be the Settings app path .github/settings.yml"
        ));
    }
    if ledger.swarm_settings == ledger.source_settings {
        violations.push(format!(
            "{LEDGER}: swarm_settings must not share the Settings app path"
        ));
    }

    let mut seen = BTreeSet::new();
    let mut produced = BTreeSet::new();
    for entry in &ledger.workflow {
        if !seen.insert(entry.path.clone()) {
            violations.push(format!("{LEDGER}: duplicate workflow {}", entry.path));
        }
        if !is_full_sha(&entry.source_blob) {
            violations.push(format!(
                "{LEDGER}: {} source_blob must be a full lowercase SHA",
                entry.path
            ));
        }
        let Some(text) = workflows.get(&entry.path) else {
            violations.push(format!(
                "{LEDGER}: {} is ledgered but missing from .github/workflows",
                entry.path
            ));
            continue;
        };
        violations.extend(entry_violations(ledger, entry, text));
        let jobs = job_blocks(text);
        for context in &entry.required_contexts {
            if jobs.iter().any(|job| job.produces(context)) {
                produced.insert(context.clone());
            } else {
                violations.push(format!(
                    "{}: no job produces ledgered required context `{context}`",
                    entry.path
                ));
            }
        }
    }

    // A source-only file must never run in swarm, so nothing in swarm may
    // call it and the swarm lane ledgers must not select it.
    for (path, text) in workflows {
        for entry in &ledger.workflow {
            if path != &entry.path && text.contains(&format!("uses: ./{}", entry.path)) {
                violations.push(format!(
                    "{path}: calls source-only workflow {}; source-only files never run in swarm",
                    entry.path
                ));
            }
        }
    }
    for (ci_path, ci_text) in ci_ledgers {
        for entry in &ledger.workflow {
            if ci_text.contains(&format!("\"{}\"", entry.path)) {
                violations.push(format!(
                    "{ci_path}: names source-only workflow {}; swarm lane ledgers describe swarm lanes only",
                    entry.path
                ));
            }
        }
    }

    match source_settings {
        None => violations.push(format!(
            "{}: missing; the source settings contract must stay at the Settings app path",
            ledger.source_settings
        )),
        Some(settings) => {
            let contexts = required_contexts(settings);
            if contexts.is_empty() {
                violations.push(format!(
                    "{}: declares no required status contexts to reconcile",
                    ledger.source_settings
                ));
            }
            for context in contexts {
                if !produced.contains(&context) {
                    violations.push(format!(
                        "{}: required context `{context}` has no source-only producer in {LEDGER}; a fast-forward would leave public PRs unable to pass it",
                        ledger.source_settings
                    ));
                }
            }
        }
    }
    violations
}

fn entry_violations(ledger: &Ledger, entry: &SourceOnlyWorkflow, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let path = &entry.path;
    if !path.starts_with(".github/workflows/source-only-") {
        violations.push(format!(
            "{path}: source-only workflows use the `.github/workflows/source-only-` prefix so swarm paths stay distinct"
        ));
    }
    match entry.phase.as_str() {
        "dispatch_only" | "source_triggers" => {}
        other => violations.push(format!(
            "{LEDGER}: {path} has unknown phase {other:?}; expected dispatch_only or source_triggers"
        )),
    }
    if entry.phase == "source_triggers" && entry.swarm_disable_observation.trim().is_empty() {
        violations.push(format!(
            "{LEDGER}: {path} is in phase source_triggers without a recorded swarm disable observation; disable the workflow in {} first",
            ledger.swarm_repository
        ));
    }
    if entry.source_events.contains_key("workflow_dispatch") {
        violations.push(format!(
            "{LEDGER}: {path} source_events must not list workflow_dispatch; dispatch is always allowed"
        ));
    }
    match event_declarations(text) {
        Err(error) => violations.push(format!("{path}: workflow events: {error}")),
        Ok(events) => {
            if !events.contains_key("workflow_dispatch") {
                violations.push(format!(
                    "{path}: source-only workflows keep workflow_dispatch for the swarm rehearsal"
                ));
            }
            if entry.phase == "source_triggers" {
                for (event, configuration) in &entry.source_events {
                    if events.get(event) != Some(configuration) {
                        violations.push(format!(
                            "{path}: phase source_triggers must declare ledgered event `{event}` exactly as {configuration:?}"
                        ));
                    }
                }
            }
        }
    }
    let jobs = job_blocks(text);
    if jobs.is_empty() {
        violations.push(format!("{path}: no jobs found under `jobs:`"));
    }
    let guarded_prefix = format!("{} && (", ledger.guard);
    for job in &jobs {
        let guarded = match &job.condition {
            Some(condition) => {
                condition == &ledger.guard
                    || condition
                        .strip_prefix(&guarded_prefix)
                        .is_some_and(closes_only_at_end)
            }
            None => false,
        };
        if !guarded {
            violations.push(format!(
                "{path}: job `{}` must open its `if:` with the source repository guard `{}`",
                job.id, ledger.guard
            ));
        }
    }
    if !text.contains("rehearse_in_swarm:") {
        violations.push(format!(
            "{path}: workflow_dispatch must declare the `rehearse_in_swarm` input the guard reads"
        ));
    }
    violations
}

#[derive(Debug)]
pub struct JobBlock {
    pub id: String,
    pub name: Option<String>,
    pub condition: Option<String>,
}

impl JobBlock {
    /// GitHub reports a job's check context by its `name:` when present and
    /// by its id otherwise.
    fn produces(&self, context: &str) -> bool {
        match &self.name {
            Some(name) => name == context,
            None => self.id == context,
        }
    }
}

/// Top-level jobs with their job-level `name:` and `if:` scalars. Multi-line
/// `if:` forms are recorded as an empty condition so they fail the guard.
pub fn job_blocks(text: &str) -> Vec<JobBlock> {
    let mut jobs: Vec<JobBlock> = Vec::new();
    let mut in_jobs = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.trim().is_empty() && !line.starts_with('#') {
            in_jobs = line.trim_end() == "jobs:";
            continue;
        }
        if !in_jobs {
            continue;
        }
        if let Some(rest) = line.strip_prefix("  ") {
            if !rest.starts_with(' ') && !rest.starts_with('#') {
                if let Some(id) = rest.trim_end().strip_suffix(':') {
                    jobs.push(JobBlock {
                        id: id.to_string(),
                        name: None,
                        condition: None,
                    });
                }
                continue;
            }
        }
        let Some(job) = jobs.last_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("    ") {
            if rest.starts_with(' ') {
                continue;
            }
            if let Some(value) = rest.strip_prefix("if:") {
                let value = value.trim();
                job.condition = Some(if value.starts_with('>') || value.starts_with('|') {
                    String::new()
                } else {
                    value.to_string()
                });
            } else if let Some(value) = rest.strip_prefix("name:") {
                job.name = Some(
                    value
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string(),
                );
            }
        }
    }
    jobs
}

/// Contexts listed under `required_status_checks: contexts:` in a Settings
/// app file.
pub fn required_contexts(settings: &str) -> Vec<String> {
    let mut contexts = Vec::new();
    let mut indent = None;
    for line in settings.lines() {
        let trimmed = line.trim_start();
        let width = line.len() - trimmed.len();
        if trimmed == "contexts:" {
            indent = Some(width);
            continue;
        }
        if let Some(base) = indent {
            if let Some(value) = trimmed.strip_prefix("- ") {
                if width > base {
                    contexts.push(value.trim().trim_matches('"').to_string());
                    continue;
                }
            }
            if !trimmed.is_empty() {
                indent = None;
            }
        }
    }
    contexts
}

/// True when the text after `guard && (` closes that parenthesis exactly at
/// its last character, so `guard && (a) || b` cannot pass as guarded.
fn closes_only_at_end(rest: &str) -> bool {
    let mut depth = 1usize;
    let mut quoted = false;
    let bytes = rest.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'\'' => quoted = !quoted,
            b'(' if !quoted => depth += 1,
            b')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return index + 1 == bytes.len();
                }
            }
            _ => {}
        }
    }
    false
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUARD: &str = "(github.repository == 'EffortlessMetrics/ripr' || (github.event_name == 'workflow_dispatch' && inputs.rehearse_in_swarm))";
    const SHA: &str = "84fc3cddfc2f74cd34632bb02a518f9e80a68218";
    const PATH: &str = ".github/workflows/source-only-ci.yml";
    const SETTINGS: &str = "branches:\n  - name: main\n    protection:\n      required_status_checks:\n        strict: true\n        contexts:\n          - rust\n          - cargo-deny\n      enforce_admins: true\n";

    fn ledger(phase: &str, observation: &str) -> Ledger {
        let text = format!(
            "schema_version = \"1.0\"\nsource_repository = \"EffortlessMetrics/ripr\"\nswarm_repository = \"EffortlessMetrics/ripr-swarm\"\nsource_origin = \"{SHA}\"\nguard = \"{GUARD}\"\nsource_settings = \".github/settings.yml\"\nsource_settings_origin_blob = \"{SHA}\"\nswarm_settings = \".github/swarm-settings.yml\"\n\n[[workflow]]\npath = \"{PATH}\"\nsource_path = \".github/workflows/ci.yml\"\nsource_blob = \"{SHA}\"\nrequired_contexts = [\"rust\", \"cargo-deny\"]\nphase = \"{phase}\"\nswarm_disable_observation = \"{observation}\"\n\n[workflow.source_events]\npull_request = [\"types: [opened, synchronize]\"]\n"
        );
        match parse_ledger(&text) {
            Ok(ledger) => ledger,
            Err(error) => panic!("fixture ledger must parse: {error}"),
        }
    }

    fn workflow(events: &str, rust_if: &str, deny_if: &str) -> String {
        format!(
            "name: Source CI\non:\n  workflow_dispatch:\n    inputs:\n      rehearse_in_swarm:\n        type: boolean\n{events}permissions: {{}}\njobs:\n  rust:\n    if: {rust_if}\n    runs-on: ubuntu-latest\n    steps:\n      - run: ':'\n  deny:\n    name: cargo-deny\n    if: {deny_if}\n    runs-on: ubuntu-latest\n    steps:\n      - if: always()\n        run: ':'\n"
        )
    }

    fn check(ledger: &Ledger, text: &str, settings: &str) -> Vec<String> {
        let workflows = BTreeMap::from([(PATH.to_string(), text.to_string())]);
        violations(ledger, &workflows, Some(settings), &[])
    }

    fn good() -> String {
        workflow(
            "",
            GUARD,
            &format!("{GUARD} && (github.event_name == 'pull_request')"),
        )
    }

    #[test]
    fn guarded_dispatch_only_workflow_producing_every_context_passes() {
        let paren = workflow(
            "",
            GUARD,
            &format!(
                "{GUARD} && (contains(github.event.pull_request.labels.*.name, 'a)b') || (github.event_name == 'push'))"
            ),
        );
        assert!(check(&ledger("dispatch_only", ""), &paren, SETTINGS).is_empty());
        let found = check(&ledger("dispatch_only", ""), &good(), SETTINGS);
        assert!(found.is_empty(), "{found:#?}");
        let jobs = job_blocks(&good());
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[1].name.as_deref(), Some("cargo-deny"));
    }

    #[test]
    fn unguarded_or_loosened_job_is_refused() {
        for rust_if in [
            "github.event_name != 'pull_request'",
            "github.event_name != 'pull_request' || (github.repository == 'EffortlessMetrics/ripr')",
            &format!("{GUARD} || always()"),
            &format!("{GUARD} && (github.event_name == 'push') || always()"),
            &format!("{GUARD} && (github.event_name == 'push'"),
        ] {
            let text = workflow("", rust_if, GUARD);
            let found = check(&ledger("dispatch_only", ""), &text, SETTINGS);
            assert!(
                found.iter().any(|v| v.contains("job `rust`")),
                "{rust_if}: {found:#?}"
            );
        }
        let missing_if = good().replace(&format!("    if: {GUARD}\n    runs-on"), "    runs-on");
        assert_ne!(missing_if, good(), "fixture must remove the rust guard");
        let found = check(&ledger("dispatch_only", ""), &missing_if, SETTINGS);
        assert!(found.iter().any(|v| v.contains("job `rust`")), "{found:#?}");
    }

    #[test]
    fn source_settings_context_without_producer_is_refused() {
        let settings = SETTINGS.replace(
            "          - cargo-deny\n",
            "          - cargo-deny\n          - vscode\n",
        );
        let found = check(&ledger("dispatch_only", ""), &good(), &settings);
        assert!(
            found
                .iter()
                .any(|v| v.contains("`vscode` has no source-only producer")),
            "{found:#?}"
        );
        let renamed = good().replace("name: cargo-deny", "name: deny");
        let found = check(&ledger("dispatch_only", ""), &renamed, SETTINGS);
        assert!(
            found
                .iter()
                .any(|v| v.contains("no job produces ledgered required context `cargo-deny`")),
            "{found:#?}"
        );
    }

    #[test]
    fn source_triggers_need_a_disable_observation_and_exact_events() {
        let events = "  pull_request:\n    types: [opened, synchronize]\n";
        let text = workflow(events, GUARD, GUARD);
        let found = check(&ledger("source_triggers", ""), &text, SETTINGS);
        assert!(
            found
                .iter()
                .any(|v| v.contains("without a recorded swarm disable observation")),
            "{found:#?}"
        );
        assert!(admitted_source_events(&ledger("source_triggers", "").workflow[0]).is_empty());

        let observed = ledger(
            "source_triggers",
            "workflow 123 disabled_manually read 2026-11-23",
        );
        assert!(check(&observed, &text, SETTINGS).is_empty());
        assert_eq!(admitted_source_events(&observed.workflow[0]).len(), 1);

        let widened = workflow(
            "  pull_request:\n    types: [opened, synchronize, labeled]\n",
            GUARD,
            GUARD,
        );
        let found = check(&observed, &widened, SETTINGS);
        assert!(found.iter().any(|v| v.contains("exactly as")), "{found:#?}");
        assert!(admitted_source_events(&ledger("dispatch_only", "x").workflow[0]).is_empty());
    }

    #[test]
    fn swarm_callers_and_lane_ledgers_may_not_name_source_only_files() {
        let mut workflows = BTreeMap::from([(PATH.to_string(), good())]);
        workflows.insert(
            ".github/workflows/caller.yml".to_string(),
            format!("jobs:\n  call:\n    uses: ./{PATH}\n"),
        );
        let lane = format!("[[lane]]\nworkflow = \"{PATH}\"\n");
        let found = violations(
            &ledger("dispatch_only", ""),
            &workflows,
            Some(SETTINGS),
            &[("policy/ci-lane-whitelist.toml", &lane)],
        );
        assert!(
            found
                .iter()
                .any(|v| v.starts_with(".github/workflows/caller.yml"))
        );
        assert!(
            found
                .iter()
                .any(|v| v.starts_with("policy/ci-lane-whitelist.toml"))
        );
    }

    #[test]
    fn settings_contexts_parse_only_the_contexts_list() {
        assert_eq!(required_contexts(SETTINGS), vec!["rust", "cargo-deny"]);
        assert!(required_contexts("labels:\n  - name: rust\n").is_empty());
    }
}
