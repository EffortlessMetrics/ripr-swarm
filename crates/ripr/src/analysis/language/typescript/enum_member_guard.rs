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

/// An enum root can only be read through a scalar member; a namespace needs
/// both the enum and scalar member segments. Bare aliases, casts, calls,
/// assignments, deletion and member mutation all leave its value unknown.
/// This runs once per imported/declared root over its whole module, including
/// hooks and other tests, rather than trusting only the assertion callback.
pub(crate) fn enum_member_root_is_read_only(code: &str, root: &str, namespace: bool) -> bool {
    !tests_extract::identifier_written_in(code, root)
        && !ts_members_written(code, root, true)
        && mock_payload_guard::identifier_offsets(code, root)
            .into_iter()
            .all(|at| {
                if !enum_member_prefix_is_read_only(&code[..at]) {
                    return false;
                }
                let mut rest = &code[at + root.len()..];
                // A namespace's first member is the enum object itself.
                // Only its next member is a scalar enum value; handing the
                // object to an alias or call can mutate every canonical alias.
                for _ in 0..if namespace { 2 } else { 1 } {
                    let Some(member) = rest.trim_start().strip_prefix('.') else {
                        return false;
                    };
                    let member = member.trim_start();
                    let length = member
                        .chars()
                        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(*ch, '_' | '$'))
                        .count();
                    if length == 0 {
                        return false;
                    }
                    rest = &member[length..];
                }
                enum_member_tail_is_read_only(rest)
            })
}

/// Parentheses and non-null assertions can preserve a member reference as
/// a callee, including its receiver (`(Level.valueOf)()`). Merely reading a
/// grouped scalar remains safe. Cast/generic-call tails are unknown rather
/// than an opportunity to infer a scalar value through another grammar.
fn enum_member_tail_is_read_only(rest: &str) -> bool {
    let rest = enum_after_invocation_wrappers(rest);
    // Normalize a member target to an identifier for the existing write
    // reader. Five operator characters distinguish assignments/updates from
    // comparisons without copying the remaining module or its operator law.
    let operator: String = rest.chars().take(5).collect();
    if tests_extract::identifier_written_in(&format!("enum_member {operator}"), "enum_member") {
        return false;
    }
    !rest.starts_with('(')
        && !rest.starts_with('`')
        && !enum_generic_invocation_tail(rest)
        && !["as", "satisfies"].iter().any(|keyword| {
            rest.strip_prefix(keyword).is_some_and(|tail| {
                !tail.starts_with(|ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'))
            })
        })
}

fn enum_member_prefix_is_read_only(mut before: &str) -> bool {
    loop {
        before = before.trim_end();
        if let Some(prefix) = before.strip_suffix('(') {
            before = prefix;
        } else {
            break;
        }
    }
    !before.ends_with("++")
        && !before.ends_with("--")
        && !before.strip_suffix("delete").is_some_and(|head| {
            !head.ends_with(|ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'))
                && !head.trim_end().ends_with('.')
        })
}

fn enum_after_invocation_wrappers(rest: &str) -> &str {
    let rest = enum_after_grouping_wrappers(rest);
    rest.strip_prefix("?.").map_or(rest, str::trim_start)
}

fn enum_after_grouping_wrappers(mut rest: &str) -> &str {
    loop {
        rest = rest.trim_start();
        if let Some(tail) = rest.strip_prefix(')') {
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('!').filter(|tail| !tail.starts_with('=')) {
            rest = tail;
        } else {
            break;
        }
    }
    rest
}

/// A plain less-than comparison remains a scalar read. Only a balanced
/// type-argument-shaped span followed by invocation creates this uncertainty.
fn enum_generic_invocation_tail(rest: &str) -> bool {
    let Some(arguments) = rest.strip_prefix('<') else {
        return false;
    };
    let mut depth = 1usize;
    let mut braces = 0usize;
    for (at, ch) in arguments.char_indices() {
        match ch {
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            ';' if braces == 0 => return false,
            '<' => depth += 1,
            '>' if arguments[..at].ends_with('=') => {}
            '>' => {
                depth -= 1;
                if depth == 0 {
                    let tail = enum_after_invocation_wrappers(&arguments[at + 1..]);
                    return tail.starts_with('(') || tail.starts_with('`');
                }
            }
            _ => {}
        }
    }
    false
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
                    || normalized_path(&test.file) != normalized_path(&owner.file))
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
    let Some(owner_identity) =
        index.enum_member_identity(&owner.file, owner_path, test, alias_map, workspace_root)
    else {
        return false;
    };
    index.enum_member_identity(&test.file, test_path, test, alias_map, workspace_root)
        == Some(owner_identity)
}
