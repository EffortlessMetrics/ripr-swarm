//! #13700: a unique related test that binds a direct owner result and never
//! reads the changed field starts the field-value repair route
//! (`benchmarks/repair_start/cases.json` case `rust/field-change`). Every
//! other shape stays fail-closed: a strong read, a second candidate test, a
//! binding passed elsewhere or shadowed, a second owner call that may observe
//! the whole value, a `use` alias of the owner, a changed field of a nested
//! record or of an `async` block's `return`, a declared return type that is
//! only an alias of the constructed struct, and a field the test cannot name.
//! Integration fixtures spell the owner bare: the in-memory index has no
//! package-name authority, so a `use <package>::owner` import is settled
//! only end to end (`tests/unread_owner_result_field_repair_start.rs`).

use super::*;

const PUBLIC_CONFIG: &str = r#"
pub struct Config {
    pub retries: u32,
    pub verbose: bool,
}

pub fn default_config() -> Config {
    Config {
        retries: 5,
        verbose: false,
    }
}
"#;

const PRIVATE_CONFIG: &str = r#"
pub struct Config {
    retries: u32,
    pub verbose: bool,
}

pub fn default_config() -> Config {
    Config {
        retries: 5,
        verbose: false,
    }
}
"#;

const UNREAD_REASON: &str = "is bound but never read by the related test";

fn integration_case(production: &str, test_source: &str) -> Result<ConstructorFieldCase, String> {
    constructor_field_case_at(
        "src/lib.rs",
        production,
        "tests/t.rs",
        test_source,
        "retries",
    )
}

fn inline_case(production: &str, test_source: &str) -> Result<ConstructorFieldCase, String> {
    let path = PathBuf::from("src/lib.rs");
    let source = format!("{production}\n{test_source}");
    classified_from_files(&[(path.clone(), source.as_str())], &path, "retries")
}

fn unread_fact_count(case: &ConstructorFieldCase) -> usize {
    case.classified
        .evidence
        .missing_discriminators
        .iter()
        .filter(|fact| fact.reason.contains(UNREAD_REASON))
        .count()
}

/// The setup must reach the gate under test: the owner runs (activation
/// `Yes`) and the named test is related through a direct owner call.
fn setup_must_reach_gate(case: &ConstructorFieldCase, test_name: &str) -> Result<(), String> {
    let evidence = &case.classified.evidence;
    if evidence.activate.state != StageState::Yes
        || !evidence.related_tests.iter().any(|test| {
            test.test_name == test_name && test.relation_reason == RelationReason::DirectOwnerCall
        })
    {
        return Err(format!(
            "fixture must activate the owner through `{test_name}`: activate={:?}, related={:?}",
            evidence.activate.state,
            evidence
                .related_tests
                .iter()
                .map(|test| (test.test_name.clone(), test.relation_reason))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn unread_route_must_be_ready(case: &ConstructorFieldCase, test_name: &str) -> Result<(), String> {
    setup_must_reach_gate(case, test_name)?;
    let required = required_field(&case.seam)?;
    let eligibility = repair_packet_eligibility(&case.classified);
    let missing = &case.classified.evidence.missing_discriminators;
    let target_name = match &eligibility.readiness.target_selection {
        RepairTargetSelection::Existing(target) => case
            .classified
            .evidence
            .related_tests
            .iter()
            .find(|test| test.test_target.as_ref() == Some(target))
            .map(|test| test.test_name.clone()),
        _ => None,
    };
    if unread_fact_count(case) != 1
        || !missing.iter().any(|fact| fact.value == required)
        || eligibility.readiness.state != RepairRouteState::Ready
        || !eligibility.eligible()
        || !is_safe_for_repair_packet(&case.classified)
        || target_name.as_deref() != Some(test_name)
    {
        return Err(format!(
            "an unread owner-result field must start one deterministic repair: class={:?}, missing={missing:?}, readiness={:?}, target={target_name:?}, eligible={}",
            case.classified.class,
            eligibility.readiness.state,
            eligibility.eligible()
        ));
    }
    Ok(())
}

fn no_unread_fact(case: &ConstructorFieldCase, label: &str) -> Result<(), String> {
    if unread_fact_count(case) != 0 {
        return Err(format!(
            "{label} must not grow the unread field fact: missing={:?}",
            case.classified.evidence.missing_discriminators
        ));
    }
    route_must_stay_unready(case, label)
}

#[test]
fn unread_public_field_in_integration_test_starts_repair() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
}
"#,
    )?;
    unread_route_must_be_ready(&case, "default_config_is_quiet")
}

#[test]
fn unread_private_field_in_inline_test_starts_repair() -> Result<(), String> {
    let case = inline_case(
        PRIVATE_CONFIG,
        r#"
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_quiet() {
        let c = default_config();
        assert!(!c.verbose);
    }
}
"#,
    )?;
    unread_route_must_be_ready(&case, "default_config_is_quiet")
}

#[test]
fn strong_read_of_the_field_gets_no_repair() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_retries_five_times() {
    let c = default_config();
    assert_eq!(c.retries, 5);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_retries_five_times")?;
    no_unread_fact(&case, "a strong field read")?;
    if repair_packet_eligibility(&case.classified).readiness.state
        != RepairRouteState::AlreadyGripped
    {
        return Err(format!(
            "a strong field read must stay AlreadyGripped: {:?}",
            repair_packet_eligibility(&case.classified).readiness.state
        ));
    }
    Ok(())
}

#[test]
fn field_read_through_an_alias_gets_no_unread_fact() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    let retries = c.retries;
    assert!(!c.verbose);
    assert!(retries > 0);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    if unread_fact_count(&case) != 0 {
        return Err("a field read through an alias is not unread".to_string());
    }
    Ok(())
}

#[test]
fn two_candidate_tests_are_not_a_unique_target() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
}

#[test]
fn default_config_is_still_quiet() {
    let d = default_config();
    assert!(!d.verbose);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    setup_must_reach_gate(&case, "default_config_is_still_quiet")?;
    no_unread_fact(&case, "two candidate tests")
}

#[test]
fn binding_passed_elsewhere_is_not_unread() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
fn inspect(config: &Config) -> bool {
    config.verbose
}

#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!inspect(&c));
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a binding passed to a helper")
}

#[test]
fn shadowed_binding_is_not_unread() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
    let c = 3;
    assert!(c > 0);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a shadowed binding")
}

#[test]
fn private_field_from_integration_test_is_not_visible() -> Result<(), String> {
    let case = integration_case(
        PRIVATE_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a field the test file cannot name")
}

#[test]
fn whole_value_debug_format_is_not_unread() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose, "{c:?}");
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    if unread_fact_count(&case) != 0 {
        return Err("a format capture of the whole value is not unread".to_string());
    }
    Ok(())
}

#[test]
fn foreign_crate_import_of_the_owner_name_is_not_unread() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
use other_crate::default_config;

#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "an owner name imported from another crate")
}

const EQ_CONFIG: &str = r#"
#[derive(Debug, PartialEq)]
pub struct Config {
    pub retries: u32,
    pub verbose: bool,
}

pub fn default_config() -> Config {
    Config {
        retries: 5,
        verbose: false,
    }
}
"#;

#[test]
fn second_owner_call_compared_whole_is_not_unread() -> Result<(), String> {
    let case = integration_case(
        EQ_CONFIG,
        r#"
fn expected_config() -> Config {
    Config { retries: 5, verbose: false }
}

#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
    assert_eq!(default_config(), expected_config());
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a second owner call compared as a whole value")
}

#[test]
fn second_owner_call_snapshotted_is_not_unread() -> Result<(), String> {
    let case = integration_case(
        EQ_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
    insta::assert_debug_snapshot!(default_config());
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a second owner call captured by a snapshot")
}

#[test]
fn owner_called_through_a_use_alias_is_not_unread() -> Result<(), String> {
    for (label, import) in [
        ("plain alias", "use super::default_config as dc;"),
        ("grouped alias", "use super::{default_config as dc};"),
    ] {
        let case = inline_case(
            EQ_CONFIG,
            &format!(
                r#"
#[cfg(test)]
mod tests {{
    use super::*;
    {import}

    fn expected_config() -> Config {{
        Config {{ retries: 5, verbose: false }}
    }}

    #[test]
    fn default_config_is_quiet() {{
        let c = default_config();
        assert!(!c.verbose);
        assert_eq!(dc(), expected_config());
    }}
}}
"#
            ),
        )?;
        setup_must_reach_gate(&case, "default_config_is_quiet")
            .map_err(|error| format!("{label}: {error}"))?;
        no_unread_fact(&case, label)?;
    }
    Ok(())
}

#[test]
fn changed_field_of_a_nested_record_is_not_unread() -> Result<(), String> {
    // `retries` belongs to `Inner`, built inside the owner's `Wrapper`; the
    // test compares `w.inner` whole, which observes it.
    let case = integration_case(
        r#"
#[derive(Debug, PartialEq)]
pub struct Inner {
    pub retries: u32,
}

pub struct Wrapper {
    pub inner: Inner,
    pub verbose: bool,
}

pub fn default_config() -> Wrapper {
    Wrapper {
        inner: Inner { retries: 5 },
        verbose: false,
    }
}
"#,
        r#"
fn expected_inner() -> Inner {
    Inner { retries: 3 }
}

#[test]
fn default_config_is_quiet() {
    let w = default_config();
    assert!(!w.verbose);
    assert_eq!(w.inner, expected_inner());
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a changed field of a nested record")
}

#[test]
fn early_return_construction_in_the_owner_starts_repair() -> Result<(), String> {
    let case = integration_case(
        r#"
pub struct Config {
    pub retries: u32,
    pub verbose: bool,
}

pub fn default_config() -> Config {
    return Config {
        retries: 5,
        verbose: false,
    };
}
"#,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
}
"#,
    )?;
    unread_route_must_be_ready(&case, "default_config_is_quiet")
}

#[test]
fn return_inside_an_async_block_is_not_unread() -> Result<(), String> {
    // The `return` leaves the future, not the owner: the changed literal is
    // not what `default_config` returns.
    let case = integration_case(
        r#"
pub struct Config {
    pub retries: u32,
    pub verbose: bool,
}

pub fn default_config() -> Config {
    let _later = async {
        return Config {
            retries: 5,
            verbose: false,
        };
    };
    Config {
        retries: 3,
        verbose: false,
    }
}
"#,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a `return` inside an async block")
}

#[test]
fn return_type_alias_of_the_constructed_struct_is_not_unread() -> Result<(), String> {
    // The declared return type `Cfg` only aliases `Config`; the equality
    // gate cannot place it, so it fails closed.
    let case = integration_case(
        r#"
pub struct Config {
    pub retries: u32,
    pub verbose: bool,
}

pub type Cfg = Config;

pub fn default_config() -> Cfg {
    Config {
        retries: 5,
        verbose: false,
    }
}
"#,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert!(!c.verbose);
}
"#,
    )?;
    setup_must_reach_gate(&case, "default_config_is_quiet")?;
    no_unread_fact(&case, "a declared return type alias")
}

#[test]
fn binding_no_assertion_reads_is_not_unread() -> Result<(), String> {
    // Every label is checked and every failure reported, so a red run names
    // each shape the gate under test lets through.
    let mut failures = Vec::new();
    for (label, body) in [
        (
            "discarded binding",
            "let _c = default_config();\n    assert!(1 > 0);",
        ),
        (
            "read outside any assertion",
            "let c = default_config();\n    let _quiet = c.verbose;\n    assert!(1 > 0);",
        ),
        (
            "binding name only inside a string literal",
            "let c = default_config();\n    assert!(1 > 0, \"c\");",
        ),
        (
            "binding name only inside a message literal",
            "let c = default_config();\n    assert!(true, \"c built\");",
        ),
        (
            "binding name only as a field of a record literal",
            "let c = default_config();\n    let s = S { c: 1 };\n    assert!(s.c > 0);",
        ),
        (
            "binding name only as a field of another value",
            "let c = default_config();\n    let s = other();\n    assert!(s.c > 0);",
        ),
        (
            "binding name only inside a comment in the assertion",
            "let c = default_config();\n    assert!(true /* c */);",
        ),
        (
            "binding read only in a format argument",
            "let c = default_config();\n    assert!(true, \"{}\", c.verbose);",
        ),
        (
            "turbofish method call through the binding (macro-split `::`)",
            "let c = default_config();\n    assert_eq!(c.get::<u64>(), 5);",
        ),
        (
            "struct-update base `..c` inside an assertion (macro-split `..`)",
            "let c = default_config();\n    assert!(!c.verbose);\n    assert_eq!(Config { verbose: true, ..c }, expected());",
        ),
        (
            "binding read only in an assert_eq message argument",
            "let c = default_config();\n    assert_eq!(1, 1, \"{}\", c.verbose);",
        ),
        (
            "binding and assertion on the same line",
            "let c = default_config(); assert!(!c.verbose);",
        ),
        (
            "binding read only in debug_assert",
            "let c = default_config();\n    debug_assert!(!c.verbose);",
        ),
        (
            "sub-field chain through a sibling field",
            "let c = default_config();\n    assert!(c.verbose.then_some(1).is_none());",
        ),
    ] {
        let case = integration_case(
            PUBLIC_CONFIG,
            &format!("\n#[test]\nfn default_config_is_quiet() {{\n    {body}\n}}\n"),
        )?;
        if let Err(error) = setup_must_reach_gate(&case, "default_config_is_quiet")
            .map_err(|error| format!("{label}: {error}"))
            .and_then(|()| no_unread_fact(&case, label))
        {
            failures.push(error);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn sibling_read_in_a_multiline_assert_eq_starts_repair() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert_eq!(
        c.verbose,
        false
    );
}
"#,
    )?;
    unread_route_must_be_ready(&case, "default_config_is_quiet")
}

#[test]
fn sibling_read_in_the_second_assert_eq_operand_starts_repair() -> Result<(), String> {
    let case = integration_case(
        PUBLIC_CONFIG,
        r#"
#[test]
fn default_config_is_quiet() {
    let c = default_config();
    assert_eq!(false, c.verbose);
}
"#,
    )?;
    unread_route_must_be_ready(&case, "default_config_is_quiet")
}
