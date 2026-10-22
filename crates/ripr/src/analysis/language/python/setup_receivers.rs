//! Receivers a unittest/pytest class builds once in its setup hook (#9417).
//!
//! `self.stock = Stock(5)` in `setUp` makes `self.stock.take(2)` in a test a
//! call on an instance of `Stock`, the same relation `v = Stock(5)` in the test
//! body gives. The binding is read only when it is unambiguous: one
//! assignment of the attribute in the whole class, made directly in a setup
//! hook, and no rebinding in the test body.

use super::source_utils::text_for_range;
use rustpython_parser::ast::{Expr, Ranged, Stmt};

const SETUP_HOOKS: [&str; 4] = ["setUp", "setUpClass", "setup_method", "setup_class"];

/// An attribute path (`self.stock`) bound once to a construction of `callee`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SetupReceiver {
    pub(super) path: String,
    pub(super) callee: String,
}

/// The unambiguous setup-hook constructions of one class body.
///
/// A `subclassed` class (another class in the module lists it as a base) reads
/// none: a subclass can replace the hook the test runs under.
pub(super) fn setup_receivers(
    source: &str,
    class_body: &[Stmt],
    subclassed: bool,
) -> Vec<SetupReceiver> {
    if subclassed {
        return Vec::new();
    }
    let class_text: String = class_body
        .iter()
        .map(|stmt| text_for_range(source, stmt.range()))
        .collect::<Vec<_>>()
        .join("\n");
    if ["setattr(", "__dict__", "vars("]
        .iter()
        .any(|needle| class_text.contains(needle))
    {
        return Vec::new();
    }
    let mut found = Vec::new();
    for stmt in class_body {
        let Stmt::FunctionDef(function) = stmt else {
            continue;
        };
        if !SETUP_HOOKS.contains(&function.name.as_str()) {
            continue;
        }
        let hook_receiver = if matches!(function.name.as_str(), "setUpClass" | "setup_class") {
            "cls"
        } else {
            "self"
        };
        for inner in &function.body {
            let Stmt::Assign(assign) = inner else {
                continue;
            };
            let [Expr::Attribute(attribute)] = assign.targets.as_slice() else {
                continue;
            };
            let Expr::Name(base) = attribute.value.as_ref() else {
                continue;
            };
            if base.id.as_str() != hook_receiver {
                continue;
            }
            let Expr::Call(call) = assign.value.as_ref() else {
                continue;
            };
            let callee = text_for_range(source, call.func.range());
            if callee.is_empty() || !callee.chars().all(is_dotted_identifier_char) {
                continue;
            }
            let attr = attribute.attr.as_str();
            // The attribute is assigned exactly once across the class, whatever
            // the receiver spelling.
            let assignments = assignment_lines(&class_text, &format!("self.{attr}"))
                + assignment_lines(&class_text, &format!("cls.{attr}"));
            if assignments != 1 {
                continue;
            }
            found.push(SetupReceiver {
                path: format!("self.{attr}"),
                callee: callee.clone(),
            });
            if hook_receiver == "cls" {
                found.push(SetupReceiver {
                    path: format!("cls.{attr}"),
                    callee,
                });
            }
        }
    }
    found
}

fn is_dotted_identifier_char(ch: char) -> bool {
    ch == '_' || ch == '.' || ch.is_ascii_alphanumeric()
}

/// Whether `text` rebinds `path` on any line: a plain, augmented, annotated or
/// tuple-target assignment, or a `for`/`with`/walrus target.
pub(super) fn rebinds_path(text: &str, path: &str) -> bool {
    assignment_lines(text, path) > 0
}

fn assignment_lines(text: &str, path: &str) -> usize {
    text.lines()
        .filter(|line| line_assigns_path(line, path))
        .count()
}

fn line_assigns_path(line: &str, path: &str) -> bool {
    let code = line.split('#').next().unwrap_or(line);
    let trimmed = code.trim_start();
    let mentions = occurrences(code, path).next().is_some();
    if mentions
        && (trimmed.starts_with("for ") || trimmed.starts_with("with ") || code.contains(":="))
    {
        return true;
    }
    let Some(eq) = assignment_eq(code) else {
        return false;
    };
    occurrences(&code[..eq], path).next().is_some()
}

/// Byte offsets of `path` as a whole dotted token in `text`.
fn occurrences<'a>(text: &'a str, path: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(path).filter_map(move |(idx, _)| {
        let before_ok = text[..idx]
            .chars()
            .next_back()
            .is_none_or(|ch| !(ch == '_' || ch == '.' || ch.is_ascii_alphanumeric()));
        let after_ok = text[idx + path.len()..]
            .chars()
            .next()
            .is_none_or(|ch| !(ch == '_' || ch.is_ascii_alphanumeric()));
        (before_ok && after_ok).then_some(idx)
    })
}

/// The byte offset of the line's top-level assignment `=` (plain, augmented or
/// annotated), skipping comparisons and keyword arguments in brackets.
fn assignment_eq(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    for (index, &byte) in bytes.iter().enumerate() {
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => {
                if bytes.get(index + 1) == Some(&b'=') {
                    return None;
                }
                let prev = index.checked_sub(1).map(|at| bytes[at]);
                let before_prev = index.checked_sub(2).map(|at| bytes[at]);
                match prev {
                    Some(b'=' | b'!') => {}
                    Some(b'<' | b'>') if before_prev != prev => {}
                    _ => return Some(index),
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_forms_rebind_and_comparisons_do_not() {
        let path = "self.stock";
        for line in [
            "self.stock = Stock(1)",
            "self.stock += 1",
            "self.stock: Stock = Stock(1)",
            "self.stock, other = pair",
            "for self.stock in items:",
            "(self.stock := make())",
        ] {
            assert!(line_assigns_path(line, path), "{line}");
        }
        for line in [
            "assert self.stock == 3",
            "assert self.stock >= 3",
            "self.assertEqual(self.stock.take(2), 3)",
            "self.stock_other = 4",
            "make(self.stock, n=1)",
            "# self.stock = 1",
        ] {
            assert!(!line_assigns_path(line, path), "{line}");
        }
    }
}
