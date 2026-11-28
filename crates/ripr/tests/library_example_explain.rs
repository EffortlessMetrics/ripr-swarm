//! The `explain` library example runs and shows what it claims (#14098).
//!
//! The example is compiled into this test crate by path, so the code a reader
//! runs with `cargo run --example explain` is the code checked here. Each
//! example has its own test file because all three load the shared
//! `examples/support/sample_workspace.rs` module, and one crate may load a
//! file as a module only once. The explanation must name the changed
//! line of the bundled sample and the missing boundary.

#[path = "../examples/explain.rs"]
mod explain;

#[test]
fn explain_names_the_changed_line_the_missing_boundary_and_a_next_step() -> Result<(), String> {
    let explanation = explain::report()?;

    for expected in [
        "./src/lib.rs:8",
        "after:  amount >= discount_threshold",
        "weakly_exposed",
        "missing discriminator amount == discount_threshold",
        "Next step",
    ] {
        assert!(
            explanation.contains(expected),
            "missing `{expected}` in:\n{explanation}"
        );
    }

    // The follow-up command the explanation prints names a root and a diff
    // that still exist once the example has returned.
    let next = explanation
        .lines()
        .find_map(|line| line.strip_prefix("Next: ripr context --root "))
        .ok_or_else(|| format!("no follow-up command in:\n{explanation}"))?;
    let (root, rest) = next
        .split_once(" --diff ")
        .ok_or_else(|| format!("no --diff in `{next}`"))?;
    let (diff, _) = rest
        .split_once(" --at ")
        .ok_or_else(|| format!("no --at in `{next}`"))?;
    assert!(
        std::path::Path::new(root).join("Cargo.toml").is_file(),
        "{next}"
    );
    assert!(std::path::Path::new(diff).is_file(), "{next}");

    explain::main()
}
