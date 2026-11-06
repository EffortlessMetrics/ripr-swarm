//! Observational composite inventory and executable source controls. These do
//! not authenticate Actions evidence or qualify a candidate.
use repo_policy::product_gate_plan::product_gate_definitions;

#[test]
fn canonical_inventory_has_one_row_per_composite_not_per_shell_command() {
    let definitions = product_gate_definitions();
    for id in [
        "product.repository.base_user_text",
        "product.evidence.pr_evidence",
        "product.evidence.schema_producer_sweep",
    ] {
        assert_eq!(
            definitions
                .iter()
                .filter(|gate| gate.id.as_str() == id)
                .count(),
            1,
            "missing or duplicated composite owner: {id}"
        );
    }
}

use repo_policy::product_gate_plan::composites::{
    BASE_RECOVERY, CompositeProducer, Event, PR_COMMANDS,
};
const WORKFLOW: &str = include_str!("../../../.github/workflows/rust-gates.yml");
const GUARD: &str = "github.event_name == 'pull_request' || github.event_name == 'merge_group'";
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn step<'a>(workflow: &'a str, name: &str) -> TestResult<&'a str> {
    let marker = format!("      - name: {name}\n");
    if workflow.matches(&marker).count() != 1 {
        return Err(format!("expected exactly one {name}").into());
    }
    let (_, tail) = workflow.split_once(&marker).ok_or("step missing")?;
    Ok(tail.split("\n      - ").next().ok_or("step body missing")?)
}

fn script(body: &str) -> TestResult<String> {
    let (_, source) = body
        .split_once("        run: |\n")
        .ok_or("literal run block missing")?;
    Ok(source
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn header_key(line: &str) -> Option<&str> {
    let (key, _) = line.trim_start().split_once(':')?;
    Some(key.trim().trim_matches(['"', '\'']))
}

/// Deliberately bounded source-shape oracle, not a general YAML/shell parser.
/// Literal shell behavior is challenged independently below.
fn source_contract(workflow: &str) -> TestResult {
    let header = workflow
        .split_once("\njobs:\n  rust-gates:\n")
        .ok_or("Rust job missing")?
        .1
        .split_once("    steps:\n")
        .ok_or("Rust steps missing")?
        .0;
    if header.lines().any(|line| {
        matches!(
            header_key(line),
            Some("if" | "continue-on-error" | "defaults")
        )
    }) {
        return Err("job suppression or shell override".into());
    }
    let definitions = product_gate_definitions();
    let mut previous = 0;
    for producer in definitions.iter().filter_map(|gate| gate.composite) {
        let contract = producer.contract(Event::MergeGroup);
        for owner in &contract.steps {
            let body = step(workflow, owner.name)?;
            let position = workflow
                .find(&format!("      - name: {}\n", owner.name))
                .ok_or("owner position")?;
            if position <= previous {
                return Err("composite order differs".into());
            }
            previous = position;
            let (header, _) = body.split_once("        run: |\n").ok_or("run block")?;
            let conditions: Vec<_> = header
                .lines()
                .filter_map(|line| line.strip_prefix("        if: "))
                .collect();
            let expected: Vec<_> = if contract.pr_events_only {
                vec![GUARD]
            } else {
                vec![]
            };
            if conditions != expected {
                return Err("event guard differs".into());
            }
            if header.lines().any(|line| {
                matches!(header_key(line), Some("continue-on-error" | "shell"))
                    || (header_key(line) == Some("if") && !line.starts_with("        if: "))
            }) {
                return Err("failure/condition override".into());
            }
            let ids: Vec<_> = header
                .lines()
                .filter_map(|line| line.strip_prefix("        id: "))
                .collect();
            if ids != owner.id.into_iter().collect::<Vec<_>>() {
                return Err("owning result changed".into());
            }
        }
        let commands: Vec<String> = contract
            .steps
            .iter()
            .map(|owner| script(step(workflow, owner.name)?))
            .collect::<TestResult<Vec<_>>>()?
            .into_iter()
            .flat_map(|source| {
                source
                    .lines()
                    .map(str::trim)
                    .filter(|line| line.starts_with("cargo "))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect();
        if commands != contract.commands {
            return Err("ordered composite commands differ".into());
        }
    }
    let prepare = script(step(workflow, "Prepare independent user-text base")?)?;
    for required in [
        "base_sha=\"$(git rev-parse HEAD^1)\"",
        "git worktree add --detach \"$base_tree\" \"$base_sha\"",
    ] {
        if !prepare.lines().any(|line| line == required) {
            return Err("first-parent worktree changed".into());
        }
    }
    let recovery = script(step(
        workflow,
        "Recover pinned base compiler arity if needed",
    )?)?;
    for required in [
        BASE_RECOVERY.path,
        BASE_RECOVERY.before_blob,
        BASE_RECOVERY.after_blob,
        "source.count(before) != 1",
        "source.replace(before, after, 1)",
        "test \"$(git -C \"$base_tree\" diff --name-only)\" = \"crates/ripr/src/app/test_stub.rs\"",
    ] {
        if !recovery.contains(required) {
            return Err("exact recovery changed".into());
        }
    }
    let matcher = script(step(
        workflow,
        "User-text goldens judged by the merge-base matcher",
    )?)?;
    for required in [
        "if ! grep -q -- '\"--tree\"' \"$base_tree/xtask/src/policy/user_text.rs\"; then",
        "matcher pin only",
        "  exit 0",
        "cd \"$base_tree\"",
        "cd \"$GITHUB_WORKSPACE\"",
    ] {
        if !matcher.contains(required) {
            return Err("matcher authority/fallback changed".into());
        }
    }
    let evidence = step(workflow, "PR evidence")?;
    for binding in [
        "          BASE_SHA: ${{ steps.admitted-group.outputs.base || steps.admitted.outputs.base || github.event.pull_request.base.sha || github.event.merge_group.base_sha }}",
        "          HEAD_SHA: ${{ steps.admitted-group.outputs.merge || steps.admitted.outputs.head || github.event.pull_request.head.sha || github.event.merge_group.head_sha }}",
    ] {
        if !evidence.lines().any(|line| line == binding) {
            return Err("admitted comparison changed".into());
        }
    }
    let summary = step(workflow, "Write gate summary")?;
    for binding in [
        "          PR_EVIDENCE_RESULT: ${{ steps.pr_evidence.outcome }}",
        "          SCHEMA_SWEEP_RESULT: ${{ steps.schema_producer_sweep.outcome }}",
    ] {
        if !summary.lines().any(|line| line == binding) {
            return Err("composite result must retain the actual owning step outcome".into());
        }
    }
    let sweep = step(workflow, "Schema producer sweep")?;
    if !sweep
        .lines()
        .any(|line| line == "          EVENT_NAME: ${{ github.event_name }}")
    {
        return Err("event binding changed".into());
    }
    for edge in CompositeProducer::SchemaSweep
        .contract(Event::MergeGroup)
        .artifacts
    {
        let binding = format!("--artifact {}={}", edge.schema.ok_or("schema")?, edge.path);
        if !sweep.contains(&binding) {
            return Err("schema artifact binding changed".into());
        }
    }
    Ok(())
}

#[test]
fn existing_source_matches_the_composite_contract() -> TestResult {
    source_contract(WORKFLOW)?;
    assert_eq!(PR_COMMANDS.len(), 10);
    for event in [Event::PullRequest, Event::MergeGroup, Event::Other] {
        let contract = CompositeProducer::SchemaSweep.contract(event);
        assert_eq!(contract.commands.len(), 2);
        assert_eq!(
            contract.artifacts.len(),
            if event.has_pr_evidence() { 3 } else { 1 }
        );
        assert_eq!(contract.artifacts[0].path, "target/ripr/sweep/doctor.json");
        assert_eq!(
            contract.result_owner().and_then(|owner| owner.id),
            Some("schema_producer_sweep")
        );
    }
    Ok(())
}

#[test]
fn hostile_source_edits_are_rejected() -> TestResult {
    let evidence = script(step(WORKFLOW, "PR evidence")?)?;
    let first_pair = format!("{}\n          {}", PR_COMMANDS[0], PR_COMMANDS[1]);
    let reversed = format!("{}\n          {}", PR_COMMANDS[1], PR_COMMANDS[0]);
    let cases = vec![
        (
            "    name: Rust gates".to_owned(),
            "    name: Rust gates\n    \"defaults\": {run: {shell: 'bash {0}'}}".to_owned(),
        ),
        (
            "        id: pr_evidence".to_owned(),
            "        id: pr_evidence\n        \"shell\": bash {0}".to_owned(),
        ),
        (
            "        id: schema_producer_sweep".to_owned(),
            "        id: schema_producer_sweep\n        'shell': bash {0}".to_owned(),
        ),
        (
            "        id: pr_evidence".to_owned(),
            "        id: pr_evidence\n        \"continue-on-error\": true".to_owned(),
        ),
        (
            "${{ steps.pr_evidence.outcome }}".to_owned(),
            "success".to_owned(),
        ),
        (
            "${{ steps.schema_producer_sweep.outcome }}".to_owned(),
            "success".to_owned(),
        ),
        (
            "    name: Rust gates".to_owned(),
            "    name: Rust gates\n    if: false".to_owned(),
        ),
        (
            "        id: schema_producer_sweep".to_owned(),
            "        id: schema_producer_sweep\n        if: false".to_owned(),
        ),
        (format!("if: {GUARD}"), "if: false".to_owned()),
        (format!("        if: {GUARD}\n"), String::new()),
        (
            "git rev-parse HEAD^1".to_owned(),
            "git rev-parse HEAD".to_owned(),
        ),
        (
            "cd \"$base_tree\"".to_owned(),
            "cd \"$GITHUB_WORKSPACE\"".to_owned(),
        ),
        (
            "matcher pin only".to_owned(),
            "cross-tree execution succeeded".to_owned(),
        ),
        (
            BASE_RECOVERY.after_blob.to_owned(),
            "unreviewed-blob".to_owned(),
        ),
        (
            "steps.admitted-group.outputs.merge".to_owned(),
            "steps.admitted-group.outputs.head".to_owned(),
        ),
        (first_pair, reversed),
        (
            PR_COMMANDS[3].to_owned(),
            format!("{} || true", PR_COMMANDS[3]),
        ),
        (
            "        id: pr_evidence".to_owned(),
            "        id: pr_evidence\n        continue-on-error: true".to_owned(),
        ),
        (
            "schemas/ripr/pr-evidence.schema.json=target/ripr/pr/repo-exposure.json".to_owned(),
            "schemas/ripr/pr-evidence.schema.json=old/repo-exposure.json".to_owned(),
        ),
    ];
    assert_eq!(evidence.lines().count(), 10);
    for (from, to) in cases {
        let changed = WORKFLOW.replace(&from, &to);
        assert_ne!(changed, WORKFLOW, "mutation did not apply: {from}");
        assert!(
            source_contract(&changed).is_err(),
            "accepted mutation: {from}"
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[path = "support/rust_composite_replay.rs"]
mod replay;
