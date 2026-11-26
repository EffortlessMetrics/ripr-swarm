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
//!
//! The same module finds methods a same-key field shadows (rule 28) and
//! methods an assignment replaces (rule 29: `this.total = ...` in the
//! constructor, `Cart.prototype.total = ...` or `Cart.total = ...` at the top
//! level).

use std::collections::BTreeSet;

use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, Class, ClassElement, Expression, MethodDefinition,
    MethodDefinitionKind, PropertyDefinition, PropertyKey, Statement, StaticMemberExpression,
};
use oxc_ast_visit::{Visit, walk};

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

/// Span starts of the methods, getters and setters in `class` that a field of
/// the same key and `static` placement shadows (#16306, RIPR-SPEC-0243 rule
/// 28).
///
/// An instance field (`total = (n) => n;`, or a bare `total;`) is defined on
/// every instance when it is constructed, after the prototype holds the
/// method, so `new Cart().total()` calls the field and never the method; a
/// static field replaces a static method on the constructor the same way.
/// Source order does not matter. A `declare` or `abstract` field defines
/// nothing at runtime and shadows nothing; a computed field key other than a
/// string literal is never compared; a field initialized from the method
/// itself ([`retains_same_member`]) still runs it.
pub(super) fn field_shadowed_method_starts(class: &Class<'_>) -> BTreeSet<u32> {
    let fields: BTreeSet<(bool, String)> = class
        .body
        .body
        .iter()
        .filter_map(|element| match element {
            ClassElement::PropertyDefinition(field)
                if !field.declare && !is_abstract(field) && !retains_same_member(field) =>
            {
                field_key(field).map(|key| (field.r#static, key))
            }
            _ => None,
        })
        .collect();
    if fields.is_empty() {
        return BTreeSet::new();
    }
    class
        .body
        .body
        .iter()
        .filter_map(|element| match element {
            ClassElement::MethodDefinition(method)
                if method.value.body.is_some()
                    && method.kind != MethodDefinitionKind::Constructor =>
            {
                let key = runtime_key(method)?;
                fields
                    .contains(&(method.r#static, key))
                    .then_some(method.span.start)
            }
            _ => None,
        })
        .collect()
}

/// Whether `field` is initialized from the member it shadows, read while the
/// field is defined: `total = this.total;` or `total = this.total.bind(this);`
/// (`this` is the instance in an instance field and the class in a static
/// one). The prototype or static method is captured before the field is
/// installed, so calling the field runs the method body, and the field does
/// not cut the method off. Only these two unambiguous shapes count; any other
/// initializer (`total = (n) => this.total(n)` reads the field itself at call
/// time) still shadows.
fn retains_same_member(field: &PropertyDefinition<'_>) -> bool {
    let (Some(value), PropertyKey::StaticIdentifier(key)) = (&field.value, &field.key) else {
        return false;
    };
    if field.computed {
        return false;
    }
    let target = match value.without_parentheses() {
        Expression::CallExpression(call) => match call.callee.without_parentheses() {
            Expression::StaticMemberExpression(bind) if bind.property.name == "bind" => {
                &bind.object
            }
            _ => return false,
        },
        other => other,
    };
    matches!(
        target.without_parentheses(),
        Expression::StaticMemberExpression(member)
            if member.property.name == key.name
                && matches!(member.object.without_parentheses(), Expression::ThisExpression(_))
    )
}

fn is_abstract(field: &PropertyDefinition<'_>) -> bool {
    matches!(
        field.r#type,
        oxc_ast::ast::PropertyDefinitionType::TSAbstractPropertyDefinition
    )
}

/// The property key a field defines, in the key space of [`runtime_key`].
fn field_key(field: &PropertyDefinition<'_>) -> Option<String> {
    match &field.key {
        PropertyKey::StaticIdentifier(identifier) if !field.computed => {
            Some(format!("public:{}", identifier.name))
        }
        PropertyKey::PrivateIdentifier(identifier) if !field.computed => {
            Some(format!("private:{}", identifier.name))
        }
        PropertyKey::StringLiteral(literal) => Some(format!("public:{}", literal.value)),
        _ => None,
    }
}

/// The property key `method` defines, when it is static text: an identifier
/// or string-literal key (bracketed or not) as a public key, a private name
/// as a private key. The two spaces never meet: `#total()` and `'#total'()`
/// are different keys.
fn runtime_key(method: &MethodDefinition<'_>) -> Option<String> {
    match &method.key {
        PropertyKey::StaticIdentifier(identifier) if !method.computed => {
            Some(format!("public:{}", identifier.name))
        }
        PropertyKey::PrivateIdentifier(identifier) if !method.computed => {
            Some(format!("private:{}", identifier.name))
        }
        PropertyKey::StringLiteral(literal) => Some(format!("public:{}", literal.value)),
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

/// Span starts of the ordinary instance methods of `class` that an
/// unconditional `this.NAME = ...` statement of its constructor replaces
/// (#16314, RIPR-SPEC-0243 rule 29).
///
/// The assignment defines an own property on each instance when it is
/// constructed, so `new Cart().total()` calls the assigned value, never the
/// prototype method. Only a plain `=` expression statement at the top level
/// of the constructor body counts: one inside an `if`, a loop or a callback
/// may not run. A right-hand side that reads the same member
/// (`this.total = this.total.bind(this)`) keeps the method running and does
/// not count. Getters, setters and static methods are never marked: an
/// assignment to an accessor calls its setter, and `this` in a constructor is
/// the instance.
pub(super) fn constructor_replaced_method_starts(class: &Class<'_>) -> BTreeSet<u32> {
    let Some(body) = class.body.body.iter().find_map(|element| match element {
        ClassElement::MethodDefinition(method)
            if method.kind == MethodDefinitionKind::Constructor =>
        {
            method.value.body.as_ref()
        }
        _ => None,
    }) else {
        return BTreeSet::new();
    };
    let replaced: BTreeSet<String> = body
        .statements
        .iter()
        .filter_map(|statement| {
            let (object, key, value) = member_assignment(statement)?;
            (matches!(object.get_inner_expression(), Expression::ThisExpression(_))
                && !reads_member(value, MemberObject::This, key))
            .then(|| format!("public:{key}"))
        })
        .collect();
    if replaced.is_empty() {
        return BTreeSet::new();
    }
    class
        .body
        .body
        .iter()
        .filter_map(|element| match element {
            ClassElement::MethodDefinition(method)
                if method.kind == MethodDefinitionKind::Method
                    && !method.r#static
                    && method.value.body.is_some() =>
            {
                let key = runtime_key(method)?;
                replaced.contains(&key).then_some(method.span.start)
            }
            _ => None,
        })
        .collect()
}

/// The `(class name, member name, static)` triples that an unconditional
/// top-level statement of `body` replaces on a class (#16314, RIPR-SPEC-0243
/// rule 29): `Cart.prototype.total = ...` replaces the instance method
/// `total` of `Cart`, `Cart.total = ...` its static method. A right-hand side
/// that reads the same member (`Cart.prototype.total =
/// wrap(Cart.prototype.total)`) keeps the method running and does not count.
pub(super) fn top_level_replaced_members(
    body: &[Statement<'_>],
) -> BTreeSet<(String, String, bool)> {
    body.iter()
        .filter_map(|statement| {
            let (object, key, value) = member_assignment(statement)?;
            let (class_name, is_static) = match object.get_inner_expression() {
                Expression::Identifier(class) => (class.name.as_str(), true),
                Expression::StaticMemberExpression(prototype)
                    if prototype.property.name == "prototype" =>
                {
                    match prototype.object.get_inner_expression() {
                        Expression::Identifier(class) => (class.name.as_str(), false),
                        _ => return None,
                    }
                }
                _ => return None,
            };
            let object = if is_static {
                MemberObject::Class(class_name)
            } else {
                MemberObject::Prototype(class_name)
            };
            (!reads_member(value, object, key))
                .then(|| (class_name.to_string(), key.to_string(), is_static))
        })
        .collect()
}

/// `(object, NAME, value)` of a plain `object.NAME = value;` expression
/// statement. Callers read `object` through parentheses and TypeScript casts
/// (`(Cart.prototype as any).total = ...`, `(this as Cart).total = ...`).
fn member_assignment<'s, 'a>(
    statement: &'s Statement<'a>,
) -> Option<(&'s Expression<'a>, &'s str, &'s Expression<'a>)> {
    let Statement::ExpressionStatement(statement) = statement else {
        return None;
    };
    let Expression::AssignmentExpression(assign) = statement.expression.without_parentheses()
    else {
        return None;
    };
    if assign.operator != AssignmentOperator::Assign {
        return None;
    }
    let AssignmentTarget::StaticMemberExpression(target) = &assign.left else {
        return None;
    };
    Some((&target.object, target.property.name.as_str(), &assign.right))
}

/// The receiver a replaced member is read from.
#[derive(Clone, Copy)]
enum MemberObject<'n> {
    /// `this`.
    This,
    /// `NAME`, for a static member.
    Class(&'n str),
    /// `NAME.prototype`, for an instance member.
    Prototype(&'n str),
}

impl MemberObject<'_> {
    fn matches(self, object: &Expression<'_>) -> bool {
        match (self, object.get_inner_expression()) {
            (Self::This, Expression::ThisExpression(_)) => true,
            (Self::Class(name), Expression::Identifier(identifier)) => identifier.name == name,
            (Self::Prototype(name), Expression::StaticMemberExpression(prototype)) => {
                prototype.property.name == "prototype"
                    && matches!(
                        prototype.object.get_inner_expression(),
                        Expression::Identifier(identifier) if identifier.name == name
                    )
            }
            _ => false,
        }
    }
}

/// Whether `value` reads `object.key` anywhere, so the original member is
/// captured (bound, wrapped or kept) by the value that replaces it.
fn reads_member(value: &Expression<'_>, object: MemberObject<'_>, key: &str) -> bool {
    struct Reads<'n> {
        object: MemberObject<'n>,
        key: &'n str,
        found: bool,
    }
    impl<'a> Visit<'a> for Reads<'_> {
        fn visit_static_member_expression(&mut self, member: &StaticMemberExpression<'a>) {
            if self.found {
                return;
            }
            self.found = member.property.name == self.key && self.object.matches(&member.object);
            walk::walk_static_member_expression(self, member);
        }
    }
    let mut reads = Reads {
        object,
        key,
        found: false,
    };
    reads.visit_expression(value);
    reads.found
}
