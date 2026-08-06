const OVERVIEW_HELP_SOURCE: &str = include_str!("../src/cli/help/overview.rs");
const AGENT_HELP_SOURCE: &str = include_str!("../src/cli/help/agent.rs");

#[test]
fn public_help_keeps_the_task_roles_distinct() {
    for needle in [
        "Diagnose setup        ripr doctor",
        "Inspect one change    ripr check",
        "Guided repo adoption  ripr pilot --root .",
        "Repair one named gap  ripr agent repair",
        "Compose PR evidence   ripr first-pr",
        "Adopt advisory CI     ripr init --ci github",
    ] {
        assert!(
            OVERVIEW_HELP_SOURCE.contains(needle),
            "public help lost the canonical task route `{needle}`"
        );
    }

    assert!(
        OVERVIEW_HELP_SOURCE.contains(
            "`ripr check` is the ordinary first-value analysis; `ripr pilot` is the guided repo-adoption workflow."
        ),
        "exhaustive help should keep check and pilot roles distinct"
    );
    assert!(
        OVERVIEW_HELP_SOURCE.contains(
            "`ripr first-pr` and `ripr start-here` compose `target/ripr/reports/start-here.{json,md}` from existing artifacts; they do not run analysis or repair a gap."
        ),
        "exhaustive help should keep PR composition distinct from analysis and repair"
    );
}

#[test]
fn agent_help_makes_repair_primary_without_removing_control_surfaces() {
    assert!(
        AGENT_HELP_SOURCE.contains("Primary workflow:"),
        "agent help should name the ordinary workflow"
    );
    assert!(
        AGENT_HELP_SOURCE.contains(
            "repair    Run the two-phase before/edit/after repair transaction for one seam."
        ),
        "agent repair should be the primary repair route"
    );
    assert!(
        AGENT_HELP_SOURCE.contains("Advanced and compatibility workflows:"),
        "lower-level agent commands should remain available under an explicit boundary"
    );
    for command in [
        "start",
        "brief",
        "packet",
        "verify",
        "verify-execute",
        "receipt",
        "review-summary",
    ] {
        assert!(
            AGENT_HELP_SOURCE.contains(command),
            "agent help lost advanced command `{command}`"
        );
    }
}
