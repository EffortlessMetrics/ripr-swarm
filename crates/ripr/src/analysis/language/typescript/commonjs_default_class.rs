//! A class that is a CommonJS module's whole export (`class Cart {}` then
//! `module.exports = Cart;`), RIPR-SPEC-0243 rule 19 as amended for #15092.
//!
//! `owners::mark_commonjs_default_binding` calls in here after its file-level
//! `module.exports` gate held.

use super::*;
use oxc_ast::ast::Statement;

/// Marks every member of the class `class_name` as the module's whole export
/// when [`commonjs_default_class`] holds, and reports whether it did.
///
/// `module.exports = Cart;` of one declared class: `require()` hands back the
/// class itself, so its members carry the default-class marker together with
/// the CommonJS one that licenses a `require()` binding (an ES default class
/// is `require(...).default` instead).
pub(super) fn mark_whole_export_class(
    program: &oxc_ast::ast::Program<'_>,
    class_name: &str,
    code: &str,
    division_code: &str,
    owners: &mut [TypeScriptOwner],
) -> bool {
    if !commonjs_default_class(program, class_name, code, division_code) {
        return false;
    }
    for owner in owners.iter_mut() {
        if owner.class_name.as_deref() == Some(class_name) {
            owner.class_default_export = true;
            owner.commonjs_default_export = true;
        }
    }
    true
}

/// `true` when `name` is one top-level `class name {}` declaration that
/// `module.exports = name;` hands out as the module's whole export.
///
/// Called only after the file-level `module.exports` gate held. Fail-closed
/// identity checks on top of it: exactly one file-level declaration binds the
/// name and it is a non-`declare` class; neither mask shows a write to the
/// name (a class binding is assignable); and the file adds no named export
/// (`module.exports.Cart = Cart`, `exports.Cart = ...`), because the marker
/// makes a named `require` of the class stop relating. A class exported both
/// ways keeps its named reading.
fn commonjs_default_class(
    program: &oxc_ast::ast::Program<'_>,
    name: &str,
    code: &str,
    division_code: &str,
) -> bool {
    let classes = program
        .body
        .iter()
        .filter(|stmt| {
            matches!(stmt, Statement::ClassDeclaration(class)
                if !class.declare && class.id.as_ref().is_some_and(|id| id.name == name))
        })
        .count();
    if classes != 1 {
        return false;
    }
    let mut declared = Vec::new();
    super::tests_extract::collect_registration_scope_names(&program.body, &mut declared, false);
    if declared
        .iter()
        .filter(|other| other.as_str() == name)
        .count()
        != 1
    {
        return false;
    }
    if super::owners::identifier_writes(&program.body).contains(name) {
        return false;
    }
    [code, division_code].iter().all(|text| {
        let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
        !compact.contains("exports.")
            && !compact.contains("exports[")
            && !super::tests_extract::identifier_written_strictly(text, name)
            && !class_binding_gains_member(text, &compact, name)
    })
}

/// `true` when the file may add a property to the class binding `name`: a
/// member assignment (`Cart.Cart = Cart`, `Cart['x'] ??= y`, `Cart.n++`) or
/// an `Object.assign` / `defineProperty` / `defineProperties` /
/// `setPrototypeOf` call whose first argument is the name. A static property
/// is a named export of the whole-export class (`const { Cart } =
/// require(...)` reads `Cart.Cart`), so the class keeps its named reading. A
/// false hit only leaves the class unmarked.
fn class_binding_gains_member(text: &str, compact: &str, name: &str) -> bool {
    let object_call = [
        "assign(",
        "defineProperty(",
        "defineProperties(",
        "setPrototypeOf(",
    ]
    .iter()
    .any(|call| {
        compact.match_indices(call).any(|(at, call)| {
            compact[at + call.len()..]
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with(',') || rest.starts_with(')'))
        })
    });
    object_call
        || super::tests_extract::identifier_occurrences(text, name)
            .any(|at| member_chain_is_assigned(&text[at + name.len()..]))
}

/// `rest` is the text after an identifier: `true` when a member chain
/// (`.a`, `?.a`, `[...]`) opens there and the chain is the target of an
/// assignment or an update (`=`, `+=`, `??=`, `++`, `--`; not `==` or `=>`).
fn member_chain_is_assigned(rest: &str) -> bool {
    let mut depth = 0usize;
    let mut saw_member = false;
    let mut end = rest.len();
    for (at, ch) in rest.char_indices() {
        if depth > 0 {
            match ch {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            continue;
        }
        match ch {
            '[' => {
                depth = 1;
                saw_member = true;
            }
            '.' => saw_member = true,
            '?' if rest[at..].starts_with("?.") => {}
            ch if ch.is_whitespace() || ch.is_alphanumeric() || ch == '_' || ch == '$' => {}
            _ => {
                end = at;
                break;
            }
        }
    }
    if !saw_member || depth > 0 {
        return false;
    }
    let tail = &rest[end..];
    if tail.starts_with("++") || tail.starts_with("--") {
        return true;
    }
    let operator_len = tail
        .char_indices()
        .find(|(_, ch)| {
            !matches!(
                ch,
                '+' | '-' | '*' | '/' | '%' | '<' | '>' | '&' | '|' | '^' | '?'
            )
        })
        .map_or(tail.len(), |(at, _)| at);
    // `<=` and `>=` compare; `<<=`, `>>=` and `>>>=` assign.
    if matches!(&tail[..operator_len], "<" | ">") {
        return false;
    }
    let after = &tail[operator_len..];
    if after.starts_with("==") || after.starts_with("=>") {
        return false;
    }
    after.starts_with('=')
}
