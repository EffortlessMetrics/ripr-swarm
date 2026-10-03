//! Pin the protected PR qualification event at the workflow boundary.
//!
//! Draft activity must not create the required `Ripr Rust Small Result` check.
//! The native Draft -> Ready transition is the only pull-request admission
//! event; main and explicit manual authorities remain separate.

use std::fs;
use std::path::Path;

const WORKFLOW: &str = ".github/workflows/routed-rust.yml";

fn workflow_source() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    fs::read_to_string(root.join(WORKFLOW)).expect("read routed Rust workflow")
}

fn event_block(source: &str) -> &str {
    source
        .split_once("\npermissions:\n")
        .map(|(events, _)| events)
        .expect("workflow keeps permissions after event declarations")
}

fn pull_request_types(events: &str) -> &str {
    events
        .lines()
        .find(|line| line.trim_start().starts_with("types: ["))
        .map(str::trim)
        .expect("pull_request keeps an explicit action allowlist")
}

fn assert_ready_only_pull_request(events: &str) {
    assert_eq!(
        pull_request_types(events),
        "types: [ready_for_review]",
        "protected PR qualification must be requested only by ready_for_review"
    );
}

#[test]
fn required_pr_context_is_withheld_until_ready() {
    let source = workflow_source();
    let events = event_block(&source);

    assert_ready_only_pull_request(events);
    assert!(events.contains("push:\n    branches: [main, master]"));
    assert!(events.contains("workflow_dispatch:"));
    assert!(source.contains("  result:\n    name: Ripr Rust Small Result"));
    assert!(!source.contains("Ripr Rust Small Ignored Label Event"));
    assert!(!source.contains("github.event.pull_request.draft"));
}

#[test]
fn contract_rejects_draft_or_mutation_triggers() {
    let source = workflow_source();
    let changed = source.replace(
        "types: [ready_for_review]",
        "types: [ready_for_review, synchronize]",
    );
    assert_ne!(changed, source, "trigger mutation must engage");

    let result = std::panic::catch_unwind(|| assert_ready_only_pull_request(event_block(&changed)));
    assert!(result.is_err(), "synchronize must violate the protected event law");
}

#[test]
fn contract_rejects_a_noncanonical_terminal_context() {
    let source = workflow_source();
    let changed = source.replace(
        "  result:\n    name: Ripr Rust Small Result",
        "  result:\n    name: Ripr Rust Small Draft Result",
    );
    assert_ne!(changed, source, "result-name mutation must engage");
    assert!(!changed.contains("  result:\n    name: Ripr Rust Small Result"));
}
