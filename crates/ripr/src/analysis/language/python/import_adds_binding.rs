//! An import line edited only to import more names (#14157).
//!
//! `from typing import Any` rewritten as `from typing import Any, cast`
//! is paired by Git with the old import line, so it reads as an edited
//! statement and got a `predicate` probe on `<module>`. A wholly new import
//! line is already no probe of its own; this module recognises the edit that
//! is the same thing in place: every old binding is still imported, the same
//! way, and the added names bind nothing the old file already mentioned.
//!
//! Everything else keeps its probe: a changed module, a dropped or renamed
//! binding, a changed alias, a star import, `__future__`, a statement that
//! spans lines or shares its line, and an added name the old file already
//! used (the import would rebind it).

use super::module_constants::mentions_identifier;

/// One imported name and its `as` alias.
#[derive(Debug, PartialEq, Eq)]
struct ImportedName<'a> {
    name: &'a str,
    alias: Option<&'a str>,
}

impl ImportedName<'_> {
    /// The name the statement binds in the importing scope.
    fn local_name(&self, plain_import: bool) -> &str {
        match self.alias {
            Some(alias) => alias,
            // `import a.b` binds `a`.
            None if plain_import => self.name.split('.').next().unwrap_or(self.name),
            None => self.name,
        }
    }
}

/// A single-line import statement.
#[derive(Debug)]
struct ImportLine<'a> {
    indent: &'a str,
    /// `None` for `import a, b`; the module text for `from M import ...`.
    from_module: Option<&'a str>,
    names: Vec<ImportedName<'a>>,
}

fn is_dotted_name(text: &str, allow_dots: bool) -> bool {
    !text.is_empty()
        && text.split('.').all(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .is_some_and(|ch| ch.is_alphabetic() || ch == '_')
                && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
        })
        && (allow_dots || !text.contains('.'))
}

/// `from` modules may be relative: `.`, `..pkg`, `pkg.sub`.
fn is_from_module(text: &str) -> bool {
    let rest = text.trim_start_matches('.');
    !text.is_empty() && (rest.is_empty() || is_dotted_name(rest, true))
}

fn parse_import_line(line: &str) -> Option<ImportLine<'_>> {
    // An import statement holds no string, so `#` always opens a comment.
    let code = line.split('#').next().unwrap_or(line).trim_end();
    if code.contains(';') || code.ends_with('\\') {
        return None;
    }
    let trimmed = code.trim_start();
    let indent = &code[..code.len() - trimmed.len()];
    let (from_module, list) = if let Some(rest) = trimmed.strip_prefix("from ") {
        let (module, list) = rest.split_once(" import ")?;
        let module = module.trim();
        if !is_from_module(module) || module == "__future__" {
            return None;
        }
        (Some(module), list.trim())
    } else {
        (None, trimmed.strip_prefix("import ")?.trim())
    };
    // `from m import (a, b)` on one line; an unclosed bracket spans lines.
    let list = match (from_module, list.strip_prefix('(')) {
        (Some(_), Some(inner)) => inner.strip_suffix(')')?,
        (None, Some(_)) => return None,
        (_, None) => list,
    };
    if list.contains(['(', ')', '*']) {
        return None;
    }
    let mut names = Vec::new();
    let mut parts = list.split(',').map(str::trim).peekable();
    while let Some(part) = parts.next() {
        // A trailing comma is legal inside the brackets only; either way it
        // adds no name.
        if part.is_empty() && parts.peek().is_none() && !names.is_empty() {
            break;
        }
        let words: Vec<&str> = part.split_whitespace().collect();
        let (name, alias) = match words.as_slice() {
            [name] => (*name, None),
            [name, "as", alias] if is_dotted_name(alias, false) => (*name, Some(*alias)),
            _ => return None,
        };
        if !is_dotted_name(name, from_module.is_none()) {
            return None;
        }
        names.push(ImportedName { name, alias });
    }
    (!names.is_empty()).then_some(ImportLine {
        indent,
        from_module,
        names,
    })
}

/// Whether `new_line` is `old_line` with more imported names and nothing else
/// changed, and none of the added names was mentioned in `old_source`.
///
/// For `import a, b` the old modules must keep their order, because the
/// order of that list is the order the modules load in.
pub(super) fn import_line_only_adds_bindings(
    old_line: &str,
    new_line: &str,
    old_source: &str,
) -> bool {
    let (Some(old), Some(new)) = (parse_import_line(old_line), parse_import_line(new_line)) else {
        return false;
    };
    if old.indent != new.indent
        || old.from_module != new.from_module
        || new.names.len() <= old.names.len()
    {
        return false;
    }
    let plain_import = new.from_module.is_none();
    let kept = if plain_import {
        let mut remaining = new.names.iter();
        old.names
            .iter()
            .all(|name| remaining.any(|candidate| candidate == name))
    } else {
        old.names.iter().all(|name| new.names.contains(name))
    };
    kept && new
        .names
        .iter()
        .filter(|name| !old.names.contains(name))
        .all(|name| !mentions_identifier(old_source, name.local_name(plain_import)))
}
