//! A changed default parameter value no call lets apply (#15393).
//!
//! `function isReadable(a, b, wcag2 = { level: 'AA', size: 'small' })` uses
//! its default only for a call that omits the third argument or passes
//! `undefined` there. When every call the related tests make passes a value
//! at that position that cannot be `undefined`, the changed default never
//! runs under the suite, so a strong oracle on those calls passes on both
//! versions. The rule only withdraws credit, and only on proof: this module
//! locates the one changed default and decides whether a call argument is
//! definitely not `undefined`; which calls exist is the classifier's glue.

use super::*;

/// Wrappers that let an owner's recorded source parse as a program: a
/// function or arrow declaration as-is, a class member inside a class body.
const WRAPPERS: [(&str, &str); 2] = [("", ""), ("class __ripr_owner__ {\n", "\n}")];

/// The one owner parameter whose default value the edit changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangedDefault {
    /// Zero-based position in the owner's parameter list.
    pub(crate) index: usize,
    /// The parameter as written before its default (`wcag2`, `{ a, b }`).
    pub(crate) name: String,
}

/// `Some` when the old and new owner sources (the changed 0-based line
/// `changed`, whose whole file line is `new_line`, replaced by `old_line`) differ only in the default value of one
/// parameter of the owner's own signature, and the changed line lies inside
/// that parameter. Any other difference, a rest parameter at or before that
/// position, or a parse failure yields `None`.
pub(crate) fn changed_default_parameter(
    file: &Path,
    owner_source: &str,
    changed: usize,
    new_line: &str,
    old_line: &str,
) -> Option<ChangedDefault> {
    if old_line.contains('\n') {
        return None;
    }
    let new_lines: Vec<&str> = owner_source.lines().collect();
    if changed >= new_lines.len() || new_lines[changed].trim() == old_line.trim() {
        return None;
    }
    // The recorded owner source starts at its declaration, so its first line
    // can be a suffix of the file line (`export ` dropped): cut the same
    // prefix from the old line.
    let old_line = if new_lines[changed] == new_line {
        old_line
    } else {
        let prefix = new_line.strip_suffix(new_lines[changed])?;
        old_line.strip_prefix(prefix)?
    };
    let mut old_lines = new_lines.clone();
    old_lines[changed] = old_line;
    let old_source = old_lines.join("\n");
    let new_source = new_lines.join("\n");
    let new_params = signature(file, &new_source)?;
    let old_params = signature(file, &old_source)?;
    if new_params.params.len() != old_params.params.len() {
        return None;
    }
    let mut changed_param: Option<usize> = None;
    for (index, (new, old)) in new_params.params.iter().zip(&old_params.params).enumerate() {
        if new.binding != old.binding {
            return None;
        }
        if new.default != old.default {
            if changed_param.is_some() || new.default.is_none() || old.default.is_none() {
                return None;
            }
            changed_param = Some(index);
        }
    }
    let index = changed_param?;
    let param = &new_params.params[index];
    // The edit is inside the parameter, no later than the line the body
    // opens on, and nothing outside the default value differs.
    if changed < param.first_line || changed > param.last_line || changed > new_params.body_line {
        return None;
    }
    let ((new_start, new_end), (old_start, old_end)) =
        (param.default_span?, old_params.params[index].default_span?);
    if new_source.get(..new_start)? != old_source.get(..old_start)?
        || new_source.get(new_end..)? != old_source.get(old_end..)?
    {
        return None;
    }
    Some(ChangedDefault {
        index,
        name: param.binding.clone(),
    })
}

/// Whether a call argument is definitely not `undefined`, so a default at
/// its position never applies: a number, string, boolean or `null` literal,
/// an object or array literal, a function, a `new` expression, or a template.
/// An identifier, a member, a call, `undefined`, `void`, a spread, or any
/// other expression may be `undefined`.
pub(crate) fn argument_is_definitely_defined(argument: &str) -> bool {
    let argument = strip_type_casts(argument.trim());
    if argument.is_empty() || argument.starts_with("...") {
        return false;
    }
    let masked = ts_mask_literals_and_comments(argument);
    if masked.len() != argument.len() {
        return false;
    }
    let first = argument.chars().next();
    let closes = |open: char, close: char| {
        argument.starts_with(open)
            && argument.ends_with(close)
            && balanced_span(&masked, open, close) == Some(masked.len() - 1)
    };
    if closes('{', '}') || closes('[', ']') {
        return true;
    }
    if matches!(first, Some('\'' | '"' | '`')) {
        // One whole quoted or template literal.
        let quote = first.unwrap_or('"');
        return argument.len() >= 2
            && argument.ends_with(quote)
            && !argument[1..argument.len() - 1].contains(quote);
    }
    if matches!(argument, "true" | "false" | "null") {
        return true;
    }
    if argument.starts_with("new ") {
        return one_expression_is(argument, |expression| {
            matches!(expression, Expression::NewExpression(_))
        });
    }
    if (argument.starts_with("function") || argument.contains("=>"))
        && !argument.starts_with("function_")
    {
        return one_expression_is(argument, |expression| {
            matches!(
                expression,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            )
        });
    }
    let unsigned = argument.strip_prefix('-').unwrap_or(argument);
    numeric_literal_value(unsigned).is_some_and(|text| text.parse::<f64>().is_ok())
}

/// The expression under trailing TypeScript casts (`x as T`,
/// `x satisfies T`, `x!`), which do not change the runtime value. Only a
/// plain type name, dotted, with optional `[]`, is read as the cast type.
fn strip_type_casts(argument: &str) -> &str {
    let mut argument = argument;
    loop {
        let before = argument;
        argument = argument.strip_suffix('!').unwrap_or(argument).trim_end();
        for keyword in [" as ", " satisfies "] {
            if let Some(at) = argument.rfind(keyword) {
                let cast = argument[at + keyword.len()..].trim();
                let plain = !cast.is_empty()
                    && cast
                        .trim_end_matches("[]")
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.'));
                if plain {
                    argument = argument[..at].trim_end();
                }
            }
        }
        if argument == before {
            return argument;
        }
    }
}

/// The index of the bracket that closes the one at position 0, in masked
/// text where literals are blanked.
fn balanced_span(masked: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (at, c) in masked.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

/// Whether the text parses as exactly one expression of the kind `accept`
/// names (a `new` expression, a function or an arrow), with nothing after it.
fn one_expression_is(argument: &str, accept: impl Fn(&Expression<'_>) -> bool) -> bool {
    let source = format!("({argument});");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    if parsed.panicked || !parsed.errors.is_empty() || parsed.program.body.len() != 1 {
        return false;
    }
    let Statement::ExpressionStatement(statement) = &parsed.program.body[0] else {
        return false;
    };
    accept(statement.expression.get_inner_expression())
}

struct Param {
    binding: String,
    default: Option<String>,
    /// Byte span of the default value in the unwrapped source.
    default_span: Option<(usize, usize)>,
    first_line: usize,
    last_line: usize,
}

struct Signature {
    params: Vec<Param>,
    /// 0-based line where the owner's body opens.
    body_line: usize,
}

/// The owner's own parameter list, read from the first function or arrow in
/// the wrapped source. A rest parameter refuses: positions after it do not
/// exist, and its own position never holds a default.
fn signature(file: &Path, source: &str) -> Option<Signature> {
    for (prefix, suffix) in WRAPPERS {
        let wrapped = format!("{prefix}{source}{suffix}");
        let prefix_lines = prefix.matches('\n').count();
        let prefix_len = prefix.len();
        let found = parse_on_worker(file, &wrapped, move |file, text, allocator| {
            let parsed = Parser::new(allocator, text, source_type_for(file)).parse();
            if parsed.panicked || !parsed.errors.is_empty() {
                return None;
            }
            let mut reader = SignatureReader {
                text,
                found: None,
                refused: false,
            };
            oxc_ast_visit::Visit::visit_program(&mut reader, &parsed.program);
            if reader.refused {
                return None;
            }
            let signature = reader.found?;
            Some(Signature {
                params: signature
                    .params
                    .into_iter()
                    .map(|param| Param {
                        default_span: param.default_span.and_then(|(start, end)| {
                            Some((start.checked_sub(prefix_len)?, end.checked_sub(prefix_len)?))
                        }),
                        first_line: param.first_line.saturating_sub(prefix_lines),
                        last_line: param.last_line.saturating_sub(prefix_lines),
                        ..param
                    })
                    .collect(),
                body_line: signature.body_line.saturating_sub(prefix_lines),
            })
        })
        .ok()
        .flatten();
        if found.is_some() {
            return found;
        }
    }
    None
}

struct SignatureReader<'t> {
    text: &'t str,
    found: Option<Signature>,
    refused: bool,
}

impl SignatureReader<'_> {
    fn line_of(&self, offset: u32) -> usize {
        self.text
            .get(..offset as usize)
            .map_or(0, |head| head.matches('\n').count())
    }

    fn slice(&self, span: oxc_span::Span) -> Option<String> {
        self.text
            .get(span.start as usize..span.end as usize)
            .map(str::to_string)
    }

    fn read(&mut self, params: &oxc_ast::ast::FormalParameters<'_>, body_start: u32) {
        if self.found.is_some() || self.refused {
            return;
        }
        if params.rest.is_some() {
            self.refused = true;
            return;
        }
        let mut read = Vec::new();
        for item in &params.items {
            let (Some(binding), default) = (
                self.slice(item.pattern.span()),
                item.initializer
                    .as_ref()
                    .map(|initializer| self.slice(initializer.span())),
            ) else {
                self.refused = true;
                return;
            };
            let default = match default {
                Some(Some(text)) => Some(text),
                Some(None) => {
                    self.refused = true;
                    return;
                }
                None => None,
            };
            read.push(Param {
                binding,
                default,
                default_span: item.initializer.as_ref().map(|initializer| {
                    let span = initializer.span();
                    (span.start as usize, span.end as usize)
                }),
                first_line: self.line_of(item.span.start),
                last_line: self.line_of(item.span.end.saturating_sub(1)),
            });
        }
        self.found = Some(Signature {
            params: read,
            body_line: self.line_of(body_start),
        });
    }
}

impl<'a> oxc_ast_visit::Visit<'a> for SignatureReader<'_> {
    fn enter_node(&mut self, kind: oxc_ast::ast_kind::AstKind<'a>) {
        use oxc_ast::ast_kind::AstKind;
        match kind {
            AstKind::Function(function) => match &function.body {
                Some(body) => self.read(&function.params, body.span.start),
                None => self.refused |= self.found.is_none(),
            },
            AstKind::ArrowFunctionExpression(arrow) => {
                self.read(&arrow.params, arrow.body.span.start);
            }
            _ => {}
        }
    }
}
