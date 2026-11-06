//! Source/oracle/relation inputs through the retained classifier (#12927).
use super::*;
use crate::analysis::classification_properties::{
    exposure_matches, oracle_upgrade, preview_finding,
};
use proptest::prelude::*;

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
    let indent = if unentered { "    " } else { "  " };
    let added = format!("{indent}return n + {delta};");
    let source = if unentered {
        format!(
            "export function shifted(n: number): number {{\n  if (n > {}) {{\n{added}\n  }}\n  return n;\n}}\n",
            n + 1
        )
    } else {
        format!("export function shifted(n: number): number {{\n{added}\n}}\n")
    };
    let (import, call) = match relation {
        0 => ("import { shifted } from '../src/subject';", "shifted"),
        1 => (
            "import { shifted as renamed } from '../src/subject';",
            "renamed",
        ),
        _ => ("import { shifted } from '../src/unrelated';", "shifted"),
    };
    let body = match oracle {
        0 => format!("{call}({n});"),
        1 => format!("expect({call}({n})).toBeDefined();"),
        2 => format!(
            "expect({call}({n})).toBe({});",
            if unentered { n } else { n + delta }
        ),
        _ => format!("expect({call}({n})).customMatcher();"),
    };
    let test = format!("test('value', () => {{ {body} }});");
    let test = if guarded {
        format!("if (ENABLED) {{ {test} }}")
    } else {
        test
    };
    let tests = format!("{import}\n{test}\n");
    let file = Path::new("src/subject.ts");
    let owners = extract_owners(file, &source);
    let extracted = extract_tests(Path::new("tests/subject.test.ts"), &tests);
    if owners.len() != 1 || extracted.len() != 1 {
        return Err(format!("fixture extraction failed: {source}\n{tests}"));
    }
    classify_change_with_alias_state(
        file,
        if unentered { 3 } else { 2 },
        &added,
        Some(&format!("{indent}return n;")),
        &owners,
        &extracted,
        None,
        &ReExportIndex::empty(),
        None,
        None,
    )
    .ok_or_else(|| "missing TypeScript property subject".to_string())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn typescript_semantic_inputs_fail_closed(n in -20i32..20, delta in 1i32..10, relation in 0u8..3, oracle in 0u8..4, guarded in any::<bool>()) {
        let result: Result<(), String> = (|| {
            let f = finding(n, delta, relation, oracle, guarded)?;
            preview_finding(&f)?;
            exposure_matches(&f.class, relation < 2 && oracle == 2 && !guarded)
        })();
        prop_assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn typescript_modeled_oracle_upgrade_never_lowers(n in -20i32..20, delta in 1i32..10, relation in 0u8..2) {
        let result: Result<(), String> = (|| {
            let weak = finding(n, delta, relation, 1, false)?;
            let strong = finding(n, delta, relation, 2, false)?;
            exposure_matches(&strong.class, true)?;
            oracle_upgrade(&weak.class, &strong.class)
        })();
        prop_assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn typescript_unentered_return_never_promotes(n in -20i32..20, delta in 1i32..10, relation in 0u8..2) {
        let result: Result<(), String> = (|| {
            let f = finding_with_arm(n, delta, relation, 2, false, true)?;
            preview_finding(&f)?;
            exposure_matches(&f.class, false)
        })();
        prop_assert!(result.is_ok(), "{:?}", result);
    }
}

#[test]
fn typescript_unknown_registration_never_exposed() -> Result<(), String> {
    let f = finding(3, 2, 0, 2, true)?;
    if f.class != ExposureClass::StaticUnknown {
        return Err(format!("runtime guard must stay unknown: {:?}", f.class));
    }
    preview_finding(&f)?;
    exposure_matches(&f.class, false)
}

#[test]
fn typescript_semantic_contract_has_nonvacuous_controls() -> Result<(), String> {
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
