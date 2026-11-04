//! Queue event wiring for the existing advisory security workflow.
const SECURITY: &str = include_str!("../../../.github/workflows/security.yml");

fn queue_contract(source: &str) -> bool {
    let Some((events, jobs)) = source.split_once("\njobs:\n") else {
        return false;
    };
    let Some((_, dependency)) = jobs.split_once("  dependency-review:\n") else {
        return false;
    };
    let Some((_, cargo)) = jobs.split_once("  cargo-deny:\n") else {
        return false;
    };
    let cargo = cargo.split("\n  # Scheduled runs").next().unwrap_or("");
    let Some((_, tracker)) = jobs.split_once("  deny-tracker:\n") else {
        return false;
    };
    let tracker = tracker.split("\n  dependency-review:").next().unwrap_or("");
    events.contains("  merge_group:\n    types: [checks_requested]\n")
        && cargo.contains("github.event_name != 'pull_request' || (contains(github.event.pull_request.labels.*.name, 'merge-ready') && (github.event.action == 'labeled' && github.event.label.name == 'merge-ready'))")
        && dependency.contains("github.event_name == 'pull_request' || github.event_name == 'merge_group'")
        && dependency.contains("github.event.action == 'labeled' && github.event.label.name == 'merge-ready'")
        && dependency.contains("actions/dependency-review-action@2031cfc080254a8a887f58cffee85186f0e49e48")
        && dependency.contains("fail-on-severity: high")
        && dependency.contains("deny-licenses: GPL-2.0, GPL-3.0, AGPL-1.0, AGPL-3.0")
        && !dependency.contains("base-ref:") && !dependency.contains("head-ref:")
        && tracker.contains("github.event_name == 'schedule'")
}

#[test]
fn security_queue_wiring_preserves_pr_guard_and_scheduled_tracker() {
    assert!(queue_contract(SECURITY));
    for (from, to) in [
        ("  merge_group:\n    types: [checks_requested]\n", ""),
        (
            "github.event_name == 'pull_request' || github.event_name == 'merge_group'",
            "github.event_name == 'pull_request'",
        ),
        (
            "github.event.action == 'labeled' && github.event.label.name == 'merge-ready'",
            "true",
        ),
        ("github.event_name == 'schedule'", "true"),
        ("fail-on-severity: high", "fail-on-severity: critical"),
    ] {
        let changed = SECURITY.replacen(from, to, 1);
        assert_ne!(changed, SECURITY);
        assert!(!queue_contract(&changed), "mutation accepted: {from}");
    }
}
