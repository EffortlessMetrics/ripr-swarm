//! Classes that inherit a changed instance member (#15924).
//!
//! A test that builds `new SpecialCart()` and calls `apply` runs
//! `BaseCart.apply` when `SpecialCart extends BaseCart` and nothing between
//! them supplies another `apply`:
//!
//! ```ts
//! // src/cart.ts
//! export class BaseCart { apply(amount: number) { /* changed */ } }
//! // src/special.ts
//! import { BaseCart } from './cart';
//! export class SpecialCart extends BaseCart { extra() { return 1; } }
//! ```
//!
//! This module records, per source file, the top-level class declarations
//! that extend a plain identifier, with every name the class can put on an
//! instance or its prototype. The workspace-level walk that binds a base
//! identifier to the changed owner's class is
//! `ReExportIndex::inheriting_subclasses`; it reads these facts and refuses a
//! class that declares the member, or that is [`ClassHeritageFact::opaque`].
//!
//! Syntax-only and fail-closed. A class is opaque (never followed) when it
//! or its file can supply or replace a member without naming it as a plain
//! identifier key: a decorator, a computed, string or numeric key, a static
//! block, a constructor that returns a value, `this[...]`, `Object.assign(
//! this`, a second declaration or a reassignment of the class name, or any
//! mention in the file of `defineProperty`, `defineProperties`,
//! `setPrototypeOf`, `__proto__` or `<Class>.prototype`. A class expression,
//! a class nested in a function or namespace, and a mixin, call or member
//! heritage (`extends mixin(Base)`, `extends ns.Base`) are not recorded.

use super::*;
use std::collections::BTreeMap;

/// One top-level class declaration that extends a plain identifier.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ClassHeritageFact {
    /// The class's own name.
    pub(crate) name: String,
    /// The identifier after `extends`, as written in the file.
    pub(crate) base: String,
    /// The class is importable under its own name or as the default export:
    /// `export class`, `export default class`, or a same-name `export { C }`.
    pub(crate) exported: bool,
    /// `export default class`.
    pub(crate) default_export: bool,
    /// Every name the class declares or assigns on `this`: methods,
    /// accessors, fields (instance and static), constructor parameter
    /// properties and `this.<name> =` targets.
    pub(crate) members: Vec<String>,
    /// The class may supply or replace a member this record cannot name.
    pub(crate) opaque: bool,
}

impl ClassHeritageFact {
    /// Approximate retained bytes, for the export-facts cache bound.
    pub(crate) fn approx_bytes(&self) -> usize {
        self.name.len()
            + self.base.len()
            + self
                .members
                .iter()
                .map(|member| member.len() + 1)
                .sum::<usize>()
            + 4
    }
}

/// File-wide text that can rewire any class without naming its members.
const FILE_OPAQUE_WORDS: [&str; 4] = [
    "defineProperty",
    "defineProperties",
    "setPrototypeOf",
    "__proto__",
];

/// The heritage facts of one parsed module.
pub(crate) fn class_heritage_facts(
    body: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &str,
) -> Vec<ClassHeritageFact> {
    // (class, exported, default export)
    let mut classes: Vec<(&Class<'_>, bool, bool)> = Vec::new();
    let mut listed_exports: Vec<String> = Vec::new();
    for statement in body {
        match statement {
            Statement::ClassDeclaration(class) => classes.push((class, false, false)),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::ClassDeclaration(class)) = export.declaration.as_ref() {
                    classes.push((class, true, false));
                } else if export.declaration.is_none()
                    && export.source.is_none()
                    && export.export_kind != ImportOrExportKind::Type
                {
                    for specifier in &export.specifiers {
                        if specifier.export_kind == ImportOrExportKind::Type {
                            continue;
                        }
                        if let (Some(local), Some(exported)) = (
                            module_export_name_text(&specifier.local),
                            module_export_name_text(&specifier.exported),
                        ) && local == exported
                        {
                            listed_exports.push(local);
                        }
                    }
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    classes.push((class, true, true));
                }
            }
            _ => {}
        }
    }
    if !classes
        .iter()
        .any(|(class, _, _)| class.super_class.is_some())
    {
        return Vec::new();
    }
    let mut declared: BTreeMap<&str, usize> = BTreeMap::new();
    for (class, _, _) in &classes {
        if let Some(id) = class.id.as_ref() {
            *declared.entry(id.name.as_str()).or_default() += 1;
        }
    }
    let file_opaque = FILE_OPAQUE_WORDS.iter().any(|word| source.contains(word));
    let mut facts = Vec::new();
    for (class, exported, default_export) in classes {
        let (Some(id), Some(base)) = (class.id.as_ref(), class.super_class.as_ref()) else {
            continue;
        };
        let Expression::Identifier(base) = unwrap_type_wrappers(base) else {
            continue;
        };
        let name = id.name.as_str();
        let text = source
            .get(class.span.start as usize..class.span.end as usize)
            .unwrap_or_default();
        let mut members: Vec<String> = Vec::new();
        let mut opaque = file_opaque
            || !class.decorators.is_empty()
            || declared.get(name) != Some(&1)
            || super::tests_extract::identifier_written_strictly(source, name)
            || source.contains(&format!("{name}.prototype"))
            || text.is_empty()
            || text.contains("this[")
            || text.contains("Object.assign(this");
        for element in &class.body.body {
            let (key, computed) = match element {
                ClassElement::MethodDefinition(method) => {
                    if !method.decorators.is_empty() {
                        opaque = true;
                    }
                    if method.kind == oxc_ast::ast::MethodDefinitionKind::Constructor {
                        let constructor = source
                            .get(method.span.start as usize..method.span.end as usize)
                            .unwrap_or_default();
                        // A constructor that returns a value hands back
                        // another object.
                        if returns_a_value(constructor) {
                            opaque = true;
                        }
                        for parameter in &method.value.params.items {
                            if parameter.accessibility.is_some()
                                || parameter.readonly
                                || parameter.r#override
                            {
                                match parameter.pattern.get_binding_identifier() {
                                    Some(binding) => {
                                        members.push(binding.name.as_str().to_string())
                                    }
                                    None => opaque = true,
                                }
                            }
                        }
                    }
                    (&method.key, method.computed)
                }
                ClassElement::PropertyDefinition(field) => {
                    if !field.decorators.is_empty() {
                        opaque = true;
                    }
                    (&field.key, field.computed)
                }
                ClassElement::AccessorProperty(field) => {
                    if !field.decorators.is_empty() {
                        opaque = true;
                    }
                    (&field.key, field.computed)
                }
                ClassElement::StaticBlock(_) => {
                    opaque = true;
                    continue;
                }
                ClassElement::TSIndexSignature(_) => continue,
            };
            match key {
                // A `#private` name is never the public member.
                PropertyKey::PrivateIdentifier(_) => {}
                key => match property_key_name(key).filter(|_| !computed) {
                    Some(member) => members.push(member),
                    None => opaque = true,
                },
            }
        }
        members.extend(this_rebound_names(text));
        members.sort();
        members.dedup();
        facts.push(ClassHeritageFact {
            name: name.to_string(),
            base: base.name.as_str().to_string(),
            exported: exported || listed_exports.iter().any(|listed| listed == name),
            default_export,
            members,
            opaque,
        });
    }
    facts
}

/// `true` when `constructor` holds a `return` that is not a bare `return;`.
/// Text-level, nested functions included: it can only over-report.
fn returns_a_value(constructor: &str) -> bool {
    constructor.match_indices("return").any(|(idx, _)| {
        let before = constructor[..idx].chars().next_back();
        let rest = &constructor[idx + "return".len()..];
        before.is_none_or(|ch| !is_javascript_identifier_char(ch) && ch != '.')
            && rest
                .chars()
                .next()
                .is_none_or(|ch| !is_javascript_identifier_char(ch))
            && !rest.trim_start().starts_with([';', '}'])
    })
}

/// Names the class text assigns, deletes or defines on `this`.
fn this_rebound_names(class_text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (idx, _) in class_text.match_indices("this.") {
        let name: String = class_text[idx + "this.".len()..]
            .chars()
            .take_while(|ch| is_javascript_identifier_char(*ch))
            .collect();
        if !name.is_empty()
            && !names.contains(&name)
            && super::class_entries::rebinds_this_member(class_text, &name)
        {
            names.push(name);
        }
    }
    names
}
