//! Owner extraction for the TypeScript preview adapter.

use super::*;
use oxc_ast::ast::{AssignmentOperator, AssignmentTarget, PropertyKind, StaticMemberExpression};

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "unit-test extraction surface; production parses through owners_from_program since #10071"
    )
)]
pub(crate) fn extract_owners(file: &Path, source: &str) -> Vec<TypeScriptOwner> {
    // Parse (and walk) on the dedicated large-stack worker behind the
    // nesting budget (#4101): a refused source or a failed worker yields no
    // owners, exactly like the previous parse-error path, while the oxc
    // recursion no longer runs on the caller's small stack.
    let Ok(owners) = parse_on_worker(file, source, |file, source, allocator| {
        let ret = Parser::new(allocator, source, source_type_for(file)).parse();
        if !ret.errors.is_empty() {
            return Vec::new();
        }
        owners_from_program(&ret.program, file, source)
    }) else {
        return Vec::new();
    };
    owners
}

/// Owners from an already-parsed program (#10071): the post-parse half of
/// [`extract_owners`], so the combined per-file facts cache can derive
/// owners, export facts, and phase-2 facts from ONE parse. Builds its own
/// line index from `source`; behavior is identical to the dedicated
/// extraction.
pub(crate) fn owners_from_program(
    program: &oxc_ast::ast::Program<'_>,
    file: &Path,
    source: &str,
) -> Vec<TypeScriptOwner> {
    let imports = extract_imports_from_statements(&program.body);
    // One line index per source; every owner line below is a binary
    // search against it rather than a rescan from byte 0.
    let source = &SourceText::new(source);
    // `(owner, is a CommonJS assignment export)`, in source order.
    let mut owners = Vec::new();
    for stmt in &program.body {
        let commonjs = matches!(stmt, Statement::ExpressionStatement(_));
        owners.extend(
            owners_from_statement(stmt, file, source, &imports)
                .into_iter()
                .map(|owner| (owner, commonjs)),
        );
    }
    let mut owners = without_reassigned_commonjs_exports(owners);
    mark_default_exported_bindings(&program.body, &mut owners);
    mark_commonjs_default_binding(program, &mut owners);
    mark_default_object_members(&program.body, &mut owners);
    // #7507: a `let`/function owner rebound before its `export default`
    // carries dead code at the declaration site — drop it so no finding
    // can credit a test with observing it.
    // #7812: keep the original owner when `export default ident`
    // snapshots the binding before that reassignment.
    owners = without_reassigned_esm_bindings(&program.body, owners);
    let entries = module_entries_by_owner(&program.body, source);
    for owner in &mut owners {
        if owner.class_name.is_none()
            && owner.owner_kind != OwnerKind::ModuleFunction
            && let Some(owner_entries) = entries.get(&owner.name)
        {
            owner.module_entries = owner_entries.clone();
        }
    }
    assign_class_entries(&mut owners, &class_instance_field_names(program));
    assign_decorated_entries(program, source, &mut owners);
    owners
}

pub(crate) fn owners_from_statement(
    stmt: &Statement<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Vec<TypeScriptOwner> {
    if let Statement::FunctionDeclaration(func) = stmt
        && let Some(id) = &func.id
    {
        return vec![owner_from_function(
            file,
            source,
            id.name.as_str(),
            func,
            function_owner_kind(
                file,
                source,
                id.name.as_str(),
                func.span.start,
                func.span.end,
            ),
            false,
            imports,
        )];
    }
    if let Statement::ExportNamedDeclaration(export) = stmt
        && let Some(decl) = export.declaration.as_ref()
    {
        return owners_from_declaration(decl, file, source, imports);
    }
    if let Statement::ExportDefaultDeclaration(export) = stmt {
        return owners_from_default_export(&export.declaration, file, source, imports);
    }
    if let Statement::ExpressionStatement(expression) = stmt {
        return owners_from_commonjs_export(&expression.expression, file, source, imports);
    }
    owners_from_statement_declaration(stmt, file, source, imports)
}

/// Owners carved from a top-level CommonJS assignment export (#4545):
///
/// - `exports.NAME = function ... / arrow` and
///   `module.exports.NAME = function ... / arrow` yield an owner named by the
///   static property `NAME`;
/// - `module.exports = function NAME(...) ... / arrow` yields the module's
///   default-export owner (named by the function id, else `default`), the
///   CommonJS mirror of `export default function`;
/// - `module.exports = { NAME() {}, NAME: function () {}, NAME: () => ... }`
///   yields one owner per static-identifier key whose value is a function.
///
/// Anything else — a non-function value, a computed or string-literal
/// property, a compound operator, a chained assignment, a getter/setter or
/// spread entry — yields no owner (fail-closed: no invented owner).
fn owners_from_commonjs_export(
    expression: &Expression<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Vec<TypeScriptOwner> {
    let Expression::AssignmentExpression(assign) = expression else {
        return Vec::new();
    };
    if assign.operator != AssignmentOperator::Assign {
        return Vec::new();
    }
    let AssignmentTarget::StaticMemberExpression(target) = &assign.left else {
        return Vec::new();
    };
    let property = target.property.name.as_str();
    let named_export_object = match &target.object {
        // `exports.NAME = ...`
        Expression::Identifier(object) => object.name == "exports",
        // `module.exports.NAME = ...`
        Expression::StaticMemberExpression(object) => is_module_exports(object),
        _ => false,
    };
    if named_export_object {
        return commonjs_function_owner(
            &assign.right,
            property,
            assign.span.start,
            file,
            source,
            imports,
        )
        .into_iter()
        .collect();
    }
    if !is_module_exports(target) {
        return Vec::new();
    }
    // `module.exports = ...`
    match &assign.right {
        Expression::FunctionExpression(func) => {
            let name = func
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("default");
            commonjs_function_owner(
                &assign.right,
                name,
                assign.span.start,
                file,
                source,
                imports,
            )
            .map(|mut owner| {
                owner.exported_as_default = true;
                owner.commonjs_default_export = true;
                owner.commonjs_whole_function = true;
                vec![owner]
            })
            .unwrap_or_default()
        }
        Expression::ArrowFunctionExpression(_) => commonjs_function_owner(
            &assign.right,
            "default",
            assign.span.start,
            file,
            source,
            imports,
        )
        .map(|mut owner| {
            owner.exported_as_default = true;
            owner.commonjs_default_export = true;
            owner.commonjs_whole_function = true;
            vec![owner]
        })
        .unwrap_or_default(),
        Expression::ObjectExpression(object) => object
            .properties
            .iter()
            .filter_map(|property| {
                let ObjectPropertyKind::ObjectProperty(property) = property else {
                    return None;
                };
                if property.computed || property.kind != PropertyKind::Init {
                    return None;
                }
                let name = property_key_name(&property.key)?;
                commonjs_function_owner(
                    &property.value,
                    &name,
                    property.span.start,
                    file,
                    source,
                    imports,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Drop every CommonJS export owner whose name another CommonJS export in
/// the same file also defines (#4638 review). `module.exports = { f: fn }`
/// followed by `module.exports.f = ...` leaves only the last value live, but
/// which one runs depends on statement order and control flow the syntax
/// walk does not model; crediting the dead one would report an exposure for
/// code no test reaches. Fail-closed: no owner for a reassigned export.
fn without_reassigned_commonjs_exports(
    owners: Vec<(TypeScriptOwner, bool)>,
) -> Vec<TypeScriptOwner> {
    // #7507: every bare `module.exports = <fn>` assigns the SAME default
    // slot, whatever the function id names it. Two such assignments mean the
    // first value is dead — the name-grouping below would keep both because
    // their ids differ. A bare assignment is exactly a CommonJS owner with
    // `exported_as_default`; more than one of them fails closed together.
    let bare_default_count = owners
        .iter()
        .filter(|(owner, commonjs)| *commonjs && owner.exported_as_default)
        .count();
    // Name multiplicity in one pass over borrowed names: a large CommonJS
    // module otherwise pays an O(owners x names) scan (#10017), and cloning
    // every CommonJS owner name into a counting map allocates one string per
    // export (#10087). Only the rare duplicated names are kept, as owned
    // strings, so the counting map's borrow of `owners` ends before the
    // consuming filter below.
    let duplicated: std::collections::HashSet<String> = {
        let mut commonjs_name_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for (owner, commonjs) in &owners {
            if *commonjs {
                *commonjs_name_counts.entry(owner.name.as_str()).or_insert(0) += 1;
            }
        }
        commonjs_name_counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(name, _)| name.to_string())
            .collect()
    };
    owners
        .into_iter()
        .filter(|(owner, commonjs)| {
            if !*commonjs {
                return true;
            }
            if owner.exported_as_default && bare_default_count > 1 {
                return false;
            }
            !duplicated.contains(&owner.name)
        })
        .map(|(owner, _)| owner)
        .collect()
}

/// Drop every ESM owner whose binding is reassigned after its declaration
/// and not captured by an earlier snapshot `export default ident` (#7507,
/// #7812). `let price = (a) => ...; price = (a) => 140; export default
/// price;` leaves the declared (changed) body dead: the export carries the
/// second value, so a test observing `price(...)` never runs the changed
/// code. Crediting it would read `exposed` for dead code — fail-closed, no
/// owner. The reverse order snapshots the first arrow, so that owner stays.
/// Only plain top-level identifier writes count (`price = ...`,
/// `price += ...`, `++price`); member writes (`price.x = ...`) mutate the
/// same value and keep the owner.
fn without_reassigned_esm_bindings(
    body: &[Statement<'_>],
    owners: Vec<TypeScriptOwner>,
) -> Vec<TypeScriptOwner> {
    if owners.is_empty() {
        return owners;
    }
    let reassigned: std::collections::HashSet<&str> =
        dead_reassigned_esm_bindings(body).into_iter().collect();
    if reassigned.is_empty() {
        return owners;
    }
    owners
        .into_iter()
        .filter(|owner| {
            // A class method shares only the binding name with a reassigned
            // top-level binding; its receiver keeps it live, so the
            // name-only drop must not take it.
            owner.class_name.is_some() || !reassigned.contains(owner.name.as_str())
        })
        .collect()
}

/// Names bound by a top-level `let|var|function` owner declaration that a
/// later top-level statement plainly reassigns, unless a snapshot
/// `export default ident` captured that binding first (#7812). `const`
/// cannot reassign; only `let`/`var`/function bindings are candidates, but
/// the walk records any plain identifier write and lets the caller
/// intersect with its owners. Live `export { ident as default }` is not a
/// snapshot, so a later write still drops (fail-closed).
fn dead_reassigned_esm_bindings<'a>(body: &'a [Statement<'a>]) -> Vec<&'a str> {
    // Membership sets borrow the AST arena instead of owning one String per
    // top-level binding: a large generated module (tens of thousands of
    // bindings) otherwise allocates that many strings twice (#10087). The
    // #10017 quadratic-scan fix is unchanged. Insertion order is preserved
    // where a consumer observes it (`reassigned` iterates in statement
    // order below); every other use is membership only.
    let mut declared: std::collections::HashSet<&'a str> = std::collections::HashSet::new();
    for stmt in body {
        match stmt {
            Statement::VariableDeclaration(decl)
                if decl.kind != oxc_ast::ast::VariableDeclarationKind::Const =>
            {
                for declarator in &decl.declarations {
                    for identifier in declarator.id.get_binding_identifiers() {
                        declared.insert(identifier.name.as_str());
                    }
                }
            }
            Statement::FunctionDeclaration(func) => {
                if let Some(id) = &func.id {
                    declared.insert(id.name.as_str());
                }
            }
            // `export let price = ...` / `export function price() ...` bind
            // exactly like their undecorated forms; without unwrapping, a
            // later reassignment misses them and the dead owner stays
            // credited.
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(oxc_ast::ast::Declaration::VariableDeclaration(decl))
                    if decl.kind != oxc_ast::ast::VariableDeclarationKind::Const =>
                {
                    for declarator in &decl.declarations {
                        for identifier in declarator.id.get_binding_identifiers() {
                            declared.insert(identifier.name.as_str());
                        }
                    }
                }
                Some(oxc_ast::ast::Declaration::FunctionDeclaration(func)) => {
                    if let Some(id) = &func.id {
                        declared.insert(id.name.as_str());
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    if declared.is_empty() {
        return Vec::new();
    }
    let mut snapshot_exports: Vec<(&'a str, u32)> = Vec::new();
    let mut reassigned: Vec<(&'a str, u32)> = Vec::new();
    let mut reassigned_names: std::collections::HashSet<&'a str> = std::collections::HashSet::new();
    for stmt in body {
        if let Some(name) = snapshot_default_export_local(stmt)
            && declared.contains(name)
            && !snapshot_exports.iter().any(|(seen, _)| *seen == name)
        {
            snapshot_exports.push((name, stmt.span().start));
        }
        // The declaration initializers themselves are not reassignments.
        if matches!(
            stmt,
            Statement::VariableDeclaration(_) | Statement::FunctionDeclaration(_)
        ) {
            continue;
        }
        let offset = stmt.span().start;
        for name in assigned_identifiers_in_statement(stmt) {
            if declared.contains(name) && reassigned_names.insert(name) {
                reassigned.push((name, offset));
            }
        }
    }
    // `UpdateExpression` (`price++`, `++price`) is covered by the walk above.
    // `export default price` snapshots the value for default importers only.
    // A binding that is also a named export (`export let price`, `export {
    // price }`, `export { price as alias }`) is live for named importers, who
    // observe the later write, so the snapshot cannot excuse it (fail-closed).
    let named_exports: std::collections::HashSet<&str> =
        named_exported_locals(body).into_iter().collect();
    reassigned
        .into_iter()
        .filter(|(name, reassigned_at)| {
            named_exports.contains(name)
                || !snapshot_exports.iter().any(|(exported, exported_at)| {
                    exported == name && *exported_at < *reassigned_at
                })
        })
        .map(|(name, _)| name)
        .collect()
}

/// Local names a top-level source-less named export exposes as live
/// bindings: `export let|var|function name`, `export { name }` and
/// `export { name as alias }` (but not `as default`, which the caller treats
/// separately).
fn named_exported_locals<'a>(body: &'a [Statement<'a>]) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::new();
    for stmt in body {
        let Statement::ExportNamedDeclaration(export) = stmt else {
            continue;
        };
        match &export.declaration {
            Some(oxc_ast::ast::Declaration::VariableDeclaration(decl)) => {
                for declarator in &decl.declarations {
                    for identifier in declarator.id.get_binding_identifiers() {
                        out.push(identifier.name.as_str());
                    }
                }
            }
            Some(oxc_ast::ast::Declaration::FunctionDeclaration(func)) => {
                if let Some(id) = &func.id {
                    out.push(id.name.as_str());
                }
            }
            _ => {}
        }
        if export.source.is_none() {
            for specifier in &export.specifiers {
                if let Some(name) = specifier.local.identifier_name() {
                    out.push(name.as_str());
                }
            }
        }
    }
    out
}

/// Local name snapshotted by `export default ident` / `export default
/// (ident)`. Live `export { ident as default }` is not a snapshot.
fn snapshot_default_export_local<'a>(stmt: &'a Statement<'a>) -> Option<&'a str> {
    let Statement::ExportDefaultDeclaration(export) = stmt else {
        return None;
    };
    let mut expression = export.declaration.as_expression()?;
    while let Expression::ParenthesizedExpression(paren) = expression {
        expression = &paren.expression;
    }
    match expression {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

/// Plain identifier writes inside one top-level statement: `name = ...`
/// (any operator), `++name` / `name++`. Member, destructuring-pattern and
/// computed writes are not plain bindings and are ignored.
fn assigned_identifiers_in_statement<'a>(stmt: &'a Statement<'a>) -> Vec<&'a str> {
    use oxc_ast::ast::Expression;
    let mut out: Vec<&'a str> = Vec::new();
    fn push_ident<'a>(target: &'a oxc_ast::ast::IdentifierReference<'a>, out: &mut Vec<&'a str>) {
        if !out.iter().any(|seen| *seen == target.name.as_str()) {
            out.push(target.name.as_str());
        }
    }
    fn walk_expression<'a>(expression: &'a Expression<'a>, out: &mut Vec<&'a str>) {
        match expression {
            Expression::AssignmentExpression(assign) => {
                if let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left {
                    push_ident(target, out);
                }
                walk_expression(&assign.right, out);
            }
            Expression::UpdateExpression(update) => {
                if let oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(target) =
                    &update.argument
                {
                    push_ident(target, out);
                }
            }
            Expression::SequenceExpression(seq) => {
                for expression in &seq.expressions {
                    walk_expression(expression, out);
                }
            }
            Expression::ParenthesizedExpression(paren) => {
                walk_expression(&paren.expression, out);
            }
            Expression::LogicalExpression(logical) => {
                walk_expression(&logical.left, out);
                walk_expression(&logical.right, out);
            }
            Expression::ConditionalExpression(conditional) => {
                walk_expression(&conditional.test, out);
                walk_expression(&conditional.consequent, out);
                walk_expression(&conditional.alternate, out);
            }
            Expression::CallExpression(call) => {
                walk_expression(&call.callee, out);
                for argument in &call.arguments {
                    if let Some(expression) = argument.as_expression() {
                        walk_expression(expression, out);
                    }
                }
            }
            // Wrapper expressions: a write nested in `await`, a unary
            // operator (`void (price = ...)`), or a binary expression
            // still reassigns the binding (PR #7651 review).
            Expression::AwaitExpression(awaited) => {
                walk_expression(&awaited.argument, out);
            }
            Expression::UnaryExpression(unary) => {
                walk_expression(&unary.argument, out);
            }
            Expression::BinaryExpression(binary) => {
                walk_expression(&binary.left, out);
                walk_expression(&binary.right, out);
            }
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {}
            _ => {}
        }
    }
    if let Statement::ExpressionStatement(expression) = stmt {
        walk_expression(&expression.expression, &mut out);
    }
    out
}

/// Mark a top-level function owner as the module's default export when a
/// separate statement exports its binding as `default`: `export default
/// price;` or a source-less `export { price as default }` (RIPR-SPEC-0243
/// rule 5). Without this, a default import of such an owner would lose its
/// declaration anchor. Only class-free owners qualify: a method is never the
/// module's default export itself.
fn mark_default_exported_bindings<'a>(body: &'a [Statement<'a>], owners: &mut [TypeScriptOwner]) {
    let mut default_locals: std::collections::HashSet<String> = std::collections::HashSet::new();
    for stmt in body {
        match stmt {
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::Identifier(ident) = &export.declaration {
                    default_locals.insert(ident.name.to_string());
                }
            }
            Statement::ExportNamedDeclaration(export)
                if export.declaration.is_none() && export.source.is_none() =>
            {
                for specifier in &export.specifiers {
                    if specifier.export_kind == ImportOrExportKind::Type {
                        continue;
                    }
                    if module_export_name_text(&specifier.exported).as_deref() == Some("default")
                        && let Some(local) = module_export_name_text(&specifier.local)
                    {
                        default_locals.insert(local);
                    }
                }
            }
            _ => {}
        }
    }
    if default_locals.is_empty() {
        return;
    }
    for owner in owners.iter_mut() {
        if owner.class_name.is_none()
            && owner.owner_kind != OwnerKind::ModuleFunction
            && default_locals.contains(owner.name.as_str())
        {
            owner.exported_as_default = true;
        }
    }
}

/// Mark a top-level function declaration as the module's CommonJS default
/// export when a separate statement assigns its binding to `module.exports`
/// (`module.exports = stringify;`), the CommonJS twin of `export default
/// stringify;` (RIPR-SPEC-0243 rule 19). Only a plain reference qualifies, and
/// only while the binding is unambiguous: exactly one assignment to
/// `module.exports` appears in the whole file, the name is declared by exactly
/// one top-level function declaration and the file never writes it, so the
/// value `require()` hands back is that declaration. An opaque file
/// (`eval`, `with`) marks nothing.
fn mark_commonjs_default_binding(
    program: &oxc_ast::ast::Program<'_>,
    owners: &mut [TypeScriptOwner],
) {
    let mut target: Option<&str> = None;
    for stmt in &program.body {
        let Statement::ExpressionStatement(expression) = stmt else {
            continue;
        };
        let Expression::AssignmentExpression(assign) = &expression.expression else {
            continue;
        };
        if assign.operator != AssignmentOperator::Assign {
            continue;
        }
        let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            continue;
        };
        if !is_module_exports(member) {
            continue;
        }
        if let Expression::Identifier(ident) = &assign.right {
            target = Some(ident.name.as_str());
        }
    }
    // No `module.exports = <ident>` twin to find and no inline default-export
    // owner whose gate could clear it: the whole pass is a no-op, including
    // its three full-source mask builds — the expensive part on a large ESM
    // module that never touches `module.exports` (#10087).
    if target.is_none() && !owners.iter().any(|owner| owner.commonjs_default_export) {
        return;
    }
    let declarations = target.map_or(0, |name| {
        program
            .body
            .iter()
            .filter(|stmt| {
                matches!(stmt, Statement::FunctionDeclaration(function)
                    if function.id.as_ref().is_some_and(|id| id.name == name))
            })
            .count()
    });
    // `module.exports = name;` of one declared function makes the whole export
    // that function whether or not the gate below holds; a named import or
    // member of it is then a property of the function, never the owner.
    let declared = target.filter(|_| declarations == 1);
    for owner in owners.iter_mut() {
        if declared.is_some_and(|name| {
            owner.class_name.is_none()
                && owner.owner_kind == OwnerKind::Function
                && owner.name == name
        }) {
            owner.commonjs_whole_function = true;
        }
    }
    // One file-level gate for the inline (`module.exports = function ...`)
    // and the declared (`module.exports = name;`) forms. It must hold on the
    // regex-aware mask, the division-only mask, and the comment-only text, so
    // a `/` misread (`t! / 2`) or a backtick inside a regex cannot hide code.
    let source = program.source_text;
    let comments: Vec<std::ops::Range<usize>> = program
        .comments
        .iter()
        .map(|comment| comment.span.start as usize..comment.span.end as usize)
        .collect();
    let visible = super::tests_extract::blank_ranges(source, &comments);
    let both = super::tests_extract::byte_preserving_code_mask(source)
        .filter(|code| commonjs_default_gate(code))
        .zip(
            super::tests_extract::byte_preserving_division_mask(source)
                .filter(|code| commonjs_default_gate(code)),
        )
        .filter(|_| commonjs_visible_text_gate(&visible))
        .filter(|_| !names_the_module_object_in_text(source));
    let Some((code, division_code)) = both else {
        // Only the inline owners this pass flagged lose their default
        // export; every other file (ESM, no `module.exports`) fails the gate.
        for owner in owners.iter_mut().filter(|o| o.commonjs_default_export) {
            owner.commonjs_default_export = false;
            owner.exported_as_default = false;
        }
        return;
    };
    let Some(name) = declared else {
        return;
    };
    // Annex B: a block-level `function name` re-binds the top-level function.
    let spellings = |text: &str| {
        super::tests_extract::function_declaration_names(text)
            .iter()
            .filter(|(declared, _)| declared == name)
            .count()
    };
    if spellings(&code) != 1
        || spellings(&division_code) != 1
        || super::tests_extract::identifier_written_strictly(&code, name)
        || super::tests_extract::identifier_written_strictly(&division_code, name)
    {
        return;
    }
    for owner in owners.iter_mut() {
        if owner.class_name.is_none()
            && owner.owner_kind == OwnerKind::Function
            && owner.name == name
        {
            owner.exported_as_default = true;
            owner.commonjs_default_export = true;
        }
    }
}

/// The module-object checks of [`commonjs_default_gate`] on text where only
/// comments are blanked: strings, templates and regex literals stay visible,
/// so a mis-lexed delimiter cannot hide a write. `eval` and `with` count
/// anywhere; `\u` escapes are not read here (they are legal in strings).
fn commonjs_visible_text_gate(visible: &str) -> bool {
    let opaque = super::tests_extract::identifier_occurrences(visible, "eval")
        .next()
        .is_some()
        || super::tests_extract::identifier_occurrences(visible, "with")
            .any(|at| visible[at + "with".len()..].trim_start().starts_with('('));
    !opaque && commonjs_module_reach_gate(visible)
}

/// `true` when `needle` occurs as a bare word (not a member name, not part of
/// a longer word) whose following text `allowed` rejects.
fn bare_word_use(compact: &str, needle: &str, allowed: impl Fn(&str) -> bool) -> bool {
    let word = |ch: Option<char>| {
        ch.is_some_and(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric() || !ch.is_ascii())
    };
    compact.match_indices(needle).any(|(at, _)| {
        let before = compact[..at].chars().next_back();
        let rest = &compact[at + needle.len()..];
        !(word(before) || word(rest.chars().next()) || before == Some('.') || allowed(rest))
    })
}

/// The module object named in a string, which the masks blank:
/// `require('module')._cache`, `'node:module'`.
fn names_the_module_object_in_text(source: &str) -> bool {
    ["'module'", "\"module\"", "`module`", "node:module"]
        .iter()
        .any(|spelling| source.contains(spelling))
}

/// The file-level refusal corpus for `module.exports` (RIPR-SPEC-0243 rule
/// 19): a bounded list of module-object routes it can name, not a proof. On masked, whitespace-free text the bare
/// identifier `module` must occur only as `module.exports=` (exactly once, not
/// `==`/`=>`) or as the base of a property access `module.exports.name`; any
/// other use (an alias, `module?.exports`, `module[...]`, compound or update
/// writes, `Object.assign(module, ...)`, `typeof module` guards) makes the
/// value unknowable. `eval`, `with` and `\u` escapes refuse the file.
fn commonjs_default_gate(code: &str) -> bool {
    !super::tests_extract::file_is_opaque(code) && commonjs_module_reach_gate(code)
}

/// Every use of the module object in `code` is `module.exports = ...` once or
/// a property access of it.
fn commonjs_module_reach_gate(code: &str) -> bool {
    let word = |ch: Option<char>| {
        ch.is_some_and(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric() || !ch.is_ascii())
    };
    // Drop every JS whitespace run (Unicode spaces, line terminators, BOM)
    // except one space kept between two word characters, so `delete module`
    // does not fuse into the word `deletemodule`.
    let is_space = |ch: char| ch.is_whitespace() || ch == '\u{feff}';
    let chars: Vec<char> = code.chars().collect();
    let mut compact = String::with_capacity(code.len());
    let mut at = 0;
    while at < chars.len() {
        if !is_space(chars[at]) {
            compact.push(chars[at]);
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && is_space(chars[at]) {
            at += 1;
        }
        let before = if start == 0 {
            None
        } else {
            Some(chars[start - 1])
        };
        if word(before) && word(chars.get(at).copied()) {
            compact.push(' ');
        }
    }
    // `require.main.exports = x`, `require.cache[...]` and
    // `process.mainModule` reach the module object without naming `module`.
    if ["require.main", "require.cache", "mainModule"]
        .iter()
        .any(|reach| compact.contains(reach))
    {
        return false;
    }
    // Other routes to the module object that never name `module`:
    // `arguments[2]`, `require['main']`, an alias of `require`, and bracket
    // access on `process`.
    if bare_word_use(&compact, "arguments", |_| false)
        || bare_word_use(&compact, "require", |rest| rest.starts_with('('))
        || compact.contains("process[")
    {
        return false;
    }
    let mut assignments = 0usize;
    for (at, _) in compact.match_indices("module") {
        let before = compact[..at].chars().next_back();
        let rest = &compact[at + "module".len()..];
        if word(before) || word(rest.chars().next()) || before == Some('.') {
            continue;
        }
        if let Some(after) = rest.strip_prefix(".exports") {
            if after.starts_with('.') {
                continue;
            }
            if after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>") {
                assignments += 1;
                continue;
            }
        }
        return false;
    }
    assignments == 1
}

/// Mark top-level function (or const arrow) owners held by the module's `export default {
/// name }` object (RIPR-SPEC-0243 rule 5). Only an object made entirely of
/// plain same-name entries qualifies: a spread, computed key, method, getter,
/// renamed entry or duplicate key could replace or rebind a member, so any
/// of them leaves every owner unmarked. A module with more than one default
/// object export marks nothing.
fn mark_default_object_members(body: &[Statement<'_>], owners: &mut [TypeScriptOwner]) {
    use oxc_ast::ast::{ObjectPropertyKind, PropertyKey, PropertyKind};
    let mut objects = body.iter().filter_map(|stmt| match stmt {
        Statement::ExportDefaultDeclaration(export) => {
            let mut expression = export.declaration.as_expression()?;
            while let Expression::ParenthesizedExpression(paren) = expression {
                expression = &paren.expression;
            }
            match expression {
                Expression::ObjectExpression(object) => Some(object),
                _ => None,
            }
        }
        _ => None,
    });
    let (Some(object), None) = (objects.next(), objects.next()) else {
        return;
    };
    let mut members: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return;
        };
        let (PropertyKind::Init, false, false) =
            (property.kind, property.computed, property.method)
        else {
            return;
        };
        let (PropertyKey::StaticIdentifier(key), Expression::Identifier(value)) =
            (&property.key, &property.value)
        else {
            return;
        };
        if key.name != value.name || !members.insert(key.name.as_str()) {
            return;
        }
    }
    // An arrow owner qualifies only when its top-level binding is `const`:
    // a `let`/`var` arrow can be rebound inside a nested function or block,
    // which the top-level reassignment pass does not see.
    let const_names: std::collections::HashSet<&str> = body
        .iter()
        .filter_map(|stmt| match stmt {
            Statement::VariableDeclaration(decl) => Some(&**decl),
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(oxc_ast::ast::Declaration::VariableDeclaration(decl)) => Some(&**decl),
                _ => None,
            },
            _ => None,
        })
        .filter(|decl| decl.kind == oxc_ast::ast::VariableDeclarationKind::Const)
        .flat_map(|decl| decl.declarations.iter())
        .flat_map(|declarator| declarator.id.get_binding_identifiers())
        .map(|identifier| identifier.name.as_str())
        .collect();
    for owner in owners.iter_mut() {
        let callable = match owner.owner_kind {
            OwnerKind::Function => true,
            OwnerKind::ArrowFunction => const_names.contains(owner.name.as_str()),
            _ => false,
        };
        if owner.class_name.is_none() && callable && members.contains(owner.name.as_str()) {
            owner.default_object_member = true;
        }
    }
}

/// `module.exports` as a static member expression on the bare `module`
/// identifier.
fn is_module_exports(member: &StaticMemberExpression<'_>) -> bool {
    member.property.name == "exports"
        && matches!(&member.object, Expression::Identifier(object) if object.name == "module")
}

/// A function owner for a CommonJS export value: `function ...` or an arrow;
/// any other value yields `None`.
fn commonjs_function_owner(
    value: &Expression<'_>,
    name: &str,
    owner_start: u32,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Option<TypeScriptOwner> {
    match value {
        Expression::FunctionExpression(func) => Some(owner_from_function(
            file,
            source,
            name,
            func,
            function_owner_kind(file, source, name, func.span.start, func.span.end),
            false,
            imports,
        )),
        Expression::ArrowFunctionExpression(arrow) => Some(owner_from_arrow(
            file,
            source,
            name,
            arrow,
            owner_start,
            false,
            imports,
        )),
        _ => None,
    }
}

pub(crate) fn owners_from_statement_declaration(
    stmt: &Statement<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Vec<TypeScriptOwner> {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            owners_from_variable_declaration(decl, file, source, imports)
        }
        Statement::ClassDeclaration(class) => owners_from_class(class, file, source, imports),
        _ => Vec::new(),
    }
}

pub(crate) fn owners_from_declaration(
    decl: &Declaration<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Vec<TypeScriptOwner> {
    match decl {
        Declaration::FunctionDeclaration(func) => func
            .id
            .as_ref()
            .map(|id| {
                vec![owner_from_function(
                    file,
                    source,
                    id.name.as_str(),
                    func,
                    function_owner_kind(
                        file,
                        source,
                        id.name.as_str(),
                        func.span.start,
                        func.span.end,
                    ),
                    false,
                    imports,
                )]
            })
            .unwrap_or_default(),
        Declaration::VariableDeclaration(decl) => {
            owners_from_variable_declaration(decl, file, source, imports)
        }
        Declaration::ClassDeclaration(class) => owners_from_class(class, file, source, imports),
        _ => Vec::new(),
    }
}

pub(crate) fn owners_from_default_export(
    decl: &ExportDefaultDeclarationKind<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Vec<TypeScriptOwner> {
    let mut owners = match decl {
        ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
            let name = func
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("default");
            vec![owner_from_function(
                file,
                source,
                name,
                func,
                function_owner_kind(file, source, name, func.span.start, func.span.end),
                false,
                imports,
            )]
        }
        ExportDefaultDeclarationKind::ClassDeclaration(class) => {
            owners_from_class(class, file, source, imports)
        }
        ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => vec![owner_from_arrow(
            file,
            source,
            "default",
            arrow,
            arrow.span.start,
            false,
            imports,
        )],
        _ => Vec::new(),
    };
    // The relation gate may only credit a default import
    // (`import local from './owner'`) against an owner that IS the module's
    // default export (#4103 under-credit). Function and arrow owners carved
    // from `export default <declaration>` are that export. A default
    // EXPORTED CLASS has no class-level owner entry — `owners_from_class`
    // yields only its methods — so those methods carry the separate
    // `class_default_export` marker (constructor matching, #4104-B) while
    // `exported_as_default` stays false: a method is not itself the
    // module's default export (review #4138).
    match decl {
        ExportDefaultDeclarationKind::ClassDeclaration(_) => {
            for owner in &mut owners {
                owner.class_default_export = true;
            }
        }
        _ => {
            for owner in &mut owners {
                owner.exported_as_default = true;
            }
        }
    }
    owners
}

pub(crate) fn owners_from_variable_declaration(
    decl: &VariableDeclaration<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Vec<TypeScriptOwner> {
    decl.declarations
        .iter()
        .filter_map(|declarator| owner_from_variable_declarator(declarator, file, source, imports))
        .collect()
}

pub(crate) fn owner_from_variable_declarator(
    declarator: &VariableDeclarator<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Option<TypeScriptOwner> {
    let name = binding_identifier_name(&declarator.id)?;
    let init = declarator.init.as_ref()?;
    match init {
        Expression::ArrowFunctionExpression(arrow) => Some(owner_from_arrow(
            file,
            source,
            name,
            arrow,
            declarator.span.start,
            false,
            imports,
        )),
        Expression::FunctionExpression(func) => Some(owner_from_function(
            file,
            source,
            name,
            func,
            function_owner_kind(file, source, name, func.span.start, func.span.end),
            false,
            imports,
        )),
        _ => Some(TypeScriptOwner {
            name: name.to_string(),
            file: file.to_path_buf(),
            start_line: source.line_for_offset(declarator.span.start as usize),
            end_line: source.line_for_offset(declarator.span.end as usize),
            owner_kind: OwnerKind::ModuleFunction,
            class_name: None,
            decorated: false,
            exported_as_default: false,
            imports: imports.to_vec(),
            method_kind: TypeScriptMethodKind::Ordinary,
            class_default_export: false,
            default_object_member: false,
            commonjs_default_export: false,
            commonjs_whole_function: false,
            module_entries: Vec::new(),
            // Non-function initializers carry no resolvable signature facts:
            // no fixed positional signature the boundary witness could
            // arity-check against.
            params: Vec::new(),
            arity: None,
            source_text: None,
        }),
    }
}

/// Parameter facts for the owner signature (issue #4102): `(Some(n), names)`
/// when every parameter is a plain binding identifier and there is no rest
/// parameter; `(None, [])` otherwise (destructuring, rest, or empty list is
/// still resolvable — a zero-parameter function is `Some(0)`).
fn parameter_facts(params: &FormalParameters<'_>) -> (Option<usize>, Vec<String>) {
    if params.rest.is_some() {
        return (None, Vec::new());
    }
    let mut names = Vec::with_capacity(params.items.len());
    for item in &params.items {
        match binding_identifier_name(&item.pattern) {
            Some(name) => names.push(name.to_string()),
            // Destructured / pattern parameters cannot be mapped to an
            // argument position — fail closed to unknown arity.
            None => return (None, Vec::new()),
        }
    }
    (Some(names.len()), names)
}

pub(crate) fn owner_from_function(
    file: &Path,
    source: &SourceText<'_>,
    name: &str,
    func: &Function<'_>,
    owner_kind: OwnerKind,
    decorated: bool,
    imports: &[TypeScriptImport],
) -> TypeScriptOwner {
    let (arity, params) = parameter_facts(&func.params);
    TypeScriptOwner {
        name: name.to_string(),
        file: file.to_path_buf(),
        start_line: source.line_for_offset(func.span.start as usize),
        end_line: source.line_for_offset(func.span.end as usize),
        owner_kind,
        class_name: None,
        decorated,
        exported_as_default: false,
        imports: imports.to_vec(),
        method_kind: TypeScriptMethodKind::Ordinary,
        class_default_export: false,
        default_object_member: false,
        commonjs_default_export: false,
        commonjs_whole_function: false,
        module_entries: Vec::new(),
        params,
        arity,
        source_text: Some(source[func.span.start as usize..func.span.end as usize].to_string()),
    }
}

pub(crate) fn owner_from_arrow(
    file: &Path,
    source: &SourceText<'_>,
    name: &str,
    arrow: &ArrowFunctionExpression<'_>,
    owner_start: u32,
    decorated: bool,
    imports: &[TypeScriptImport],
) -> TypeScriptOwner {
    let (arity, params) = parameter_facts(&arrow.params);
    TypeScriptOwner {
        name: name.to_string(),
        file: file.to_path_buf(),
        start_line: source.line_for_offset(owner_start as usize),
        end_line: source.line_for_offset(arrow.span.end as usize),
        owner_kind: arrow_owner_kind(file, source, name, arrow.span.start, arrow.span.end),
        class_name: None,
        decorated,
        exported_as_default: false,
        imports: imports.to_vec(),
        method_kind: TypeScriptMethodKind::Ordinary,
        class_default_export: false,
        default_object_member: false,
        commonjs_default_export: false,
        commonjs_whole_function: false,
        module_entries: Vec::new(),
        params,
        arity,
        source_text: Some(source[arrow.span.start as usize..arrow.span.end as usize].to_string()),
    }
}

pub(crate) fn owners_from_class(
    class: &Class<'_>,
    file: &Path,
    source: &SourceText<'_>,
    imports: &[TypeScriptImport],
) -> Vec<TypeScriptOwner> {
    let mut owners = Vec::new();
    let class_decorated = !class.decorators.is_empty();
    let class_name = class
        .id
        .as_ref()
        .map(|identifier| identifier.name.as_str().to_string());
    for element in &class.body.body {
        if let ClassElement::MethodDefinition(method) = element
            && let Some(owner) = owner_from_method(
                method,
                file,
                source,
                class_decorated,
                class_name.as_deref(),
                imports,
            )
        {
            owners.push(owner);
        }
    }
    owners
}

pub(crate) fn owner_from_method(
    method: &MethodDefinition<'_>,
    file: &Path,
    source: &SourceText<'_>,
    class_decorated: bool,
    class_name: Option<&str>,
    imports: &[TypeScriptImport],
) -> Option<TypeScriptOwner> {
    if method.computed {
        return None;
    }
    let name = property_key_name(&method.key)?;
    let (arity, params) = parameter_facts(&method.value.params);
    Some(TypeScriptOwner {
        name,
        file: file.to_path_buf(),
        start_line: source.line_for_offset(method.span.start as usize),
        end_line: source.line_for_offset(method.span.end as usize),
        owner_kind: if method.r#static {
            OwnerKind::ClassMethod
        } else {
            OwnerKind::Method
        },
        class_name: class_name.map(str::to_string),
        decorated: class_decorated || !method.decorators.is_empty(),
        exported_as_default: false,
        imports: imports.to_vec(),
        method_kind: match method.kind {
            oxc_ast::ast::MethodDefinitionKind::Constructor => TypeScriptMethodKind::Constructor,
            oxc_ast::ast::MethodDefinitionKind::Get => TypeScriptMethodKind::Getter,
            oxc_ast::ast::MethodDefinitionKind::Set => TypeScriptMethodKind::Setter,
            oxc_ast::ast::MethodDefinitionKind::Method => TypeScriptMethodKind::Ordinary,
        },
        class_default_export: false,
        default_object_member: false,
        commonjs_default_export: false,
        commonjs_whole_function: false,
        module_entries: Vec::new(),
        params,
        arity,
        source_text: Some(source[method.span.start as usize..method.span.end as usize].to_string()),
    })
}

pub(crate) fn binding_identifier_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.as_str()),
        _ => None,
    }
}

pub(crate) fn property_key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        _ => None,
    }
}

pub(crate) fn function_owner_kind(
    file: &Path,
    source: &str,
    name: &str,
    start: u32,
    end: u32,
) -> OwnerKind {
    if looks_like_component_owner(file, source, name, start, end) {
        OwnerKind::Component
    } else {
        OwnerKind::Function
    }
}

pub(crate) fn arrow_owner_kind(
    file: &Path,
    source: &str,
    name: &str,
    start: u32,
    end: u32,
) -> OwnerKind {
    if looks_like_component_owner(file, source, name, start, end) {
        OwnerKind::Component
    } else {
        OwnerKind::ArrowFunction
    }
}

pub(crate) fn looks_like_component_owner(
    file: &Path,
    source: &str,
    name: &str,
    start: u32,
    end: u32,
) -> bool {
    if !matches!(
        file.extension().and_then(|extension| extension.to_str()),
        Some("tsx" | "jsx")
    ) || !starts_with_uppercase(name)
    {
        return false;
    }
    let start = start as usize;
    let end = end as usize;
    let Some(slice) = source.get(start..end) else {
        return false;
    };
    contains_jsx_like_return(slice)
}

pub(crate) fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|ch| ch.is_uppercase())
}

pub(crate) fn contains_jsx_like_return(slice: &str) -> bool {
    slice.contains("return <")
        || slice.contains("=> <")
        || slice
            .split("return (")
            .skip(1)
            .any(|tail| tail.trim_start().starts_with('<'))
        || slice
            .split("=> (")
            .skip(1)
            .any(|tail| tail.trim_start().starts_with('<'))
}

pub(crate) fn extract_imports_from_statements(
    statements: &[Statement<'_>],
) -> Vec<TypeScriptImport> {
    extract_imports_from_statements_inner(statements, true)
}

/// Imports for a TEST file (#7507): `export { x } from './y'` re-exports are
/// NOT imports — recording them as `imported/local` bindings lets a test
/// file's own re-export anchor a bare `owner(...)` call to the owner module,
/// and a file-level `const owner = ...` shadow is then missed (file-level
/// bindings never shadow imports). Owner-module indexing keeps the
/// re-export hop via `extract_imports_from_statements`.
pub(crate) fn extract_imports_for_test_file(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
) -> Vec<TypeScriptImport> {
    extract_imports_from_statements_inner(statements, false)
}

fn extract_imports_from_statements_inner(
    statements: &[Statement<'_>],
    include_reexports: bool,
) -> Vec<TypeScriptImport> {
    let mut out: Vec<TypeScriptImport> = Vec::new();
    // A file that binds its own `require` (`const require = ...`,
    // `function require()`) is not calling Node's loader, so a `require()`
    // call there says nothing about which module a local comes from (#10401).
    // The ambient-`require` contract of `createRequire(import.meta.url)` keeps
    // its reading.
    let mut top_level_names = Vec::new();
    // A block-level `function require()` hoists in sloppy CommonJS, so count it
    // as a file-level binding (the conservative reading).
    super::tests_extract::collect_registration_scope_names(statements, &mut top_level_names, false);
    // ES imports hoist, so the loader predicate sees all of them whatever their
    // position relative to `const require = createRequire(...)`.
    let require_is_ambient = !top_level_names.iter().any(|name| name == "require") || {
        let es_imports: Vec<TypeScriptImport> = statements
            .iter()
            .filter(|statement| matches!(statement, Statement::ImportDeclaration(_)))
            .flat_map(|statement| {
                extract_imports_from_statements_inner(std::slice::from_ref(statement), false)
            })
            .collect();
        super::admission::program_require_is_known(statements, &es_imports, false)
    };
    // Names the file assigns to anywhere (computed on the first mutable
    // `require()` binding).
    let mut written_names: Option<std::collections::HashSet<String>> = None;
    for stmt in statements {
        // ES module import declarations: `import { x } from './y'`
        if let Statement::ImportDeclaration(import) = stmt {
            if import.import_kind == ImportOrExportKind::Type {
                continue;
            }
            let source = import.source.value.to_string();
            let Some(specifiers) = &import.specifiers else {
                continue;
            };
            for specifier in specifiers {
                match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                        if specifier.import_kind == ImportOrExportKind::Type {
                            continue;
                        }
                        let Some(imported) = module_export_name_text(&specifier.imported) else {
                            continue;
                        };
                        push_unique_import(
                            &mut out,
                            TypeScriptImport {
                                source: source.clone(),
                                imported: Some(imported),
                                local: specifier.local.name.as_str().to_string(),
                                namespace: false,
                            },
                        );
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                        push_unique_import(
                            &mut out,
                            TypeScriptImport {
                                source: source.clone(),
                                imported: Some("default".to_string()),
                                local: specifier.local.name.as_str().to_string(),
                                namespace: false,
                            },
                        );
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                        push_unique_import(
                            &mut out,
                            TypeScriptImport {
                                source: source.clone(),
                                imported: None,
                                local: specifier.local.name.as_str().to_string(),
                                namespace: true,
                            },
                        );
                    }
                }
            }
            continue;
        }

        // Simple re-exports: `export { x } from './y'`
        // These appear as ExportNamedDeclaration with a source string and no
        // inline declaration.  We record each re-exported specifier as an
        // import so that `import_source_matches_owner` can resolve the
        // re-export chain one hop back. Test files never use this hop
        // (#7507): their re-exports are not imports.
        if let Statement::ExportNamedDeclaration(export) = stmt {
            if !include_reexports {
                continue;
            }
            if export.declaration.is_none()
                && let Some(re_source) = &export.source
            {
                let src = re_source.value.to_string();
                for specifier in &export.specifiers {
                    if specifier.export_kind == ImportOrExportKind::Type {
                        continue;
                    }
                    let Some(local_name) = module_export_name_text(&specifier.local) else {
                        continue;
                    };
                    let exported_name = module_export_name_text(&specifier.exported)
                        .unwrap_or_else(|| local_name.clone());
                    push_unique_import(
                        &mut out,
                        TypeScriptImport {
                            source: src.clone(),
                            imported: Some(local_name),
                            local: exported_name,
                            namespace: false,
                        },
                    );
                }
            }
            continue;
        }

        // CommonJS require(): `const { x } = require('./y')` or
        // `const x = require('./y')`.
        // Only handles the simple synchronous require() form with a string
        // literal path.  Dynamic require / factory-returning requires are
        // fail-closed (not extracted → no import match → caller emits
        // `typescript_target_unresolved` when ownership cannot resolve).
        if let Statement::VariableDeclaration(var_decl) = stmt {
            for declarator in &var_decl.declarations {
                let Some(init) = &declarator.init else {
                    continue;
                };
                if !require_is_ambient {
                    continue;
                }
                // A `let`/`var` binding the file assigns to again may no longer
                // hold the module's export, so it is not an import (#10401). A
                // `const` cannot be reassigned.
                let mutable = var_decl.kind != oxc_ast::ast::VariableDeclarationKind::Const;
                let mut written = |name: &str| {
                    mutable
                        && written_names
                            .get_or_insert_with(|| identifier_writes(statements))
                            .contains(name)
                };
                // One member of a literal require: `const f = require('./y').f`
                // picks the named export `f` out of the module (#9188). It is
                // the twin of `const { f } = require('./y')`, so it records the
                // same named import. A call on the member or a computed key
                // other than a string literal is not read.
                if let Some((member_source, member)) = require_member_source(init)
                    && let Some(name) = binding_identifier_name(&declarator.id)
                {
                    if written(name) {
                        continue;
                    }
                    push_unique_import(
                        &mut out,
                        TypeScriptImport {
                            source: member_source,
                            imported: Some(member),
                            local: name.to_string(),
                            namespace: false,
                        },
                    );
                    continue;
                }
                let source_str = require_string_literal_source(init);
                let Some(source_str) = source_str else {
                    continue;
                };
                // Simple binding: `const x = require('./y')`
                if let Some(name) = binding_identifier_name(&declarator.id) {
                    if written(name) {
                        continue;
                    }
                    push_unique_import(
                        &mut out,
                        TypeScriptImport {
                            source: source_str.clone(),
                            imported: Some("default".to_string()),
                            local: name.to_string(),
                            namespace: true, // namespace-like: callers access via x.method
                        },
                    );
                    continue;
                }
                // Destructured binding: `const { x, y: z } = require('./y')`
                if let BindingPattern::ObjectPattern(obj) = &declarator.id {
                    for prop in &obj.properties {
                        let Some(key_name) = object_binding_key_name(prop) else {
                            continue;
                        };
                        let local_name = binding_identifier_name(&prop.value)
                            .unwrap_or(key_name)
                            .to_string();
                        if written(&local_name) {
                            continue;
                        }
                        push_unique_import(
                            &mut out,
                            TypeScriptImport {
                                source: source_str.clone(),
                                imported: Some(key_name.to_string()),
                                local: local_name,
                                namespace: false,
                            },
                        );
                    }
                }
            }
        }
    }
    out
}

/// Extract the string literal source from a `require('...')` call expression.
///
/// Returns `Some(source)` only for the simple form `require("./literal")`.
/// Dynamic arguments (variables, template literals, concatenations) return
/// `None` so the caller can fail closed.
pub(crate) fn require_string_literal_source(expr: &Expression<'_>) -> Option<String> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name.as_str() != "require" {
        return None;
    }
    let first_arg = call.arguments.first()?;
    let oxc_ast::ast::Argument::StringLiteral(literal) = first_arg else {
        return None;
    };
    Some(literal.value.to_string())
}

/// The source and member of `require("<literal>").member` (or the string
/// literal spelling `require("<literal>")["member"]`), for `const f =
/// require("./y").f` (#9188). `None` for any other initializer, so a call on
/// the member, a computed key and a dynamic `require` argument stay unread.
fn require_member_source(expr: &Expression<'_>) -> Option<(String, String)> {
    let read = require_member_parts(expr)?;
    // `.default` is an ES default only for a transpiled module; a native
    // CommonJS module has no such property, so it is not read as the import.
    (read.1 != "default").then_some(read)
}

fn require_member_parts(expr: &Expression<'_>) -> Option<(String, String)> {
    match expr {
        Expression::StaticMemberExpression(member) if !member.optional => {
            let source = require_string_literal_source(&member.object)?;
            Some((source, member.property.name.as_str().to_string()))
        }
        Expression::ComputedMemberExpression(member) if !member.optional => {
            let source = require_string_literal_source(&member.object)?;
            let Expression::StringLiteral(key) = &member.expression else {
                return None;
            };
            Some((source, key.value.to_string()))
        }
        _ => None,
    }
}

/// Extract the key name from an object-pattern binding property.
///
/// Handles `{ x }` (shorthand) and `{ x: y }` (renamed) but not computed
/// keys (`{ [expr]: y }`).
pub(crate) fn object_binding_key_name<'a>(
    prop: &'a oxc_ast::ast::BindingProperty<'a>,
) -> Option<&'a str> {
    if prop.computed {
        return None;
    }
    match &prop.key {
        PropertyKey::StaticIdentifier(ident) => Some(ident.name.as_str()),
        PropertyKey::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

pub(crate) fn push_unique_import(out: &mut Vec<TypeScriptImport>, import: TypeScriptImport) {
    if !out.iter().any(|existing| existing == &import) {
        out.push(import);
    }
}

pub(crate) fn push_unique_string(out: &mut Vec<String>, value: String) {
    if !out.iter().any(|existing| existing == &value) {
        out.push(value);
    }
}

pub(crate) fn module_export_name_text(name: &ModuleExportName<'_>) -> Option<String> {
    match name {
        ModuleExportName::IdentifierName(ident) => Some(ident.name.as_str().to_string()),
        ModuleExportName::IdentifierReference(ident) => Some(ident.name.as_str().to_string()),
        ModuleExportName::StringLiteral(literal) => Some(literal.value.to_string()),
    }
}

// ── Owner-extraction gap detection (#4104-A) ─────────────────────────────────

/// Detect the first changed owner-shaped construct the syntax-first owner
/// extractor does not index (#4104-A).
///
/// Real producer for the `typescript_owner_extraction_partial` named
/// limitation (classification-neutral additive disclosure, mirroring
/// `typescript_test_extraction_partial`): the owner universe still contains
/// exactly what `extract_owners` extracts; this function only reports that
/// it is PARTIAL for the changed lines, so a silent zero-finding result
/// (`analysis_complete: true`, no findings, no other limitations) is known
/// to mean "the changed line sits in an unsupported owner shape", not "the
/// changed behavior has no owning seam".
///
/// Detected shapes (bounded preview slice — extracting these shapes is a
/// separate backlog item; this lane only discloses them):
///
/// - `class C { f = (a) => {...} }` — arrow-function class fields.
/// - `class C { #adjust(a) {...} }` (also string/numeric/computed keys) —
///   class methods whose key the extractor cannot name.
/// - `class C { static {...} }` — class static blocks.
/// - `class C { accessor level = 1 }` — TC39 auto-accessors.
/// - `enum PlanRate { Basic = 9 }` — TS enum member value changes.
/// - `namespace Pricing { export function tax ... }` — TS module/namespace
///   declarations.
///
/// Returns `None` when the file fails to parse (parse-error disclosure owns
/// that case), when no changed line is supplied, or when every changed line
/// falls inside supported owner shapes (the negative-control contract).
pub(crate) fn detect_owner_extraction_gap(
    file: &Path,
    source: &str,
    changed_lines: &[usize],
    workspace_root: &Path,
    alias_map: Option<&super::tsconfig::TsAliasMap>,
) -> Option<TypeScriptOwnerExtractionGap> {
    if changed_lines.is_empty() {
        return None;
    }
    let changed: std::collections::HashSet<usize> = changed_lines.iter().copied().collect();
    // Parse-error content yields default (empty) facts, which correctly
    // produces no gap — parse-error disclosure owns that case.
    let facts =
        super::extraction_cache::phase_two_facts_cached(file, source, workspace_root, alias_map)?;
    for shape in &facts.gap_shapes {
        if let Some(sample_line) =
            (shape.lines.0..=shape.lines.1).find(|line| changed.contains(line))
        {
            return Some(TypeScriptOwnerExtractionGap {
                file: file.to_path_buf(),
                sample_line,
                shape: shape.shape,
                snippet: snippet_for_span(source, shape.span.0, shape.span.1),
            });
        }
    }
    None
}

/// Changed lines that sit inside a runtime enum member (`High = 3,`). The
/// owner extractor indexes no enum, so such a line has no owner and the
/// classifier returns nothing for it; without this the edit is silent apart
/// from the file-level `typescript_owner_extraction_partial` disclosure
/// (#9436). Ambient `declare enum` is type-only and never listed. A file that
/// does not parse yields nothing, as its parse-error disclosure owns it.
pub(crate) fn changed_enum_member_lines(
    file: &Path,
    source: &str,
    changed_lines: &[usize],
    workspace_root: &Path,
    alias_map: Option<&super::tsconfig::TsAliasMap>,
) -> std::collections::BTreeMap<usize, String> {
    // A file without the keyword has no enum; skip the facts lookup.
    if changed_lines.is_empty() || !source.contains("enum") {
        return std::collections::BTreeMap::new();
    }
    let changed: std::collections::HashSet<usize> = changed_lines.iter().copied().collect();
    // Parse-error content yields default (empty) facts, matching the
    // parse-error disclosure owning that case.
    let Some(facts) =
        super::extraction_cache::phase_two_facts_cached(file, source, workspace_root, alias_map)
    else {
        return std::collections::BTreeMap::new();
    };
    let mut lines = std::collections::BTreeMap::new();
    for member in &facts.enum_members {
        for line in (member.lines.0..=member.lines.1).filter(|line| changed.contains(line)) {
            lines.entry(line).or_insert_with(|| member.identity.clone());
        }
    }
    lines
}

/// 1-based inclusive line ranges of top-level ambient declarations
/// (`declare function`, `export declare const`, `declare module 'x' { ... }`,
/// `declare global { ... }`), read from the syntax tree. They are type-only
/// and erased at compile time, so a changed line inside one has no runtime
/// behavior to probe. A file that does not parse yields no ranges, so its
/// lines stay probed.
pub(crate) fn ambient_declaration_lines(
    file: &Path,
    source: &str,
    workspace_root: &Path,
    alias_map: Option<&super::tsconfig::TsAliasMap>,
) -> Vec<(usize, usize)> {
    // Parse-error content yields default (empty) facts, so its lines stay
    // probed — exactly the previous contract.
    super::extraction_cache::phase_two_facts_cached(file, source, workspace_root, alias_map)
        .map(|facts| facts.ambient_ranges)
        .unwrap_or_default()
}

/// Parse-derived facts phase 2 reads per changed file (#10051). One parse
/// per `(path, content)` serves all three phase-2 consumers, which used to
/// re-parse the changed file three separate times (~2.0 s on a 5 MB file
/// per save). Every fact is a pure function of the source bytes; the
/// per-diff `changed_lines` filtering happens at call time, so one cached
/// extraction serves every diff of the same content.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PhaseTwoFacts {
    /// Every unsupported owner shape span in source order. Filtering to the
    /// first entry whose line range intersects a changed line reproduces the
    /// previous first-hit short-circuit exactly.
    pub(crate) gap_shapes: Vec<GapShape>,
    /// Non-declare enum members with `EnumName::member_name` identities (the
    /// names, not the lines, identify a member so finding ids survive line
    /// shifts).
    pub(crate) enum_members: Vec<EnumMember>,
    /// 1-based inclusive line ranges of top-level ambient declarations.
    pub(crate) ambient_ranges: Vec<(usize, usize)>,
}

/// One unsupported owner shape: its disclosure label, byte span (for the
/// snippet), and precomputed 1-based line range (for the changed-line gate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GapShape {
    pub(crate) shape: &'static str,
    pub(crate) span: (usize, usize),
    pub(crate) lines: (usize, usize),
}

/// One runtime enum member: its 1-based line range and stable identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnumMember {
    pub(crate) lines: (usize, usize),
    pub(crate) identity: String,
}

/// Extract every phase-2 fact from one already-parsed program (#10071):
/// the parse-free half of the former `extract_phase_two_facts`, so the
/// combined per-file facts cache can derive owners, export facts, and
/// phase-2 facts from ONE parse. Parse errors never reach this fn (the
/// caller substitutes the default facts); every fact is a pure function of
/// the source bytes, and the per-diff `changed_lines` filtering happens at
/// call time, so one extraction serves every diff of the same content.
pub(crate) fn phase_two_facts_from_program(
    program: &oxc_ast::ast::Program<'_>,
    source: &str,
) -> PhaseTwoFacts {
    let text = SourceText::new(source);
    let mut facts = PhaseTwoFacts::default();
    collect_gap_shapes(&program.body, &text, &mut facts.gap_shapes);
    collect_enum_members(&program.body, source, &text, &mut facts.enum_members);
    collect_ambient_ranges(&program.body, &text, &mut facts.ambient_ranges);
    facts
}

/// Walk top-level statements for unsupported owner shapes, collecting every
/// shape span in source order.
fn collect_gap_shapes(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &SourceText<'_>,
    out: &mut Vec<GapShape>,
) {
    let mut push = |shape: &'static str, span: oxc_span::Span| {
        out.push(GapShape {
            shape,
            span: (span.start as usize, span.end as usize),
            lines: (
                source.line_for_offset(span.start as usize),
                source.line_for_offset(span.end as usize),
            ),
        });
    };
    for stmt in statements {
        match stmt {
            Statement::TSEnumDeclaration(decl) => {
                push("enum declaration", decl.span);
            }
            Statement::TSModuleDeclaration(decl) => {
                push("module/namespace declaration", decl.span);
            }
            Statement::ClassDeclaration(class) => {
                collect_class_element_shapes(class, &mut push);
            }
            Statement::ExportNamedDeclaration(export) => match export.declaration.as_ref() {
                Some(Declaration::TSEnumDeclaration(decl)) => {
                    push("enum declaration", decl.span);
                }
                Some(Declaration::TSModuleDeclaration(decl)) => {
                    push("module/namespace declaration", decl.span);
                }
                Some(Declaration::ClassDeclaration(class)) => {
                    collect_class_element_shapes(class, &mut push);
                }
                _ => {}
            },
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    collect_class_element_shapes(class, &mut push);
                }
            }
            Statement::ExpressionStatement(statement) => {
                collect_expression_statement_function_shapes(&statement.expression, &mut push);
            }
            _ => {}
        }
    }
}

/// A function value carried by a top-level expression statement that the
/// extractor never turns into an owner (#4754):
///
/// - `res.send = function send() {...}` / `Foo.prototype.bar = () => ...`
///   (a member-assigned function on an object other than the CommonJS
///   `exports` / `module.exports` targets, which are export shapes);
/// - a function inside a top-level call: an IIFE (`(function () {...})()`,
///   `(function () {...}.call(this))`, `!function () {...}()`) or a wrapper
///   argument (`define([...], function () {...})`).
///
/// Shapes are collected in the order the previous first-hit scan checked
/// them (callee, then arguments) so changed-line filtering picks the same
/// sample.
fn collect_expression_statement_function_shapes(
    expression: &Expression<'_>,
    push: &mut impl FnMut(&'static str, oxc_span::Span),
) {
    match expression.without_parentheses() {
        Expression::AssignmentExpression(assign) => {
            let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(target) = &assign.left
            else {
                return;
            };
            if is_commonjs_export_target(target) {
                return;
            }
            let value = assign.right.without_parentheses();
            if is_function_value(value) {
                push("member-assigned function", value.span());
            }
        }
        Expression::UnaryExpression(unary) => {
            collect_expression_statement_function_shapes(&unary.argument, push);
        }
        Expression::CallExpression(call) => {
            let callee = match call.callee.without_parentheses() {
                // `(function () {...}).call(this)` / `.apply(...)`
                Expression::StaticMemberExpression(member)
                    if matches!(member.property.name.as_str(), "call" | "apply") =>
                {
                    member.object.without_parentheses()
                }
                callee => callee,
            };
            if is_function_value(callee) {
                push("function inside a top-level call", callee.span());
            }
            for argument in call
                .arguments
                .iter()
                .filter_map(|argument| argument.as_expression())
            {
                let argument = argument.without_parentheses();
                if is_function_value(argument) {
                    push("function inside a top-level call", argument.span());
                }
            }
        }
        _ => {}
    }
}

fn is_function_value(expression: &Expression<'_>) -> bool {
    matches!(
        expression,
        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
    )
}

/// `exports.NAME`, `module.exports`, or `module.exports.NAME`: CommonJS
/// export targets, owned as export shapes rather than disclosed here.
fn is_commonjs_export_target(target: &oxc_ast::ast::StaticMemberExpression<'_>) -> bool {
    match target.object.without_parentheses() {
        Expression::Identifier(object) => {
            object.name == "exports"
                || (object.name == "module" && target.property.name == "exports")
        }
        Expression::StaticMemberExpression(object) => {
            object.property.name == "exports"
                && matches!(&object.object, Expression::Identifier(module) if module.name == "module")
        }
        _ => false,
    }
}

/// Collect the unsupported member shapes the extractor skips, in element
/// order.
fn collect_class_element_shapes(
    class: &Class<'_>,
    push: &mut impl FnMut(&'static str, oxc_span::Span),
) {
    for element in &class.body.body {
        let (shape, span) = match element {
            ClassElement::PropertyDefinition(prop) => {
                // Only function-valued fields are owner-shaped; a plain data
                // field change is not an extracted-owner regression.
                let is_function_init = matches!(
                    prop.value.as_ref(),
                    Some(Expression::ArrowFunctionExpression(_))
                        | Some(Expression::FunctionExpression(_))
                );
                if !is_function_init {
                    continue;
                }
                ("arrow-function class field", prop.span)
            }
            ClassElement::StaticBlock(block) => ("class static block", block.span),
            ClassElement::AccessorProperty(accessor) => ("accessor auto-accessor", accessor.span),
            ClassElement::MethodDefinition(method) => {
                // `#private`, string-literal, numeric, and computed keys are
                // all skipped by `property_key_name`; changed lines inside
                // them are silent today (#4104-A F2b).
                if property_key_name(&method.key).is_some() {
                    continue;
                }
                ("class method with unsupported key", method.span)
            }
            _ => continue,
        };
        push(shape, span);
    }
}

/// Runtime enum members of top-level (or exported-top-level) declarations,
/// in source order. Ambient `declare enum` is type-only and never listed.
fn collect_enum_members(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    raw: &str,
    source: &SourceText<'_>,
    out: &mut Vec<EnumMember>,
) {
    let mut collect = |decl: &oxc_ast::ast::TSEnumDeclaration<'_>| {
        if decl.declare {
            return;
        }
        for member in &decl.body.members {
            // Enum and member names, not the line, identify the member so
            // the finding id survives unrelated line shifts.
            let name = raw
                .get(member.id.span().start as usize..member.id.span().end as usize)
                .unwrap_or("");
            out.push(EnumMember {
                lines: (
                    source.line_for_offset(member.span.start as usize),
                    source.line_for_offset(member.span.end as usize),
                ),
                identity: format!("{}::{name}", decl.id.name),
            });
        }
    };
    for stmt in statements {
        match stmt {
            Statement::TSEnumDeclaration(decl) => collect(decl),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::TSEnumDeclaration(decl)) = export.declaration.as_ref() {
                    collect(decl);
                }
            }
            _ => {}
        }
    }
}

/// 1-based inclusive line ranges of top-level ambient declarations
/// (`declare function`, `export declare const`, `declare module 'x' { ... }`,
/// `declare global { ... }`), read from the syntax tree. They are type-only
/// and erased at compile time, so a changed line inside one has no runtime
/// behavior to probe.
fn collect_ambient_ranges(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &SourceText<'_>,
    out: &mut Vec<(usize, usize)>,
) {
    for stmt in statements {
        let ambient = match stmt {
            Statement::ExportNamedDeclaration(export) => export
                .declaration
                .as_ref()
                .is_some_and(|declaration| declaration.declare()),
            _ => stmt
                .as_declaration()
                .is_some_and(|declaration| declaration.declare()),
        };
        if ambient {
            let span = stmt.span();
            out.push((
                source.line_for_offset(span.start as usize),
                source.line_for_offset(span.end as usize),
            ));
        }
    }
}

/// Every identifier the statements assign to: plain and destructuring
/// assignment targets, `++`/`--`, `for (x of ...)` heads, and an identifier
/// under a TypeScript wrapper (`(x as T) = v`). Member writes (`x.p = v`) do
/// not rebind `x` and are not counted.
fn identifier_writes(statements: &[Statement<'_>]) -> std::collections::HashSet<String> {
    #[derive(Default)]
    struct Writes {
        names: std::collections::HashSet<String>,
        wrapped: bool,
    }
    impl<'a> oxc_ast_visit::Visit<'a> for Writes {
        fn visit_simple_assignment_target(
            &mut self,
            target: &oxc_ast::ast::SimpleAssignmentTarget<'a>,
        ) {
            use oxc_ast::ast::SimpleAssignmentTarget as Target;
            match target {
                Target::AssignmentTargetIdentifier(id) => {
                    self.names.insert(id.name.to_string());
                }
                Target::TSAsExpression(_)
                | Target::TSSatisfiesExpression(_)
                | Target::TSNonNullExpression(_)
                | Target::TSTypeAssertion(_) => {
                    let outer = std::mem::replace(&mut self.wrapped, true);
                    oxc_ast_visit::walk::walk_simple_assignment_target(self, target);
                    self.wrapped = outer;
                    return;
                }
                _ => {}
            }
            oxc_ast_visit::walk::walk_simple_assignment_target(self, target);
        }

        fn visit_assignment_target_property_identifier(
            &mut self,
            property: &oxc_ast::ast::AssignmentTargetPropertyIdentifier<'a>,
        ) {
            self.names.insert(property.binding.name.to_string());
            oxc_ast_visit::walk::walk_assignment_target_property_identifier(self, property);
        }

        fn visit_identifier_reference(&mut self, id: &oxc_ast::ast::IdentifierReference<'a>) {
            if self.wrapped {
                self.names.insert(id.name.to_string());
            }
        }
    }
    let mut writes = Writes::default();
    for statement in statements {
        oxc_ast_visit::walk::walk_statement(&mut writes, statement);
    }
    writes.names
}
