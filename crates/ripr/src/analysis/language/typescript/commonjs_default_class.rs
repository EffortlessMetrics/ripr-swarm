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

/// Clears the whole-export markers of an inline `module.exports = class ...`
/// when the file also adds a named export (`module.exports.helper = ...`,
/// `exports.Cart = ...`): `require()` then hands back the class with an extra
/// property, and a named `require` of that property is not the class.
/// Called only after the file-level `module.exports` gate held; a class
/// marked through `module.exports = Name;` already passed the same check in
/// [`commonjs_default_class`].
pub(super) fn keep_inline_class_whole_export(
    code: &str,
    division_code: &str,
    owners: &mut [TypeScriptOwner],
) {
    let named_export = [code, division_code].iter().any(|text| {
        let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
        compact.contains("exports.") || compact.contains("exports[")
    });
    if !named_export {
        return;
    }
    for owner in owners
        .iter_mut()
        .filter(|owner| owner.commonjs_default_export)
    {
        unmark_inline_class_member(owner);
    }
}

/// Withdraws the whole-export reading from a member of an inline
/// `module.exports = class ...` (the only owners that carry both markers
/// before the declared-class pass runs). The class expression's own name
/// binds only inside its body, so once the class is not the provable whole
/// export the name is not a module binding either: it is cleared, and no
/// named import or same-file construction can reach the member through it.
pub(super) fn unmark_inline_class_member(owner: &mut TypeScriptOwner) {
    if owner.class_default_export && owner.commonjs_default_export {
        owner.class_default_export = false;
        owner.commonjs_default_export = false;
        owner.class_name = None;
    }
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

/// Marks the top-level function owners other than `target` as unreachable by
/// name when `module.exports = target;` (or `export = target;`) is the
/// module's whole export and nothing adds them back (#16366).
///
/// `require()` then hands back `target` itself, so a named binding
/// (`const { total } = require(...)`) or a namespace member (`lib.total`)
/// reads a property of `target`, never a sibling function. Called only after
/// the file-level `module.exports` gate held. The sibling stays readable when
/// the file may add a named property: any `exports.` / `exports[` spelling,
/// any ES export statement, a member written onto `target` or an
/// `Object.assign` / `defineProperty` onto it, a second declaration of
/// `target` (a merged `namespace target {}` exports its members as
/// properties), a `target` that is not a declared function or a plain
/// declared class, or any use of the name other than its declaration, a call
/// or construction, and the whole-export write (an alias could add the
/// property).
pub(super) fn mark_unexported_siblings(
    program: &oxc_ast::ast::Program<'_>,
    target: &str,
    code: &str,
    division_code: &str,
    owners: &mut [TypeScriptOwner],
) {
    let es_export = program.body.iter().any(|stmt| {
        matches!(
            stmt,
            Statement::ExportNamedDeclaration(_)
                | Statement::ExportDefaultDeclaration(_)
                | Statement::ExportAllDeclaration(_)
        )
    });
    if es_export {
        return;
    }
    let mut declared = Vec::new();
    super::tests_extract::collect_registration_scope_names(&program.body, &mut declared, false);
    if declared
        .iter()
        .filter(|name| name.as_str() == target)
        .count()
        != 1
    {
        return;
    }
    // Only a value whose own properties are all written by statements the
    // checks below read: a declared function, or a declared class with no
    // heritage, static member or static block. An object, a call result or a
    // `require()` may carry the sibling as a property from the start
    // (`const api = { total }; module.exports = api;`, review of #16376).
    let shape_known = program.body.iter().any(|stmt| match stmt {
        Statement::FunctionDeclaration(function) => {
            function.id.as_ref().is_some_and(|id| id.name == target)
        }
        Statement::ClassDeclaration(class) => {
            !class.declare
                && class.super_class.is_none()
                && class.id.as_ref().is_some_and(|id| id.name == target)
                && class
                    .body
                    .body
                    .iter()
                    .all(|element| !element.r#static() && !element.is_static_block())
        }
        _ => false,
    });
    if !shape_known {
        return;
    }
    let adds_property = [code, division_code].iter().any(|text| {
        let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
        compact.contains("exports.")
            || compact.contains("exports[")
            || class_binding_gains_member(text, &compact, target)
            || binding_escapes(text, target)
    });
    if adds_property {
        return;
    }
    for owner in owners.iter_mut().filter(|owner| {
        owner.class_name.is_none()
            && owner.owner_kind == OwnerKind::Function
            && owner.name != target
    }) {
        owner.commonjs_whole_function = true;
    }
}

/// `true` when `name` occurs in `text` other than as its own `function name`
/// / `class name` declaration, a call or construction (`name(`), or the
/// right side of the whole-export write (`module.exports = name`, `export =
/// name`). Any other use (`const alias = name`, `f(name)`, `typeof name`) may
/// let a write through another reference add a property.
fn binding_escapes(text: &str, name: &str) -> bool {
    super::tests_extract::identifier_occurrences(text, name).any(|at| {
        let before = text[..at].trim_end();
        let after = text[at + name.len()..].trim_start();
        let declaration = before.ends_with("function") || before.ends_with("class");
        let whole_export = before.strip_suffix('=').is_some_and(|lhs| {
            let lhs = lhs.trim_end();
            lhs.ends_with("module.exports") || lhs.ends_with("export")
        });
        !(after.starts_with('(') || declaration || whole_export)
    })
}
