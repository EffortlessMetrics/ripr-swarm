//! Authored qualification inputs for #12598 / #10746, not analyzer support.
//!
//! These ordinary tests call the exact fixture implementations. The proof
//! bodies are compiled out without cfg(kani); no native verifier runs here.

use std::cmp::Ordering;

#[path = "fixtures/kani_guard_transitions/false_domain.rs"]
mod false_domain;
#[path = "fixtures/kani_guard_transitions/narrow_correct.rs"]
mod narrow_correct;
#[path = "fixtures/kani_guard_transitions/narrow_wrong.rs"]
mod narrow_wrong;
#[path = "fixtures/kani_guard_transitions/strong.rs"]
mod strong;
#[path = "fixtures/kani_guard_transitions/weak.rs"]
mod weak;
#[path = "fixtures/kani_guard_transitions/weak_wrong.rs"]
mod weak_wrong;
#[path = "fixtures/kani_guard_transitions/wrong.rs"]
mod wrong;

const STRONG: &str = include_str!("fixtures/kani_guard_transitions/strong.rs");
const WRONG: &str = include_str!("fixtures/kani_guard_transitions/wrong.rs");
const WEAK: &str = include_str!("fixtures/kani_guard_transitions/weak.rs");
const WEAK_WRONG: &str = include_str!("fixtures/kani_guard_transitions/weak_wrong.rs");
const NARROW_WRONG: &str = include_str!("fixtures/kani_guard_transitions/narrow_wrong.rs");
const NARROW_CORRECT: &str = include_str!("fixtures/kani_guard_transitions/narrow_correct.rs");
const FALSE_DOMAIN: &str = include_str!("fixtures/kani_guard_transitions/false_domain.rs");

const ASSERTION: &str = "        assert_eq!(super::fits(n, max), n < max);";
const WEAK_ASSERTION: &str = "        let _ = super::fits(n, max);\n        assert!(true);";

type Subject = fn(u8, u8) -> bool;

fn source_parts(source: &str) -> Result<(&str, &str), String> {
    source
        .split_once("#[cfg(kani)]")
        .ok_or_else(|| "fixture must separate implementation and Kani proof".to_string())
}

#[test]
fn fixed_guard_inputs_change_only_the_implementation() -> Result<(), String> {
    let expected = r#"pub fn fits(n: u8, max: u8) -> bool {
    n < max
}

#[cfg(kani)]
mod proofs {
    #[kani::proof]
    fn pins_boundary() {
        let n: u8 = kani::any();
        let max: u8 = kani::any();
        assert_eq!(super::fits(n, max), n < max);
    }
}
"#;
    assert_eq!(STRONG, expected);
    assert_eq!(WRONG, STRONG.replacen("    n < max\n", "    n <= max\n", 1));
    assert_eq!(source_parts(STRONG)?.1, source_parts(WRONG)?.1);
    assert_eq!(source_parts(WEAK)?.1, source_parts(WEAK_WRONG)?.1);
    assert_eq!(
        source_parts(NARROW_CORRECT)?.1,
        source_parts(NARROW_WRONG)?.1
    );
    Ok(())
}

#[test]
fn guard_only_inputs_preserve_exact_implementation_bytes() -> Result<(), String> {
    let baseline = source_parts(STRONG)?.0;
    for source in [WEAK, NARROW_CORRECT, FALSE_DOMAIN] {
        assert_eq!(source_parts(source)?.0, baseline);
    }
    let mutant = source_parts(WRONG)?.0;
    for source in [WEAK_WRONG, NARROW_WRONG] {
        assert_eq!(source_parts(source)?.0, mutant);
    }
    assert_eq!(WEAK, STRONG.replace(ASSERTION, WEAK_ASSERTION));
    assert_eq!(WEAK_WRONG, WRONG.replace(ASSERTION, WEAK_ASSERTION));
    let narrowed = format!("        kani::assume(n < max);\n{ASSERTION}");
    assert_eq!(NARROW_CORRECT, STRONG.replace(ASSERTION, &narrowed));
    assert_eq!(NARROW_WRONG, WRONG.replace(ASSERTION, &narrowed));
    let empty = format!("        kani::assume(false);\n{ASSERTION}");
    assert_eq!(FALSE_DOMAIN, STRONG.replace(ASSERTION, &empty));
    Ok(())
}

#[test]
fn fixture_implementations_differ_on_exactly_the_equality_boundary() {
    let correct: [(&str, Subject); 4] = [
        ("strong", strong::fits),
        ("weak", weak::fits),
        ("narrow_correct", narrow_correct::fits),
        ("false_domain", false_domain::fits),
    ];
    let mutants: [(&str, Subject); 3] = [
        ("wrong", wrong::fits),
        ("weak_wrong", weak_wrong::fits),
        ("narrow_wrong", narrow_wrong::fits),
    ];
    let mut visited = 0_usize;
    let mut disagreements = [0_usize; 3];
    for n in u8::MIN..=u8::MAX {
        for max in u8::MIN..=u8::MAX {
            visited += 1;
            // The fixture requirement is strict capacity, not a value learned
            // from either implementation or from an analyzer's output.
            let required = n.cmp(&max) == Ordering::Less;
            for (name, subject) in correct {
                assert_eq!(subject(n, max), required, "{name}: ({n}, {max})");
            }
            for (index, (name, subject)) in mutants.iter().enumerate() {
                let differs = subject(n, max) != required;
                assert_eq!(differs, n == max, "{name}: ({n}, {max})");
                if differs {
                    disagreements[index] += 1;
                }
            }
        }
    }
    assert_eq!(visited, 65_536);
    assert_eq!(disagreements, [256, 256, 256]);
    assert!(!strong::fits(0, 0));
    assert!(wrong::fits(0, 0));
    assert!(!strong::fits(u8::MAX, u8::MAX));
    assert!(wrong::fits(u8::MAX, u8::MAX));
}

#[test]
fn partial_domain_is_nonempty_but_excludes_all_equality_faults() {
    let mut admitted = 0_usize;
    let mut excluded_faults = 0_usize;
    for n in u8::MIN..=u8::MAX {
        for max in u8::MIN..=u8::MAX {
            let required = n.cmp(&max) == Ordering::Less;
            let wrong_value = narrow_wrong::fits(n, max);
            if n < max {
                admitted += 1;
                assert_eq!(wrong_value, required);
                assert_eq!(narrow_correct::fits(n, max), required);
            } else if wrong_value != required {
                excluded_faults += 1;
            }
        }
    }
    assert_eq!(admitted, 32_640);
    assert_eq!(excluded_faults, 256);
}

#[test]
fn fixture_guard_restoration_returns_exact_original_source() {
    assert_ne!(WEAK, STRONG);
    assert_eq!(WEAK.replace(WEAK_ASSERTION, ASSERTION), STRONG);
    // Restoring a guard over still-wrong code is a different terminal state.
    assert_eq!(WEAK_WRONG.replace(WEAK_ASSERTION, ASSERTION), WRONG);
    assert_ne!(WRONG, STRONG);
}
