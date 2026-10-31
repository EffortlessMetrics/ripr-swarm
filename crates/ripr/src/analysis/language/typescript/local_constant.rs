//! Single immutable integer constants a test passes to an owner call
//! (#10707, #10706).
//!
//! `const limit = 50; expect(tier(limit)).toBe("basic")` reaches the same
//! boundary as `tier(50)`, but only the entity-identity route may say so:
//! the value is substituted for the name only when the name provably has
//! exactly one binding and that binding is a plain integer literal. Two
//! scopes resolve here, both on the oxc token stream so comments, strings,
//! templates and regular expressions never pass for code:
//!
//! - the test's own callback: one `const NAME = <integer literal>` declared
//!   directly in the callback block (never in a nested block, loop header,
//!   or parameter list), every other occurrence of the name in the callback a
//!   plain read that follows the declaration;
//! - the test file's module level, through the same rule the owner's module
//!   constants use: one top-level `const NAME = <integer literal>`, every
//!   other occurrence in the file a plain read.
//!
//! Anything the rules do not positively recognize resolves to nothing, and
//! the argument keeps its name: `let`/`var`, a reassigned or updated name,
//! a shadowing parameter, catch, function or inner declaration, a
//! destructuring or object-key spelling, a computed or non-integer
//! initializer, an unresolved import, a second declaration, `eval`/`with`,
//! and a member read (`config.limit`, whose property a test can rewrite).

use super::boundary_input::{
    Tok, integer_literal, is_safe_number_integer, module_is_opaque, occurrence_is_plain_read,
    ts_module_integer_constant,
};
use super::*;
use oxc_parser::config::TokensParserConfig;

/// The integer `name` holds at the test file's module level, as canonical
/// decimal text: the file's one top-level immutable integer `const`, with no
/// other level of the test's enclosing scopes binding the name.
pub(crate) fn test_file_integer_constant(
    test: &TypeScriptTest,
    name: &str,
    workspace_root: Option<&Path>,
) -> Option<String> {
    if !classifier::is_bare_identifier(name) {
        return None;
    }
    // A module-level binding is a file-level scope binding. Any other level
    // only shadows it, and nothing binds the name at all otherwise.
    if !test
        .scope_bindings
        .iter()
        .any(|binding| binding.name == name && binding.file_level)
        || test
            .scope_bindings
            .iter()
            .any(|binding| binding.name == name && !binding.file_level)
    {
        return None;
    }
    let root = workspace_root?;
    let source = String::from_utf8(
        crate::analysis::committed_source::read_source_bytes(root, &test.file).ok()??,
    )
    .ok()?;
    ts_module_integer_constant(&test.file, &source, name).map(|value| value.to_string())
}

pub(crate) enum BodyConstant {
    Resolved(String),
    /// The callback declares or rebinds the name in a way the rules refuse.
    Refused,
    /// The callback never declares the name.
    Undeclared,
}

/// A parse of the callback text: its tokens and every block-level
/// `const NAME = <integer literal>` declarator of the name.
struct ParsedBody {
    source: String,
    tokens: Vec<Tok>,
    /// `(name offset, value, declarator end)` of each such declarator.
    declarations: Vec<(usize, i64, usize)>,
}

thread_local! {
    static BODY_CACHE: std::cell::RefCell<std::collections::HashMap<u64, Option<ParsedBodyOutcome>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[derive(Clone, Copy)]
struct ParsedBodyOutcome(Option<i64>, bool);

/// What the test callback's own text says about `name`.
pub(crate) fn test_body_integer_constant(file: &Path, body: &str, name: &str) -> BodyConstant {
    if !classifier::is_bare_identifier(name) {
        return BodyConstant::Undeclared;
    }
    // Cheap exit: a callback that never spells the name binds nothing.
    if !body.contains(name) {
        return BodyConstant::Undeclared;
    }
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (file, body, name).hash(&mut hasher);
        hasher.finish()
    };
    let cached = BODY_CACHE.with(|cache| cache.borrow().get(&key).copied());
    let outcome = match cached {
        Some(outcome) => outcome,
        None => {
            let outcome = compute_body_constant(file, body, name);
            BODY_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                if cache.len() >= 512 {
                    cache.clear();
                }
                cache.insert(key, outcome);
            });
            outcome
        }
    };
    match outcome {
        // The callback could not be read as code at all: nothing resolves.
        None => BodyConstant::Refused,
        Some(ParsedBodyOutcome(Some(value), _)) => BodyConstant::Resolved(value.to_string()),
        Some(ParsedBodyOutcome(None, true)) => BodyConstant::Refused,
        Some(ParsedBodyOutcome(None, false)) => BodyConstant::Undeclared,
    }
}

fn compute_body_constant(file: &Path, body: &str, name: &str) -> Option<ParsedBodyOutcome> {
    let parsed = parse_body(file, body, name)?;
    Some(resolve_in_body(&parsed, name))
}

/// Read the callback text as code: as written (the registering call, an
/// arrow, or a statement list), parenthesized (`function () {}`), or as the
/// statements of a wrapper function.
fn parse_body(file: &Path, body: &str, name: &str) -> Option<ParsedBody> {
    let variants = [
        body.to_string(),
        format!("({body})"),
        format!("function __ripr_test_body() {{\n{body}\n}}"),
    ];
    for source in variants {
        let name = name.to_string();
        let parsed = parse_on_worker(file, &source, move |file, source, allocator| {
            let ret = Parser::new(allocator, source, source_type_for(file))
                .with_config(TokensParserConfig)
                .parse();
            if ret.panicked || !ret.errors.is_empty() {
                return None;
            }
            let mut declarations = Vec::new();
            collect_block_declarations(&ret.program.body, &name, &mut declarations);
            let tokens: Vec<Tok> = ret
                .tokens
                .iter()
                .map(|token| Tok {
                    kind: token.kind(),
                    start: token.start() as usize,
                    end: token.end() as usize,
                    escaped: token.escaped(),
                })
                .collect();
            Some((tokens, declarations))
        });
        if let Ok(Some((tokens, declarations))) = parsed {
            return Some(ParsedBody {
                source,
                tokens,
                declarations,
            });
        }
    }
    None
}

/// Collect the `const NAME = <integer literal>` declarators that sit
/// directly in the callback's own top-level statement list: the program's
/// statements for a bare statement list, otherwise the block body of the
/// function a top-level registering call (or the text itself) is. Nothing
/// nested is entered: not a function passed to an inner call, not a nested
/// function declaration, block, loop header or parameter list. A same-name
/// declaration in any of those is left to the occurrence rules, which
/// refuse the name.
fn collect_block_declarations(
    statements: &[Statement<'_>],
    name: &str,
    out: &mut Vec<(usize, i64, usize)>,
) {
    for statement in statements {
        match statement {
            Statement::ExpressionStatement(statement) => {
                collect_callback_bodies(&statement.expression, name, out);
            }
            // The wrapper function of the statement-list parse variant.
            Statement::FunctionDeclaration(function) => {
                if let Some(body) = &function.body {
                    collect_direct_declarations(&body.statements, name, out);
                }
            }
            _ => {}
        }
    }
    // A bare statement list declares at the program level.
    collect_direct_declarations(statements, name, out);
}

/// The declarators among `statements` themselves.
fn collect_direct_declarations(
    statements: &[Statement<'_>],
    name: &str,
    out: &mut Vec<(usize, i64, usize)>,
) {
    for statement in statements {
        let Statement::VariableDeclaration(declaration) = statement else {
            continue;
        };
        if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const || declaration.declare {
            continue;
        }
        for declarator in &declaration.declarations {
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };
            if id.name != name {
                continue;
            }
            if let Some(Expression::NumericLiteral(literal)) = &declarator.init
                && let Some(value) = literal
                    .raw
                    .as_ref()
                    .and_then(|raw| integer_literal(raw.as_str()))
                && is_safe_number_integer(value)
            {
                out.push((id.span.start as usize, value, declarator.span.end as usize));
            }
        }
    }
}

/// The callback of the statement's expression: the function itself, or a
/// function passed directly to the registering call. Only the block body of
/// that one function is read.
fn collect_callback_bodies(
    expression: &Expression<'_>,
    name: &str,
    out: &mut Vec<(usize, i64, usize)>,
) {
    match expression {
        Expression::CallExpression(call) => {
            for argument in &call.arguments {
                if let Some(expression) = argument.as_expression() {
                    collect_function_body(expression, name, out);
                }
            }
        }
        other => collect_function_body(other, name, out),
    }
}

fn collect_function_body(
    expression: &Expression<'_>,
    name: &str,
    out: &mut Vec<(usize, i64, usize)>,
) {
    match expression {
        Expression::ParenthesizedExpression(inner) => {
            collect_function_body(&inner.expression, name, out);
        }
        Expression::FunctionExpression(function) => {
            if let Some(body) = &function.body {
                collect_direct_declarations(&body.statements, name, out);
            }
        }
        Expression::ArrowFunctionExpression(arrow) if !arrow.expression => {
            collect_direct_declarations(&arrow.body.statements, name, out);
        }
        _ => {}
    }
}

fn resolve_in_body(parsed: &ParsedBody, name: &str) -> ParsedBodyOutcome {
    let source = parsed.source.as_str();
    let tokens = parsed.tokens.as_slice();
    // `(None, true)`: a binding of the name the rules refuse.
    let refused = ParsedBodyOutcome(None, true);
    if module_is_opaque(source, tokens) {
        return refused;
    }
    let occurrence = |at: usize| {
        let token = &tokens[at];
        token.kind.is_identifier_name() && token.text(source) == name
    };
    if !(0..tokens.len()).any(occurrence) {
        return ParsedBodyOutcome(None, false);
    }
    let [(declaration_at, value, declaration_end)] = parsed.declarations.as_slice() else {
        // The name is spelled but not declared by exactly one recognized
        // block-level `const`: a second declaration, or any other binding
        // form (`let`, parameter, nested const, ...), is a refusal; a bare
        // read is simply not this scope's constant.
        let bound = parsed.declarations.len() > 1
            || (0..tokens.len())
                .any(|at| occurrence(at) && !occurrence_is_plain_read(source, tokens, at));
        return ParsedBodyOutcome(None, bound);
    };
    for at in (0..tokens.len()).filter(|at| occurrence(*at)) {
        if tokens[at].start == *declaration_at {
            continue;
        }
        // Every other occurrence reads the binding after it is initialised.
        if tokens[at].start < *declaration_end || !occurrence_is_plain_read(source, tokens, at) {
            return refused;
        }
    }
    ParsedBodyOutcome(Some(*value), false)
}
