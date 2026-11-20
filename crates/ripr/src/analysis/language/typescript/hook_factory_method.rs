//! Attribute calls on a beforeEach-created value to an edit inside the
//! particular object method it calls. Neither a source-to-bundle redirect
//! nor a method call establishes what the test assertion observes.

use super::*;
use std::collections::BTreeSet;
use std::ops::Range;

pub(super) struct MethodCandidate<'a> {
    pub(super) lines: Range<usize>,
    pub(super) candidate: TypeScriptRelatedCandidate<'a>,
}

pub(super) fn candidates<'a>(
    owner: &TypeScriptOwner,
    tests: &[&'a TypeScriptTest],
    root: Option<&Path>,
    reexports: &ReExportIndex,
    map: Option<&TsAliasMap>,
) -> Vec<MethodCandidate<'a>> {
    // Only parse an owner after an indexed candidate supplies both a tested
    // hook receiver and an import that could be this factory.
    let methods = std::cell::OnceCell::new();
    let mut candidates = Vec::new();
    for &test in tests {
        if test.setup_hook_bodies.is_empty() || !test.mocks_in_file.is_empty() {
            continue;
        }
        for binding in test.scope_bindings.iter() {
            let Some(factory) = &binding.hook_factory else { continue };
            if !test.imports_in_file.iter().any(|import| import.local == *factory)
                || test.member_replacements_in_file.iter().any(|name| {
                    name == "*" || name == &binding.name || name == factory
                })
                || local_identifier_declared_in_test_body(&test.body_text, &binding.name)
            {
                continue;
            }
            // The hook extractor proved a direct call of this exact
            // identifier. Ask the ordinary import resolver about that call,
            // retaining mocks, import identity, shadow and replacement facts.
            let mut view = test.clone();
            view.body_text = format!("{factory}()");
            view.reach_body_text = None;
            let direct = matches!(
                owner_call_relation(&view, owner, reexports, map, root),
                Some(TypeScriptRelationKind::ImportedOwnerCall
                    | TypeScriptRelationKind::DirectOwnerCall
                    | TypeScriptRelationKind::ImportAliasOwnerCall)
            );
            if !direct && !map.zip(root).is_some_and(|(map, root)| {
                related_tests::build_entry_call(&view, owner, reexports, map, root).is_some()
            }) {
                continue;
            }
            let body = test.reach_body_text.as_deref().unwrap_or(&test.body_text);
            let body = super::classifier::normalize_member_access(&ts_mask_literals_and_comments(body));
            for (method, lines) in methods.get_or_init(|| method_lines(owner)) {
                if !test.member_replacements_in_file.iter().any(|name| name == method)
                    && contains_member_call_name(&body, &binding.name, method)
                {
                    candidates.push(MethodCandidate {
                        lines: lines.clone(),
                        candidate: related_tests::related_candidate(
                            test, TypeScriptRelationKind::HookFactoryMethod, owner, map, root,
                        ),
                    });
                }
            }
        }
    }
    candidates
}

/// Only method interiors of a single, unconditional, final object-literal
/// return qualify. One-line/shared delimiters and computed or duplicate keys
/// are deliberately outside this slice.
fn method_lines(owner: &TypeScriptOwner) -> Vec<(String, Range<usize>)> {
    let Some(text) = &owner.source_text else { return Vec::new() };
    let first = owner.start_line;
    super::parse::parse_on_worker(&owner.file, text, move |file, source, allocator| {
        let parsed = Parser::new(allocator, source, source_type_for(file)).parse();
        let [Statement::FunctionDeclaration(function)] = parsed.program.body.as_slice() else {
            return None;
        };
        if !parsed.errors.is_empty() || function.generator || function.r#async {
            return None;
        }
        let (last, setup) = function.body.as_ref()?.statements.split_last()?;
        if !setup.iter().all(|stmt| matches!(stmt,
            Statement::VariableDeclaration(_) | Statement::ExpressionStatement(_))) {
            return None;
        }
        let Statement::ReturnStatement(ret) = last else { return None };
        let Expression::ObjectExpression(object) = ret.argument.as_ref()?.without_parentheses() else {
            return None;
        };
        let source = SourceText::new(source);
        let mut names = BTreeSet::new();
        let mut methods = Vec::new();
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else { return None };
            let PropertyKey::StaticIdentifier(key) = &property.key else { return None };
            if property.computed || key.name == "__proto__"
                || !names.insert(key.name.as_str())
                || property.kind != oxc_ast::ast::PropertyKind::Init
            {
                return None;
            }
            if !property.method { continue; }
            let Expression::FunctionExpression(method) = &property.value else { return None };
            if method.generator || method.r#async { continue; }
            let span = method.body.as_ref()?.span;
            let start = first + source.line_for_offset(span.start as usize);
            let end = first + source.line_for_offset(span.end as usize) - 1;
            if start < end {
                methods.push((key.name.to_string(), start..end));
            }
        }
        Some(methods)
    }).ok().flatten().unwrap_or_default()
}
