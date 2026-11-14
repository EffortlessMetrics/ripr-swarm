//! Source-only setup contract; this does not execute the GitHub Actions engine.
const GATES: &str = include_str!("../../../.github/workflows/rust-gates.yml");
const SHARDS: &str = include_str!("../../../.github/workflows/rust-test-shards.yml");
const INSTALLER: &str = "taiki-e/install-action@1e29709c0ec63babbe8f266397a465f29a825f91";

fn step<'a>(source: &'a str, name: &str) -> &'a str {
    let marker = format!("      - name: {name}\n");
    assert_eq!(source.matches(&marker).count(), 1, "unique step: {name}");
    let tail = source.split_once(&marker).unwrap_or_default().1;
    tail.split("\n      - ").next().unwrap_or_default()
}

fn field<'a>(step: &'a str, key: &str) -> Vec<&'a str> {
    step.lines()
        .filter_map(|line| line.strip_prefix(key))
        .collect()
}

#[test]
fn installer_guard_is_complete_and_fail_closed() {
    let install = step(GATES, "Install CI tools");
    assert_eq!(
        field(install, "        if: "),
        ["inputs.use-scratch || inputs.test-shards != 4"]
    );
    assert!(field(install, "        continue-on-error:").is_empty());
}

// A deliberately small evaluator for the pinned, typed workflow_call condition.
// Reject unfamiliar syntax rather than claiming general Actions compatibility.
// Number fixtures are finite; exact ordering retains Actions equality (no epsilon).
fn install_selected(scratch: bool, shards: f64) -> bool {
    let install = step(GATES, "Install CI tools");
    let guards = field(install, "        if: ");
    assert_eq!(guards.len(), 1);
    let mut selected = false;
    for clause in guards[0].split(" || ") {
        selected |= match clause {
            "inputs.use-scratch" => scratch,
            "inputs.test-shards != 4" => shards.total_cmp(&4.0) != std::cmp::Ordering::Equal,
            unexpected => {
                assert_eq!(
                    unexpected, "inputs.test-shards != 4",
                    "unsupported predicate"
                );
                shards.total_cmp(&4.0) != std::cmp::Ordering::Equal
            }
        };
    }
    selected
}

#[test]
fn only_four_shard_hosted_setup_is_omitted() {
    for (scratch, shards, expected) in [
        (false, 4.0, false),
        (false, 1.0, true),
        (true, 1.0, true),
        (true, 4.0, true),
        (false, 0.0, true),
        (false, 2.0, true),
        (false, -1.0, true),
        (false, 8.0, true),
        (false, 4.5, true),
        (false, 3.5, true),
        (true, 0.0, true),
        (true, 8.0, true),
    ] {
        assert_eq!(
            install_selected(scratch, shards),
            expected,
            "{scratch}/{shards}"
        );
    }
}

#[test]
fn installer_pin_and_tool_selection_are_retained() {
    let install = step(GATES, "Install CI tools");
    assert_eq!(
        field(install, "        uses: "),
        [format!("{INSTALLER} # v2.87.31")]
    );
    assert_eq!(
        field(install, "          tool: "),
        ["${{ inputs.use-scratch && 'sccache,nextest' || 'nextest' }}"]
    );
    // No input may override the pinned installer's checksum/fallback defaults.
    assert_eq!(
        install
            .lines()
            .filter(|line| line.starts_with("          "))
            .count(),
        1
    );
}

#[test]
fn singleton_and_shard_consumers_keep_their_installer() {
    let tests = step(GATES, "Required Rust tests");
    assert_eq!(field(tests, "        if: "), ["inputs.test-shards == 1"]);
    assert!(tests.contains("          cargo nextest run --workspace --profile ci\n"));
    let scratch = step(GATES, "Start sccache server");
    assert_eq!(field(scratch, "        if: "), ["inputs.use-scratch"]);
    let install = step(SHARDS, "Install CI tools");
    assert!(field(install, "        if:").is_empty());
    assert_eq!(field(install, "          tool: "), ["nextest@0.9.149"]);
    assert_eq!(
        field(install, "        uses: "),
        [format!("{INSTALLER} # v2.87.31")]
    );
}
