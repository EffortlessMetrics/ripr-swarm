//! Same-module bare-call facts for Python function and method owners.
//!
//! Records the names an owner's body calls as bare `name(` calls, so a test
//! that calls a same-module caller of a changed helper can be related to it
//! with a weak `helper_owner_call` relation (#4568). Attribute calls, names the
//! body rebinds, parameters, calls inside comments or strings, and nested
//! definitions' own names are not edges. Facts are textual and advisory: a
//! missing edge only withholds a weak relation, never grants an oracle.
//!
//! The text is the owner's whole span, so a call inside a nested `def` or
//! lambda counts as an edge of the enclosing owner (an over-approximation;
//! the relation it feeds stays weak). Call-site shape is shared with
//! `contains_call_name` through `is_bare_call_site`. The route-helper
//! relation (#14150) claims the handler itself calls the helper, so it reads
//! [`own_scope_callees`] instead: the same call-site reading over the
//! handler's own scope, with the scopes it runs only by calling or iterating
//! them removed by the extents of the parse the adapter already made, and with
//! the names that scope binds read from the same parse.

use rustpython_parser::ast::{Comprehension, Expr, Ranged, Stmt};
use rustpython_parser::text_size::{TextRange, TextSize};

use super::PythonParameter;
use super::module_constants::{walk_arguments, walk_expr};
use super::related_tests::{is_bare_call_site, is_python_identifier_char, text_binds_name};

/// Most distinct callee names kept per owner, so a generated body cannot make
/// the fact unbounded.
const MAX_CALLEES: usize = 64;

const NON_CALL_KEYWORDS: &[&str] = &[
    "if", "elif", "while", "for", "return", "and", "or", "not", "in", "is", "assert", "yield",
    "await", "lambda", "with", "except", "raise", "else", "del", "print", "match", "case",
];

/// Sorted, de-duplicated names `owner_text` calls as bare `name(` calls.
pub(super) fn bare_callees(owner_text: &str, parameters: &[PythonParameter]) -> Vec<String> {
    bare_callees_in(owner_text, parameters, &mut |name| {
        text_binds_name(owner_text, name)
    })
}

/// [`bare_callees`] with the call sites read from `call_text` and `bound`
/// deciding, for each name, that something binds it and it is not a call of
/// what the name was bound to before.
fn bare_callees_in(
    call_text: &str,
    parameters: &[PythonParameter],
    bound: &mut impl FnMut(&str) -> bool,
) -> Vec<String> {
    let mut callees: Vec<String> = Vec::new();
    for (open, _) in call_text.match_indices('(') {
        let prefix = &call_text[..open];
        let Some(start) = prefix
            .char_indices()
            .rev()
            .take_while(|(_, ch)| is_python_identifier_char(*ch))
            .last()
            .map(|(index, _)| index)
        else {
            continue;
        };
        let name = &prefix[start..];
        if name.chars().next().is_none_or(|ch| ch.is_ascii_digit())
            || NON_CALL_KEYWORDS.contains(&name)
            || callees.iter().any(|existing| existing == name)
            || parameters.iter().any(|parameter| parameter.name == name)
            || !is_bare_call_site(call_text, start)
            || bound(name)
        {
            continue;
        }
        callees.push(name.to_string());
        if callees.len() >= MAX_CALLEES {
            break;
        }
    }
    callees.sort();
    callees
}

/// Sorted, de-duplicated names the handler's own scope calls as bare `name(`
/// calls (#14150). The route-helper relation reads this, because it claims
/// the handler itself calls the helper; the relations that fed
/// [`bare_callees`] before keep that reading.
///
/// `body` is the handler's statement list from the parse of `source`, and
/// what is read is what runs while the handler runs. Python defers three
/// things to a later call or iteration, and each is blanked out by its
/// parsed extent: the block of a nested function, whose decorators,
/// defaults and annotations do run where the handler runs and are read; the
/// body of a `lambda`; and the element, targets, conditions and later
/// iterables of a generator expression, whose first iterable is evaluated
/// where it is written and is read. A list, set or dict display runs at
/// once, in a scope of its own: it stays, and only each call in it to a
/// name that is one of its targets is blanked, which is a call of that
/// local and not of the module helper. A class is blanked whole: its block
/// runs at once, but in a namespace of its own, which is not read. The
/// prune withholds, never grants. The extents come from the one parse the
/// adapter made, so the spelling of a line, its indentation and an `await`
/// do not matter.
///
/// A name is not a call when the handler's own scope binds it, read from the
/// same parse ([`own_scope_binds`]): a nested `def is_adult` shadows an
/// imported `is_adult`, and the handler's own `is_adult(30)` then calls that
/// one. The call site itself is read by the shared reader, [`bare_callees`],
/// so a name and its parenthesis must be adjacent.
pub(super) fn own_scope_callees(
    source: &str,
    body: &[Stmt],
    parameters: &[PythonParameter],
) -> Vec<String> {
    let Some(span) = span_of(body) else {
        return Vec::new();
    };
    let start = usize::from(read_start(span, body));
    let Some(text) = source.get(start..usize::from(span.end())) else {
        return Vec::new();
    };
    let mut bytes = text.as_bytes().to_vec();
    // Spaces for the scope's own bytes; line ends stay, so the call-site
    // reader still finds the handler's lines where they were.
    let blank = |bytes: &mut Vec<u8>, range: TextRange| {
        let from = usize::from(range.start()).saturating_sub(start);
        let to = usize::from(range.end()).saturating_sub(start);
        if let Some(scope) = bytes.get_mut(from..to) {
            for byte in scope {
                if *byte != b'\n' && *byte != b'\r' {
                    *byte = b' ';
                }
            }
        }
    };
    let blank_local_calls = |bytes: &mut Vec<u8>,
                             parts: Vec<TextRange>,
                             generators: &[Comprehension]| {
        let mut names: Vec<String> = Vec::new();
        for generator in generators {
            bind_target(&generator.target, &mut |name| names.push(name.to_owned()));
        }
        for part in parts {
            let Some(text) = source.get(usize::from(part.start())..usize::from(part.end())) else {
                continue;
            };
            for (from, to) in calls_of_names(text, &names) {
                blank(
                    bytes,
                    TextRange::new(
                        part.start() + TextSize::from(from as u32),
                        part.start() + TextSize::from(to as u32),
                    ),
                );
            }
        }
    };
    own_scope_expressions(body, &mut |expr| match expr {
        Expr::Lambda(lambda) => blank(&mut bytes, lambda.body.range()),
        Expr::GeneratorExp(expression) => {
            blank(&mut bytes, expression.elt.range());
            for (index, generator) in expression.generators.iter().enumerate() {
                blank(&mut bytes, generator.target.range());
                for condition in &generator.ifs {
                    blank(&mut bytes, condition.range());
                }
                // Python evaluates the first iterable where the expression
                // is written, and the others when the generator runs.
                if index > 0 {
                    blank(&mut bytes, generator.iter.range());
                }
            }
        }
        Expr::ListComp(list) => blank_local_calls(
            &mut bytes,
            comprehension_parts(&list.elt, None, &list.generators),
            &list.generators,
        ),
        Expr::SetComp(set) => blank_local_calls(
            &mut bytes,
            comprehension_parts(&set.elt, None, &set.generators),
            &set.generators,
        ),
        Expr::DictComp(dict) => blank_local_calls(
            &mut bytes,
            comprehension_parts(&dict.key, Some(&dict.value), &dict.generators),
            &dict.generators,
        ),
        _ => {}
    });
    // A nested function's block runs only when it is called; its
    // decorators, defaults and annotations run where the handler runs.
    // The block is blanked from its first statement's first decorator: a
    // decorated definition there has an extent that starts at `def`, and
    // its decorators run only when the block runs.
    let blank_block = |bytes: &mut Vec<u8>, block: &[Stmt]| {
        if let Some(span) = span_of(block) {
            blank(bytes, TextRange::new(read_start(span, block), span.end()));
        }
    };
    super::module_constants::visit_statements(body, &mut |stmt| match stmt {
        Stmt::FunctionDef(function) => blank_block(&mut bytes, &function.body),
        Stmt::AsyncFunctionDef(function) => blank_block(&mut bytes, &function.body),
        Stmt::ClassDef(class) => blank(&mut bytes, class.range()),
        _ => {}
    });
    let Some(own) = String::from_utf8(bytes).ok() else {
        return Vec::new();
    };
    bare_callees_in(&own, parameters, &mut |name| own_scope_binds(body, name))
}

/// The extent of a statement list, from its first statement to its last.
fn span_of(statements: &[Stmt]) -> Option<TextRange> {
    let (first, last) = (statements.first()?, statements.last()?);
    Some(TextRange::new(first.range().start(), last.range().end()))
}

/// The parts of an eager comprehension that run in its own scope: its
/// element (a key and a value for a dict display), its targets, its
/// conditions and its later iterables. Its first iterable is evaluated
/// where the display is written and is left out.
fn comprehension_parts(
    first: &Expr,
    second: Option<&Expr>,
    generators: &[Comprehension],
) -> Vec<TextRange> {
    let mut parts = vec![first.range()];
    parts.extend(second.map(Ranged::range));
    for (index, generator) in generators.iter().enumerate() {
        parts.push(generator.target.range());
        parts.extend(generator.ifs.iter().map(Ranged::range));
        if index > 0 {
            parts.push(generator.iter.range());
        }
    }
    parts
}

/// Where `text` calls one of `names` as a bare `name(`, as `(from, to)`
/// byte offsets of the name. A name behind a dot is an attribute, not a
/// bare call.
fn calls_of_names(text: &str, names: &[String]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut run: Option<(usize, String)> = None;
    for (index, ch) in text.char_indices() {
        if is_python_identifier_char(ch) {
            run.get_or_insert((index, String::new())).1.push(ch);
            continue;
        }
        if let Some((from, spelled)) = run.take()
            && names.contains(&spelled)
            && ch == '('
            && !text[..from].ends_with('.')
        {
            found.push((from, index));
        }
    }
    found
}

/// Where a statement list is read from: its first statement's first
/// decorator, which lies before that statement's own extent.
fn read_start(span: TextRange, statements: &[Stmt]) -> TextSize {
    let decorators = match statements.first() {
        Some(Stmt::FunctionDef(function)) => &function.decorator_list,
        Some(Stmt::AsyncFunctionDef(function)) => &function.decorator_list,
        Some(Stmt::ClassDef(class)) => &class.decorator_list,
        _ => return span.start(),
    };
    decorators
        .first()
        .map_or(span.start(), |decorator| decorator.range().start())
}

/// Whether the scope that holds `statements` binds `name`. Read from the
/// parse, so every binding form counts, among them the ones a line-by-line
/// reader misses: an annotated, tuple or chained assignment, a `match`
/// capture pattern, a walrus and an `except ... as name`. A binding that
/// exists only inside a nested scope does not count, because it shadows
/// nothing where the scope's own statements run; the one exception withholds
/// only, a walrus written inside a `lambda`. A comprehension's target and a
/// `global` declaration bind nothing in this scope, so they do not count.
fn own_scope_binds(statements: &[Stmt], name: &str) -> bool {
    let mut found = false;
    own_scope_bindings(statements, &mut |bound| found |= bound == name);
    found
}

/// Every name the scope that holds `statements` binds, through the same
/// statement walk as [`own_scope_expressions`]. A nested `def`, `async def`
/// and `class` lends its own name only. Every statement kind is named.
fn own_scope_bindings(statements: &[Stmt], out: &mut impl FnMut(&str)) {
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => out(function.name.as_str()),
            Stmt::AsyncFunctionDef(function) => out(function.name.as_str()),
            Stmt::ClassDef(class) => out(class.name.as_str()),
            Stmt::Delete(stmt) => stmt
                .targets
                .iter()
                .for_each(|target| bind_target(target, out)),
            Stmt::Assign(assign) => assign
                .targets
                .iter()
                .for_each(|target| bind_target(target, out)),
            Stmt::TypeAlias(alias) => bind_target(&alias.name, out),
            Stmt::AugAssign(assign) => bind_target(&assign.target, out),
            Stmt::AnnAssign(assign) => bind_target(&assign.target, out),
            Stmt::For(stmt) => {
                bind_target(&stmt.target, out);
                own_scope_bindings(&stmt.body, out);
                own_scope_bindings(&stmt.orelse, out);
            }
            Stmt::AsyncFor(stmt) => {
                bind_target(&stmt.target, out);
                own_scope_bindings(&stmt.body, out);
                own_scope_bindings(&stmt.orelse, out);
            }
            Stmt::While(stmt) => {
                own_scope_bindings(&stmt.body, out);
                own_scope_bindings(&stmt.orelse, out);
            }
            Stmt::If(stmt) => {
                own_scope_bindings(&stmt.body, out);
                own_scope_bindings(&stmt.orelse, out);
            }
            Stmt::With(stmt) => with_bindings(&stmt.items, &stmt.body, out),
            Stmt::AsyncWith(stmt) => with_bindings(&stmt.items, &stmt.body, out),
            Stmt::Match(stmt) => {
                for case in &stmt.cases {
                    bind_pattern(&case.pattern, out);
                    own_scope_bindings(&case.body, out);
                }
            }
            Stmt::Try(stmt) => try_bindings(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                out,
            ),
            Stmt::TryStar(stmt) => try_bindings(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                out,
            ),
            Stmt::Import(stmt) => {
                for alias in &stmt.names {
                    match &alias.asname {
                        Some(asname) => out(asname.as_str()),
                        // `import pkg.mod` binds `pkg`.
                        None => out(alias.name.as_str().split('.').next().unwrap_or("")),
                    }
                }
            }
            Stmt::ImportFrom(stmt) => {
                for alias in &stmt.names {
                    out(alias.asname.as_ref().unwrap_or(&alias.name).as_str());
                }
            }
            // `nonlocal` names a binding of an enclosing function, which is
            // not the module's. `global` names none: the name stays the
            // module's, so it binds nothing here.
            Stmt::Nonlocal(stmt) => stmt.names.iter().for_each(|name| out(name.as_str())),
            Stmt::Global(_)
            | Stmt::Return(_)
            | Stmt::Raise(_)
            | Stmt::Assert(_)
            | Stmt::Expr(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_) => {}
        }
    }
    // What the scope's own expressions bind: a walrus, also one inside a
    // comprehension, which Python binds in the scope that holds it. A
    // comprehension's own target binds in the comprehension's scope, in
    // every form, so it is not read here.
    own_scope_expressions(statements, &mut |expr| {
        if let Expr::NamedExpr(named) = expr {
            bind_target(&named.target, out);
        }
    });
}

/// [`own_scope_bindings`] for a `with` statement, whose `as` targets bind.
fn with_bindings(
    items: &[rustpython_parser::ast::WithItem],
    body: &[Stmt],
    out: &mut impl FnMut(&str),
) {
    for item in items {
        if let Some(vars) = &item.optional_vars {
            bind_target(vars, out);
        }
    }
    own_scope_bindings(body, out);
}

/// [`own_scope_bindings`] for a `try` statement, whose handlers'
/// `except ... as name` bind.
fn try_bindings(
    body: &[Stmt],
    handlers: &[rustpython_parser::ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
    out: &mut impl FnMut(&str),
) {
    own_scope_bindings(body, out);
    for handler in handlers {
        let rustpython_parser::ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(name) = &handler.name {
            out(name.as_str());
        }
        own_scope_bindings(&handler.body, out);
    }
    own_scope_bindings(orelse, out);
    own_scope_bindings(finalbody, out);
}

/// The names an assignment or loop target binds: a name, and each name of a
/// tuple, list or starred target. An attribute or subscript target binds none.
fn bind_target(target: &Expr, out: &mut impl FnMut(&str)) {
    match target {
        Expr::Name(name) => out(name.id.as_str()),
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| bind_target(elt, out)),
        Expr::List(list) => list.elts.iter().for_each(|elt| bind_target(elt, out)),
        Expr::Starred(starred) => bind_target(&starred.value, out),
        _ => {}
    }
}

/// The names a `match` pattern captures into the scope that holds the
/// `match` statement. A value, singleton or class-name pattern captures none;
/// every `as`, star, `**rest` and nested pattern does.
fn bind_pattern(pattern: &rustpython_parser::ast::Pattern, out: &mut impl FnMut(&str)) {
    use rustpython_parser::ast::Pattern;
    match pattern {
        Pattern::MatchValue(_) | Pattern::MatchSingleton(_) => {}
        Pattern::MatchSequence(sequence) => sequence
            .patterns
            .iter()
            .for_each(|inner| bind_pattern(inner, out)),
        Pattern::MatchMapping(mapping) => {
            mapping
                .patterns
                .iter()
                .for_each(|inner| bind_pattern(inner, out));
            if let Some(rest) = &mapping.rest {
                out(rest.as_str());
            }
        }
        Pattern::MatchClass(class) => {
            class
                .patterns
                .iter()
                .chain(&class.kwd_patterns)
                .for_each(|inner| bind_pattern(inner, out));
        }
        Pattern::MatchStar(star) => {
            if let Some(name) = &star.name {
                out(name.as_str());
            }
        }
        Pattern::MatchAs(capture) => {
            if let Some(inner) = &capture.pattern {
                bind_pattern(inner, out);
            }
            if let Some(name) = &capture.name {
                out(name.as_str());
            }
        }
        Pattern::MatchOr(alternatives) => alternatives
            .patterns
            .iter()
            .for_each(|inner| bind_pattern(inner, out)),
    }
}

/// Every expression of the scope that holds `statements`: those of each
/// statement and of the statements nested in its blocks. A nested `def`,
/// `async def` and `class` lends only what runs where it is defined (its
/// decorators, defaults, annotations and bases); its block is its own scope.
/// Every statement kind is named, so a new one cannot pass unvisited.
fn own_scope_expressions(statements: &[Stmt], visit: &mut impl FnMut(&Expr)) {
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => {
                for decorator in &function.decorator_list {
                    walk_expr(decorator, visit);
                }
                walk_arguments(&function.args, visit);
                if let Some(returns) = &function.returns {
                    walk_expr(returns, visit);
                }
            }
            Stmt::AsyncFunctionDef(function) => {
                for decorator in &function.decorator_list {
                    walk_expr(decorator, visit);
                }
                walk_arguments(&function.args, visit);
                if let Some(returns) = &function.returns {
                    walk_expr(returns, visit);
                }
            }
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    walk_expr(decorator, visit);
                }
                for base in &class.bases {
                    walk_expr(base, visit);
                }
                for keyword in &class.keywords {
                    walk_expr(&keyword.value, visit);
                }
            }
            Stmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    walk_expr(value, visit);
                }
            }
            Stmt::Delete(stmt) => {
                for target in &stmt.targets {
                    walk_expr(target, visit);
                }
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    walk_expr(target, visit);
                }
                walk_expr(&assign.value, visit);
            }
            Stmt::TypeAlias(alias) => {
                walk_expr(&alias.name, visit);
                walk_expr(&alias.value, visit);
            }
            Stmt::AugAssign(assign) => {
                walk_expr(&assign.target, visit);
                walk_expr(&assign.value, visit);
            }
            Stmt::AnnAssign(assign) => {
                walk_expr(&assign.target, visit);
                walk_expr(&assign.annotation, visit);
                if let Some(value) = &assign.value {
                    walk_expr(value, visit);
                }
            }
            Stmt::For(stmt) => {
                walk_expr(&stmt.target, visit);
                walk_expr(&stmt.iter, visit);
                own_scope_expressions(&stmt.body, visit);
                own_scope_expressions(&stmt.orelse, visit);
            }
            Stmt::AsyncFor(stmt) => {
                walk_expr(&stmt.target, visit);
                walk_expr(&stmt.iter, visit);
                own_scope_expressions(&stmt.body, visit);
                own_scope_expressions(&stmt.orelse, visit);
            }
            Stmt::While(stmt) => {
                walk_expr(&stmt.test, visit);
                own_scope_expressions(&stmt.body, visit);
                own_scope_expressions(&stmt.orelse, visit);
            }
            Stmt::If(stmt) => {
                walk_expr(&stmt.test, visit);
                own_scope_expressions(&stmt.body, visit);
                own_scope_expressions(&stmt.orelse, visit);
            }
            Stmt::With(stmt) => with_items(&stmt.items, &stmt.body, visit),
            Stmt::AsyncWith(stmt) => with_items(&stmt.items, &stmt.body, visit),
            Stmt::Match(stmt) => {
                walk_expr(&stmt.subject, visit);
                for case in &stmt.cases {
                    if let Some(guard) = &case.guard {
                        walk_expr(guard, visit);
                    }
                    own_scope_expressions(&case.body, visit);
                }
            }
            Stmt::Raise(stmt) => {
                if let Some(exc) = &stmt.exc {
                    walk_expr(exc, visit);
                }
                if let Some(cause) = &stmt.cause {
                    walk_expr(cause, visit);
                }
            }
            Stmt::Try(stmt) => try_block(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                visit,
            ),
            Stmt::TryStar(stmt) => try_block(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                visit,
            ),
            Stmt::Assert(stmt) => {
                walk_expr(&stmt.test, visit);
                if let Some(msg) = &stmt.msg {
                    walk_expr(msg, visit);
                }
            }
            Stmt::Expr(stmt) => walk_expr(&stmt.value, visit),
            Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::Global(_)
            | Stmt::Nonlocal(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_) => {}
        }
    }
}

/// [`own_scope_expressions`] for a `with` statement.
fn with_items(
    items: &[rustpython_parser::ast::WithItem],
    body: &[Stmt],
    visit: &mut impl FnMut(&Expr),
) {
    for item in items {
        walk_expr(&item.context_expr, visit);
        if let Some(vars) = &item.optional_vars {
            walk_expr(vars, visit);
        }
    }
    own_scope_expressions(body, visit);
}

/// [`own_scope_expressions`] for a `try` statement, with each handler's
/// exception type, which is an expression.
fn try_block(
    body: &[Stmt],
    handlers: &[rustpython_parser::ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
    visit: &mut impl FnMut(&Expr),
) {
    own_scope_expressions(body, visit);
    for handler in handlers {
        let rustpython_parser::ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(exception) = &handler.type_ {
            walk_expr(exception, visit);
        }
        own_scope_expressions(&handler.body, visit);
    }
    own_scope_expressions(orelse, visit);
    own_scope_expressions(finalbody, visit);
}

/// Whether `owner_text` calls `name` anywhere other than its own `def name(`
/// header: `name(`, `self.name(` or `mod.name(`. Textual, so a mention in a
/// string or comment also counts; the claim it guards fails closed.
pub(super) fn calls_own_name(owner_text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    owner_text.match_indices(name).any(|(start, _)| {
        let before = owner_text[..start].chars().next_back();
        let rest = &owner_text[start + name.len()..];
        let boundary_before = !before.is_some_and(is_python_identifier_char);
        let boundary_after = !rest.chars().next().is_some_and(is_python_identifier_char);
        boundary_before
            && boundary_after
            && rest.trim_start().starts_with('(')
            && !owner_text[..start].trim_end().ends_with("def")
    })
}

#[cfg(test)]
mod own_scope_tests {
    use super::*;
    use rustpython_parser::ast::Mod;
    use rustpython_parser::{Mode, parse};

    /// The callees of the handler's own scope: `source` ends with the
    /// handler's definition.
    fn own_scope(source: &str) -> Vec<String> {
        let Ok(Mod::Module(module)) = parse(source, Mode::Module, "<own-scope-test>") else {
            return vec!["<did not parse>".to_owned()];
        };
        let Some(Stmt::FunctionDef(handler)) = module.body.last() else {
            return vec!["<no handler>".to_owned()];
        };
        own_scope_callees(source, &handler.body, &[])
    }

    #[test]
    fn a_binding_inside_a_nested_scope_does_not_hide_the_handlers_own_call() {
        assert_eq!(
            own_scope(
                "def age():\n    def unused():\n        for is_adult in []:\n            pass\n        return is_adult\n    return is_adult(30)\n"
            ),
            ["is_adult"]
        );
    }

    #[test]
    fn a_name_the_scope_binds_is_not_a_callee() {
        for (label, body) in [
            (
                "a nested def",
                "    def is_adult(value):\n        return True\n",
            ),
            (
                "a match capture",
                "    match {\"checker\": 1}:\n        case {\"checker\": is_adult}:\n            pass\n",
            ),
            ("an annotated assignment", "    is_adult: object = None\n"),
            ("a tuple target", "    (mode, is_adult) = (1, None)\n"),
            ("a chained assignment", "    checker = is_adult = None\n"),
            (
                "an except clause",
                "    try:\n        pass\n    except ValueError as is_adult:\n        pass\n",
            ),
            ("a walrus", "    if (is_adult := True):\n        pass\n"),
        ] {
            assert!(
                own_scope(&format!("def age():\n{body}    return is_adult(30)\n")).is_empty(),
                "{label}"
            );
        }
    }

    #[test]
    fn a_nested_functions_header_is_read_and_its_block_is_not() {
        // The decorators, defaults and annotations of a nested definition
        // run while the handler runs; its block runs only when it is called.
        let inner = "    @adult(30)\n    def inner(check=is_ok(31), other: object = None) -> object:\n        return is_adult(32)\n";
        let first = format!("def age():\n{inner}    return True\n");
        let after_a_statement = format!("def age():\n    value = 1\n{inner}    return True\n");
        for (label, source) in [
            ("as the first statement", first),
            ("after a statement", after_a_statement),
        ] {
            assert_eq!(own_scope(&source), ["adult", "is_ok"], "{label}");
        }
    }

    #[test]
    fn a_class_is_not_read_even_as_the_first_statement() {
        // A class's block runs at once, but in a namespace of its own.
        let source = "def age():\n    @adult(30)\n    class Inner:\n        other = is_ok(31)\n    return Inner\n";
        assert_eq!(own_scope(source), ["adult"]);
    }

    #[test]
    fn a_lambda_is_read_only_in_its_parameters_defaults() {
        let source =
            "def age():\n    check = lambda value=is_ok(31): adult(value)\n    return check\n";
        assert_eq!(own_scope(source), ["is_ok"]);
    }

    #[test]
    fn a_generator_expression_is_read_only_in_its_first_iterable() {
        // Python evaluates a generator expression's first iterable where the
        // expression is written; the rest runs when something iterates it.
        let source = "def age():\n    rows = (clean(row) for row in adult(30) if guard(31) for other in watch(32))\n    return rows\n";
        assert_eq!(own_scope(source), ["adult"]);
    }

    #[test]
    fn a_comprehension_target_does_not_bind_the_scope() {
        // In every Python 3 comprehension the target belongs to the
        // comprehension's own scope, so the later call is the module's.
        let source =
            "def age():\n    checks = [is_adult for is_adult in [1]]\n    return is_adult(30)\n";
        assert_eq!(own_scope(source), ["is_adult"]);
    }

    #[test]
    fn a_global_declaration_binds_nothing_and_a_nonlocal_name_is_not_read() {
        // `global is_adult` leaves the name the module's; `nonlocal` names a
        // binding of an enclosing function, which is not the module's.
        let declared = "def age():\n    global is_adult\n    return is_adult(30)\n";
        assert_eq!(own_scope(declared), ["is_adult"]);
        let enclosed = "def age():\n    nonlocal is_adult\n    return is_adult(30)\n";
        assert!(own_scope(enclosed).is_empty());
    }

    #[test]
    fn a_nested_blocks_first_decorator_is_not_the_handlers_call() {
        // The inner definition's extent starts at `def`; its decorator runs
        // only when the block around it runs.
        let inner = "    def unused():\n        @is_adult(1)\n        def inner():\n            return 1\n    return {}\n";
        assert!(own_scope(&format!("def age():\n{inner}")).is_empty());
        let async_inner = "    async def unused():\n        @is_adult(1)\n        class Inner:\n            pass\n    return {}\n";
        assert!(own_scope(&format!("def age():\n{async_inner}")).is_empty());
        // Two levels down, and after a statement of its own.
        let deeper = "    def outer():\n        value = 1\n        def unused():\n            @is_adult(1)\n            def inner():\n                return 1\n    return {}\n";
        assert!(own_scope(&format!("def age():\n{deeper}")).is_empty());
        // The handler's own decorator on its own first statement is its call.
        let own =
            "def age():\n    @is_adult(1)\n    def inner():\n        return 1\n    return {}\n";
        assert_eq!(own_scope(own), ["is_adult"]);
    }

    #[test]
    fn a_call_to_an_eager_comprehensions_target_is_not_the_helpers() {
        // A list, set or dict display runs at once, in a scope of its own:
        // its targets are locals there, in every generator.
        for (label, body) in [
            (
                "a list display",
                "    return [is_adult(x) for is_adult in rules]\n",
            ),
            (
                "a set display",
                "    return {is_adult(x) for is_adult in rules}\n",
            ),
            (
                "a dict key",
                "    return {is_adult(k): v for is_adult, v in rules}\n",
            ),
            (
                "a dict value",
                "    return {k: is_adult(v) for v, is_adult in rules}\n",
            ),
            (
                "a later iterable",
                "    return [y for x in xs for is_adult in [rules] for y in is_adult(x)]\n",
            ),
            (
                "a condition",
                "    return [x for x in xs for is_adult in [rules] if is_adult(x)]\n",
            ),
        ] {
            assert!(
                own_scope(&format!("def age():\n{body}")).is_empty(),
                "{label}"
            );
        }
        // Nested in a generator expression's first iterable, which is
        // evaluated where it is written; the display's own scope stands. The
        // handler's own call of `list` is read; the helper's is not.
        let nested =
            "def age():\n    return list(x for x in [is_adult(y) for is_adult in rules])\n";
        assert_eq!(own_scope(nested), ["list"]);
    }

    #[test]
    fn an_eager_comprehensions_call_to_another_name_is_the_helpers() {
        // The display runs while the handler runs, and nothing local
        // shadows the helper there.
        for (label, body) in [
            (
                "a list display",
                "    return [is_adult(x) for x in rules]\n",
            ),
            (
                "a dict value",
                "    return {k: is_adult(v) for k, v in rules}\n",
            ),
            (
                "its first iterable",
                "    return [x for x in is_adult(rules)]\n",
            ),
            (
                "a later iterable",
                "    return [x for x in xs for y in is_adult(xs)]\n",
            ),
            ("a condition", "    return [x for x in xs if is_adult(x)]\n"),
            (
                "a target named otherwise",
                "    return [checker(x) for checker in rules]\n    return is_adult(30)\n",
            ),
        ] {
            assert_eq!(
                own_scope(&format!("def age():\n{body}")),
                ["is_adult"],
                "{label}"
            );
        }
    }
}
