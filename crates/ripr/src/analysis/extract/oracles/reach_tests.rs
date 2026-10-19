//! RIPR-SPEC-0231 rule 8 (#7638): statements that never run are not oracles.

use super::extract_assertions;
use super::reach::{Unreached, unreached_lines};

/// One-based lines of `body` that the reach reading marks unreached.
fn unreached(body: &str) -> Vec<(usize, Unreached)> {
    unreached_lines(body)
        .into_iter()
        .map(|(offset, kind)| (offset + 1, kind))
        .collect()
}

/// The texts of the oracles the line scanner extracts from `body`.
fn oracle_texts(body: &str) -> Vec<String> {
    extract_assertions(body, 1)
        .into_iter()
        .map(|oracle| oracle.text)
        .collect()
}

fn credits(body: &str, needle: &str) -> bool {
    oracle_texts(body).iter().any(|text| text.contains(needle))
}

fn assert_not_credited(bodies: &[&str], needle: &str, why: &str) {
    for body in bodies {
        let texts = oracle_texts(body);
        assert!(
            texts.iter().all(|text| !text.contains(needle)),
            "{why}: `{needle}` must not be credited in {body:?}: {texts:?}"
        );
    }
}

fn assert_credited(bodies: &[&str], needle: &str, why: &str) {
    for body in bodies {
        assert!(
            credits(body, needle),
            "{why}: `{needle}` must stay credited in {body:?}: {:?}",
            oracle_texts(body)
        );
    }
}

/// Never-executed result-side and helper readings, which no later
/// admission refuses, inside each block shape.
const NEVER_RUN_READINGS: &[&str] = &[
    "check(300).unwrap_err();",
    "check(300).unwrap();",
    "check(300).expect(\"ok\");",
    "assert!(check(300).is_err());",
    "assert_check_eq(check(300), Err(E::Big));",
];

#[test]
fn spec_0231_rule_8_literal_false_blocks_are_unreached() {
    for reading in NEVER_RUN_READINGS {
        let bodies = [
            format!("fn t() {{\n    if false {{ {reading} }}\n}}"),
            format!("fn t() {{\n    if false {{\n        {reading}\n    }}\n}}"),
            format!("fn t() {{\n    if !true {{ {reading} }}\n}}"),
            format!("fn t() {{\n    if ! true {{ {reading} }}\n}}"),
            format!("fn t() {{\n    while false {{ {reading} }}\n}}"),
            format!(
                "fn t() {{\n    let x = 1;\n    if x > 0 {{\n    }} else if false {{\n        {reading}\n    }}\n}}"
            ),
        ];
        let bodies: Vec<&str> = bodies.iter().map(String::as_str).collect();
        assert_not_credited(&bodies, "check(300)", "literal-false block");
    }
}

#[test]
fn spec_0231_rule_8_live_branches_keep_their_oracles() {
    // Positive controls: a live condition, the `else` block of a dead
    // `if`, a condition that merely starts with `false`, a reading after a
    // dead block, and a commented-out spelling.
    for reading in NEVER_RUN_READINGS {
        let bodies = [
            format!("fn t() {{\n    if ready() {{\n        {reading}\n    }}\n}}"),
            format!("fn t() {{\n    if true {{\n        {reading}\n    }}\n}}"),
            format!(
                "fn t() {{\n    if false {{\n        skip();\n    }} else {{\n        {reading}\n    }}\n}}"
            ),
            format!("fn t() {{\n    if false || ready() {{\n        {reading}\n    }}\n}}"),
            format!("fn t() {{\n    if falsey {{\n        {reading}\n    }}\n}}"),
            format!("fn t() {{\n    if false {{ skip(); }}\n    {reading}\n}}"),
            format!("fn t() {{\n    // if false {{ comment }}\n    {reading}\n}}"),
        ];
        let bodies: Vec<&str> = bodies.iter().map(String::as_str).collect();
        assert_credited(&bodies, "check(300)", "reached block");
    }
    // A reached statement sharing the dead block's line keeps that line.
    assert_credited(
        &["fn t() {\n    if false { skip(); } else { check(300).unwrap(); }\n}"],
        "check(300)",
        "shared line",
    );
}

#[test]
fn spec_0231_rule_8_uncalled_closures_are_unreached() {
    assert_not_credited(
        &[
            "fn t() {\n    let _never = || check(300).unwrap();\n}",
            "fn t() {\n    let never = |x| check(x).unwrap();\n    let other = 1;\n    use_it(other);\n}",
            "fn t() {\n    let mut never = move |x: u32| {\n        assert!(check(x).is_err());\n    };\n    run();\n}",
        ],
        "check(",
        "uncalled closure",
    );
}

#[test]
fn spec_0231_rule_8_referenced_closures_keep_their_oracles() {
    // Positive controls: called, passed, or otherwise named later.
    assert_credited(
        &[
            "fn t() {\n    let probe = || check(300).unwrap();\n    probe();\n}",
            "fn t() {\n    let probe = |x| check(x).unwrap();\n    [300].into_iter().for_each(probe);\n}",
            "fn t() {\n    let probe = || check(300).unwrap();\n    std::thread::spawn(probe).join().ok();\n}",
            "fn t() {\n    let probe = || check(300).unwrap();\n    run!(probe);\n}",
            "fn t() {\n    std::thread::spawn(|| check(300).unwrap()).join().ok();\n}",
        ],
        "check(",
        "referenced closure",
    );
}

#[test]
fn spec_0231_rule_8_code_after_divergence_is_unreached() {
    for head in [
        "return;",
        "return Ok(());",
        "panic!(\"stop\");",
        "std::panic!(\"stop\");",
        "unreachable!();",
        "todo!();",
        "std::process::exit(0);",
        "process::exit(0);",
    ] {
        let bodies = [
            format!("fn t() {{\n    {head}\n    check(300).unwrap_err();\n}}"),
            format!("fn t() {{\n    {head}\n    let x = check(300).unwrap();\n    use_it(x);\n}}"),
            format!(
                "fn t() {{\n    if ready() {{\n        {head}\n        assert!(check(300).is_err());\n    }}\n}}"
            ),
        ];
        let bodies: Vec<&str> = bodies.iter().map(String::as_str).collect();
        assert_not_credited(&bodies, "check(300)", head);
    }
    // RIPR-SPEC-0197 execution admission sees no exit in a diverging macro
    // or `process::exit(..)`, so even a strong `assert_eq!` is dropped there.
    for head in [
        "panic!(\"stop\");",
        "unreachable!();",
        "std::process::exit(0);",
    ] {
        let body = format!("fn t() {{\n    {head}\n    assert_eq!(score(2), 4);\n}}");
        assert_not_credited(&[body.as_str()], "score(2)", head);
    }
}

#[test]
fn spec_0231_rule_8_leaves_admission_governed_assertions_to_their_refusal() {
    // A strong `assert_eq!` (or strong `assert!`) in a literal-false block, an
    // uncalled closure or after a `return` stays: RIPR-SPEC-0197 refuses
    // its owner pin and names why. A weak one is dropped like any reading.
    assert_credited(
        &[
            "fn t() {\n    if false { assert_eq!(score(2), 4); }\n}",
            "fn t() {\n    if false {\n        assert_eq!(score(2), 4);\n    }\n}",
            "fn t() {\n    let _never = || assert_eq!(score(2), 4);\n}",
            "fn t() {\n    return;\n    assert_eq!(score(2), 4);\n}",
            "fn t() {\n    return;\n    std::assert_eq!(score(2), 4);\n}",
        ],
        "score(2)",
        "admission-governed assertion",
    );
    assert_not_credited(
        &[
            "fn t() {\n    if false { assert!(score(2) > 1); }\n}",
            "fn t() {\n    if false {\n        assert!(score(2) == 4);\n    }\n}",
            "fn t() {\n    let _never = || assert_ne!(score(2), 5);\n}",
            "fn t() {\n    return;\n    assert_score_eq(score(2), 4);\n}",
        ],
        "score(2)",
        "non-governed assertion",
    );
}

#[test]
fn spec_0231_rule_8_pre_divergence_and_conditional_divergence_keep_oracles() {
    // Positive controls: statements before the divergence, after a
    // conditional or nested divergence, in a match arm, after the block
    // that diverges, a nested fn after a `panic!` (callable earlier), an
    // attribute-gated divergence, an unknown macro, and a reached statement
    // sharing the diverging line.
    assert_credited(
        &[
            "fn t() {\n    check(300).unwrap_err();\n    return;\n}",
            "fn t() {\n    check(300).unwrap_err();\n    panic!(\"stop\");\n}",
            "fn t() {\n    if done() { panic!(\"stop\"); }\n    check(300).unwrap_err();\n}",
            "fn t() {\n    if done() {\n        return;\n    }\n    check(300).unwrap_err();\n}",
            "fn t() {\n    let v = match f() { Some(v) => v, None => return };\n    check(v).unwrap_err();\n}",
            "fn t() {\n    let v = f().unwrap_or_else(|| panic!(\"none\"));\n    check(v).unwrap_err();\n}",
            "fn t() {\n    {\n        panic!(\"stop\");\n    }\n}\nfn u() {\n    check(300).unwrap_err();\n}",
            "fn t() {\n    helper();\n    panic!(\"stop\");\n    fn helper() {\n        check(300).unwrap_err();\n    }\n}",
            "fn t() {\n    let returned = 1;\n    check(returned).unwrap_err();\n}",
            "fn t() {\n    #[cfg(never)]\n    panic!(\"stop\");\n    check(300).unwrap_err();\n}",
            "fn t() {\n    my_panic!(\"x\");\n    check(300).unwrap_err();\n}",
            "fn t() {\n    panic!(\"stop\"); check(300).unwrap_err();\n}",
        ],
        "check(",
        "reached statement",
    );
}

#[test]
fn spec_0231_rule_8_fails_closed_on_unbalanced_and_macro_input() {
    // Unbalanced text (a fragment) keeps every line reached.
    assert!(unreached("fn t() {\n    if false { x.unwrap();\n").is_empty());
    // Inside a macro invocation's input the expansion is unknown.
    assert!(
        unreached("fn t() {\n    check! {\n        if false { x.unwrap(); }\n    }\n}").is_empty()
    );
    assert!(
        unreached("fn t() {\n    quote!(return; x.unwrap(););\n}").is_empty(),
        "macro input never forms a block"
    );
    // A string or comment spelling of the shapes is masked.
    assert!(
        unreached("fn t() {\n    let s = \"if false { x }\";\n    // return;\n    f(s);\n}")
            .is_empty()
    );
}

#[test]
fn spec_0231_rule_8_marks_only_lines_whose_code_is_all_unreached() {
    use Unreached::{AdmissionRefused, Unrefused};
    let body = "fn t() {\n    if false {\n        a();\n    } else {\n        b();\n    }\n    return;\n    c();\n}";
    // Line 2 `if false {` and 3 `a();` are dead; line 4 `} else {` shares
    // a live `else`; line 8 `c();` follows the `return;` on line 7.
    assert_eq!(
        unreached(body),
        vec![
            (2, AdmissionRefused),
            (3, AdmissionRefused),
            (8, AdmissionRefused)
        ]
    );
    let closure =
        "fn t() {\n    let f = || {\n        a();\n    };\n    let g = || b();\n    g();\n}";
    assert_eq!(
        unreached(closure),
        vec![
            (2, AdmissionRefused),
            (3, AdmissionRefused),
            (4, AdmissionRefused)
        ]
    );
    let diverging = "fn t() {\n    panic!(\"x\");\n    a();\n    if false { b(); }\n}";
    // Line 4 lies in both the unrefused tail after `panic!` and a refused
    // `if false` block, whose refusal still applies to an assertion there.
    assert_eq!(
        unreached(diverging),
        vec![(3, Unrefused), (4, AdmissionRefused)]
    );
}
