//! CommonJS re-export edges for the bounded re-export index (#15284).
//!
//! A CommonJS barrel forwards another module's export without an ES
//! `export ... from`:
//!
//! ```js
//! module.exports = { parse: require('./parse') };
//! exports.parse = require('./parse').parse;
//! module.exports = require('./parse');
//! var parse = require('./parse'); module.exports = { parse: parse };
//! ```
//!
//! Each shape becomes an edge the ESM resolver already follows: a named edge
//! `(barrel, exported) -> (original, source)` (`original` is `default` when
//! the whole `require()` value is forwarded) or a whole-module forward
//! `barrel -> source`. The extraction is syntax-only and fail-closed: only
//! top-level statements with a literal relative `require()` are read, and the
//! whole file is refused when any other use of `module`, `exports` or a local
//! `require` binding could rewrite the exports.

use super::owners::{
    object_binding_key_name, require_member_source, require_string_literal_source,
};
use super::related_tests::{normalized_module_path, normalized_relative_import_module};
use super::tsconfig::TsAliasMap;
use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BindingPattern, Expression, ObjectPropertyKind,
    PropertyKey, PropertyKind, Statement, VariableDeclarationKind,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// CommonJS re-export edges of one module.
#[derive(Debug, Default)]
pub(crate) struct CommonJsReExports {
    /// `((module, exported), (original, source))`, the same shape as the ES
    /// named re-export entries.
    pub(crate) entries: Vec<((String, String), (String, String))>,
    /// `(module, source)` for `module.exports = require('./source')`.
    pub(crate) forwards: Vec<(String, String)>,
}

/// One forwarded value: the `require()` source and the member picked off it
/// (`None` is the whole `require()` value).
type Forward = (String, Option<String>);

#[derive(Default)]
struct ModuleUses {
    /// Identifier references to `module` / `exports`.
    export_roots: usize,
    /// A binding named `require` (the file is not calling Node's loader).
    binds_require: bool,
}

impl<'a> oxc_ast_visit::Visit<'a> for ModuleUses {
    fn visit_identifier_reference(&mut self, identifier: &oxc_ast::ast::IdentifierReference<'a>) {
        if matches!(identifier.name.as_str(), "module" | "exports") {
            self.export_roots += 1;
        }
    }
    fn visit_binding_identifier(&mut self, identifier: &oxc_ast::ast::BindingIdentifier<'a>) {
        if matches!(identifier.name.as_str(), "module" | "exports" | "require") {
            self.binds_require = true;
        }
    }
}

pub(crate) fn commonjs_reexports(
    relative: &Path,
    body: &[Statement<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: &Path,
) -> CommonJsReExports {
    let mut out = CommonJsReExports::default();
    let module = normalized_module_path(relative);
    let mut uses = ModuleUses::default();
    {
        use oxc_ast_visit::Visit;
        for statement in body {
            uses.visit_statement(statement);
        }
    }
    if uses.binds_require {
        return out;
    }
    let locals = require_locals(body);
    let forward_of = |expression: &Expression<'_>| -> Option<Forward> {
        let expression = expression.get_inner_expression();
        if let Some(source) = require_string_literal_source(expression) {
            return Some((source, None));
        }
        if let Some((source, member)) = require_member_source(expression, &HashMap::new()) {
            return Some((source, Some(member)));
        }
        let Expression::Identifier(identifier) = expression else {
            return None;
        };
        locals.get(identifier.name.as_str()).cloned()
    };
    let resolve = |source: &str| {
        normalized_relative_import_module(relative, source, alias_map, Some(workspace_root))
    };
    // (exported name, forwarded value, bare `exports.NAME` form)
    let mut named: Vec<(String, Option<Forward>, bool, usize)> = Vec::new();
    let mut whole: Vec<(Option<Forward>, usize)> = Vec::new();
    // References to `module`/`exports` the recognized statements account for.
    let mut accounted = 0usize;
    for (position, statement) in body.iter().enumerate() {
        let Statement::ExpressionStatement(statement) = statement else {
            continue;
        };
        let Expression::AssignmentExpression(assign) = statement.expression.get_inner_expression()
        else {
            continue;
        };
        if assign.operator != AssignmentOperator::Assign {
            continue;
        }
        match export_target(&assign.left) {
            Some(ExportTarget::Whole) => {
                accounted += 1;
                match assign.right.get_inner_expression() {
                    Expression::ObjectExpression(object) => {
                        whole.push((None, position));
                        if object.properties.iter().any(|property| {
                            matches!(property, ObjectPropertyKind::SpreadProperty(_))
                        }) {
                            // A spread can overwrite or add any key.
                            named.push((String::new(), None, false, position));
                            continue;
                        }
                        for property in &object.properties {
                            let ObjectPropertyKind::ObjectProperty(property) = property else {
                                continue;
                            };
                            let key = match &property.key {
                                PropertyKey::StaticIdentifier(identifier) if !property.computed => {
                                    identifier.name.to_string()
                                }
                                PropertyKey::StringLiteral(literal) => literal.value.to_string(),
                                _ => {
                                    named.push((String::new(), None, false, position));
                                    continue;
                                }
                            };
                            let value = (property.kind == PropertyKind::Init && !property.method)
                                .then(|| forward_of(&property.value))
                                .flatten();
                            named.push((key, value, false, position));
                        }
                    }
                    other => whole.push((forward_of(other), position)),
                }
            }
            Some(ExportTarget::Named { name, bare }) => {
                accounted += 1;
                named.push((name, forward_of(&assign.right), bare, position));
            }
            None => {}
        }
    }
    if uses.export_roots != accounted {
        return out;
    }
    // One whole assignment at most; after it, bare `exports.NAME` writes
    // reach an orphaned object and never the module's exports.
    if whole.len() > 1 {
        return out;
    }
    let whole_at = whole.last().map(|(_, at)| *at);
    let whole = whole.pop().map(|(forward, _)| forward);
    // A write before the whole assignment is overwritten by it; one after it
    // overlays it. Bare `exports.NAME` never reaches the module's exports
    // once `module.exports` is reassigned.
    named.retain(|(name, _, bare, position)| {
        name.is_empty() || whole_at.is_none_or(|at| !*bare && *position >= at)
    });
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (name, _, _, _) in &named {
        {
            *counts.entry(name.as_str()).or_default() += 1;
        }
    }
    if let Some(Some((source, member))) = &whole
        && member.is_none()
        && let Some(target) = resolve(source)
    {
        // `module.exports = require('./x')` forwards every export, so a
        // later `module.exports.NAME = ...` overlay could shadow one name;
        // it is recorded as a named edge below and wins over the forward.
        out.forwards.push((module.clone(), target));
    } else if let Some(Some((source, Some(member)))) = &whole
        && let Some(target) = resolve(source)
    {
        out.entries.push((
            (module.clone(), "default".to_string()),
            (member.clone(), target),
        ));
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for (name, value, _, _) in &named {
        if name.is_empty() || counts.get(name.as_str()) != Some(&1) {
            continue;
        }
        if !seen.insert(name.as_str()) {
            continue;
        }
        let Some((source, member)) = value else {
            continue;
        };
        let Some(target) = resolve(source) else {
            continue;
        };
        out.entries.push((
            (module.clone(), name.clone()),
            (
                member.clone().unwrap_or_else(|| "default".to_string()),
                target,
            ),
        ));
    }
    // An unreadable key (computed, spread) may be any name: no edge is safe.
    // An overlay that is not a literal `require()` forward replaces a name
    // the whole-module forward would otherwise supply.
    if named
        .iter()
        .any(|(name, value, _, _)| name.is_empty() || value.is_none())
    {
        out.forwards.clear();
        if named.iter().any(|(name, _, _, _)| name.is_empty()) {
            out.entries.clear();
        }
    }
    out
}

enum ExportTarget {
    /// `module.exports`
    Whole,
    /// `exports.NAME` (`bare`) or `module.exports.NAME`
    Named { name: String, bare: bool },
}

fn export_target(target: &AssignmentTarget<'_>) -> Option<ExportTarget> {
    let is_module_exports = |expression: &Expression<'_>| {
        matches!(
            expression.get_inner_expression(),
            Expression::StaticMemberExpression(member)
                if member.property.name == "exports"
                    && matches!(&member.object, Expression::Identifier(object) if object.name == "module")
        )
    };
    let (object, name) = match target {
        AssignmentTarget::StaticMemberExpression(member) => {
            if is_module_exports_member(member) {
                return Some(ExportTarget::Whole);
            }
            (&member.object, member.property.name.to_string())
        }
        AssignmentTarget::ComputedMemberExpression(member) => {
            let Expression::StringLiteral(key) = &member.expression else {
                return None;
            };
            (&member.object, key.value.to_string())
        }
        _ => return None,
    };
    let bare = matches!(object.get_inner_expression(), Expression::Identifier(identifier) if identifier.name == "exports");
    (bare || is_module_exports(object)).then_some(ExportTarget::Named { name, bare })
}

fn is_module_exports_member(member: &oxc_ast::ast::StaticMemberExpression<'_>) -> bool {
    member.property.name == "exports"
        && matches!(&member.object, Expression::Identifier(object) if object.name == "module")
}

/// Top-level `const|var|let x = require('./y')`, `= require('./y').m` and
/// `{ m: x } = require('./y')` bindings: local -> forwarded value. A `var`
/// or `let` the file writes again, or a name declared more than once, is not
/// a stable forward and is dropped.
fn require_locals(body: &[Statement<'_>]) -> HashMap<String, Forward> {
    let writes = super::owners::identifier_writes(body);
    let mut locals: HashMap<String, Forward> = HashMap::new();
    let mut declared: HashMap<String, usize> = HashMap::new();
    for statement in body {
        let Statement::VariableDeclaration(declaration) = statement else {
            continue;
        };
        for declarator in &declaration.declarations {
            for identifier in declarator.id.get_binding_identifiers() {
                *declared.entry(identifier.name.to_string()).or_default() += 1;
            }
            let Some(init) = &declarator.init else {
                continue;
            };
            let init = init.get_inner_expression();
            let constant = declaration.kind == VariableDeclarationKind::Const;
            let mut bind = |name: &str, forward: Forward| {
                if constant || !writes.contains(name) {
                    locals.insert(name.to_string(), forward);
                }
            };
            match &declarator.id {
                BindingPattern::BindingIdentifier(identifier) => {
                    if let Some(source) = require_string_literal_source(init) {
                        bind(identifier.name.as_str(), (source, None));
                    } else if let Some((source, member)) =
                        require_member_source(init, &HashMap::new())
                    {
                        bind(identifier.name.as_str(), (source, Some(member)));
                    }
                }
                BindingPattern::ObjectPattern(pattern) => {
                    let Some(source) = require_string_literal_source(init) else {
                        continue;
                    };
                    for property in &pattern.properties {
                        let (Some(key), BindingPattern::BindingIdentifier(identifier)) =
                            (object_binding_key_name(property), &property.value)
                        else {
                            continue;
                        };
                        bind(
                            identifier.name.as_str(),
                            (source.clone(), Some(key.to_string())),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    locals.retain(|name, _| declared.get(name) == Some(&1));
    locals
}
