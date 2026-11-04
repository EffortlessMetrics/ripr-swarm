//! The old and new returned expression of a changed line inside a multi-line
//! `return` statement (#12380).
//!
//! A continuation line (`    typeof input === "function";`) reads as a bare
//! expression when the changed line is parsed alone. The expression that
//! decides the returned value is the statement's argument: the whole argument
//! of a multi-line `return`, or the one property value of a returned object
//! literal that holds the changed line. Both versions of the owner are parsed
//! with the same wrapper and the same descent, and the pair is reported only
//! when nothing outside that expression differs, so a change to a key, a
//! neighbouring property or the `return` keyword's own line never hides
//! behind an evaluated expression. Any parse failure, ambiguity or
//! unsupported shape yields `None` and the caller keeps its existing verdict.

use super::*;
use oxc_ast::ast::{ObjectPropertyKind as Kind, PropertyKind};

/// Wrappers that let an owner's recorded source parse as a program: a
/// function or arrow declaration as-is, a class member inside a class body.
const WRAPPERS: [(&str, &str); 2] = [("", ""), ("class __ripr_owner__ { ", " }")];

/// The old and new text of the returned expression (or the returned object's
/// one property value) that holds the changed line `changed` (0-based index
/// into `owner_lines`), with every other byte of the owner identical.
pub(crate) fn changed_multiline_return_expressions(
    file: &Path,
    owner_lines: &[String],
    changed: usize,
    old_line: &str,
) -> Option<(String, String)> {
    if old_line.contains('\n') || changed >= owner_lines.len() {
        return None;
    }
    let new_source = owner_lines.join("\n");
    let mut old_lines = owner_lines.to_vec();
    *old_lines.get_mut(changed)? = old_line.to_string();
    let old_source = old_lines.join("\n");
    for (prefix, suffix) in WRAPPERS {
        let Some(new_span) = locate(file, &new_source, prefix, suffix, changed) else {
            continue;
        };
        let old_span = locate(file, &old_source, prefix, suffix, changed)?;
        let new_expression = new_source.get(new_span.0..new_span.1)?;
        let old_expression = old_source.get(old_span.0..old_span.1)?;
        let outside = |source: &str, span: (usize, usize)| -> Option<String> {
            Some(format!(
                "{}\u{0}{}",
                source.get(..span.0)?,
                source.get(span.1..)?
            ))
        };
        if outside(&new_source, new_span)? != outside(&old_source, old_span)? {
            return None;
        }
        return Some((old_expression.to_string(), new_expression.to_string()));
    }
    None
}

/// Byte span, in `source` coordinates, of the expression the descent selects,
/// or `None` when the wrapped source does not parse or no return covers the
/// changed line.
fn locate(
    file: &Path,
    source: &str,
    prefix: &str,
    suffix: &str,
    changed: usize,
) -> Option<(usize, usize)> {
    let wrapped = format!("{prefix}{source}{suffix}");
    let prefix_len = prefix.len();
    let source_len = source.len();
    parse_on_worker(file, &wrapped, move |file, text, allocator| {
        let parsed = Parser::new(allocator, text, source_type_for(file)).parse();
        if parsed.panicked || !parsed.errors.is_empty() {
            return None;
        }
        let line_of = |offset: usize| text.get(..offset).map(|head| head.matches('\n').count());
        let covers = |span: oxc_span::Span| {
            let (start, last) = (span.start as usize, (span.end as usize).saturating_sub(1));
            matches!(
                (line_of(start), line_of(last)),
                (Some(first), Some(last)) if first <= changed && changed <= last
            )
        };
        let mut finder = Finder {
            covers: &covers,
            found: None,
            refused: false,
        };
        oxc_ast_visit::Visit::visit_program(&mut finder, &parsed.program);
        if finder.refused {
            return None;
        }
        let span = finder.found?;
        let (start, end) = (
            (span.start as usize).checked_sub(prefix_len)?,
            (span.end as usize).checked_sub(prefix_len)?,
        );
        (start < end && end <= source_len).then_some((start, end))
    })
    .ok()
    .flatten()
}

/// The outermost `return <argument>` whose lines include the changed line,
/// reduced to the expression that holds the changed line.
struct Finder<'f> {
    covers: &'f dyn Fn(oxc_span::Span) -> bool,
    found: Option<oxc_span::Span>,
    refused: bool,
}

impl<'a> oxc_ast_visit::Visit<'a> for Finder<'_> {
    fn visit_return_statement(&mut self, it: &oxc_ast::ast::ReturnStatement<'a>) {
        if self.found.is_some() || self.refused {
            return;
        }
        if !(self.covers)(it.span) {
            oxc_ast_visit::walk::walk_return_statement(self, it);
            return;
        }
        let Some(argument) = &it.argument else {
            self.refused = true;
            return;
        };
        let mut expression = argument.get_inner_expression();
        // A returned object literal: only the one property value holding
        // the changed line is the changed expression.
        while let Expression::ObjectExpression(object) = expression {
            let mut holding = object.properties.iter().filter(|property| match property {
                Kind::ObjectProperty(property) => (self.covers)(property.span),
                Kind::SpreadProperty(spread) => (self.covers)(spread.span),
            });
            let (Some(first), None) = (holding.next(), holding.next()) else {
                break;
            };
            let Kind::ObjectProperty(property) = first else {
                self.refused = true;
                return;
            };
            if property.computed
                || property.method
                || property.shorthand
                || property.kind != PropertyKind::Init
            {
                self.refused = true;
                return;
            }
            expression = property.value.get_inner_expression();
        }
        self.found = Some(expression.span());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(owner: &str, changed: usize, old_line: &str) -> Option<(String, String)> {
        let lines: Vec<String> = owner.lines().map(str::to_string).collect();
        changed_multiline_return_expressions(Path::new("f.ts"), &lines, changed, old_line)
    }

    const HEAD: &str = "export function f(input: unknown, flag: boolean): unknown {\n";

    /// #12380: the statement-level expression, not the continuation line.
    #[test]
    fn a_continuation_line_reads_as_the_whole_returned_expression() -> Result<(), String> {
        let owner = format!("{HEAD}  return flag &&\n    typeof input !== \"function\";\n}}\n");
        let (old, new) = pair(&owner, 2, "    typeof input === \"function\";")
            .ok_or("multi-line return not located")?;
        assert_eq!(old, "flag &&\n    typeof input === \"function\"");
        assert_eq!(new, "flag &&\n    typeof input !== \"function\"");
        Ok(())
    }

    #[test]
    fn the_return_keyword_line_and_a_ternary_arm_are_located() -> Result<(), String> {
        let first = format!("{HEAD}  return typeof input !== \"function\" &&\n    flag;\n}}\n");
        let (old, _) = pair(&first, 1, "  return typeof input === \"function\" &&")
            .ok_or("first-line clause not located")?;
        assert_eq!(old, "typeof input === \"function\" &&\n    flag");
        let ternary =
            format!("{HEAD}  return flag\n    ? typeof input !== \"function\"\n    : false;\n}}\n");
        let (old, new) = pair(&ternary, 2, "    ? typeof input === \"function\"")
            .ok_or("ternary arm not located")?;
        assert_eq!(
            old,
            "flag\n    ? typeof input === \"function\"\n    : false"
        );
        assert!(new.contains("!=="));
        Ok(())
    }

    #[test]
    fn a_returned_object_reduces_to_the_one_property_value_holding_the_line() -> Result<(), String>
    {
        let owner = "export function f(input: unknown, flag: boolean) {\n  return {\n    callable: flag && typeof input !== \"function\",\n    other: 1,\n  };\n}\n";
        let (old, new) = pair(
            owner,
            2,
            "    callable: flag && typeof input === \"function\",",
        )
        .ok_or("object property not located")?;
        assert_eq!(old, "flag && typeof input === \"function\"");
        assert_eq!(new, "flag && typeof input !== \"function\"");
        Ok(())
    }

    /// A change outside the selected expression is never hidden behind it.
    #[test]
    fn a_change_outside_the_expression_yields_no_pair() -> Result<(), String> {
        let object = "export function f(input: unknown, flag: boolean) {\n  return {\n    callable: flag && typeof input !== \"function\",\n  };\n}\n";
        // A renamed key.
        assert_eq!(
            pair(
                object,
                2,
                "    callable2: flag && typeof input !== \"function\","
            ),
            None
        );
        // A second property on the changed line: no single property holds it,
        // so the whole object is the expression and the evaluator, which has
        // no object values, then declines it.
        let two = "export function f(a: number, b: number) {\n  return {\n    x: a > 1, y: b > 2,\n  };\n}\n";
        let (old, new) = pair(two, 2, "    x: a > 1, y: b > 1,").ok_or("object not located")?;
        assert!(old.starts_with('{') && new.starts_with('{'));
        // A guard on the `return` line itself.
        let guarded = format!("{HEAD}  if (flag) return input &&\n    flag;\n}}\n");
        assert_eq!(pair(&guarded, 1, "  if (!flag) return input &&"), None);
        // A computed key and a spread are refused.
        let computed = "export function f(k: string, flag: boolean) {\n  return {\n    [k]: flag && k !== \"a\",\n  };\n}\n";
        assert_eq!(pair(computed, 2, "    [k]: flag && k === \"a\","), None);
        let spread = "export function f(o: object, flag: boolean) {\n  return {\n    ...(flag ? o : {}),\n  };\n}\n";
        assert_eq!(pair(spread, 2, "    ...(flag ? {} : {}),"), None);
        Ok(())
    }

    #[test]
    fn a_line_outside_any_return_yields_no_pair() {
        let owner = format!(
            "{HEAD}  const ok = flag &&\n    typeof input !== \"function\";\n  return ok;\n}}\n"
        );
        assert_eq!(pair(&owner, 2, "    typeof input === \"function\";"), None);
    }

    #[test]
    fn a_class_member_owner_is_located_through_the_class_wrapper() -> Result<(), String> {
        let owner = "check(input: unknown, flag: boolean) {\n  return flag &&\n    typeof input !== \"function\";\n}\n";
        let (old, _) = pair(owner, 2, "    typeof input === \"function\";")
            .ok_or("class member not located")?;
        assert!(old.starts_with("flag &&"));
        Ok(())
    }
}
