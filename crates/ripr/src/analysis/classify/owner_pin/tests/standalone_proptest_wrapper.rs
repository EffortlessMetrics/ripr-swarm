//! #14116: each audited `proptest` wrapper confines its arguments on its own,
//! not only inside `proptest!`: a strategy helper's `prop_oneof!` pastes its
//! arguments into an expression just the same.

use super::*;

fn audited_at(label: &str, version: &str, sibling: &str) -> Result<bool, String> {
    let lock = lock_of(&[("proptest", version, REGISTRY)]);
    Ok(prelude_rebound(prelude_refusal(
        &format!("standalone-{label}"),
        "proptest = \"1\"\n",
        Some(&lock),
        sibling,
    )?))
}

const GLOB: &str = "use proptest::prelude::*;\n";

fn helper(call: &str) -> String {
    format!(
        "pub fn strategy() -> impl Strategy<Value = u8> {{\n    {call}![Just(1u8).prop_map(|v| {{ assert_eq!(v, v); v }}), Just(2u8)]\n}}\n"
    )
}

#[test]
fn a_standalone_wrapper_confines_its_arguments_to_the_call() -> Result<(), String> {
    // Fixture premise: the mention refuses the test elsewhere when the
    // locked version is not the audited one.
    let bare = format!("{GLOB}{}", helper("prop_oneof"));
    assert!(bare.contains("prop_oneof![Just(1u8).prop_map(|v| { assert_eq!(v, v); v })"));
    assert!(audited_at("premise", "1.11.0", &bare)?);
    for (label, sibling) in [
        ("bare", bare.clone()),
        ("qualified", helper("proptest::prop_oneof")),
        ("external", helper("::proptest::prop_oneof")),
        (
            "qualified-with-glob",
            format!("{GLOB}{}", helper("proptest::prop_oneof")),
        ),
        (
            "bare-assume",
            format!(
                "{GLOB}pub fn check(v: u8) {{ prop_assume!({{ assert_eq!(v, v); true }}); }}\n"
            ),
        ),
    ] {
        assert!(
            !audited_at(label, "1.12.0", &sibling)?,
            "{label}: {sibling}"
        );
    }
    Ok(())
}

#[test]
fn a_standalone_wrapper_without_the_audited_path_keeps_the_site() -> Result<(), String> {
    for (label, sibling) in [
        // A bare name needs the audited prelude glob in its module.
        ("no-glob", helper("prop_oneof")),
        // Another root names whatever that module exports.
        ("other-root", helper("dep::prop_oneof")),
        (
            "crate-root",
            format!("{GLOB}{}", helper("crate::prop_oneof")),
        ),
        // A local definition takes the bare name.
        (
            "local-definition",
            format!(
                "{GLOB}macro_rules! prop_oneof {{ ($($t:tt)*) => {{ $($t)* }} }}\n{}",
                helper("prop_oneof")
            ),
        ),
        // `prop_compose!` defines an item and is no audited wrapper.
        (
            "not-a-wrapper",
            format!(
                "{GLOB}prop_compose! {{ fn pair()(v in 0u8..3) -> u8 {{ assert_eq!(v, v); v }} }}\n"
            ),
        ),
        // A definition in the arguments can leave the call.
        (
            "definition-in-arguments",
            format!(
                "{GLOB}pub fn s() {{ prop_assume!({{ macro_rules! assert_eq {{ () => {{}} }} true }}); }}\n"
            ),
        ),
    ] {
        assert!(audited_at(label, "1.12.0", &sibling)?, "{label}: {sibling}");
    }
    Ok(())
}
