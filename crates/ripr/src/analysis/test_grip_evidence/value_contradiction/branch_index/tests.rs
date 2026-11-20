//! The indexed branch reading answers exactly what the per-pair reading
//! answered (#11738).

use super::super::{
    fold_condition, fold_expression, line_of_offset, normalized_expression,
    parameter_is_rebound_or_mutated, seam_branch_reached_by_call, single_compared_owner_call,
};
use super::BRANCH_INDEX_READS;
use crate::analysis::facts::TestFact as TestSummary;
use crate::analysis::facts::{FunctionSourceRole, FunctionSummary, SharedFactSlice};
use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::syntax::run_memo;
use crate::domain::SymbolId;
use ra_ap_syntax::{AstNode, ast, ast::HasName};
use std::collections::BTreeMap;
use std::path::PathBuf;

// The replaced reading, kept verbatim so the comparison below is against the
// behavior it replaces.

pub(super) fn reference_seam_branch_reached_by_call(
    owner: &FunctionSummary,
    seam_expression: &str,
    seam_line: usize,
    oracle_text: &str,
    test: &TestSummary,
) -> Option<bool> {
    // The assertion may pin a local the call was bound to
    // (`let got = owner(..); assert_eq!(got, 50)`), so resolve the compared
    // operands through their `let` initializers before looking for the call.
    let call_arguments = single_compared_owner_call(owner, oracle_text, test, true)?;
    let mut values = Vec::with_capacity(call_arguments.len());
    for argument in &call_arguments {
        values.push(fold_expression(argument, &BTreeMap::new())?);
    }
    let parse = parse_clean_source_file(owner.body.as_str())?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let block = function.body()?;
    let mut names = Vec::new();
    for param in function.param_list()?.params() {
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        names.push(ident.name()?.text().to_string());
    }
    if names.len() != values.len() {
        return None;
    }
    let target = normalized_expression(seam_expression);
    // The seam's own line, in the coordinates of the parsed body. `body` starts
    // at the owner's `fn` token, so body line 1 is `owner.start_line`. Matching
    // the seam's line is what tells two same-text occurrences apart: a sibling
    // `return` after the `if` is a different line, so it can neither stand in
    // for the seam's branch nor make the branch scan ambiguous (#14496 review).
    let body = owner.body.as_str();
    let seam_body_line = seam_line.checked_sub(owner.start_line)? + 1;
    let mut matches = Vec::new();
    for if_expr in block.syntax().descendants().filter_map(ast::IfExpr::cast) {
        let then_block = if_expr.then_branch()?;
        let else_block = match if_expr.else_branch() {
            Some(ast::ElseBranch::Block(else_block)) => Some(else_block),
            _ => None,
        };
        // Both arms holding the same expression means the seam's own branch
        // is not established by text, so keep today's credit. This is a
        // text-only check: the two arms are different lines, so the line
        // match below would otherwise pick one and withhold credit for a
        // value the other arm returns identically.
        if block_holds_expression(&then_block, &target)
            && else_block
                .as_ref()
                .is_some_and(|else_block| block_holds_expression(else_block, &target))
        {
            return None;
        }
        let in_then = block_holds_expression_at_line(&then_block, &target, body, seam_body_line);
        let in_else = else_block.as_ref().is_some_and(|else_block| {
            block_holds_expression_at_line(else_block, &target, body, seam_body_line)
        });
        if in_then || in_else {
            matches.push((if_expr, in_then));
        }
    }
    // No `if` branch holds the seam's own occurrence: the seam is not
    // branch-local (a plain tail or a sibling `return`), so keep today's
    // credit. More than one `if` holding it is ambiguous for the same reason.
    let [(if_expr, in_then)] = matches.as_slice() else {
        return None;
    };
    // The environment binds the call's literal arguments to the parameter
    // names once. A `let` rebind, an assignment or a `for` pattern that
    // targets one of those names changes what the condition reads, so the
    // fold could disagree with the branch the call actually reaches. Keep
    // today's credit rather than withhold a real discriminator. A `let` in a
    // nested block that closes before the seam's path shadows nothing the
    // fold reads, so it does not count (#14569).
    if parameter_is_rebound_or_mutated(&block, &names, Some(if_expr)) {
        return None;
    }
    let environment: BTreeMap<String, i128> = names.into_iter().zip(values).collect();
    let condition = fold_condition(
        &if_expr.condition()?.syntax().text().to_string(),
        &environment,
    )?;
    let reached = if *in_then { condition } else { !condition };
    if !reached {
        return Some(false);
    }
    // The seam's own branch is taken, but an enclosing `if` can still keep the
    // call from evaluating it. Every enclosing branch on the path must be the
    // taken one too.
    enclosing_branches_reached(if_expr, &environment)
}

/// Whether every `if` enclosing `if_expr` takes the branch that holds it.
/// `None` when an enclosing branch cannot be read, so the caller keeps
/// today's credit.
fn enclosing_branches_reached(
    if_expr: &ast::IfExpr,
    environment: &BTreeMap<String, i128>,
) -> Option<bool> {
    let range = if_expr.syntax().text_range();
    let mut current = if_expr.syntax().parent();
    while let Some(node) = current {
        if let Some(ancestor) = ast::IfExpr::cast(node.clone()) {
            let in_then = ancestor
                .then_branch()
                .is_some_and(|block| block.syntax().text_range().contains_range(range));
            let in_else = match ancestor.else_branch() {
                Some(ast::ElseBranch::Block(block)) => {
                    block.syntax().text_range().contains_range(range)
                }
                Some(ast::ElseBranch::IfExpr(inner)) => {
                    inner.syntax().text_range().contains_range(range)
                }
                None => false,
            };
            if !in_then && !in_else {
                return None;
            }
            let condition = fold_condition(
                &ancestor.condition()?.syntax().text().to_string(),
                environment,
            )?;
            let reached = if in_then { condition } else { !condition };
            if !reached {
                return Some(false);
            }
        }
        current = node.parent();
    }
    Some(true)
}

/// Whether `block`'s tail expression or one of its statements is `target`,
/// ignoring the line. Used only for the identical-arms check, where the two
/// arms are different lines but return the same value.
fn block_holds_expression(block: &ast::BlockExpr, target: &str) -> bool {
    if block
        .tail_expr()
        .is_some_and(|tail| normalized_expression(&tail.syntax().text().to_string()) == target)
    {
        return true;
    }
    block
        .statements()
        .any(|statement| normalized_expression(&statement.syntax().text().to_string()) == target)
}

/// Whether `block`'s tail expression or one of its statements is `target` and
/// starts on `line` (a body-relative line). The line check is what keeps a
/// same-text sibling `return` from standing in for the seam's own occurrence.
fn block_holds_expression_at_line(
    block: &ast::BlockExpr,
    target: &str,
    body: &str,
    line: usize,
) -> bool {
    let at_line = |expr: &ast::Expr| {
        normalized_expression(&expr.syntax().text().to_string()) == target
            && line_of_offset(body, usize::from(expr.syntax().text_range().start())) == line
    };
    if block.tail_expr().is_some_and(|tail| at_line(&tail)) {
        return true;
    }
    block.statements().any(|statement| {
        normalized_expression(&statement.syntax().text().to_string()) == target
            && line_of_offset(body, usize::from(statement.syntax().text_range().start())) == line
    })
}

/// A small deterministic generator, so the corpus is the same on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }
}

const OWNER_START: usize = 10;

fn owner(body: &str) -> FunctionSummary {
    FunctionSummary {
        id: SymbolId("rust:src/lib.rs::f".to_string()),
        name: "f".to_string(),
        file: PathBuf::from("src/lib.rs"),
        start_line: OWNER_START,
        end_line: OWNER_START + body.lines().count(),
        body: body.into(),
        calls: SharedFactSlice::empty(),
        returns: Vec::new(),
        literals: SharedFactSlice::empty(),
        source_role: FunctionSourceRole::Production,
        attrs: SharedFactSlice::empty(),
        impl_attrs: Vec::new(),
        nested_fn_names: SharedFactSlice::empty(),
        let_bindings: SharedFactSlice::empty(),
        item: Default::default(),
        impl_context: Default::default(),
    }
}

fn test_calling(body: &str) -> TestSummary {
    TestSummary {
        name: "t".to_string(),
        file: PathBuf::from("src/lib.rs"),
        start_line: 100,
        end_line: 102,
        body: body.into(),
        calls: SharedFactSlice::empty(),
        assertions: Vec::new(),
        literals: SharedFactSlice::empty(),
        attrs: SharedFactSlice::empty(),
        nested_fn_names: SharedFactSlice::empty(),
        let_bindings: SharedFactSlice::empty(),
        generated: None,
    }
}

const CONDITIONS: &[&str] = &[
    "a > 3",
    "a < 0",
    "a == 2",
    "a >= b",
    "b != 1",
    "a + b > 4",
    "a % 2 == 0",
];

/// One generated statement: an `if` (maybe with `else`, `else if`, or a
/// nested `if`), a sibling `return`, or a parameter rebinding.
fn statement(rng: &mut Lcg, depth: usize, out: &mut String) {
    let value = rng.below(4);
    let condition = CONDITIONS[rng.below(CONDITIONS.len() as u64) as usize];
    match rng.below(9) {
        0 => out.push_str(&format!("return {value};\n")),
        1 if rng.below(6) == 0 => out.push_str("let a = a + 1;\n"),
        2 => out.push_str(&format!(
            "if {condition} {{\nreturn {value};\n}} else {{\nreturn {};\n}}\n",
            rng.below(4)
        )),
        3 => out.push_str(&format!(
            "if {condition} {{ return {value}; }} else if a < 1 {{ return {}; }}\n",
            rng.below(4)
        )),
        4 if depth < 3 => {
            out.push_str(&format!("if {condition} {{\n"));
            for _ in 0..1 + rng.below(3) {
                statement(rng, depth + 1, out);
            }
            out.push_str("}\n");
        }
        5 => out.push_str(&format!("if {condition} {{ return {value}; }}\n")),
        _ => out.push_str(&format!("if {condition} {{\nreturn {value};\n}}\n")),
    }
}

fn corpus() -> Vec<String> {
    let mut bodies: Vec<String> = [
        "fn f(a: i32, b: i32) -> i32 {\nif a > 3 {\nreturn 1;\n}\n0\n}",
        "fn f(a: i32, b: i32) -> i32 {\nif a > 3 {\nreturn 1;\n} else {\nreturn 1;\n}\n}",
        "fn f(a: i32, b: i32) -> i32 {\nif a > 3 {\nreturn 1;\n}\nreturn 1;\n}",
        "fn f(a: i32, b: i32) -> i32 {\nif a > 3 { if b != 1 {\nreturn 2;\n} }\n0\n}",
        "fn f(a: i32, b: i32) -> i32 {\nlet c = &mut a;\nif a > 3 {\nreturn 1;\n}\n0\n}",
        "fn f((a, b): (i32, i32)) -> i32 {\nif a > 3 {\nreturn 1;\n}\n0\n}",
        "fn f(a: i32, b: i32) -> i32 {\nif a > 3 {\nreturn 1;\n}\nif a > 3 {\nreturn 1;\n}\n0\n}",
        "fn f(a: i32, b: i32) -> i32 {\nif a > 3 { return 1; } if a < 0 { return 2; } 0\n}",
        "fn f(a: i32, b: i32) -> i32 {\nif a > 3 {\nreturn 1;\n",
    ]
    .iter()
    .map(|body| (*body).to_string())
    .collect();
    let mut rng = Lcg(0x5eed_11738);
    for _ in 0..120 {
        let mut body = String::from("fn f(a: i32, b: i32) -> i32 {\n");
        for _ in 0..1 + rng.below(6) {
            statement(&mut rng, 0, &mut body);
        }
        body.push_str("0\n}");
        bodies.push(body);
    }
    bodies
}

const SEAMS: &[&str] = &[
    "return 0;",
    "return 1;",
    "return 2;",
    "return 3",
    "0",
    "1",
    "let a = a + 1;",
];
const CALLS: &[&str] = &[
    "assert_eq!(f(0, 0), 1)",
    "assert_eq!(f(5, 1), 1)",
    "assert_eq!(f(-2, 7), 2)",
    "assert_eq!(f(2, 2), 0)",
    "assert_eq!(f(4, 9), 3)",
    "assert_eq!(f(1), 1)",
    "assert_eq!(g(1, 1), 1)",
];

#[test]
fn indexed_branch_answer_equals_the_replaced_reading_on_every_seam_line_and_call() {
    let tests: Vec<TestSummary> = CALLS.iter().map(|call| test_calling(call)).collect();
    let mut counts = BTreeMap::new();
    for scoped in [true, false] {
        let _scope = scoped.then(run_memo::enter);
        for body in corpus() {
            let owner = owner(&body);
            let lines = body.lines().count();
            for seam in SEAMS {
                let text = seam.trim_end_matches(';');
                for seam_line in OWNER_START - 1..=OWNER_START + lines {
                    // Every line for the first seam; for the others, only the
                    // lines that spell it, where an answer other than `None`
                    // can occur.
                    let spelled = seam_line
                        .checked_sub(OWNER_START)
                        .and_then(|at| body.lines().nth(at))
                        .is_some_and(|line| line.contains(text));
                    if *seam != SEAMS[0] && !spelled {
                        continue;
                    }
                    for (call, test) in CALLS.iter().zip(&tests) {
                        let indexed =
                            seam_branch_reached_by_call(&owner, seam, seam_line, call, test);
                        let reference = reference_seam_branch_reached_by_call(
                            &owner, seam, seam_line, call, test,
                        );
                        assert_eq!(
                            indexed, reference,
                            "{body:?}\nseam {seam:?} at line {seam_line}, {call}"
                        );
                        *counts.entry(format!("{indexed:?}")).or_insert(0usize) += 1;
                    }
                }
            }
        }
    }
    // The comparison is only worth having if every answer occurs often.
    for answer in ["Some(true)", "Some(false)", "None"] {
        let seen = counts.get(answer).copied().unwrap_or_default();
        assert!(seen > 200, "{answer} occurred {seen} times: {counts:?}");
    }
}

#[test]
fn an_owner_is_indexed_once_per_scope_however_many_seams_ask() {
    let reads = || BRANCH_INDEX_READS.with(std::cell::Cell::get);
    let mut body = String::from("fn f(a: i32, b: i32) -> i32 {\n");
    for k in 0..40 {
        body.push_str(&format!("if a >= {k} {{\nreturn {k};\n}}\n"));
    }
    body.push_str("0\n}");
    let owner = owner(&body);
    let test = test_calling("assert_eq!(f(5, 0), 5)");
    {
        let _scope = run_memo::enter();
        let before = reads();
        for k in 0..40usize {
            let line = OWNER_START + 2 + 3 * k;
            let answer = seam_branch_reached_by_call(
                &owner,
                &format!("return {k};"),
                line,
                "assert_eq!(f(5, 0), 5)",
                &test,
            );
            // `a >= k` holds for k up to 5 at a = 5.
            assert_eq!(answer, Some(k <= 5), "seam {k}");
        }
        assert_eq!(reads() - before, 1, "forty seams of one owner");
    }
    let _scope = run_memo::enter();
    let before = reads();
    assert_eq!(
        seam_branch_reached_by_call(
            &owner,
            "return 9;",
            OWNER_START + 29,
            "assert_eq!(f(5, 0), 5)",
            &test
        ),
        Some(false)
    );
    assert_eq!(reads() - before, 1, "a new scope reads again");
}
