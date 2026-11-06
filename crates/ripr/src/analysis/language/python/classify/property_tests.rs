//! Shrinking semantic source inputs, not synthetic Rust stage tuples (#12927).
use super::super::owners_tests::{extract_owners, extract_tests};
use super::classify_change_with_source;
use crate::analysis::classification_properties::{
    exposure_matches, oracle_upgrade, preview_finding,
};
use crate::domain::Finding;
use proptest::prelude::*;
use std::path::Path;

fn finding(n: i32, delta: i32, relation: u8, oracle: u8, guarded: bool) -> Result<Finding, String> {
    finding_with_arm(n, delta, relation, oracle, guarded, false)
}

fn finding_with_arm(
    n: i32,
    delta: i32,
    relation: u8,
    oracle: u8,
    guarded: bool,
    unentered: bool,
) -> Result<Finding, String> {
    let indent = if unentered { "        " } else { "    " };
    let added = format!("{indent}return n + {delta}");
    let source = if unentered {
        format!(
            "def shifted(n):\n    if n > {}:\n{added}\n    return n\n",
            n + 1
        )
    } else {
        format!("def shifted(n):\n{added}\n")
    };
    let (import, call) = match relation {
        0 => ("from src.subject import shifted", "shifted"),
        1 => ("from src.subject import shifted as renamed", "renamed"),
        _ => ("from unrelated import shifted", "shifted"),
    };
    let body = match oracle {
        0 => format!("{call}({n})"),
        1 => format!("assert {call}({n}) is not None"),
        2 => format!(
            "assert {call}({n}) == {}",
            if unentered { n } else { n + delta }
        ),
        _ => format!("assert custom_check({call}({n}))"),
    };
    let decorator = if guarded {
        "@pytest.mark.skipif(DISABLED, reason='runtime')\n"
    } else {
        ""
    };
    let tests = format!("import pytest\n{import}\n{decorator}def test_value():\n    {body}\n");
    let file = Path::new("src/subject.py");
    let owners = extract_owners(file, &source);
    let extracted = extract_tests(Path::new("tests/test_subject.py"), &tests);
    // Python also retains a module owner; require the intended function,
    // rather than assuming the adapter has TypeScript's owner cardinality.
    if owners
        .iter()
        .filter(|owner| owner.name == "shifted")
        .count()
        != 1
        || extracted.len() != 1
    {
        return Err(format!(
            "fixture extraction failed: {} owners, {} tests: {source}\n{tests}",
            owners.len(),
            extracted.len()
        ));
    }
    classify_change_with_source(
        file,
        if unentered { 3 } else { 2 },
        &added,
        Some(&format!("{indent}return n")),
        &owners,
        &extracted,
        Some(&source),
    )
    .ok_or_else(|| "missing Python property subject".to_string())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn python_semantic_inputs_fail_closed(n in -20i32..20, delta in 1i32..10, relation in 0u8..3, oracle in 0u8..4, guarded in any::<bool>()) {
        let result: Result<(), String> = (|| {
            let f = finding(n, delta, relation, oracle, guarded)?;
            preview_finding(&f)?;
            exposure_matches(&f.class, relation < 2 && oracle == 2 && !guarded)
        })();
        prop_assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn python_modeled_oracle_upgrade_never_lowers(n in -20i32..20, delta in 1i32..10, relation in 0u8..2) {
        let result: Result<(), String> = (|| {
            let weak = finding(n, delta, relation, 1, false)?;
            let strong = finding(n, delta, relation, 2, false)?;
            exposure_matches(&strong.class, true)?;
            oracle_upgrade(&weak.class, &strong.class)
        })();
        prop_assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn python_unentered_return_never_promotes(n in -20i32..20, delta in 1i32..10, relation in 0u8..2) {
        let result: Result<(), String> = (|| {
            let f = finding_with_arm(n, delta, relation, 2, false, true)?;
            preview_finding(&f)?;
            exposure_matches(&f.class, false)
        })();
        prop_assert!(result.is_ok(), "{:?}", result);
    }
}

#[test]
fn python_unknown_registration_never_exposed() -> Result<(), String> {
    let f = finding(3, 2, 0, 2, true)?;
    if f.class != crate::domain::ExposureClass::StaticUnknown {
        return Err(format!("runtime guard must stay unknown: {:?}", f.class));
    }
    preview_finding(&f)?;
    exposure_matches(&f.class, false)
}

#[test]
fn python_semantic_contract_has_nonvacuous_controls() -> Result<(), String> {
    for relation in 0..3 {
        for oracle in 0..4 {
            for guarded in [false, true] {
                let f = finding(3, 2, relation, oracle, guarded)?;
                preview_finding(&f)?;
                exposure_matches(&f.class, relation < 2 && oracle == 2 && !guarded)?;
            }
        }
    }
    Ok(())
}
