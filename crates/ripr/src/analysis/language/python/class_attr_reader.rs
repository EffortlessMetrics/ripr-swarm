//! Non-string class-attribute value edits for the Python preview adapter
//! (#16009).
//!
//! A changed class-body `_PER_YEAR = 12` -> `13` is a value the class carries
//! to the methods that read it. A test that constructs the class and asserts
//! on a method that never reads the name reaches the owner without observing
//! the value, so its strong oracle is not discrimination of the edit. This is
//! the value counterpart of the string-text gate in `class_attr_pin.rs`
//! (#15294).
//!
//! The edit keeps its verdict when a related test with an oracle reads the
//! name itself or calls a method of the class that reads it, directly or
//! through a same-class call. Otherwise it reads `weakly_exposed`. The gate
//! engages only where the readers can be listed from the class alone, and
//! fails open (keeps the existing verdict) when:
//!
//! - the class has a decorator, a base other than `object` or a keyword
//!   (dataclass-like fields, inherited or metaclass readers);
//! - the name is a dunder (`__nonzero__ = __bool__` dispatches through the
//!   interpreter, not through a method call a test spells);
//! - another class-body statement, a nested class or a dunder method
//!   (`__init__`, `__str__`, ...) reads the name or a reader;
//! - no method of the class reads the name (it is read elsewhere, if at all);
//! - code of the same file outside the class mentions the name or a reader;
//! - any test mentions the name, related to the class or not.

use super::admission::mentions_word;
use super::class_attr_pin::ClassAttrPinGap;
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use super::{PythonOwner, PythonTest};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn python_class_attr_reader_gap(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    line: usize,
    related_candidates: &[PythonRelatedCandidate<'_>],
    all_tests: &[PythonTest],
) -> Option<ClassAttrPinGap> {
    let (name, new_value) = assigned_value(line_text)?;
    let (old_name, old_value) = assigned_value(old_line_text?)?;
    if name != old_name || old_value == new_value || is_dunder(&name) {
        return None;
    }
    let source = owner_source?;
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<class-attr-reader>"), source)
    else {
        return None;
    };
    let class = enclosing_class(&module.body, source, line)?;
    if owner.start_line != line_of(source, usize::from(class.range.start())) {
        return None;
    }
    let readers = readers_of(class, source, line, &name)?;
    // The same file outside the class can call a reader on an instance the
    // test never names, so the readers are not the whole list.
    let class_text =
        source.get(usize::from(class.range.start())..usize::from(class.range.end()))?;
    let outside = source.replacen(class_text, "", 1);
    if std::iter::once(&name)
        .chain(readers.iter())
        .any(|word| mentions_word(&outside, word))
    {
        return None;
    }
    let observed = related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            let body = &candidate.test.body_text;
            mentions_word(body, &name) || readers.iter().any(|reader| reads_attribute(body, reader))
        });
    // A test that reads the name without building the class
    // (`assert Clock._PER_YEAR == 12`) is not a related candidate, but it may
    // still pin the value.
    if observed
        || all_tests
            .iter()
            .any(|test| mentions_word(&test.body_text, &name))
    {
        return None;
    }
    let listed = readers
        .iter()
        .map(|reader| format!("`{reader}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(ClassAttrPinGap {
        discriminator: format!("class attribute `{name}` value `{new_value}`"),
        reason: format!(
            "no related test with an assertion reads `{name}` or calls a method that reads it ({listed}), so the tests reach `{}` without observing the changed value `{old_value}` -> `{new_value}`",
            owner.name
        ),
        next_step: format!(
            "Add an assertion on a method that reads `{name}` ({listed}) whose result differs between `{old_value}` and `{new_value}`."
        ),
    })
}

/// The name and value text of a line that is exactly `name = value` or
/// `name: T = value`, where the value is not a string constant (those are
/// read by `class_attr_pin.rs`).
fn assigned_value(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<class-attr-value>"), trimmed)
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
    if let Expr::Constant(constant) = value
        && matches!(constant.value, ast::Constant::Str(_))
    {
        return None;
    }
    let text = trimmed.get(usize::from(value.start())..usize::from(value.end()))?;
    Some((name.id.to_string(), text.to_string()))
}

/// The innermost class whose body holds, as a direct statement, the
/// assignment that starts on `line`.
fn enclosing_class<'a>(
    body: &'a [Stmt],
    source: &str,
    line: usize,
) -> Option<&'a ast::StmtClassDef> {
    for statement in body {
        let Stmt::ClassDef(class) = statement else {
            continue;
        };
        let start = line_of(source, usize::from(class.range.start()));
        let end = line_of(source, usize::from(class.range.end()));
        if line < start || line > end {
            continue;
        }
        if let Some(inner) = enclosing_class(&class.body, source, line) {
            return Some(inner);
        }
        let direct = class.body.iter().any(|member| {
            matches!(member, Stmt::Assign(_) | Stmt::AnnAssign(_))
                && line_of(source, usize::from(member.start())) == line
        });
        return direct.then_some(class);
    }
    None
}

/// The methods of `class` that read `name`, closed under same-class calls.
/// `None` when the readers cannot be listed from the class alone.
fn readers_of(
    class: &ast::StmtClassDef,
    source: &str,
    line: usize,
    name: &str,
) -> Option<BTreeSet<String>> {
    let plain_base = |base: &Expr| matches!(base, Expr::Name(base) if base.id.as_str() == "object");
    if !class.decorator_list.is_empty()
        || !class.keywords.is_empty()
        || !class.bases.iter().all(plain_base)
    {
        return None;
    }
    let mut methods = Vec::new();
    for member in &class.body {
        let text = source.get(usize::from(member.start())..usize::from(member.end()))?;
        match member {
            Stmt::FunctionDef(function) => methods.push((function.name.to_string(), text)),
            Stmt::AsyncFunctionDef(function) => methods.push((function.name.to_string(), text)),
            _ if line_of(source, usize::from(member.start())) == line => {}
            // Another class-body statement (a derived constant, a nested
            // class) reads the value outside any method.
            _ if mentions_word(text, name) => return None,
            _ => {}
        }
    }
    let mut readers = BTreeSet::new();
    for (method, text) in &methods {
        if mentions_word(text, name) {
            if is_dunder(method) {
                return None;
            }
            readers.insert(method.clone());
        }
    }
    if readers.is_empty() {
        return None;
    }
    loop {
        let mut grew = false;
        for (method, text) in &methods {
            if readers.contains(method)
                || !readers.iter().any(|reader| reads_attribute(text, reader))
            {
                continue;
            }
            if is_dunder(method) {
                return None;
            }
            readers.insert(method.clone());
            grew = true;
        }
        if !grew {
            return Some(readers);
        }
    }
}

/// Whether `text` reads `.attribute` as a whole identifier (`obj.years(...)`,
/// `self.years`, a property read).
fn reads_attribute(text: &str, attribute: &str) -> bool {
    let is_word = |c: char| c == '_' || c.is_alphanumeric();
    let needle = format!(".{attribute}");
    let mut from = 0usize;
    while let Some(rel) = text.get(from..).and_then(|rest| rest.find(&needle)) {
        let start = from + rel;
        let end = start + needle.len();
        if !text
            .get(end..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(is_word)
        {
            return true;
        }
        from = end;
    }
    false
}

fn is_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

/// The 1-based line of byte `offset` in `source`.
fn line_of(source: &str, offset: usize) -> usize {
    source
        .get(..offset)
        .map_or(0, |head| head.matches('\n').count())
        + 1
}

#[cfg(test)]
mod tests;
