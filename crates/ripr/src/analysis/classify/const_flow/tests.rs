use super::expr::{
    Machine, Ty, Value, evaluate_initializer, parse_assertion, parse_const_decl, parse_expression,
    parse_reader,
};
use super::{ChangedConsts, mentions_call};
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::facts::{FunctionSourceRole, FunctionSummary};
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::path::{Path, PathBuf};

const U32: Ty = Ty::Int {
    min: 0,
    max: u32::MAX as i64,
};

/// Whether the assertion holds when `NAME` is `value` for the reader source.
fn holds(reader: &str, constant: (&str, Ty, Value), assertion: &str) -> Option<bool> {
    let readers = [parse_reader(reader)?];
    let machine = Machine {
        readers: &readers,
        constant: constant.0,
        constant_ty: constant.1,
        constant_value: &constant.2,
    };
    machine.holds(&parse_assertion(assertion)?)
}

const SHOULD_RETRY: &str = "fn should_retry(attempt: u32) -> bool {\n    attempt < LIMIT\n}";
const RENDER: &str = "pub fn render() -> String {\n    format!(\"{LABEL}\\n\")\n}";

#[test]
fn a_pinned_boundary_notices_the_changed_value_and_an_unpinned_one_does_not() {
    let ten = ("LIMIT", U32, Value::Int(10));
    let eleven = ("LIMIT", U32, Value::Int(11));
    for (assertion, notices) in [
        ("assert!(should_retry(10));", false),
        ("assert!(!should_retry(10));", true),
    ] {
        // `should_retry(10)` is false at 10 and true at 11.
        let before = holds(SHOULD_RETRY, ten.clone(), assertion);
        let after = holds(SHOULD_RETRY, eleven.clone(), assertion);
        if notices {
            assert_eq!((before, after), (Some(true), Some(false)), "{assertion}");
        } else {
            assert_eq!((before, after), (Some(false), Some(true)), "{assertion}");
        }
    }
    // An input far from the boundary reads the same under both values.
    let (three, four) = (("LIMIT", U32, Value::Int(3)), ("LIMIT", U32, Value::Int(4)));
    assert_eq!(
        holds(SHOULD_RETRY, three, "assert!(should_retry(0));"),
        Some(true)
    );
    assert_eq!(
        holds(SHOULD_RETRY, four, "assert!(should_retry(0));"),
        Some(true)
    );
}

#[test]
fn a_rendered_label_is_compared_to_its_literal_text() {
    let ty = Ty::Str;
    let gold = ("LABEL", ty, Value::Str("tier: gold".into()));
    let premium = ("LABEL", ty, Value::Str("tier: premium".into()));
    let exact = "assert_eq!(render(), \"tier: gold\\n\");";
    assert_eq!(holds(RENDER, gold.clone(), exact), Some(true));
    assert_eq!(holds(RENDER, premium.clone(), exact), Some(false));
    let weak = "assert!(!render().is_empty());";
    assert_eq!(holds(RENDER, gold, weak), Some(true));
    assert_eq!(holds(RENDER, premium, weak), Some(true));
}

#[test]
fn a_reader_outside_the_grammar_is_unreadable_not_approximated() {
    for reader in [
        // control flow, bindings, calls, methods, lookups, generics, `self`
        "fn f(n: u32) -> bool { if n > 1 { n < LIMIT } else { false } }",
        "fn f(n: u32) -> bool { let x = n; x < LIMIT }",
        "fn f(n: u32) -> u32 { helper(n) + LIMIT }",
        "fn f(n: u32) -> u32 { n.pow(LIMIT) }",
        "fn f(n: &str) -> u32 { table.get(n).copied().unwrap_or(LIMIT) }",
        "fn f<T>(n: u32) -> bool { n < LIMIT }",
        "fn f(&self, n: u32) -> bool { n < LIMIT }",
        "const fn f(n: u32) -> bool { n < LIMIT }",
        "fn f(n: u32) -> bool { return n < LIMIT; }",
        "fn f(n: u32) -> bool { n < Self::LIMIT }",
        "fn f(n: u32) -> bool { n < other::LIMIT }",
        // a conditional item
        "#[cfg(windows)]\nfn f(n: u32) -> bool { n < LIMIT }",
        // a suffixed literal, a float, a char
        "fn f(n: u32) -> bool { n < 3u32 }",
        "fn f(n: u32) -> bool { n < 3.0 }",
        "fn f(c: u32) -> bool { c < 'a' }",
    ] {
        assert!(parse_reader(reader).is_none(), "{reader}");
    }
}

#[test]
fn an_assertion_outside_the_grammar_is_unreadable() {
    for assertion in [
        // a message, a non-literal expected value, a binding, a debug twin
        "assert!(should_retry(0), \"must retry\");",
        "assert_eq!(render(), LABEL);",
        "assert_eq!(render(), format!(\"{LABEL}\\n\"));",
        "assert!(should_retry(attempt));",
        "debug_assert!(should_retry(0));",
        "assert!(matches!(should_retry(0), true));",
        "assert_eq!(a, b, c);",
    ] {
        let machine_value = Value::Int(3);
        let outcome = parse_assertion(assertion).and_then(|parsed| {
            let readers = [parse_reader(SHOULD_RETRY)?];
            Machine {
                readers: &readers,
                constant: "LIMIT",
                constant_ty: U32,
                constant_value: &machine_value,
            }
            .holds(&parsed)
        });
        assert_eq!(outcome, None, "{assertion}");
    }
}

#[test]
fn arithmetic_that_leaves_the_declared_type_is_unreadable() {
    let reader = "fn f(n: u8) -> u8 { n + LIMIT }";
    let ty = Ty::Int { min: 0, max: 255 };
    assert_eq!(
        holds(
            reader,
            ("LIMIT", ty, Value::Int(10)),
            "assert_eq!(f(1), 11);"
        ),
        Some(true)
    );
    // 250 + 10 overflows u8: Rust panics, the evaluator reads nothing.
    assert_eq!(
        holds(
            reader,
            ("LIMIT", ty, Value::Int(10)),
            "assert_eq!(f(250), 4);"
        ),
        None
    );
    // 1 - 3 underflows an unsigned type.
    let sub = "fn g(n: u32) -> u32 { n - LIMIT }";
    assert_eq!(
        holds(sub, ("LIMIT", U32, Value::Int(3)), "assert_eq!(g(1), 0);"),
        None
    );
    // An argument outside its parameter's type is not a call the compiler accepts.
    assert_eq!(
        holds(
            reader,
            ("LIMIT", ty, Value::Int(10)),
            "assert_eq!(f(300), 0);"
        ),
        None
    );
    // Division by zero.
    let div = "fn h(n: u32) -> u32 { n / LIMIT }";
    assert_eq!(
        holds(div, ("LIMIT", U32, Value::Int(0)), "assert_eq!(h(4), 0);"),
        None
    );
}

#[test]
fn a_parameter_spelled_like_the_constant_is_a_shadow_not_a_read() {
    let shadowed = "fn f(LIMIT: u32) -> bool { LIMIT < 4 }";
    assert_eq!(
        holds(shadowed, ("LIMIT", U32, Value::Int(9)), "assert!(f(1));"),
        None
    );
}

#[test]
fn a_called_name_that_is_not_a_reader_is_unreadable() {
    assert_eq!(
        holds(
            SHOULD_RETRY,
            ("LIMIT", U32, Value::Int(3)),
            "assert!(other(0));"
        ),
        None
    );
}

#[test]
fn constant_declarations_parse_with_their_types() {
    for (line, ty) in [
        ("pub const LIMIT: u32 = 3;", U32),
        ("const LIMIT: u32 = 5 + 5;", U32),
        ("pub(crate) static LIMIT: u32 = 3;", U32),
        ("pub const LABEL: &str = \"tier: gold\";", Ty::Str),
        ("pub const LABEL: &'static str = \"tier: gold\";", Ty::Str),
        ("pub const ON: bool = true;", Ty::Bool),
    ] {
        let decl = parse_const_decl(line).map(|decl| decl.ty);
        assert_eq!(decl, Some(ty), "{line}");
    }
    for line in [
        "pub const LIMIT: f64 = 3.0;",
        "pub const LIMIT: [u32; 2] = [1, 2];",
        "pub static mut LIMIT: u32 = 3;",
        "pub const LIMIT: u32 = 3",
        "let LIMIT: u32 = 3;",
        "pub const LIMIT: u32 = compute();",
    ] {
        let value =
            parse_const_decl(line).and_then(|decl| evaluate_initializer(&decl.init, decl.ty));
        assert_eq!(value, None, "{line}");
    }
}

#[test]
fn an_initializer_outside_its_declared_type_is_unreadable() {
    let value = |line: &str| {
        parse_const_decl(line).and_then(|decl| evaluate_initializer(&decl.init, decl.ty))
    };
    assert_eq!(value("pub const A: u8 = 256;"), None);
    assert_eq!(value("pub const A: u8 = 255;"), Some(Value::Int(255)));
    assert_eq!(value("pub const A: u32 = 5 + 5;"), Some(Value::Int(10)));
    assert_eq!(value("pub const A: &str = 3;"), None);
}

#[test]
fn comparisons_do_not_chain() {
    assert!(parse_expression("a < b < c").is_none());
    assert!(parse_expression("a < b && b < c").is_some());
}

#[test]
fn a_call_is_matched_by_whole_name_followed_by_a_paren() {
    assert!(mentions_call("assert!(should_retry(0));", "should_retry"));
    assert!(!mentions_call(
        "assert!(should_retry_later(0));",
        "should_retry"
    ));
    assert!(!mentions_call("let f = should_retry;", "should_retry"));
}

fn line(text: &str, line: usize) -> ChangedLine {
    ChangedLine {
        line,
        text: text.to_string(),
        new_side_line: line,
    }
}

fn file(removed: &[(&str, usize)], added: &[(&str, usize)]) -> ChangedFile {
    ChangedFile {
        path: PathBuf::from("./src/lib.rs"),
        added_lines: added.iter().map(|(t, n)| line(t, *n)).collect(),
        removed_lines: removed.iter().map(|(t, n)| line(t, *n)).collect(),
    }
}

#[test]
fn an_edited_constant_pairs_its_old_and_new_declaration_by_name() {
    let changed = ChangedConsts::from_changed_files([&file(
        &[("pub const LIMIT: u32 = 3;", 3)],
        &[("pub const LIMIT: u32 = 4;", 3)],
    )]);
    assert!(
        changed
            .lookup(std::path::Path::new("src/lib.rs"), 3)
            .is_some()
    );
    assert!(
        changed
            .lookup(std::path::Path::new("src/lib.rs"), 4)
            .is_none()
    );
    assert!(
        changed
            .lookup(std::path::Path::new("src/other.rs"), 3)
            .is_none()
    );
}

#[test]
fn an_ambiguous_or_unpaired_constant_pairs_with_nothing() {
    let none = |removed: &[(&str, usize)], added: &[(&str, usize)]| {
        ChangedConsts::from_changed_files([&file(removed, added)])
            .lookup(std::path::Path::new("src/lib.rs"), 3)
            .is_none()
    };
    // added only, removed only, a different name, and a name edited twice
    assert!(none(&[], &[("pub const LIMIT: u32 = 4;", 3)]));
    assert!(none(&[("pub const LIMIT: u32 = 3;", 3)], &[]));
    assert!(none(
        &[("pub const OLD: u32 = 3;", 3)],
        &[("pub const LIMIT: u32 = 4;", 3)]
    ));
    assert!(none(
        &[
            ("pub const LIMIT: u32 = 3;", 3),
            ("pub const LIMIT: u32 = 3;", 9)
        ],
        &[("pub const LIMIT: u32 = 4;", 3)]
    ));
    assert!(none(
        &[("pub const LIMIT: u32 = 3;", 3)],
        &[
            ("pub const LIMIT: u32 = 4;", 3),
            ("pub const LIMIT: u32 = 4;", 9)
        ]
    ));
}

#[test]
fn a_value_keeping_rewrite_is_asked_about_its_neighbours() {
    use super::neighbours;
    assert_eq!(
        neighbours(&Value::Int(10), U32),
        Some(vec![Value::Int(9), Value::Int(11)])
    );
    // The type's edge removes the neighbour it does not admit.
    assert_eq!(neighbours(&Value::Int(0), U32), Some(vec![Value::Int(1)]));
    let byte = Ty::Int { min: 0, max: 255 };
    assert_eq!(
        neighbours(&Value::Int(255), byte),
        Some(vec![Value::Int(254)])
    );
    // Only integers have neighbours.
    assert_eq!(neighbours(&Value::Str("a".into()), Ty::Str), None);
    assert_eq!(neighbours(&Value::Bool(true), Ty::Bool), None);
}

#[test]
fn both_neighbours_must_break_a_pinned_boundary() {
    // `within(10)` and `!within(11)` pin `value <= LIMIT` from both sides.
    let reader = "pub fn within(value: u32) -> bool {\n    value <= LIMIT\n}";
    let pinned = ["assert!(within(10));", "assert!(!within(11));"];
    let noticed = |limit: i64| -> Vec<Option<bool>> {
        pinned
            .iter()
            .map(|assertion| holds(reader, ("LIMIT", U32, Value::Int(limit)), assertion))
            .collect()
    };
    assert_eq!(noticed(10), vec![Some(true), Some(true)]);
    // 9 breaks `within(10)`; 11 breaks `!within(11)`: each neighbour is noticed.
    assert_eq!(noticed(9)[0], Some(false));
    assert_eq!(noticed(11)[1], Some(false));
    // A single-sided pin leaves the other neighbour unnoticed.
    let one_sided = "assert!(within(10));";
    let high = holds(reader, ("LIMIT", U32, Value::Int(11)), one_sided);
    assert_eq!(
        high,
        Some(true),
        "11 passes a pin that only looks from below"
    );
}

#[test]
fn the_explained_result_is_the_readers_return_not_the_comparison() -> Result<(), String> {
    let readers = [parse_reader(SHOULD_RETRY).ok_or("the reader source parses")?];
    let value = Value::Int(10);
    let machine = Machine {
        readers: &readers,
        constant: "LIMIT",
        constant_ty: U32,
        constant_value: &value,
    };
    // `should_retry(10)` is false at LIMIT 10; the negated assertion is true.
    for (text, expected) in [
        ("assert!(!should_retry(10));", Value::Bool(false)),
        ("assert_eq!(should_retry(10), false);", Value::Bool(false)),
        ("assert!(should_retry(9));", Value::Bool(true)),
    ] {
        let parsed = parse_assertion(text);
        assert_eq!(
            parsed.and_then(|assertion| machine.call_result(&assertion)),
            Some(expected),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn only_a_literal_written_in_the_test_is_an_expectation() -> Result<(), String> {
    for (text, pinned) in [
        ("assert_eq!(should_retry(1), false);", true),
        ("assert!(should_retry(3) && !should_retry(12));", true),
        ("assert!(render(1).len() == 5);", true),
        // Both sides move with the edit, or the expectation names the constant.
        ("assert_eq!(should_retry(1), should_retry(2));", false),
        ("assert_eq!(should_retry(1), LIMIT > 3);", false),
        ("assert!(should_retry(1) == should_retry(2));", false),
        ("assert!(should_retry(1) && LIMIT > 3);", false),
    ] {
        let assertion = parse_assertion(text).ok_or(format!("parses: {text}"))?;
        assert_eq!(assertion.has_literal_expectation(), pinned, "{text}");
    }
    Ok(())
}

#[test]
fn a_reader_in_another_module_does_not_share_the_declarations_scope() {
    let source = "pub const LIMIT: u32 = 10;\n\
                  pub fn within(n: u32) -> bool { n < LIMIT }\n\
                  mod other {\n\
                  use crate::elsewhere::LIMIT;\n\
                  pub fn other_within(n: u32) -> bool { n < LIMIT }\n\
                  }\n";
    assert!(super::shares_module_scope(source, 1, &[2]));
    assert!(!super::shares_module_scope(source, 1, &[5]));
    assert!(!super::shares_module_scope(source, 1, &[2, 5]));
}

#[test]
fn a_declaration_inside_a_block_is_not_shared_with_a_free_function() {
    let source = "pub fn charge() -> u32 {\n\
                  const RATE: u32 = 3;\n\
                  RATE\n\
                  }\n\
                  pub fn rate() -> u32 { RATE }\n";
    assert!(!super::shares_module_scope(source, 2, &[5]));
}

#[test]
fn modules_with_the_same_chain_share_a_scope() {
    let source = "mod tuning {\n\
                  pub const FLOOR: u32 = 1;\n\
                  pub fn floored(n: u32) -> u32 { n.max(FLOOR) }\n\
                  }\n";
    assert!(super::shares_module_scope(source, 2, &[3]));
}

/// Run the whole decision on a one-file crate: its production functions that
/// name `LIMIT` are the readers and its `#[test]`s the related tests.
fn decide(tests: &str, old: &str, new: &str) -> Result<Option<super::ConstFlow>, String> {
    let source = format!("{old}\npub fn within(n: u32) -> bool {{ n <= LIMIT }}\n{tests}\n");
    let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
    let readers: Vec<&FunctionSummary> = facts
        .functions
        .iter()
        .filter(|f| f.source_role == FunctionSourceRole::Production && f.body.contains("LIMIT"))
        .collect();
    let related: Vec<_> = facts.tests.iter().collect();
    let change = super::ConstChange {
        old_declaration: old.to_string(),
        new_declaration: new.to_string(),
    };
    Ok(super::evaluate_const_flow(
        &change,
        &readers,
        &related,
        &|name| facts.functions.iter().filter(|f| f.name == name).count(),
        None,
    ))
}

const OLD: &str = "pub const LIMIT: u32 = 10;";

#[test]
fn a_rewrite_is_credited_only_when_every_neighbour_is_noticed() -> Result<(), String> {
    let rewrite = "pub const LIMIT: u32 = 5 + 5;";
    // 9 breaks `within(10)`; 11 breaks `!within(11)`; one test pins both.
    let both = "#[test]\nfn pins() {\n    assert!(within(10));\n    assert!(!within(11));\n}";
    let credited = decide(both, OLD, rewrite)?;
    let Some(super::ConstFlow::Discriminated(detail)) = credited else {
        return Err(format!(
            "both neighbours pinned should credit: {credited:?}"
        ));
    };
    assert!(detail.rewrite);
    assert_eq!(
        detail.neighbour_breaks.len(),
        2,
        "{:?}",
        detail.neighbour_breaks
    );
    assert!(detail.neighbour_breaks[0].starts_with("9 breaks"));
    assert!(detail.neighbour_breaks[1].starts_with("11 breaks"));

    // The same pins split over two tests still notice both neighbours.
    let split =
        "#[test]\nfn low() { assert!(within(10)); }\n#[test]\nfn high() { assert!(!within(11)); }";
    assert!(matches!(
        decide(split, OLD, rewrite)?,
        Some(super::ConstFlow::Discriminated(_))
    ));

    // Pinned from below only: 11 still satisfies `within(10)`, so the edit
    // being one too high goes unnoticed and the rewrite abstains.
    let below = "#[test]\nfn pins_one_side() { assert!(within(10)); }";
    assert_eq!(decide(below, OLD, rewrite)?, None);
    // Pinned from above only: 9 still satisfies `!within(11)`.
    let above = "#[test]\nfn pins_other_side() { assert!(!within(11)); }";
    assert_eq!(decide(above, OLD, rewrite)?, None);
    Ok(())
}

#[test]
fn a_changed_value_is_discriminated_or_indistinguishable_by_the_pins() -> Result<(), String> {
    let new = "pub const LIMIT: u32 = 11;";
    let pinned = "#[test]\nfn pins() { assert!(!within(11)); }";
    assert!(matches!(
        decide(pinned, OLD, new)?,
        Some(super::ConstFlow::Discriminated(_))
    ));
    let unpinned = "#[test]\nfn far_below() { assert!(within(3)); }";
    assert!(matches!(
        decide(unpinned, OLD, new)?,
        Some(super::ConstFlow::Indistinguishable(_))
    ));
    Ok(())
}

const TRAIT_SOURCE: &str = "pub trait Limits {\n    const MAX: u32;\n\n    fn clamp(v: u32) -> u32 {\n        if v > Self::MAX { Self::MAX } else { v }\n    }\n}\n\npub struct Small;\npub struct Wide;\n";

/// Run the associated-constant decision on a one-file crate: `imp` holds the
/// impl blocks (the edited `const MAX` declaration on the line it names) and
/// `tests` the `#[test]`s.
fn decide_assoc(
    imp: &str,
    tests: &str,
    old: &str,
    new: &str,
) -> Result<Option<super::ConstFlow>, String> {
    let source = format!("{TRAIT_SOURCE}\n{imp}\n{tests}\n");
    let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
    let line = source
        .lines()
        .position(|line| line.trim_start().starts_with("const MAX: u32 = "))
        .ok_or("no edited declaration")?
        + 1;
    let Some(binding) =
        super::associated_const_binding(&source, line, "MAX", facts.functions.iter())
    else {
        return Ok(None);
    };
    let related: Vec<_> = facts.tests.iter().collect();
    let change = super::ConstChange {
        old_declaration: old.to_string(),
        new_declaration: new.to_string(),
    };
    Ok(super::evaluate_const_flow(
        &change,
        &binding.readers,
        &related,
        &|name| facts.functions.iter().filter(|f| f.name == name).count(),
        Some(&binding.self_type),
    ))
}

const SMALL: &str = "impl Limits for Small {\n    const MAX: u32 = 10;\n}";
const SMALL_PINS: &str = "#[test]\nfn pins() {\n    assert_eq!(Small::clamp(50), 10);\n    assert_eq!(Small::clamp(5), 5);\n}";

#[test]
fn an_associated_constant_rewrite_is_credited_through_the_default_method() -> Result<(), String> {
    let rewritten = SMALL.replace("= 10;", "= 5 + 5;");
    let flow = decide_assoc(
        &rewritten,
        SMALL_PINS,
        "    const MAX: u32 = 10;",
        "    const MAX: u32 = 5 + 5;",
    )?;
    let Some(super::ConstFlow::Discriminated(detail)) = flow else {
        return Err(format!("both neighbours are pinned: {flow:?}"));
    };
    assert!(detail.rewrite);
    assert_eq!(detail.neighbour_breaks.len(), 2, "{detail:?}");
    Ok(())
}

#[test]
fn a_changed_associated_constant_is_discriminated_or_indistinguishable() -> Result<(), String> {
    let edited = SMALL.replace("= 10;", "= 20;");
    let old = "    const MAX: u32 = 10;";
    let new = "    const MAX: u32 = 20;";
    let flow = decide_assoc(&edited, SMALL_PINS, old, new)?;
    assert!(
        matches!(flow, Some(super::ConstFlow::Discriminated(_))),
        "clamp(50) is 10 before and 20 after: {flow:?}"
    );
    // Both inputs sit below either limit, so no assertion can tell them apart.
    let blind = "#[test]\nfn below() {\n    assert_eq!(Small::clamp(5), 5);\n}";
    let flow = decide_assoc(&edited, blind, old, new)?;
    assert!(
        matches!(flow, Some(super::ConstFlow::Indistinguishable(_))),
        "{flow:?}"
    );
    Ok(())
}

#[test]
fn a_call_through_another_type_or_path_is_not_read() -> Result<(), String> {
    let edited = SMALL.replace("= 10;", "= 20;");
    let imp = format!("{edited}\nimpl Limits for Wide {{\n    const MAX: u32 = 100;\n}}");
    let old = "    const MAX: u32 = 10;";
    let new = "    const MAX: u32 = 20;";
    for call in [
        "Wide::clamp(50)",
        "<Small as Limits>::clamp(50)",
        "other::Small::clamp(50)",
        "clamp(50)",
    ] {
        let tests = format!("#[test]\nfn reads() {{\n    assert_eq!({call}, 10);\n}}");
        assert_eq!(
            decide_assoc(&imp, &tests, old, new)?,
            None,
            "`{call}` is not a call through the edited impl's type"
        );
    }
    Ok(())
}

#[test]
fn an_associated_constant_that_breaks_a_binding_rule_is_not_evaluated() -> Result<(), String> {
    let old = "    const MAX: u32 = 10;";
    let new = "    const MAX: u32 = 20;";
    // The impl overrides the default method: two functions named `clamp`.
    let overridden = "impl Limits for Small {\n    const MAX: u32 = 20;\n\n    fn clamp(v: u32) -> u32 {\n        v.min(3)\n    }\n}";
    assert_eq!(decide_assoc(overridden, SMALL_PINS, old, new)?, None);
    // A generic impl is not bound.
    let generic = "impl<T> Limits for Small {\n    const MAX: u32 = 20;\n}";
    assert_eq!(decide_assoc(generic, SMALL_PINS, old, new)?, None);
    // A `cfg`-gated impl is not bound.
    let gated = "#[cfg(unix)]\nimpl Limits for Small {\n    const MAX: u32 = 20;\n}";
    assert_eq!(decide_assoc(gated, SMALL_PINS, old, new)?, None);
    // The expected value is the constant itself.
    let edited = SMALL.replace("= 10;", "= 20;");
    let own = "#[test]\nfn own() {\n    assert_eq!(Small::clamp(50), Small::MAX);\n}";
    assert_eq!(decide_assoc(&edited, own, old, new)?, None);
    Ok(())
}

#[test]
fn a_trait_with_a_default_constant_or_a_helper_condition_is_not_evaluated() -> Result<(), String> {
    let edited = SMALL.replace("= 10;", "= 20;");
    let old = "    const MAX: u32 = 10;";
    let new = "    const MAX: u32 = 20;";
    let with_default = |trait_source: &str| -> Result<Option<super::ConstFlow>, String> {
        let source = format!("{trait_source}\n{edited}\n{SMALL_PINS}\n");
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let line = source
            .lines()
            .position(|l| l.starts_with("    const MAX: u32 = 20;"))
            .ok_or("no declaration")?
            + 1;
        Ok(
            super::associated_const_binding(&source, line, "MAX", facts.functions.iter()).and_then(
                |binding| {
                    let related: Vec<_> = facts.tests.iter().collect();
                    let change = super::ConstChange {
                        old_declaration: old.to_string(),
                        new_declaration: new.to_string(),
                    };
                    super::evaluate_const_flow(
                        &change,
                        &binding.readers,
                        &related,
                        &|name| facts.functions.iter().filter(|f| f.name == name).count(),
                        Some(&binding.self_type),
                    )
                },
            ),
        )
    };
    let defaulted = TRAIT_SOURCE.replace("const MAX: u32;", "const MAX: u32 = 4;");
    assert_eq!(with_default(&defaulted)?, None);
    let helper = TRAIT_SOURCE.replace("if v > Self::MAX", "if is_big(v)");
    assert_eq!(with_default(&helper)?, None);
    Ok(())
}

#[test]
fn only_a_differently_evaluated_value_of_one_type_is_a_changed_value() {
    let changes = |old: &str, new: &str| {
        super::const_value_changes(&super::ConstChange {
            old_declaration: old.to_string(),
            new_declaration: new.to_string(),
        })
    };
    assert!(changes("pub const N: u32 = 10;", "pub const N: u32 = 11;"));
    assert!(changes(
        "pub const G: &str = \"hello\";",
        "pub const G: &str = \"hi\";"
    ));
    // Same value, other spelling or visibility; other type; other name.
    assert!(!changes(
        "pub const N: u32 = 10;",
        "pub const N: u32 = 5 + 5;"
    ));
    assert!(!changes("const N: u32 = 10;", "pub const N: u32 = 10;"));
    assert!(!changes("pub const N: u32 = 10;", "pub const N: u64 = 11;"));
    assert!(!changes("pub const N: u32 = 10;", "pub const M: u32 = 11;"));
    // A value the evaluator cannot read is not known to change.
    assert!(!changes(
        "pub const N: u32 = f(1);",
        "pub const N: u32 = f(2);"
    ));
}
