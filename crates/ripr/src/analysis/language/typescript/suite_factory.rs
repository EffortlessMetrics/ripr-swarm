//! RIPR-SPEC-0243 rule 13: a suite factory called with its arguments.
//!
//! `function runAverageSuite(name, impl) { describe(name, () => { ... impl(x)
//! ... }) }` registers nothing until `runAverageSuite("average", average)`
//! runs it, and the assertions reach the owner only through the parameter.
//! Rather than model parameter binding in the extractor, each eligible call
//! is expanded: the factory's statements, with each parameter replaced by the
//! argument text, are appended to the source as ordinary top-level
//! statements, and the unchanged extractor runs on that source. The
//! conditions below keep the replacement a plain renaming; anything else
//! leaves the call unexpanded, and its tests stay unextracted as before.

use super::tests_extract::{identifier_occurrences, word_occurrences};
use super::*;
use oxc_ast::ast::{Argument, Expression, Function, Statement};

/// Calls expanded per file, so a file of many calls cannot multiply its test
/// index without bound.
const MAX_EXPANSIONS: usize = 8;

/// The source with the expansions appended, and where each expansion starts.
pub(crate) struct SuiteFactoryExpansion {
    pub(crate) source: String,
    /// `(first line of an appended expansion, line of the call it came from)`,
    /// ascending. A test found on an appended line reports the call's line.
    pub(crate) call_lines: Vec<(usize, usize)>,
}

impl SuiteFactoryExpansion {
    /// The line a test found at `line` of the expanded source reports.
    pub(crate) fn reported_line(&self, line: usize) -> usize {
        self.call_lines
            .iter()
            .rev()
            .find(|(start, _)| *start <= line)
            .map_or(line, |(_, call_line)| *call_line)
    }
}

struct Factory<'a> {
    name: &'a str,
    params: Vec<&'a str>,
    /// The body statements' source text, one statement per line.
    body: String,
}

/// Expand every eligible suite-factory call of a top-level function
/// declaration. `None` when nothing expands, which leaves extraction as is.
pub(crate) fn expand_suite_factories(
    program: &[Statement<'_>],
    source: &str,
) -> Option<SuiteFactoryExpansion> {
    // An escaped spelling of a name is invisible to the mention count below.
    if source.contains("\\u") {
        return None;
    }
    let lines = SourceText::new(source);
    let mut expanded = source.to_string();
    let mut call_lines = Vec::new();
    for stmt in program {
        let Statement::FunctionDeclaration(function) = stmt else {
            continue;
        };
        let Some(factory) = factory_of(function, source) else {
            continue;
        };
        let calls: Vec<&oxc_ast::ast::CallExpression<'_>> = program
            .iter()
            .filter_map(|stmt| factory_call(stmt, factory.name))
            .collect();
        // Every mention is the declaration or one of the calls: no
        // reassignment, export, alias or call from inside another function.
        if calls.is_empty() || word_occurrences(source, factory.name).count() != 1 + calls.len() {
            continue;
        }
        for call in calls {
            if call_lines.len() >= MAX_EXPANSIONS {
                break;
            }
            let Some(text) = substituted_body(&factory, call, source) else {
                continue;
            };
            expanded.push('\n');
            let start = expanded.matches('\n').count() + 1;
            expanded.push_str(&text);
            call_lines.push((start, lines.line_for_offset(call.span.start as usize)));
        }
    }
    (!call_lines.is_empty()).then_some(SuiteFactoryExpansion {
        source: expanded,
        call_lines,
    })
}

/// A function whose parameters are plain names and whose body is only call
/// statements (registrations and hooks): no declaration, return or control
/// flow, so splicing the statements out of it changes nothing else.
fn factory_of<'a>(function: &'a Function<'_>, source: &str) -> Option<Factory<'a>> {
    let name = function.id.as_ref()?.name.as_str();
    let body = function.body.as_ref()?;
    if function.generator
        || function.r#async
        || function.params.rest.is_some()
        || !body.directives.is_empty()
        || body.statements.is_empty()
    {
        return None;
    }
    let params = function
        .params
        .items
        .iter()
        .map(|param| {
            // A default value is not a plain parameter: its pattern is still the
            // bare name.
            if param.initializer.is_some() {
                return None;
            }
            super::owners::binding_identifier_name(&param.pattern)
        })
        .collect::<Option<Vec<_>>>()?;
    if params
        .iter()
        .enumerate()
        .any(|(at, param)| params[..at].contains(param))
    {
        return None;
    }
    let mut text = Vec::new();
    for stmt in &body.statements {
        let Statement::ExpressionStatement(statement) = stmt else {
            return None;
        };
        if !matches!(statement.expression, Expression::CallExpression(_)) {
            return None;
        }
        text.push(source.get(statement.span.start as usize..statement.span.end as usize)?);
    }
    Some(Factory {
        name,
        params,
        body: text.join("\n"),
    })
}

/// `name(...)` as a top-level expression statement, without type arguments
/// or spread.
fn factory_call<'s, 'a>(
    stmt: &'s Statement<'a>,
    name: &str,
) -> Option<&'s oxc_ast::ast::CallExpression<'a>> {
    let Statement::ExpressionStatement(statement) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = &statement.expression else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    (callee.name == name && call.type_arguments.is_none()).then_some(&**call)
}

/// The factory body with each parameter replaced by its argument, or `None`
/// when any parameter use is not a plain reference to that argument.
///
/// A parameter bound to an identifier must be used only as a callee
/// (`impl(...)`); one bound to a string or number literal only as an
/// argument (`describe(name, ...)`). The body may not rebind the parameter or
/// already mention the argument's name, so the renaming captures nothing.
fn substituted_body(
    factory: &Factory<'_>,
    call: &oxc_ast::ast::CallExpression<'_>,
    source: &str,
) -> Option<String> {
    if call.arguments.len() != factory.params.len() {
        return None;
    }
    let body = &factory.body;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for (param, argument) in factory.params.iter().zip(&call.arguments) {
        let (replacement, as_callee) = match argument {
            Argument::Identifier(identifier) => (identifier.name.to_string(), true),
            Argument::StringLiteral(literal) => (span_text(source, literal.span)?, false),
            Argument::NumericLiteral(literal) => (span_text(source, literal.span)?, false),
            _ => return None,
        };
        if as_callee && word_occurrences(body, &replacement).next().is_some() {
            return None;
        }
        if super::related_tests::local_identifier_declared_in_test_body(body, param) {
            return None;
        }
        let hits: Vec<usize> = word_occurrences(body, param).collect();
        if hits.len() != identifier_occurrences(body, param).count() {
            return None;
        }
        for at in hits {
            let before = body[..at].trim_end();
            let after = body[at + param.len()..].trim_start();
            let plain = if as_callee {
                after.starts_with('(') && !before.ends_with(['"', '\'', '`'])
            } else {
                before.ends_with(['(', ',']) && after.starts_with([',', ')'])
            };
            if !plain {
                return None;
            }
            edits.push((at, at + param.len(), replacement.clone()));
        }
    }
    edits.sort_by_key(|(at, _, _)| std::cmp::Reverse(*at));
    let mut text = body.clone();
    for (start, end, replacement) in edits {
        text.replace_range(start..end, &replacement);
    }
    Some(text)
}

fn span_text(source: &str, span: oxc_span::Span) -> Option<String> {
    source
        .get(span.start as usize..span.end as usize)
        .map(str::to_string)
}
