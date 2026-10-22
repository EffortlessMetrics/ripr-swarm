//! RIPR-SPEC-0243 decision 19: a same-file assertion helper called with plain
//! arguments.
//!
//! `function checkWrites(stats, want) { assert.equal(stats.writes, want); }`
//! holds the assertion, so the test that calls `checkWrites(stats, 2)` has none
//! the extractor can see. Rather than model parameter binding, each eligible
//! call is replaced, on its own line, by the helper's one assertion with each
//! parameter renamed to its argument, and the unchanged extractor runs on that
//! source, keeping every binding, oracle and admission gate in force. The
//! conditions below keep the replacement a plain renaming; anything else
//! leaves the call as written.

use super::suite_factory::code_bytes;
use super::tests_extract::{identifier_occurrences, word_occurrences};
use oxc_ast::ast::{Expression, Function, Statement};

/// Calls replaced per file.
const MAX_REPLACEMENTS: usize = 16;

struct Helper<'a> {
    name: String,
    params: Vec<String>,
    /// The one assertion, without its `;`, on a single line.
    body: &'a str,
    declaration: std::ops::Range<usize>,
}

/// The source with each eligible helper call replaced, or `None` when nothing
/// is. Line numbers are unchanged: a call and its replacement are one line.
pub(crate) fn expand_assertion_helpers(program: &[Statement<'_>], source: &str) -> Option<String> {
    if source.contains("\\u") {
        return None;
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for stmt in program {
        let Statement::FunctionDeclaration(function) = stmt else {
            continue;
        };
        let Some(helper) = helper_of(function, source) else {
            continue;
        };
        let mentions: Vec<usize> = word_occurrences(source, &helper.name)
            .filter(|at| !helper.declaration.contains(at))
            .collect();
        let replacements: Vec<(usize, usize, String)> = mentions
            .iter()
            .filter_map(|at| replacement(&helper, source, *at))
            .collect();
        // Every mention is a call this module replaces: no alias, export or
        // call from another position.
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

fn helper_of<'a>(function: &Function<'_>, source: &'a str) -> Option<Helper<'a>> {
    let name = function.id.as_ref()?.name.to_string();
    let body = function.body.as_ref()?;
    if function.generator
        || function.r#async
        || function.params.rest.is_some()
        || !body.directives.is_empty()
        || body.statements.len() != 1
    {
        return None;
    }
    let params = function
        .params
        .items
        .iter()
        .map(|param| {
            if param.initializer.is_some() {
                return None;
            }
            super::owners::binding_identifier_name(&param.pattern).map(str::to_string)
        })
        .collect::<Option<Vec<_>>>()?;
    if params
        .iter()
        .enumerate()
        .any(|(at, param)| params[..at].contains(param))
    {
        return None;
    }
    let Statement::ExpressionStatement(statement) = &body.statements[0] else {
        return None;
    };
    if !matches!(statement.expression, Expression::CallExpression(_)) {
        return None;
    }
    let text = source
        .get(statement.span.start as usize..statement.span.end as usize)?
        .trim_end_matches(';')
        .trim();
    (!text.contains('\n')).then_some(Helper {
        name,
        params,
        body: text,
        declaration: function.span.start as usize..function.span.end as usize,
    })
}

/// `(start, end, text)` replacing the call at `at` with the renamed assertion.
fn replacement(helper: &Helper<'_>, source: &str, at: usize) -> Option<(usize, usize, String)> {
    let before = source[..at].trim_end();
    if !(before.is_empty() || before.ends_with([';', '{', '}'])) {
        return None;
    }
    let args_start = at + helper.name.len();
    let rest = source[args_start..].strip_prefix('(')?;
    let close = rest.find(')')?;
    let arguments = &rest[..close];
    if arguments.contains(['\n', '"', '\'', '`', '(', '[', '{']) {
        return None;
    }
    let end = args_start + 1 + close + 1;
    if !source[end..].trim_start().starts_with(';') {
        return None;
    }
    let arguments: Vec<&str> = if arguments.trim().is_empty() {
        Vec::new()
    } else {
        arguments.split(',').map(str::trim).collect()
    };
    if arguments.len() != helper.params.len() {
        return None;
    }
    let body = helper.body;
    let code = code_bytes(body)?;
    // The renamed assertion runs at the call site, so a free name (a module
    // constant) must not be declared a second time where a local could shadow it.
    if free_identifiers(body, &helper.params)
        .iter()
        .any(|name| declaration_count(source, name) > 1)
    {
        return None;
    }
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    for (param, argument) in helper.params.iter().zip(&arguments) {
        let identifier = argument
            .chars()
            .all(|c| c == '_' || c == '$' || c.is_ascii_alphanumeric())
            && !argument.starts_with(|c: char| c.is_ascii_digit());
        let number = !argument.is_empty()
            && argument.chars().all(|c| c.is_ascii_digit())
            && !(argument.len() > 1 && argument.starts_with('0'));
        if !identifier && !number {
            return None;
        }
        if argument != param && word_occurrences(body, argument).next().is_some() {
            return None;
        }
        let hits: Vec<usize> = word_occurrences(body, param).collect();
        if hits.len() != identifier_occurrences(body, param).count()
            || hits.iter().any(|hit| !code[*hit])
        {
            return None;
        }
        for hit in hits {
            let lead = body[..hit].trim_end();
            let after = body[hit + param.len()..].trim_start();
            let member_base = after.starts_with('.') && !after.starts_with("..");
            let plain = lead.ends_with(['(', ','])
                && (member_base && identifier || after.starts_with([',', ')']));
            if !plain {
                return None;
            }
            edits.push((hit, hit + param.len(), argument));
        }
    }
    edits.sort_by_key(|(at, _, _)| std::cmp::Reverse(*at));
    let mut text = body.to_string();
    for (start, end, replacement) in edits {
        text.replace_range(start..end, replacement);
    }
    Some((at, end, text))
}

/// Identifiers in `body` that are neither parameters, member names, nor the
/// call's own callee chain root.
fn free_identifiers(body: &str, params: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, end: usize, names: &mut Vec<String>| {
        if word.is_empty() {
            return;
        }
        let start = end - word.len();
        let member = body[..start].trim_end().ends_with('.');
        let numeric = word.starts_with(|c: char| c.is_ascii_digit());
        if !member && !numeric && !params.contains(word) && !names.contains(word) {
            names.push(word.clone());
        }
        word.clear();
    };
    for (at, c) in body.char_indices() {
        if c == '_' || c == '$' || c.is_ascii_alphanumeric() {
            word.push(c);
        } else {
            flush(&mut word, at, &mut names);
        }
    }
    flush(&mut word, body.len(), &mut names);
    names
}

/// How many `const`/`let`/`var`/`function`/`class` declarations of `name` the
/// source holds.
fn declaration_count(source: &str, name: &str) -> usize {
    word_occurrences(source, name)
        .filter(|at| {
            let before = source[..*at].trim_end();
            ["const", "let", "var", "function", "class"]
                .iter()
                .any(|keyword| {
                    before.strip_suffix(keyword).is_some_and(|rest| {
                        !rest.ends_with(|c: char| c == '_' || c == '$' || c.is_ascii_alphanumeric())
                    })
                })
        })
        .count()
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
        expand_assertion_helpers(&ret.program.body, source)
    }

    const HELPER: &str = "function check(stats, want) {\n  assert.equal(stats.writes, want);\n}\n";

    #[test]
    fn plain_call_is_replaced_on_its_own_line() {
        let source = format!("{HELPER}test('t', () => {{\n  check(s, 2);\n}});\n");
        let expanded = expand(&source);
        assert!(expanded.is_some(), "plain call must expand");
        let expanded = expanded.unwrap_or_default();
        assert!(expanded.contains("  assert.equal(s.writes, 2);\n"));
        assert_eq!(expanded.lines().count(), source.lines().count());
    }

    #[test]
    fn every_mention_must_be_a_replaceable_call() {
        let aliased = format!("{HELPER}const c = check;\ntest('t', () => {{ check(s, 2); }});\n");
        assert_eq!(expand(&aliased), None);
        let in_expression = format!("{HELPER}test('t', () => {{\n  const r = check(s, 2);\n}});\n");
        assert_eq!(expand(&in_expression), None);
    }

    #[test]
    fn arguments_must_be_names_or_integers() {
        for call in [
            "check(make(), 2)",
            "check(s, 2 + 1)",
            "check(s, 'x')",
            "check(s.inner, 2)",
            "check(s)",
            "check(s, 2, 3)",
        ] {
            let source = format!("{HELPER}test('t', () => {{\n  {call};\n}});\n");
            assert_eq!(expand(&source), None, "{call}");
        }
    }

    #[test]
    fn helper_with_more_than_one_statement_or_a_default_is_left_alone() {
        let two = "function check(s, w) {\n  s.x = 1;\n  assert.equal(s.writes, w);\n}\ntest('t', () => {\n  check(s, 2);\n});\n";
        assert_eq!(expand(two), None);
        let default = "function check(s, w = 1) {\n  assert.equal(s.writes, w);\n}\ntest('t', () => {\n  check(s, 2);\n});\n";
        assert_eq!(expand(default), None);
    }

    #[test]
    fn argument_naming_a_helper_free_name_is_refused() {
        let source = "function check(s, w) {\n  assert.equal(s.writes, w + LIMIT);\n}\n";
        assert_eq!(
            expand(&format!(
                "{source}test('t', () => {{\n  check(s, LIMIT);\n}});\n"
            )),
            None
        );
    }

    #[test]
    fn free_name_declared_again_in_the_test_is_refused() {
        let source = "const LIMIT = 1;\nfunction check(s, w) {\n  assert.equal(s.writes, w, LIMIT);\n}\ntest('t', () => {\n  const LIMIT = 9;\n  check(s, 2);\n});\n";
        assert_eq!(expand(source), None);
        let single = "const LIMIT = 1;\nfunction check(s, w) {\n  assert.equal(s.writes, w, LIMIT);\n}\ntest('t', () => {\n  check(s, 2);\n});\n";
        assert!(expand(single).is_some());
    }
}
