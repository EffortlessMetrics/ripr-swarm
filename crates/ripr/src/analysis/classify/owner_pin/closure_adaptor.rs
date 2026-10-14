//! RIPR-SPEC-0260: an owner whose whole body is one closure in an iterator
//! adaptor over a slice parameter (`xs.iter().filter(|&&x| x > 0).count()`).
//!
//! The closure runs once per element of the parameter, so a test call whose
//! argument for that parameter is a literal sequence of literal values shows
//! exactly which values the closure binding takes (rule 1), and a non-empty
//! such argument shows the pinned call evaluates the closure at least once
//! (rule 2). Every other shape keeps the pre-0260 reading: the closure is a
//! part of the tail evaluated only on some inputs, and its binding is a value
//! ripr cannot map to a test input.

use super::{evaluates_conditionally, has_unbounded_macro, whole_word_offsets};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{FunctionSummary, TestSummary};
use crate::analysis::syntax::{HelperLoan, constant_table_column, parse_clean_source_file};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};

/// The adaptors whose closure runs on each element the adaptor reaches.
const ADAPTORS: &[&str] = &["filter", "map", "any", "all"];

/// Terminal calls that consume the adaptor without a closure of their own.
const TERMINALS: &[&str] = &["count", "collect", "sum"];

/// An owner tail `P.iter().A(|PAT| E)[.T()]` admitted by RIPR-SPEC-0260.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) struct AdaptorTail {
    /// The slice parameter `P` the adaptor iterates.
    pub(in crate::analysis::classify) parameter: String,
    /// `P`'s position among the owner's non-`self` parameters, which is its
    /// argument position in a call of the owner.
    pub(in crate::analysis::classify) position: usize,
    /// The identifier the closure pattern binds.
    pub(in crate::analysis::classify) binding: String,
    /// How many `*` the binding may carry and still read the element: the
    /// adaptor's reference layers (`filter` hands `&&T`, the others `&T`)
    /// minus the `&` the pattern already strips.
    pub(in crate::analysis::classify) element_derefs: usize,
    /// The closure body `E`, whitespace-collapsed.
    pub(in crate::analysis::classify) closure_body: String,
    /// The whole tail, whitespace-collapsed.
    pub(in crate::analysis::classify) tail: String,
    /// The tail's first line, as an offset from the owner text's first line.
    pub(in crate::analysis::classify) tail_line_offset: usize,
    /// For `any` (`Some(true)`) and `all` (`Some(false)`): the closure value
    /// that stops the walk and decides the result. `None` for `filter` and
    /// `map`, which run the closure on every element.
    deciding: Option<bool>,
    /// When the closure body is one comparison of the binding: its operator
    /// and whether the binding is the left operand.
    comparison: Option<(String, bool)>,
    /// Whether the tail's `collect` relies on the bare name `Vec` meaning
    /// the standard vector, which the owner's file may rebind.
    collects_named_vec: bool,
}

impl AdaptorTail {
    /// Whether the tail's result keeps every closure value in the owner's
    /// file `source`: a `collect` that names a bare `Vec` holds only while
    /// that file binds no other item to `Vec` (`use .. as Vec`, `use x::Vec`,
    /// a glob outside `std` / `core` / `alloc`, `type Vec`, `struct Vec`,
    /// `enum Vec`, `union Vec`, `trait Vec`). An unread file fails closed.
    pub(in crate::analysis::classify) fn keeps_standard_vec(
        &self,
        source: Option<&str>,
        index: &crate::analysis::rust_index::RustIndex,
    ) -> bool {
        if !self.collects_named_vec {
            return true;
        }
        let Some(source) = source else {
            return false;
        };
        let masked = mask_comments_and_strings(source);
        let declares = ["struct", "enum", "union", "type", "trait"]
            .iter()
            .any(|keyword| {
                whole_word_offsets(&masked, keyword)
                    .into_iter()
                    .any(|offset| {
                        super::starts_with_word(
                            masked[offset + keyword.len()..].trim_start(),
                            "Vec",
                        )
                    })
            });
        !declares && !super::declaring_file_rebinds(source, "Vec", false, index)
    }

    /// Whether `operand` is the closure binding read as one element of `P`:
    /// `x`, `*x` or `**x`, never more `*` than the pattern leaves.
    pub(in crate::analysis::classify) fn reads_element(&self, operand: &str) -> bool {
        let operand = operand.trim();
        let name = operand.trim_start_matches('*');
        let derefs = operand.len() - name.len();
        name == self.binding && derefs <= self.element_derefs
    }

    /// Whether the changed `expression` is the whole closure body: the
    /// comparison `E` itself, not a part of it.
    pub(in crate::analysis::classify) fn closure_body_is(&self, expression: &str) -> bool {
        collapse(expression.trim().trim_end_matches(';')) == self.closure_body
    }

    /// Whether one run (the elements one call passes, in order) reaches the
    /// boundary value `boundary`: some element equals it and, for `any` /
    /// `all`, the closure's current value on every other element of the run
    /// is the non-deciding one. A short-circuit walk may otherwise stop
    /// before the boundary element, or another element may decide the
    /// result whatever the boundary element gives. An element ripr cannot
    /// evaluate counts as deciding (fail closed).
    pub(in crate::analysis::classify) fn run_reaches_boundary(
        &self,
        run: &[String],
        boundary: &str,
    ) -> bool {
        let Some(at) = run.iter().position(|element| same_value(element, boundary)) else {
            return false;
        };
        let Some(deciding) = self.deciding else {
            return true;
        };
        let Some((operator, element_left)) = &self.comparison else {
            return false;
        };
        run.iter().enumerate().all(|(position, element)| {
            position == at
                || same_value(element, boundary)
                || evaluate(operator, *element_left, element, boundary)
                    .is_some_and(|value| value != deciding)
        })
    }

    /// Whether the changed `expression` is the whole tail.
    pub(in crate::analysis::classify) fn tail_is(&self, expression: &str) -> bool {
        let expression = collapse(expression.trim().trim_end_matches(';'));
        let expression = expression
            .strip_prefix("return ")
            .map_or(expression.as_str(), str::trim);
        expression == self.tail
    }
}

/// The owner's admitted adaptor tail: the body holds no statement, only the
/// tail, so the parameter is not rebound and the tail is the only exit.
pub(in crate::analysis::classify) fn owner_adaptor_tail(
    owner: &FunctionSummary,
) -> Option<AdaptorTail> {
    if !owner.item.has_body {
        return None;
    }
    adaptor_tail_of(&owner.body)
}

/// [`owner_adaptor_tail`] over the owner's text from `fn` to its closing
/// brace.
pub(in crate::analysis::classify) fn adaptor_tail_of(body: &str) -> Option<AdaptorTail> {
    let masked = mask_comments_and_strings(body);
    if has_unbounded_macro(&masked)
        || !whole_word_offsets(&masked, "return").is_empty()
        || masked.contains('?')
    {
        return None;
    }
    let parse = parse_clean_source_file(body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if function.async_token().is_some() || function.const_token().is_some() {
        return None;
    }
    let block = function.body()?;
    if block.statements().next().is_some() {
        return None;
    }
    let tail = block.tail_expr()?;
    let mut call = method_call(&tail)?;
    let mut terminal = None;
    if TERMINALS.contains(&method_name(&call)?.as_str()) {
        if call.arg_list()?.args().next().is_some() {
            return None;
        }
        terminal = Some(call.clone());
        call = method_call(&call.receiver()?)?;
    }
    let adaptor = method_name(&call)?;
    if !ADAPTORS.contains(&adaptor.as_str()) || call.generic_arg_list().is_some() {
        return None;
    }
    let mut arguments = call.arg_list()?.args();
    let (Some(ast::Expr::ClosureExpr(closure)), None) = (arguments.next(), arguments.next()) else {
        return None;
    };
    let iter = method_call(&call.receiver()?)?;
    if method_name(&iter)?.as_str() != "iter"
        || iter.generic_arg_list().is_some()
        || iter.arg_list()?.args().next().is_some()
    {
        return None;
    }
    let ast::Expr::PathExpr(receiver) = iter.receiver()? else {
        return None;
    };
    let parameter = single_segment(&receiver)?;
    let position = slice_parameter_position(&function, &parameter)?;
    let (binding, pattern_refs) = closure_binding(&closure)?;
    let layers = if adaptor == "filter" { 2 } else { 1 };
    let element_derefs = layers - pattern_refs.min(layers);
    if pattern_refs > layers {
        return None;
    }
    let closure_body = closure.body()?;
    if !unconditional_closure_body(&closure_body, &binding)
        || !admitted_consumer(&function, &adaptor, terminal.as_ref(), &closure_body)
    {
        return None;
    }
    let deciding = match adaptor.as_str() {
        "any" => Some(true),
        "all" => Some(false),
        _ => None,
    };
    let comparison = binding_comparison(&closure_body, &binding, layers - pattern_refs);
    let collects_named_vec = terminal
        .as_ref()
        .is_some_and(|terminal| method_name(terminal).as_deref() == Some("collect"));
    Some(AdaptorTail {
        parameter,
        position,
        binding,
        element_derefs,
        closure_body: collapse(&closure_body.syntax().text().to_string()),
        tail: collapse(&tail.syntax().text().to_string()),
        tail_line_offset: body
            .get(..usize::from(tail.syntax().text_range().start()))?
            .matches('\n')
            .count(),
        deciding,
        comparison,
        collects_named_vec,
    })
}

/// Whether the adaptor and its terminal keep every closure value in the
/// owner's result, so a changed closure value changes what the owner
/// returns. Admitted: `filter` + `count`, `filter` / `map` + `collect` into a
/// `Vec`, `map` + `sum` over an arithmetic body, and `any` / `all` with no
/// terminal. Refused (fail closed): `map` + `count` (ignores the closure
/// value), `filter` + `sum` (an element equal to the additive identity
/// contributes nothing whether kept or dropped), `collect` into anything not
/// provably a `Vec` (a set loses order and multiplicity), `map` + `sum` over
/// a comparison or cast, a bare `filter` / `map` (a lazy iterator), and a
/// terminal after `any` / `all`.
fn admitted_consumer(
    function: &ast::Fn,
    adaptor: &str,
    terminal: Option<&ast::MethodCallExpr>,
    closure_body: &ast::Expr,
) -> bool {
    let Some(terminal) = terminal else {
        return matches!(adaptor, "any" | "all");
    };
    let Some(name) = method_name(terminal) else {
        return false;
    };
    match (adaptor, name.as_str()) {
        ("filter", "count") => terminal.generic_arg_list().is_none(),
        ("filter" | "map", "collect") => collects_into_vec(function, terminal),
        ("map", "sum") => arithmetic_body(closure_body),
        _ => false,
    }
}

/// Whether `collect` provably builds a `Vec`: a `collect::<Vec<..>>()`
/// turbofish, or no turbofish and an owner return type `Vec<..>`.
fn collects_into_vec(function: &ast::Fn, terminal: &ast::MethodCallExpr) -> bool {
    let target = match terminal.generic_arg_list() {
        Some(arguments) => {
            let mut arguments = arguments.generic_args();
            let (Some(argument), None) = (arguments.next(), arguments.next()) else {
                return false;
            };
            argument.syntax().text().to_string()
        }
        None => match function.ret_type().and_then(|ret| ret.ty()) {
            Some(ty) => ty.syntax().text().to_string(),
            None => return false,
        },
    };
    let target = collapse(&target);
    let target = ["std::vec::", "alloc::vec::"]
        .iter()
        .find_map(|prefix| target.strip_prefix(prefix))
        .unwrap_or(&target);
    target.starts_with("Vec<") && target.ends_with('>')
}

/// Whether a `map` body summed by `sum` is arithmetic: no comparison, no
/// `!` and no cast, so no closure value is a `bool` folded into a count.
fn arithmetic_body(body: &ast::Expr) -> bool {
    body.syntax().descendants_with_tokens().all(|element| {
        !matches!(
            element.kind(),
            ra_ap_syntax::SyntaxKind::CAST_EXPR
                | ra_ap_syntax::SyntaxKind::L_ANGLE
                | ra_ap_syntax::SyntaxKind::R_ANGLE
                | ra_ap_syntax::SyntaxKind::LTEQ
                | ra_ap_syntax::SyntaxKind::GTEQ
                | ra_ap_syntax::SyntaxKind::EQ2
                | ra_ap_syntax::SyntaxKind::NEQ
                | ra_ap_syntax::SyntaxKind::BANG
                | ra_ap_syntax::SyntaxKind::TRUE_KW
                | ra_ap_syntax::SyntaxKind::FALSE_KW
        )
    })
}

/// The comparison operator of a closure body `x OP v` / `v OP x`, and
/// whether the binding (with at most `derefs` `*`) is the left operand.
fn binding_comparison(body: &ast::Expr, binding: &str, derefs: usize) -> Option<(String, bool)> {
    let ast::Expr::BinExpr(comparison) = body else {
        return None;
    };
    let operator = comparison.op_token()?.text().to_string();
    if !matches!(operator.as_str(), "<" | "<=" | ">" | ">=" | "==" | "!=") {
        return None;
    }
    let reads = |operand: Option<ast::Expr>| {
        operand.is_some_and(|operand| {
            let text = collapse(&operand.syntax().text().to_string());
            let name = text.trim_start_matches('*');
            name == binding && text.len() - name.len() <= derefs
        })
    };
    match (reads(comparison.lhs()), reads(comparison.rhs())) {
        (true, false) => Some((operator, true)),
        (false, true) => Some((operator, false)),
        _ => None,
    }
}

/// The value of `element OP boundary` (or `boundary OP element`) for two
/// plain numeric literals of one kind (both integers or both floats);
/// `None` for anything else.
fn evaluate(operator: &str, element_left: bool, element: &str, boundary: &str) -> Option<bool> {
    let ordering = match (numeric(element)?, numeric(boundary)?) {
        (Number::Integer(element), Number::Integer(boundary)) => element.cmp(&boundary),
        (Number::Float(element), Number::Float(boundary)) => element.partial_cmp(&boundary)?,
        _ => return None,
    };
    let ordering = if element_left {
        ordering
    } else {
        ordering.reverse()
    };
    Some(match operator {
        "<" => ordering.is_lt(),
        "<=" => ordering.is_le(),
        ">" => ordering.is_gt(),
        ">=" => ordering.is_ge(),
        "==" => ordering.is_eq(),
        "!=" => ordering.is_ne(),
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Number {
    Integer(i128),
    Float(f64),
}

fn numeric(text: &str) -> Option<Number> {
    let text = text.trim().replace('_', "");
    let digits = text.strip_prefix('-').unwrap_or(&text);
    if !digits.starts_with(|character: char| character.is_ascii_digit())
        || !digits
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return None;
    }
    if text.contains('.') {
        text.parse().ok().map(Number::Float)
    } else {
        text.parse().ok().map(Number::Integer)
    }
}

/// Two literal spellings of one value: equal text, or equal numbers.
pub(in crate::analysis::classify) fn same_value(left: &str, right: &str) -> bool {
    left.trim() == right.trim()
        || numeric(left)
            .zip(numeric(right))
            .is_some_and(|(left, right)| left == right)
}

/// RIPR-SPEC-0260 rule 2: whether the pinned owner call `call_text` passes
/// the iterated parameter (argument `position`) a value whose every run
/// ripr reads as a literal sequence, and at least one run is non-empty, so
/// the closure runs at least once on the pinned input. The argument may be
/// the literal itself, a column of a constant-row `for` table in the test,
/// or, for an assertion borrowed from a test-local helper (`loan`), the
/// helper's parameter, read at each of the test's eager calls of the
/// helper. Anything ripr cannot read refuses (fail closed).
pub(in crate::analysis::classify) fn pinned_call_runs_closure(
    test: &TestSummary,
    loan: Option<&HelperLoan>,
    call_text: &str,
    name: &str,
    position: usize,
) -> bool {
    call_argument_texts(call_text, name)
        .and_then(|arguments| arguments.get(position).cloned())
        .and_then(|argument| argument_sequences(test, loan, &argument))
        .is_some_and(|runs| runs.iter().any(|elements| !elements.is_empty()))
}

/// The element sequences `argument` takes over every run of the call that
/// passes it, or `None` when any run is unreadable.
pub(in crate::analysis::classify) fn argument_sequences(
    test: &TestSummary,
    loan: Option<&HelperLoan>,
    argument: &str,
) -> Option<Vec<Vec<String>>> {
    if let Some(elements) = literal_sequence(argument) {
        return Some(vec![elements]);
    }
    let name = argument.trim();
    if !super::is_plain_identifier(name) {
        return None;
    }
    match loan {
        Some(loan) => {
            let index = loan
                .parameters
                .iter()
                .position(|parameter| parameter == name)?;
            if loan.call_lines.is_empty() {
                return None;
            }
            loan.call_lines
                .iter()
                .map(|line| {
                    let text = test.body.lines().nth(line.checked_sub(test.start_line)?)?;
                    let arguments = call_argument_texts(text, &loan.name)?;
                    literal_sequence(arguments.get(index)?)
                })
                .collect()
        }
        None => constant_table_column(&test.body, name)?
            .iter()
            .map(|cell| literal_sequence(cell))
            .collect(),
    }
}

/// The top-level argument texts of the first call of `name` in `text`.
fn call_argument_texts(text: &str, name: &str) -> Option<Vec<String>> {
    let masked = mask_comments_and_strings(text);
    let open = whole_word_offsets(&masked, name)
        .into_iter()
        .map(|offset| offset + name.len())
        .find(|end| masked[*end..].starts_with('('))?;
    let close = super::matching_close(&masked, open, b'(', b')')?;
    Some(split_top_level(&text[open + 1..close]))
}

/// The top-level comma-separated parts of `text`.
pub(in crate::analysis::classify) fn top_level_arguments(text: &str) -> Vec<String> {
    split_top_level(text)
}

fn split_top_level(text: &str) -> Vec<String> {
    let masked = mask_comments_and_strings(text);
    let mut arguments = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (offset, byte) in masked.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                arguments.push(text[start..offset].trim().to_string());
                start = offset + 1;
            }
            _ => {}
        }
    }
    let last = text[start..].trim();
    if !last.is_empty() {
        arguments.push(last.to_string());
    }
    arguments
}

fn method_call(expression: &ast::Expr) -> Option<ast::MethodCallExpr> {
    match expression {
        ast::Expr::MethodCallExpr(call) => Some(call.clone()),
        _ => None,
    }
}

fn method_name(call: &ast::MethodCallExpr) -> Option<String> {
    Some(call.name_ref()?.text().to_string())
}

fn single_segment(path: &ast::PathExpr) -> Option<String> {
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let segment = path.segment()?;
    if segment.generic_arg_list().is_some() {
        return None;
    }
    Some(segment.name_ref()?.text().to_string())
}

/// `P`'s position among the owner's non-`self` parameters when `P` is a
/// plain identifier parameter of a slice-like type (`&[T]`, `&mut [T]`,
/// `[T; N]`, `&Vec<T>`, `Vec<T>`), whose `iter` is the slice's own.
fn slice_parameter_position(function: &ast::Fn, parameter: &str) -> Option<usize> {
    let list = function.param_list()?;
    let mut found = None;
    for (position, param) in list.params().enumerate() {
        let ast::Pat::IdentPat(pattern) = param.pat()? else {
            continue;
        };
        if pattern.name()?.text() != parameter {
            continue;
        }
        if found.is_some()
            || pattern.ref_token().is_some()
            || pattern.mut_token().is_some()
            || pattern.pat().is_some()
            || !slice_like_type(&param.ty()?.syntax().text().to_string())
        {
            return None;
        }
        found = Some(position);
    }
    found
}

fn slice_like_type(ty: &str) -> bool {
    let ty = collapse(ty);
    let mut rest = ty.as_str();
    if let Some(referent) = rest.strip_prefix('&') {
        rest = referent.trim_start();
        if let Some(lifetime) = rest.strip_prefix('\'') {
            let end = lifetime
                .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .unwrap_or(lifetime.len());
            rest = lifetime[end..].trim_start();
        }
        if let Some(mutable) = rest.strip_prefix("mut ") {
            rest = mutable.trim_start();
        }
    }
    rest.starts_with('[') || rest.starts_with("Vec<")
}

/// The identifier the closure's single parameter binds, and how many `&`
/// its pattern strips: `x`, `&x` or `&&x`. A typed, `mut`, `ref` or
/// destructuring parameter, a `move` / `async` closure, or a closure with a
/// declared return type is not read.
fn closure_binding(closure: &ast::ClosureExpr) -> Option<(String, usize)> {
    if closure.move_token().is_some()
        || closure.async_token().is_some()
        || closure.ret_type().is_some()
        || closure.static_token().is_some()
    {
        return None;
    }
    let list = closure.param_list()?;
    let mut params = list.params();
    let (Some(param), None) = (params.next(), params.next()) else {
        return None;
    };
    if param.ty().is_some() {
        return None;
    }
    let mut pattern = param.pat()?;
    let mut refs = 0usize;
    loop {
        match pattern {
            ast::Pat::RefPat(reference) if reference.mut_token().is_none() => {
                refs += 1;
                pattern = reference.pat()?;
            }
            ast::Pat::IdentPat(ident)
                if ident.ref_token().is_none()
                    && ident.mut_token().is_none()
                    && ident.pat().is_none() =>
            {
                return Some((ident.name()?.text().to_string(), refs));
            }
            _ => return None,
        }
    }
}

/// Whether the closure body evaluates every part on every call: no nested
/// closure, `&&`, `||`, branch, loop, early exit, `?`, `.await` or macro,
/// no block, and no variable other than the binding (a captured local
/// would be an input the element rows do not carry).
fn unconditional_closure_body(body: &ast::Expr, binding: &str) -> bool {
    if matches!(body, ast::Expr::BlockExpr(_)) {
        return false;
    }
    let text = body.syntax().text().to_string();
    let masked = mask_comments_and_strings(&text);
    if evaluates_conditionally(&masked)
        || masked.contains('?')
        || !super::macro_invocations(&masked).is_empty()
        || ["return", "await", "async", "yield", "unsafe"]
            .iter()
            .any(|keyword| !whole_word_offsets(&masked, keyword).is_empty())
    {
        return false;
    }
    body.syntax()
        .descendants()
        .filter_map(ast::PathExpr::cast)
        .all(|path| {
            let Some(path) = path.path() else {
                return false;
            };
            let Some(last) = path
                .segment()
                .and_then(|segment| segment.name_ref())
                .map(|name| name.text().to_string())
            else {
                return false;
            };
            if path.qualifier().is_none() {
                last == binding || constant_like(&last)
            } else {
                constant_like(&last)
            }
        })
}

fn constant_like(name: &str) -> bool {
    name.chars().any(|character| character.is_ascii_uppercase())
        && name.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

/// The elements of a literal sequence argument: `&[a, b]`, `[a, b]`,
/// `vec![a, b]` or `&vec![a, b]`, where every element is one literal value
/// (an integer or float without suffix, possibly negated, `true`, `false`,
/// or a char literal). `Some(vec![])` for an empty sequence; `None` for any
/// other argument (a local, a repeat `[v; n]`, a range, a call, a suffixed
/// or computed element).
pub(in crate::analysis::classify) fn literal_sequence(argument: &str) -> Option<Vec<String>> {
    let argument = argument.trim();
    let argument = argument.strip_prefix('&').map_or(argument, str::trim_start);
    let inner = if let Some(rest) = argument.strip_prefix("vec!") {
        rest.trim_start().strip_prefix('[')?.strip_suffix(']')?
    } else {
        argument.strip_prefix('[')?.strip_suffix(']')?
    };
    let mut elements = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (offset, character) in inner.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ';' | '"' => return None,
            ',' if depth == 0 => {
                elements.push(inner[start..offset].trim());
                start = offset + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        elements.push(last);
    }
    elements
        .into_iter()
        .map(|element| literal_element(element).then(|| element.to_string()))
        .collect()
}

fn literal_element(element: &str) -> bool {
    if matches!(element, "true" | "false") {
        return true;
    }
    if let Some(body) = element
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return match body.len() {
            1 => body != "\\" && body != "'",
            2 => body.starts_with('\\') && matches!(&body[1..], "n" | "t" | "r" | "0" | "\\" | "'"),
            _ => false,
        };
    }
    let digits = element.strip_prefix('-').unwrap_or(element);
    let mut dots = 0usize;
    digits.starts_with(|character: char| character.is_ascii_digit())
        && !digits.ends_with('.')
        && !digits.ends_with('_')
        && digits.chars().all(|character| {
            if character == '.' {
                dots += 1;
            }
            character.is_ascii_digit() || character == '_' || character == '.'
        })
        && dots <= 1
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail(body: &str) -> Option<AdaptorTail> {
        adaptor_tail_of(body)
    }

    /// (parameter, position, binding, element derefs, closure body).
    fn shape(body: &str) -> Option<(String, usize, String, usize, String)> {
        tail(body).map(|tail| {
            (
                tail.parameter,
                tail.position,
                tail.binding,
                tail.element_derefs,
                tail.closure_body,
            )
        })
    }

    #[test]
    fn admits_each_adaptor_over_a_slice_parameter() {
        let owned = |parameter: &str, position, binding: &str, derefs, body: &str| {
            Some((
                parameter.to_string(),
                position,
                binding.to_string(),
                derefs,
                body.to_string(),
            ))
        };
        assert_eq!(
            shape("pub fn f(xs: &[i32]) -> usize {\n    xs.iter().filter(|&&x| x > 0).count()\n}"),
            owned("xs", 0, "x", 0, "x > 0")
        );
        assert_eq!(
            shape("fn f(xs: &[i32]) -> usize { xs.iter().filter(|x| **x > 0).count() }"),
            owned("xs", 0, "x", 2, "**x > 0")
        );
        assert_eq!(
            shape("fn f(n: u8, days: &[i64]) -> bool { days.iter().any(|&d| d > 30) }"),
            owned("days", 1, "d", 0, "d > 30")
        );
        assert_eq!(
            shape(
                "fn f(xs: Vec<u32>) -> Vec<u32> { xs.iter().map(|x| x * 2).collect::<Vec<_>>() }"
            ),
            owned("xs", 0, "x", 1, "x * 2")
        );
        assert_eq!(
            shape("fn f(xs: &Vec<u64>) -> u64 { xs.iter().map(|&p| p * 100).sum() }"),
            owned("xs", 0, "p", 0, "p * 100")
        );
        assert_eq!(
            shape("fn f(xs: &'a mut [u8]) -> bool { xs.iter().all(|&b| b < MAX) }"),
            owned("xs", 0, "b", 0, "b < MAX")
        );
        let filter = tail("fn f(xs: &[i32]) -> usize { xs.iter().filter(|x| **x > 0).count() }");
        assert_eq!(
            filter
                .as_ref()
                .map(|tail| ["x", "*x", "**x", "***x", "y"]
                    .map(|operand| tail.reads_element(operand))),
            Some([true, true, true, false, false])
        );
        assert_eq!(
            filter.as_ref().map(|tail| (
                tail.tail_is("xs.iter().filter(|x|\n  **x > 0).count();"),
                tail.tail_is("xs.iter().filter(|x| **x > 1).count()"),
                tail.closure_body_is("**x  > 0"),
                tail.closure_body_is("x > 0"),
            )),
            Some((true, false, true, false))
        );
    }

    #[test]
    fn refuses_shapes_outside_the_spec() {
        for body in [
            // Not one of the listed adaptors.
            "fn f(xs: &[i32]) -> Option<&i32> { xs.iter().find(|&&x| x > 0) }",
            "fn f(xs: &[i32]) -> i32 { xs.iter().fold(0, |a, &x| a + x) }",
            "fn f(xs: &[i32]) -> usize { xs.iter().take_while(|x| **x > 0).count() }",
            "fn f(xs: &[i32]) -> Option<usize> { xs.iter().position(|&x| x > 0) }",
            // A nested closure, `&&`, `||`, a branch or a macro in the body.
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| [1].iter().any(|&y| y == x)).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| x > 0 && x < 9).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| x > 0 || x < -9).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| if x > 0 { true } else { false }).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| matches!(x, 1)).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| { x > 0 }).count() }",
            // A captured local or parameter.
            "fn f(xs: &[i32], limit: i32) -> usize { xs.iter().filter(|&&x| x > limit).count() }",
            // A receiver other than a slice parameter, or a statement before.
            "fn f(xs: &[i32]) -> usize { xs.to_vec().iter().filter(|&&x| x > 0).count() }",
            "fn f(xs: &Coll) -> usize { xs.iter().filter(|&&x| x > 0).count() }",
            "fn f(xs: &[i32]) -> usize { let xs = &xs[1..]; xs.iter().filter(|&&x| x > 0).count() }",
            "fn f(xs: impl Iterator<Item = i32>) -> usize { xs.filter(|&x| x > 0).count() }",
            // A destructuring, typed or `mut` pattern, a `move` closure.
            "fn f(xs: &[(i32, i32)]) -> usize { xs.iter().filter(|&&(a, _)| a > 0).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|x: &&i32| **x > 0).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(move |&&x| x > 0).count() }",
            // More `&` than the adaptor hands.
            "fn f(xs: &[i32]) -> bool { xs.iter().any(|&&x| x > 0) }",
            // A terminal with arguments, or another terminal.
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| x > 0).count(1) }",
            "fn f(xs: &[i32]) -> Vec<i32> { xs.iter().map(|&x| x * 2).collect(v) }",
            "fn f(xs: &[i32]) -> Option<i32> { xs.iter().map(|&x| x * 2).max() }",
            // Not the slice's own `iter`.
            "fn f(xs: Vec<i32>) -> usize { xs.into_iter().filter(|&x| x > 0).count() }",
            "fn f(xs: Vec<i32>) -> usize { xs.iter_mut().filter(|x| **x > 0).count() }",
        ] {
            assert_eq!(tail(body), None, "{body}");
        }
    }

    /// Only adaptor/terminal pairs that keep every closure value in the
    /// owner's result are admitted.
    #[test]
    fn admits_only_consumers_that_keep_every_closure_value() {
        for body in [
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| x > 0).count() }",
            "fn f(xs: &[i32]) -> Vec<&i32> { xs.iter().filter(|&&x| x > 0).collect() }",
            "fn f(xs: &[i32]) -> Vec<bool> { xs.iter().map(|&x| x > 0).collect() }",
            "fn f(xs: &[i32]) -> Vec<i32> { xs.iter().map(|&x| x * 2).collect::<Vec<i32>>() }",
            "fn f(xs: &[i32]) -> i32 { xs.iter().map(|&x| x * 2 + 1).sum() }",
            "fn f(xs: &[i32]) -> i64 { xs.iter().map(|&x| x - 3).sum::<i64>() }",
            "fn f(xs: &[i32]) -> bool { xs.iter().any(|&x| x > 0) }",
            "fn f(xs: &[i32]) -> bool { xs.iter().all(|&x| x > 0) }",
        ] {
            assert!(tail(body).is_some(), "{body}");
        }
        for body in [
            // `map` + `count` ignores the closure value.
            "fn f(xs: &[i32]) -> usize { xs.iter().map(|&x| x > 0).count() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().map(|&x| x * 2).count() }",
            // A set loses order and multiplicity, by turbofish or return type.
            "fn f(xs: &[i32]) -> HashSet<bool> { xs.iter().map(|&x| x > 0).collect() }",
            "fn f(xs: &[i32]) -> BTreeSet<bool> { xs.iter().map(|&x| x > 0).collect() }",
            "fn f(xs: &[i32]) -> usize { xs.iter().map(|&x| x > 0).collect::<HashSet<_>>() }",
            "fn f(xs: &[i32]) -> HashSet<&i32> { xs.iter().filter(|&&x| x > 0).collect() }",
            "fn f(xs: &[i32]) -> std::collections::BTreeSet<&i32> { xs.iter().filter(|&&x| x > 0).collect() }",
            // A collect target ripr cannot see, or an alias.
            "fn f(xs: &[i32]) -> impl Sized { xs.iter().map(|&x| x > 0).collect::<_>() }",
            "fn f(xs: &[i32]) -> Bools { xs.iter().map(|&x| x > 0).collect() }",
            // `filter` + `sum`: a kept or dropped additive identity sums alike.
            "fn f(xs: &[i32]) -> i32 { xs.iter().filter(|&&x| x > 0).sum() }",
            "fn f(xs: &[i32]) -> i32 { xs.iter().filter(|&&x| x >= 0).sum::<i32>() }",
            // `map` + `sum` over a comparison, a negation or a cast.
            "fn f(xs: &[i32]) -> u32 { xs.iter().map(|&x| (x > 0) as u32).sum() }",
            "fn f(xs: &[i32]) -> u32 { xs.iter().map(|&x| x as u32).sum() }",
            "fn f(xs: &[i32]) -> i32 { xs.iter().map(|&x| x == 0).sum() }",
            // A lazy iterator, or a terminal after `any` / `all`.
            "fn f(xs: &[i32]) -> impl Iterator { xs.iter().filter(|&&x| x > 0) }",
            "fn f(xs: &[i32]) -> impl Iterator { xs.iter().map(|&x| x * 2) }",
            "fn f(xs: &[i32]) -> usize { xs.iter().any(|&x| x > 0).count() }",
            // `filter` + `count` with a turbofish is not the plain terminal.
            "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| x > 0).count::<usize>() }",
        ] {
            assert_eq!(tail(body), None, "{body}");
        }
    }

    #[test]
    fn short_circuit_runs_reach_the_boundary_only_when_it_decides() {
        let reaches = |body: &str, run: &[&str], boundary: &str| {
            let run = run
                .iter()
                .map(|element| element.to_string())
                .collect::<Vec<_>>();
            tail(body).map(|tail| tail.run_reaches_boundary(&run, boundary))
        };
        let filter = "fn f(xs: &[i32]) -> usize { xs.iter().filter(|&&x| x > 0).count() }";
        assert_eq!(reaches(filter, &["5", "0"], "0"), Some(true));
        assert_eq!(reaches(filter, &["5", "1_0"], "10"), Some(true));
        assert_eq!(reaches(filter, &["5", "7"], "0"), Some(false));
        let any = "fn f(xs: &[i32]) -> bool { xs.iter().any(|&d| d > 30) }";
        assert_eq!(reaches(any, &["30", "2"], "30"), Some(true));
        assert_eq!(reaches(any, &["31", "30"], "30"), Some(false));
        assert_eq!(reaches(any, &["30", "31"], "30"), Some(false));
        // The binding on the right: `30 < d` is false for 2.
        let reversed = "fn f(xs: &[i32]) -> bool { xs.iter().any(|&d| 30 < d) }";
        assert_eq!(reaches(reversed, &["2", "30"], "30"), Some(true));
        let all = "fn f(xs: &[i32]) -> bool { xs.iter().all(|&d| d >= 30) }";
        assert_eq!(reaches(all, &["40", "30"], "30"), Some(true));
        assert_eq!(reaches(all, &["2", "30"], "30"), Some(false));
        // An element the comparison cannot evaluate counts as deciding.
        assert_eq!(reaches(any, &["30", "2.5"], "30"), Some(false));
        let float = "fn f(xs: &[f64]) -> bool { xs.iter().any(|&d| d > 1.5) }";
        assert_eq!(reaches(float, &["1.5", "0.5"], "1.5"), Some(true));
    }

    fn test_body(start_line: usize, body: &str) -> TestSummary {
        TestSummary {
            name: "checks".to_string(),
            file: std::path::PathBuf::from("src/lib.rs"),
            start_line,
            end_line: start_line + body.lines().count(),
            body: body.to_string().into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    /// Rule 2 reads a borrowed helper assertion's argument at each of the
    /// test's eager helper calls.
    #[test]
    fn helper_parameter_runs_are_read_at_each_helper_call() {
        let test = test_body(
            10,
            "fn checks() {\n    check(&[31], true);\n    check(&[], false);\n    let v = [3];\n    check(&v, true);\n}",
        );
        let loan = |call_lines: Vec<usize>| HelperLoan {
            name: "check".to_string(),
            helper: "fn check(days: &[i64], want: bool) { assert_eq!(any_overdue(days), want); }"
                .to_string(),
            parameters: vec!["days".to_string(), "want".to_string()],
            call_lines,
        };
        let runs = |call_lines: Vec<usize>, argument: &str| {
            argument_sequences(&test, Some(&loan(call_lines)), argument)
        };
        assert_eq!(
            runs(vec![11, 12], "days"),
            Some(vec![vec!["31".to_string()], Vec::new()])
        );
        // The `&v` call is unreadable, so every run is.
        assert_eq!(runs(vec![11, 12, 14], "days"), None);
        // No eager call, or a name that is not the helper's parameter.
        assert_eq!(runs(Vec::new(), "days"), None);
        assert_eq!(runs(vec![11], "other"), None);
        let pinned = |call_lines: Vec<usize>| {
            pinned_call_runs_closure(
                &test,
                Some(&loan(call_lines)),
                "any_overdue(days)",
                "any_overdue",
                0,
            )
        };
        assert!(pinned(vec![11, 12]));
        assert!(!pinned(vec![12]));
        assert!(!pinned(vec![11, 14]));
    }

    #[test]
    fn literal_sequences_read_only_literal_elements() {
        assert_eq!(
            literal_sequence("&[-1, 0, 3, 7]"),
            Some(vec!["-1".into(), "0".into(), "3".into(), "7".into()])
        );
        assert_eq!(literal_sequence("[31]"), Some(vec!["31".into()]));
        assert_eq!(
            literal_sequence("vec![1.5, 2]"),
            Some(vec!["1.5".into(), "2".into()])
        );
        assert_eq!(
            literal_sequence("&vec![true, 'a']"),
            Some(vec!["true".into(), "'a'".into()])
        );
        assert_eq!(literal_sequence("&[]"), Some(Vec::new()));
        assert_eq!(literal_sequence("vec![]"), Some(Vec::new()));
        for unreadable in [
            "&v",
            "&ages",
            "vec![40; n]",
            "&[0; 4]",
            "&[1, n]",
            "&[1 + 1]",
            "&[3u32]",
            "&[f(1)]",
            "&[\"a\"]",
            "0..4",
            "&[1, 2][..]",
            "&[.5]",
            "&[_1]",
        ] {
            assert_eq!(literal_sequence(unreadable), None, "{unreadable}");
        }
    }
}
