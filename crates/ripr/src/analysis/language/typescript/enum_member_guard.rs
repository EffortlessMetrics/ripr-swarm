//! Declaration-backed enum identity for RIPR-SPEC-0243 rule 15.
//!
//! Local spelling is not provenance: a renamed import can be the same enum,
//! while a same-named import or object can be another value.

use super::*;

/// Code in which imports, exports of bindings, enum declarations and the
/// supported function signatures cannot look like value escapes. Everything
/// else remains visible to the conservative read-only-root check.
pub(crate) fn enum_member_use_code(body: &[Statement<'_>], source: &str) -> String {
    let mut ranges = Vec::new();
    for statement in body {
        match statement {
            Statement::ImportDeclaration(_) | Statement::ExportAllDeclaration(_) => {
                ranges.push(statement.span());
            }
            Statement::TSEnumDeclaration(declaration) => ranges.push(declaration.span),
            Statement::FunctionDeclaration(function) => {
                enum_function_types(function, &mut ranges);
            }
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                None => ranges.push(export.span),
                Some(Declaration::TSEnumDeclaration(declaration)) => {
                    ranges.push(declaration.span);
                }
                Some(Declaration::FunctionDeclaration(function)) => {
                    enum_function_types(function, &mut ranges);
                }
                _ => {}
            },
            _ => {}
        }
    }
    let ranges: Vec<_> = ranges
        .into_iter()
        .map(|span| span.start as usize..span.end as usize)
        .collect();
    let code = tests_extract::blank_ranges(source, &ranges);
    classifier::ts_mask_literals_and_comments(&code)
}

fn enum_function_types(function: &Function<'_>, ranges: &mut Vec<oxc_span::Span>) {
    for parameter in &function.params.items {
        if let Some(annotation) = &parameter.type_annotation {
            ranges.push(annotation.span);
        }
    }
    if let Some(annotation) = &function.return_type {
        ranges.push(annotation.span);
    }
}

/// A root can only be read through a member. Bare aliases, casts, calls,
/// assignments, deletion and member mutation all leave its value unknown.
/// This runs once per imported/declared root over its whole module, including
/// hooks and other tests, rather than trusting only the assertion callback.
pub(crate) fn enum_member_root_is_read_only(code: &str, root: &str) -> bool {
    !tests_extract::identifier_written_in(code, root)
        && !ts_members_written(code, root, true)
        && mock_payload_guard::identifier_offsets(code, root)
            .into_iter()
            .all(|at| code[at + root.len()..].trim_start().starts_with('.'))
}

pub(crate) fn ts_same_enum_member(
    test_path: &str,
    owner_path: &str,
    test: &TypeScriptTest,
    owner: &TypeScriptOwner,
    index: &ReExportIndex,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    let Some(root) = test_path.split('.').next() else {
        return false;
    };
    if local_identifier_declared_in_test_body(&test.body_text, root)
        || test.scope_bindings.iter().any(|binding| {
            binding.name == root
                && (!binding.file_level
                    || normalized_module_path(&test.file) != normalized_module_path(&owner.file))
        })
        || test
            .unstable_member_roots_in_file
            .iter()
            .any(|name| name == root)
    {
        return false;
    }
    let Some(owner_root) = owner_path.split('.').next() else {
        return false;
    };
    if owner.params.iter().any(|name| name == owner_root)
        || !owner
            .source_text
            .as_deref()
            .is_some_and(|source| ts_parameter_is_stable(source, owner_root))
    {
        return false;
    }
    let Some(owner_identity) = index.enum_member_identity(
        &owner.file,
        owner_path,
        &owner.imports,
        test,
        alias_map,
        workspace_root,
    ) else {
        return false;
    };
    index.enum_member_identity(
        &test.file,
        test_path,
        &test.imports_in_file,
        test,
        alias_map,
        workspace_root,
    ) == Some(owner_identity)
}
