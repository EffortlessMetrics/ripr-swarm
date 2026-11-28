//! The fold reads a function text once per run scope and answers exactly
//! what the per-call reading answered (#11738).

use super::{
    FOLD_PLAN_READS, RangeTail, Value, declared_integer_range, fold_block, fold_function_value,
    tokenize,
};
use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::syntax::run_memo;
use ra_ap_syntax::{AstNode, ast, ast::HasName};
use std::collections::BTreeMap;

/// The fold as it was before the plan: every step repeated for each call.
/// Kept verbatim so the comparison below is against the replaced behavior.
fn reference_fold_function_value(body: &str, arguments: &[i128]) -> Option<Value> {
    let parse = parse_clean_source_file(body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    for ancestor in function.syntax().ancestors().skip(1) {
        if ast::Impl::can_cast(ancestor.kind())
            || ast::Trait::can_cast(ancestor.kind())
            || ast::Fn::can_cast(ancestor.kind())
        {
            return None;
        }
    }
    let param_list = function.param_list()?;
    if param_list.self_param().is_some() {
        return None;
    }
    let mut names = Vec::new();
    let mut widths = Vec::new();
    for param in param_list.params() {
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        // A subpattern (`x @ (a, b)`) does not bind the written name alone.
        if ident.pat().is_some() {
            return None;
        }
        names.push(ident.name()?.text().to_string());
        // A declared-width parameter bounds the inputs the real function
        // accepts; an argument outside it names a call that cannot compile.
        let width = param
            .ty()
            .and_then(|ty| declared_integer_range(&ty.syntax().text().to_string()));
        widths.push(width);
    }
    if names.len() != arguments.len() {
        return None;
    }
    for (width, value) in widths.iter().zip(arguments.iter().copied()) {
        if let Some((low, high)) = width
            && !(*low..=*high).contains(&value)
        {
            return None;
        }
    }
    // The fold uses the return type's declared width when it is a
    // recognized integer type: release builds wrap at that width, so a
    // mathematical result outside it does not describe a value the owner
    // can return and the pair stays not evaluable instead of contradicting
    // a passing wrapping assertion (#6701 review).
    // No recognized return type: the fold keeps exact i128 arithmetic,
    // which can only withhold credit.
    let fold_range = function
        .ret_type()
        .and_then(|ret| declared_integer_range(&ret.syntax().text().to_string()));
    let block = function.body()?;
    let environment: BTreeMap<String, i128> =
        names.into_iter().zip(arguments.iter().copied()).collect();
    if let Some(tail) = RangeTail::of_block(&block) {
        return tail.fold(&environment, fold_range);
    }
    // Comments and strings carry no arithmetic; masking them first keeps the
    // tokenizer away from brace-bearing content inside either.
    let masked =
        crate::analysis::extract::mask_comments_and_strings(&block.syntax().text().to_string());
    let tokens = tokenize(&masked)?;
    let uniform = fold_range.filter(|range| widths.iter().all(|width| *width == Some(*range)));
    fold_block(&tokens, &environment, fold_range, uniform)
}

const BODIES: &[&str] = &[
    "fn inc(x: u32) -> u32 { x + 1 }",
    "fn dec(x: u8) -> u8 { x - 1 }",
    "fn wide(x: i64) -> i64 { x * 3 + 7 }",
    "fn mixed(x: u8, y: u32) -> u32 { y + 2 }",
    "fn two(a: i32, b: i32) -> i32 { a * b - a }",
    "fn no_width(x: Count) -> Count { x + 1 }",
    "fn no_return(x: u32) { x + 1; }",
    "fn is_big(x: u32) -> bool { x > 10 }",
    "fn branch(x: i32) -> i32 { if x > 3 { x - 3 } else { x + 3 } }",
    "fn early(x: i32) -> i32 { if x < 0 { return 0; } x * 2 }",
    "fn local(x: u16) -> u16 { let y = x + 4; y * 2 }",
    "fn commented(x: u32) -> u32 { /* } */ x + 1 // { \n }",
    "fn text(x: u32) -> u32 { let _s = \"}{\"; x + 2 }",
    "fn none() -> u32 { 41 + 1 }",
    "fn pattern((a, b): (u32, u32)) -> u32 { a + b }",
    "fn bound(x @ 1..=9: u32) -> u32 { x }",
    "fn call(x: u32) -> u32 { other(x) }",
    "fn wraps(x: u8) -> u8 { x + 200 }",
    "fn pointer(x: usize) -> usize { x + 1 }",
    "fn huge(x: u128) -> u128 { x + 1 }",
    "impl S { fn method(&self, x: u32) -> u32 { x + 1 } }",
    "impl S { fn assoc(x: u32) -> u32 { x + 1 } }",
    "trait T { fn provided(x: u32) -> u32 { x + 1 } }",
    "fn outer(x: u32) -> u32 { fn inner(y: u32) -> u32 { y } x }",
    "fn total(n: u32) -> u32 { (0..n).sum() }",
    "fn total_inclusive(n: u64) -> u64 { (1..=n).sum() }",
    "fn counted(a: u32, b: u32) -> usize { (a..b).count() }",
    "fn within(x: u32) -> bool { (3..=9).contains(&x) }",
    "fn bucket(x: u32) -> u32 { match x { 0 => 10, 1..=9 => 20, 10 | 11 => 30, _ => 40 } }",
    "fn narrow(n: u32) -> u8 { (0..n).count() as u8 }",
    "fn broken(x: u32) -> u32 { x + }",
    "struct NotAFunction;",
    "",
];

const ARGUMENTS: &[&[i128]] = &[
    &[],
    &[0],
    &[1],
    &[-1],
    &[5],
    &[11],
    &[100],
    &[255],
    &[256],
    &[65_535],
    &[4_294_967_295],
    &[4_294_967_296],
    &[i64::MAX as i128],
    &[0, 0],
    &[3, 4],
    &[255, 7],
    &[256, 7],
    &[-2, 9],
    &[1, 2, 3],
];

#[test]
fn planned_fold_equals_the_replaced_fold_on_every_body_and_argument_list() {
    let _scope = run_memo::enter();
    let mut evaluated = 0usize;
    let mut withheld = 0usize;
    // Twice, so the second pass answers every body from its stored plan.
    for _pass in 0..2 {
        for body in BODIES {
            for arguments in ARGUMENTS {
                let planned = fold_function_value(body, arguments);
                let reference = reference_fold_function_value(body, arguments);
                assert_eq!(planned, reference, "{body:?} at {arguments:?}");
                match planned {
                    Some(_) => evaluated += 1,
                    None => withheld += 1,
                }
            }
        }
    }
    // The comparison is only worth having if both answers occur often.
    assert!(evaluated > 100, "evaluated {evaluated}");
    assert!(withheld > 100, "withheld {withheld}");
}

#[test]
fn planned_fold_without_a_scope_equals_the_replaced_fold() {
    for body in BODIES {
        for arguments in ARGUMENTS {
            assert_eq!(
                fold_function_value(body, arguments),
                reference_fold_function_value(body, arguments),
                "{body:?} at {arguments:?}"
            );
        }
    }
}

#[test]
fn a_function_text_is_read_once_per_scope_however_many_inputs_are_folded() {
    let reads = || FOLD_PLAN_READS.with(std::cell::Cell::get);
    let body = "fn inc(x: u32) -> u32 { x + 1 }";
    let other = "fn dec(x: u32) -> u32 { x - 1 }";
    {
        let _scope = run_memo::enter();
        let before = reads();
        for input in 1..=50 {
            assert_eq!(
                fold_function_value(body, &[input]),
                Some(Value::Int(input + 1))
            );
        }
        assert_eq!(reads() - before, 1, "one text, fifty inputs");
        assert_eq!(fold_function_value(other, &[9]), Some(Value::Int(8)));
        assert_eq!(reads() - before, 2, "a second text is read on its own");
        // A text the fold cannot read is remembered as such, not re-read.
        for _ in 0..5 {
            assert_eq!(
                fold_function_value("fn broken(x: u32) -> u32 { x + }", &[1]),
                None
            );
        }
        assert_eq!(reads() - before, 3);
    }
    // The plan does not outlive the scope that read it.
    let _scope = run_memo::enter();
    let before = reads();
    assert_eq!(fold_function_value(body, &[1]), Some(Value::Int(2)));
    assert_eq!(reads() - before, 1);
}
