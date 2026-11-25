//! Class methods a later same-key member overwrites (#16261),
//! RIPR-SPEC-0243 rule 26.
//!
//! A class body defines its members in order, so a later member with the same
//! key and the same placement (prototype or constructor) replaces an earlier
//! one: `class Cart { total() { return 1; } total() { return 2; } }` leaves
//! only the second `total` reachable. The earlier body still yields an owner
//! (its change is reported), but no test can execute it, so
//! [`super::owners::owners_from_class`] marks it
//! [`TypeScriptMethodKind::Overwritten`] and the relation funnel gives it no
//! related test.

use std::collections::BTreeSet;

use oxc_ast::ast::{Class, ClassElement, MethodDefinition, MethodDefinitionKind, PropertyKey};

/// Span starts of the methods in `class` whose key a later member overwrites.
///
/// Counted as a later definition of the same key: a method, getter or setter
/// with a body, the same `static` placement and the same property key, where
/// `total`, `'total'`, `"total"` and `['total']` all name the key `total` and
/// `#total` stays a distinct private key. A getter and a setter together form
/// one accessor property and do not overwrite each other. Body-less
/// signatures (TypeScript overloads) define nothing at runtime and are
/// ignored on both sides; any other computed key (`[name]()`) is never
/// compared, so it does not make an earlier method dead.
pub(super) fn overwritten_method_starts(class: &Class<'_>) -> BTreeSet<u32> {
    let methods: Vec<(&MethodDefinition<'_>, String)> = class
        .body
        .body
        .iter()
        .filter_map(|element| match element {
            ClassElement::MethodDefinition(method)
                if method.value.body.is_some()
                    && method.kind != MethodDefinitionKind::Constructor =>
            {
                runtime_key(method).map(|key| (&**method, key))
            }
            _ => None,
        })
        .collect();
    let mut dead = BTreeSet::new();
    for (index, (earlier, key)) in methods.iter().enumerate() {
        let replaced = methods[index + 1..].iter().any(|(later, later_key)| {
            later_key == key
                && later.r#static == earlier.r#static
                && !accessor_pair(earlier.kind, later.kind)
        });
        if replaced {
            dead.insert(earlier.span.start);
        }
    }
    dead
}

/// The property key `method` defines, when it is static text: an identifier
/// or string-literal key (bracketed or not) by its name, a private name with
/// its `#`.
fn runtime_key(method: &MethodDefinition<'_>) -> Option<String> {
    match &method.key {
        PropertyKey::StaticIdentifier(identifier) if !method.computed => {
            Some(identifier.name.to_string())
        }
        PropertyKey::PrivateIdentifier(identifier) if !method.computed => {
            Some(format!("#{}", identifier.name))
        }
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        _ => None,
    }
}

/// `true` for a getter and a setter of one key, in either order.
fn accessor_pair(earlier: MethodDefinitionKind, later: MethodDefinitionKind) -> bool {
    matches!(
        (earlier, later),
        (MethodDefinitionKind::Get, MethodDefinitionKind::Set)
            | (MethodDefinitionKind::Set, MethodDefinitionKind::Get)
    )
}
