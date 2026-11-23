//! Class-attribute string constant pin evidence for the Python preview
//! adapter (#15294).
//!
//! A changed class-body `name = "old text"` -> `name = "new text"` is a value
//! the class carries to whatever reads it later (a message formatted into an
//! exception, a label rendered into output). Tests that only construct the
//! class and compare an unrelated return value reach the owner without ever
//! observing the text, so a strong oracle on any such test must not read as
//! discrimination of the edit.
//!
//! The change is discriminated only when a related test's assertion carries a
//! plain string literal that the OLD text accepts and the NEW text rejects
//! (`pytest.raises(E, match="...")` naming the old message, or an equality on
//! the formatted message). `{placeholders}` in the text are wildcards. A
//! literal that fits both texts (an unchanged prefix) pins neither, and a
//! literal with regex syntax or an f-string brace is unreadable; neither
//! credits the edit. The gate engages only on a message-only edit of a
//! string-literal assignment directly owned by a class; every other shape
//! keeps the existing verdict.

use super::PythonOwner;
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

/// Why a class-attribute text edit is not discriminated.
pub(super) struct ClassAttrPinGap {
    pub(super) discriminator: String,
    pub(super) reason: String,
    /// The repair sentence that follows the reason.
    pub(super) next_step: String,
}

pub(super) fn python_class_attr_pin_gap(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    line: usize,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<ClassAttrPinGap> {
    let (name, new_text) = assigned_string(line_text)?;
    let (old_name, old_text) = assigned_string(old_line_text?)?;
    if name != old_name || old_text == new_text {
        return None;
    }
    let lines: Vec<&str> = owner_source?.lines().collect();
    let start = owner.start_line.checked_sub(1)?;
    let changed = line.checked_sub(1)?;
    if !lines.get(start)?.trim_start().starts_with("class ")
        || changed <= start
        || changed >= owner.end_line.min(lines.len())
    {
        return None;
    }
    let old = tokens(&old_text);
    let new = tokens(&new_text);
    let pinned = related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .flat_map(|candidate| candidate.test.assertions.iter())
        .flat_map(|assertion| pinning_literals(&assertion.text, &assertion.gate_text))
        .any(|pin| {
            let literal: Vec<char> = pin.text.chars().collect();
            // The old text must fit with the half-fixed rule; the new text only
            // needs one fixed character (none when it is all placeholders), so
            // a placeholder-heavy new text that could still produce the
            // literal does not read as rejecting it.
            // A short literal is ambiguous as a fragment, but whole-text
            // equality with it is exact.
            (literal.len() >= 4 || (pin.whole && !literal.is_empty()))
                && text_matches(&old, &literal, &pin, true)
                && !text_matches(&new, &literal, &pin, false)
        });
    if pinned {
        return None;
    }
    Some(ClassAttrPinGap {
        discriminator: format!("class attribute `{name}` text {new_text:?}"),
        reason: format!(
            "no related assertion carries a literal that the old `{name}` text {old_text:?} accepts and the new text {new_text:?} rejects, so the tests reach `{}` without observing the changed text",
            owner.name
        ),
        next_step: "Add an assertion (for example `pytest.raises(..., match=...)`) that the old text satisfies and the new text does not.".to_string(),
    })
}

/// The name and literal of a line that is exactly `name = "text"` or
/// `name: T = "text"`.
fn assigned_string(line: &str) -> Option<(String, String)> {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<class-attr>"), line.trim())
    else {
        return None;
    };
    let (target, value) = match module.body.as_slice() {
        [Stmt::Assign(assign)] => match assign.targets.as_slice() {
            [target] => (target, assign.value.as_ref()),
            _ => return None,
        },
        [Stmt::AnnAssign(assign)] => (assign.target.as_ref(), assign.value.as_deref()?),
        _ => return None,
    };
    let Expr::Name(name) = target else {
        return None;
    };
    let Expr::Constant(constant) = value else {
        return None;
    };
    let ast::Constant::Str(text) = &constant.value else {
        return None;
    };
    Some((name.id.to_string(), text.clone()))
}

#[derive(Clone, Copy)]
enum Token {
    Char(char),
    Wildcard,
}

/// The text as characters with each `{...}` placeholder as one wildcard.
fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
            }
            out.push(Token::Wildcard);
        } else {
            out.push(Token::Char(ch));
        }
    }
    out
}

/// Whether some instantiation of the wildcards contains `literal` as a
/// substring (`pin.whole`: equals it). With `pin.regex`, a `.` in the literal
/// stands for any one character (`re.search` semantics); it never counts as
/// fixed text. With `strict`, at least half the literal must be fixed text;
/// otherwise at least one character (a placeholder alone would fit any text).
fn text_matches(tokens: &[Token], literal: &[char], pin: &PinLiteral, strict: bool) -> bool {
    // A text made only of placeholders can render as any literal. An empty
    // text has no placeholder and renders only as the empty string (#15646).
    if !strict && !tokens.is_empty() && tokens.iter().all(|token| matches!(token, Token::Wildcard))
    {
        return true;
    }
    let regex = pin.regex;
    let begins = if pin.whole { 0..=0 } else { 0..=tokens.len() };
    begins.into_iter().any(|begin| {
        let mut states = vec![(begin, 0usize, 0usize)];
        // `fixed` stays in the key: with a regex `.`, two paths to the same
        // `(i, j)` can align different literal characters to fixed text.
        let mut seen = std::collections::BTreeSet::new();
        while let Some((i, j, fixed)) = states.pop() {
            if !seen.insert((i, j, fixed)) {
                continue;
            }
            // At least half the literal must be fixed text of the constant, so
            // a placeholder cannot absorb the whole literal.
            if j == literal.len()
                && (if strict {
                    fixed * 2 >= literal.len()
                } else {
                    fixed >= 1
                })
                && (!pin.whole
                    || tokens.get(i..).is_some_and(|rest| {
                        rest.iter().all(|token| matches!(token, Token::Wildcard))
                    }))
            {
                return true;
            }
            match tokens.get(i) {
                Some(Token::Char(ch)) if literal.get(j) == Some(ch) => {
                    let exact = !(regex && *ch == '.');
                    states.push((i + 1, j + 1, fixed + usize::from(exact)))
                }
                Some(Token::Char(ch)) if regex && literal.get(j) == Some(&'.') && *ch != '\n' => {
                    states.push((i + 1, j + 1, fixed))
                }
                Some(Token::Wildcard) => {
                    states.push((i + 1, j, fixed));
                    if j < literal.len() {
                        states.push((i, j + 1, fixed));
                    }
                }
                _ => {}
            }
        }
        false
    })
}

/// A literal an assertion pins the changed text with.
struct PinLiteral {
    text: String,
    /// The literal is a `re.search` pattern (`match=`, `assertRaisesRegex`).
    regex: bool,
    /// The literal must equal the whole text (`==`, `assertEqual`), not just
    /// occur in it (`in`, `assertIn`, a `match=` search).
    whole: bool,
}

/// The literals that can tell the old text from the new: the `match=` /
/// `assertRaisesRegex` pattern of the assertion head (only `.` allowed as
/// regex syntax), and the string operand of an `assert x == "..."`,
/// `assert "..." in x` or `assertEqual(x, "...")` in the assertion's gate. An
/// `assert a and b` pins through each operand it requires.
/// Call arguments, assert messages, `!=` and `not in` operands never pin, and
/// any statement that does not parse is skipped.
fn pinning_literals(text: &str, gate_text: &str) -> Vec<PinLiteral> {
    let mut out = Vec::new();
    if let Some(pattern) = super::message_pin::literal_message_pattern(text)
        && !pattern.chars().any(|c| "\\^$*+?()[]{}|\n".contains(c))
    {
        out.push(PinLiteral {
            text: pattern,
            regex: true,
            whole: false,
        });
    }
    // The gate text keeps only an `and` chain's strongest conjuncts; the
    // assertion's own text still holds a weaker one the test also requires
    // (`assert f() == 1 and "text" in C.message`).
    for statement in gate_statements(gate_text)
        .into_iter()
        .chain(gate_statements(text))
    {
        let literals = match &statement {
            Stmt::Assert(assertion) => positive_conjuncts(&assertion.test)
                .into_iter()
                .filter_map(compared_literal)
                .collect(),
            Stmt::Expr(expression) => Vec::from_iter(assert_equal_literal(&expression.value)),
            _ => Vec::new(),
        };
        out.extend(literals.into_iter().map(|(text, whole)| PinLiteral {
            text,
            regex: false,
            whole,
        }));
    }
    out
}

/// The statements of an assertion's gate text. The gate of one assertion can
/// span several physical lines (`assert str(\n    exc.value\n) == "..."`), so
/// the text is parsed whole (#15646). A gate whose comparison lost its
/// enclosing parentheses (`assert (\n    x\n    == "..."\n)`) is read inside
/// one pair. Text that still does not parse is read one physical line at a
/// time, and a line that does not parse is skipped.
fn gate_statements(gate_text: &str) -> Vec<Stmt> {
    let parse = |text: &str| match parse_module_result(Path::new("<class-attr-pin>"), text) {
        Ok(Mod::Module(module)) => Some(module.body),
        _ => None,
    };
    let whole = gate_text.trim();
    parse(whole)
        .or_else(|| parse(&format!("assert ({})", whole.strip_prefix("assert ")?)))
        .unwrap_or_else(|| {
            gate_text
                .lines()
                .filter_map(|line| parse(line.trim()))
                .flatten()
                .collect()
        })
}

/// The operands an `assert` requires all at once: the test itself, or every
/// operand of a (nested) `and`. An `or` or a `not` requires none of its
/// operands, so it stays one opaque operand (#15646).
fn positive_conjuncts(test: &Expr) -> Vec<&Expr> {
    match test {
        Expr::BoolOp(operation) if operation.op == ast::BoolOp::And => operation
            .values
            .iter()
            .flat_map(positive_conjuncts)
            .collect(),
        _ => vec![test],
    }
}

/// The string operand of `x == "..."`, `"..." == x` (whole text) or
/// `"..." in x` (substring). Two constants compare nothing the owner made.
fn compared_literal(test: &Expr) -> Option<(String, bool)> {
    let Expr::Compare(compare) = test else {
        return None;
    };
    let ([op], [right]) = (compare.ops.as_slice(), compare.comparators.as_slice()) else {
        return None;
    };
    match op {
        ast::CmpOp::Eq => match (str_constant(&compare.left), str_constant(right)) {
            (Some(_), Some(_)) | (None, None) => None,
            (Some(text), None) | (None, Some(text)) => Some((text, true)),
        },
        ast::CmpOp::In if !is_literal_value(right) => {
            str_constant(&compare.left).map(|text| (text, false))
        }
        _ => None,
    }
}

/// The string argument of `self.assertEqual(x, "...")` (whole text) or
/// `self.assertIn("...", x)` (substring).
fn assert_equal_literal(value: &Expr) -> Option<(String, bool)> {
    let Expr::Call(call) = value else {
        return None;
    };
    let callee = super::expr_full_name(&call.func)?;
    if !matches!(callee.rsplit('.').next(), Some("assertEqual" | "assertIn")) {
        return None;
    }
    let [first, second, ..] = call.args.as_slice() else {
        return None;
    };
    if callee.ends_with("assertIn") {
        if is_literal_value(second) {
            return None;
        }
        return str_constant(first).map(|text| (text, false));
    }
    match (str_constant(first), str_constant(second)) {
        (Some(_), Some(_)) | (None, None) => None,
        (Some(text), None) | (None, Some(text)) => Some((text, true)),
    }
}

/// A constant or a literal container: membership in it compares nothing the
/// owner made.
fn is_literal_value(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Constant(_) | Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) | Expr::Dict(_)
    )
}

fn str_constant(expr: &Expr) -> Option<String> {
    let Expr::Constant(constant) = expr else {
        return None;
    };
    let ast::Constant::Str(text) = &constant.value else {
        return None;
    };
    Some(text.clone())
}
