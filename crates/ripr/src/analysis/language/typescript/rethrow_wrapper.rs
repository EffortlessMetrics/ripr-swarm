//! RIPR-SPEC-0243 decision 20: a same-file wrapper that only forwards a
//! failure.
//!
//! `function logged(fn) { try { fn(); } catch (e) { throw e; } }` runs its
//! callback and rethrows whatever it throws, so `logged(() => assert.equal(a, b));`
//! fails the test exactly as the bare assertion does. Rather than model the
//! call, each eligible call is replaced, on its own line, by the callback's
//! expression, and the unchanged extractor runs on that source, keeping every
//! binding, oracle and admission gate in force. A wrapper that swallows,
//! wraps or transforms the failure is not eligible, and its calls stay as
//! written (no related assertions).

use super::suite_factory::code_bytes;
use super::tests_extract::word_occurrences;
use oxc_ast::ast::{Expression, Function, Statement};

/// Calls replaced per file.
const MAX_REPLACEMENTS: usize = 16;

/// The source with each eligible wrapper call replaced by its callback's
/// expression, or `None` when nothing is. A call and its replacement are one
/// line, so line numbers do not move.
pub(crate) fn expand_rethrow_wrappers(program: &[Statement<'_>], source: &str) -> Option<String> {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for stmt in program {
        let Statement::FunctionDeclaration(function) = stmt else {
            continue;
        };
        let Some(name) = rethrow_wrapper_name(function) else {
            continue;
        };
        let declaration = function.span.start as usize..function.span.end as usize;
        let mentions: Vec<usize> = word_occurrences(source, name)
            .filter(|at| !declaration.contains(at))
            .collect();
        let replacements: Vec<(usize, usize, String)> = mentions
            .iter()
            .filter_map(|at| replacement(source, *at, name))
            .collect();
        // Every mention is a call this module replaces: no alias, second
        // declaration, export or call from another position.
        if mentions.is_empty() || replacements.len() != mentions.len() {
            continue;
        }
        if edits.len() + replacements.len() > MAX_REPLACEMENTS {
            break;
        }
        edits.extend(replacements);
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(at, _, _)| std::cmp::Reverse(*at));
    let mut expanded = source.to_string();
    for (start, end, text) in edits {
        expanded.replace_range(start..end, &text);
    }
    Some(expanded)
}

/// The name of `function name(fn) { try { fn(); } catch (e) { throw e; } }`:
/// a synchronous function of one plain parameter whose whole body calls that
/// parameter with no arguments inside a `try`, and whose handler's only
/// statement throws the caught binding back. No `finally`, no second
/// statement, nothing between the call and the rethrow.
fn rethrow_wrapper_name<'a>(function: &'a Function<'_>) -> Option<&'a str> {
    let name = function.id.as_ref()?.name.as_str();
    let body = function.body.as_ref()?;
    if function.generator
        || function.r#async
        || function.params.rest.is_some()
        || !body.directives.is_empty()
    {
        return None;
    }
    let [param] = function.params.items.as_slice() else {
        return None;
    };
    if param.initializer.is_some() {
        return None;
    }
    let callback = super::owners::binding_identifier_name(&param.pattern)?;
    let [Statement::TryStatement(try_stmt)] = body.statements.as_slice() else {
        return None;
    };
    if try_stmt.finalizer.is_some() {
        return None;
    }
    let [Statement::ExpressionStatement(call)] = try_stmt.block.body.as_slice() else {
        return None;
    };
    let Expression::CallExpression(call) = call.expression.get_inner_expression() else {
        return None;
    };
    if !call.arguments.is_empty()
        || !matches!(
            call.callee.get_inner_expression(),
            Expression::Identifier(callee) if callee.name == callback
        )
    {
        return None;
    }
    let handler = try_stmt.handler.as_ref()?;
    let caught = super::owners::binding_identifier_name(&handler.param.as_ref()?.pattern)?;
    let [Statement::ThrowStatement(throw_stmt)] = handler.body.body.as_slice() else {
        return None;
    };
    matches!(
        throw_stmt.argument.get_inner_expression(),
        Expression::Identifier(thrown) if thrown.name == caught
    )
    .then_some(name)
}

/// `(start, end, text)` replacing the statement `name(() => expr);` at `at`
/// with `expr;`, or `None` when the mention is any other position or shape.
fn replacement(source: &str, at: usize, name: &str) -> Option<(usize, usize, String)> {
    let before = source[..at].trim_end();
    if !(before.is_empty() || before.ends_with([';', '{', '}'])) {
        return None;
    }
    let line_end = source[at..].find('\n').map_or(source.len(), |n| at + n);
    let line = source[at..line_end].trim_end();
    let call = line
        .strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(");")?;
    let expression = call
        .trim_start()
        .strip_prefix("()")?
        .trim_start()
        .strip_prefix("=>")?
        .trim();
    if expression.is_empty() || expression.starts_with('{') || !is_one_argument(expression) {
        return None;
    }
    Some((at, at + line.len(), format!("{expression};")))
}

/// `true` when `text` is balanced and has no comma outside brackets, so it is
/// one expression rather than the first of several arguments or a call that
/// continues past the wrapper's closing parenthesis.
fn is_one_argument(text: &str) -> bool {
    let Some(code) = code_bytes(text) else {
        return false;
    };
    let mut depth = 0usize;
    for (at, byte) in text.bytes().enumerate() {
        if !code[at] {
            continue;
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            b',' if depth == 0 => return false,
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn expand(source: &str) -> Option<String> {
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(ret.errors.is_empty(), "fixture must parse");
        expand_rethrow_wrappers(&ret.program.body, source)
    }

    const WRAPPER: &str = "function logged(fn: () => void): void {\n  try {\n    fn();\n  } catch (error) {\n    throw error;\n  }\n}\n";

    #[test]
    fn a_rethrow_only_wrapper_call_is_replaced_on_its_own_line() {
        let source = format!(
            "{WRAPPER}test('t', () => {{\n  logged(() => assert.equal(f(3, 7), 21));\n}});\n"
        );
        let expanded = expand(&source).unwrap_or_default();
        assert!(
            expanded.contains("\n  assert.equal(f(3, 7), 21);\n"),
            "{expanded}"
        );
        assert_eq!(expanded.lines().count(), source.lines().count());
    }

    #[test]
    fn a_swallowing_or_transforming_wrapper_is_not_eligible() {
        let call = "test('t', () => {\n  logged(() => assert.equal(f(3, 7), 21));\n});\n";
        for wrapper in [
            "function logged(fn) {\n  try {\n    fn();\n  } catch {\n  }\n}\n",
            "function logged(fn) {\n  try {\n    fn();\n  } catch (error) {\n  }\n}\n",
            "function logged(fn) {\n  try {\n    fn();\n  } catch (error) {\n    throw new Error('x');\n  }\n}\n",
            "function logged(fn) {\n  try {\n    fn();\n  } catch (error) {\n    console.log(error);\n    throw error;\n  }\n}\n",
            "function logged(fn) {\n  try {\n    fn();\n  } catch (error) {\n    throw error;\n  } finally {\n    return;\n  }\n}\n",
            "function logged(fn) {\n  try {\n    fn();\n    other();\n  } catch (error) {\n    throw error;\n  }\n}\n",
            "async function logged(fn) {\n  try {\n    fn();\n  } catch (error) {\n    throw error;\n  }\n}\n",
            "function logged(fn, extra) {\n  try {\n    fn();\n  } catch (error) {\n    throw error;\n  }\n}\n",
        ] {
            assert!(expand(&format!("{wrapper}{call}")).is_none(), "{wrapper}");
        }
    }

    #[test]
    fn a_call_in_any_other_shape_leaves_the_wrapper_unexpanded() {
        for call in [
            "const run = logged;",
            "logged(() => { assert.equal(f(3, 7), 21); });",
            "logged(() => assert.equal(f(3, 7), 21), 1);",
            "logged(async () => assert.equal(f(3, 7), 21));",
            "logged((x) => assert.equal(f(x, 7), 21));",
            "logged(() => assert.equal(f(3, 7), 21))(1);",
            "if (cond) logged(() => assert.equal(f(3, 7), 21));",
            "logged(() => assert.equal(f(3, 7), 21)); // note",
        ] {
            let source = format!("{WRAPPER}test('t', () => {{\n  {call}\n}});\n");
            assert!(expand(&source).is_none(), "{call}");
        }
    }

    #[test]
    fn one_unreplaceable_mention_keeps_every_call_as_written() {
        let source = format!(
            "{WRAPPER}test('t', () => {{\n  logged(() => assert.equal(f(3, 7), 21));\n  const alias = logged;\n}});\n"
        );
        assert!(expand(&source).is_none());
    }
}
