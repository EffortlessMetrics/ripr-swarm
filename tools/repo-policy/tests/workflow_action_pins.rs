//! #9529: immutable revisions retain the original step's explicit tool choice.
//! Resolved via exact upstream heads/tags on 2026-10-22; rust-cache v2 is
//! annotated, so the pin is its peeled commit, not its tag-object SHA.
#[path = "../test_support.rs"]
mod test_support;

use std::collections::BTreeMap;

// (workflow, action, original ref, resolved commit, required input, count).
type Pin = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    usize,
);

const PINS: &[Pin] = &[
    (
        "ci.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        3,
    ),
    (
        "ci.yml",
        "dtolnay/rust-toolchain",
        "1.95.0",
        "035fceb3c12fb0c6a9461b52775e16473133e591",
        Some("toolchain: 1.95.0"),
        4,
    ),
    (
        "dx-scoreboard.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "dx-scoreboard.yml",
        "dtolnay/rust-toolchain",
        "stable",
        "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
        Some("toolchain: stable"),
        1,
    ),
    (
        "first-run-archive.yml",
        "dtolnay/rust-toolchain",
        "stable",
        "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
        Some("toolchain: stable"),
        1,
    ),
    (
        "first-run-install.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "first-run-install.yml",
        "dtolnay/rust-toolchain",
        "stable",
        "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
        Some("toolchain: stable"),
        2,
    ),
    (
        "future-clippy.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "future-clippy.yml",
        "dtolnay/rust-toolchain",
        "1.95.0",
        "035fceb3c12fb0c6a9461b52775e16473133e591",
        Some("toolchain: 1.95.0"),
        1,
    ),
    (
        "macos-advisory.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "macos-advisory.yml",
        "dtolnay/rust-toolchain",
        "1.95",
        "035fceb3c12fb0c6a9461b52775e16473133e591",
        Some("toolchain: \"1.95\""),
        1,
    ),
    (
        "pilot-ranking.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "pilot-ranking.yml",
        "dtolnay/rust-toolchain",
        "1.95.0",
        "035fceb3c12fb0c6a9461b52775e16473133e591",
        Some("toolchain: 1.95.0"),
        1,
    ),
    (
        "printed-command-paste.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "printed-command-paste.yml",
        "dtolnay/rust-toolchain",
        "1.95",
        "035fceb3c12fb0c6a9461b52775e16473133e591",
        Some("toolchain: \"1.95\""),
        1,
    ),
    (
        "public-proof.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "public-proof.yml",
        "dtolnay/rust-toolchain",
        "stable",
        "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
        Some("toolchain: stable"),
        1,
    ),
    (
        "routed-rust.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "routed-rust.yml",
        "dtolnay/rust-toolchain",
        "v1",
        "683b588c487a69d01f6c6ddf40f036ff682cc2c8",
        Some("toolchain: 1.95.0"),
        1,
    ),
    (
        "rust-corpus.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "rust-corpus.yml",
        "dtolnay/rust-toolchain",
        "stable",
        "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
        Some("toolchain: stable"),
        1,
    ),
    (
        "rust-gates.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        1,
    ),
    (
        "rust-gates.yml",
        "dtolnay/rust-toolchain",
        "v1",
        "683b588c487a69d01f6c6ddf40f036ff682cc2c8",
        Some("toolchain: 1.95.0"),
        1,
    ),
    (
        "rust-gates.yml",
        "taiki-e/install-action",
        "v2",
        "a4d3f3f761dcd95cce1b72cb61cb6d1963438177",
        Some("tool: ${{ inputs.use-scratch && 'sccache,nextest' || 'nextest' }}"),
        1,
    ),
    (
        "security.yml",
        "dtolnay/rust-toolchain",
        "stable",
        "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
        Some("toolchain: stable"),
        1,
    ),
    (
        "security.yml",
        "taiki-e/install-action",
        "cargo-deny",
        "7ff3827688bd742ffc0b597ab1bf503967134e46",
        Some("tool: cargo-deny"),
        1,
    ),
    (
        "source-of-truth.yml",
        "dtolnay/rust-toolchain",
        "stable",
        "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
        Some("toolchain: stable"),
        2,
    ),
    (
        "windows-advisory.yml",
        "Swatinem/rust-cache",
        "v2",
        "d1527c8f67ef9af357290475d4af4b0fab8f12e0",
        None,
        2,
    ),
    (
        "windows-advisory.yml",
        "dtolnay/rust-toolchain",
        "1.95",
        "035fceb3c12fb0c6a9461b52775e16473133e591",
        Some("toolchain: \"1.95\""),
        2,
    ),
    (
        "zed-extension.yml",
        "dtolnay/rust-toolchain",
        "1.95.0",
        "035fceb3c12fb0c6a9461b52775e16473133e591",
        Some("toolchain: 1.95.0"),
        1,
    ),
];

fn source(name: &str) -> Result<String, String> {
    std::fs::read_to_string(
        test_support::workspace_root()?
            .join(".github/workflows")
            .join(name),
    )
    .map_err(|e| e.to_string())
}

fn verify(text: &str, pin: &Pin) -> Result<usize, String> {
    let (_, action, original_ref, revision, input, _) = pin;
    let header = format!("uses: {action}@{revision} # {original_ref}");
    let mut count = 0;
    for step in text.split("\n      - ").skip(1) {
        if !step.lines().any(|line| line.trim() == header) {
            continue;
        }
        count += 1;
        if let Some(input) = input
            && !step.lines().any(|line| line.trim() == *input)
        {
            return Err(format!("{header} lost its step-local {input}"));
        }
    }
    Ok(count)
}

#[test]
fn burn_down_pins_preserve_step_local_tool_selections() -> Result<(), String> {
    let mut total = 0;
    for pin in PINS {
        let count = verify(&source(pin.0)?, pin)?;
        assert_eq!(count, pin.5, "{} {}@{}", pin.0, pin.1, pin.2);
        total += count;
    }
    assert_eq!(total, 39, "include both current-main first-run additions");
    Ok(())
}

#[test]
fn pin_contract_rejects_missing_wrong_or_neighboring_inputs() -> Result<(), String> {
    let pin = PINS
        .iter()
        .find(|pin| pin.0 == "future-clippy.yml" && pin.1 == "dtolnay/rust-toolchain")
        .ok_or("missing toolchain control")?;
    let text = source(pin.0)?;
    assert_eq!(verify(&text, pin)?, 1);
    for wrong in [
        text.replace("          toolchain: 1.95.0\n", ""),
        text.replace("toolchain: 1.95.0", "toolchain: stable"),
        text.replace("          toolchain: 1.95.0\n", "")
            + "\n      - name: unrelated\n        with:\n          toolchain: 1.95.0\n",
    ] {
        assert_ne!(wrong, text, "mutation must change the source");
        assert!(
            verify(&wrong, pin).is_err(),
            "wrong step-local toolchain passed"
        );
    }
    let wrong_revision = text.replace(pin.3, "0000000000000000000000000000000000000000");
    assert_eq!(verify(&wrong_revision, pin)?, 0);
    Ok(())
}

#[test]
fn first_run_main_composition_keeps_both_new_actions_pinned() -> Result<(), String> {
    let text = source("first-run-install.yml")?;
    let (_, main_job) = text
        .split_once("\n  first-run-main:\n")
        .ok_or("current-main companion job must exist")?;
    for action in ["dtolnay/rust-toolchain", "Swatinem/rust-cache"] {
        let pin = PINS
            .iter()
            .find(|pin| pin.0 == "first-run-install.yml" && pin.1 == action)
            .ok_or("missing first-run action contract")?;
        assert_eq!(verify(main_job, pin)?, 1);
        let pinned = format!("{}@{} # {}", pin.1, pin.3, pin.2);
        let unpinned = main_job.replace(&pinned, &format!("{}@{}", pin.1, pin.2));
        assert_ne!(unpinned, main_job, "must unpin the real companion step");
        let violations = repo_policy::workflow_unpinned_action_violations(
            ".github/workflows/first-run-install.yml",
            &unpinned,
            &BTreeMap::new(),
        );
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains(action), "{violations:?}");
    }
    Ok(())
}
