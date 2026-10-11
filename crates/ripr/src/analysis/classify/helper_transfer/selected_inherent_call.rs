//! Declaration-bound call identity only, in the canonical helper-transfer owner.
//! No argument, activation, return, field, collection or oracle transfer occurs.

use crate::analysis::facts::{FunctionSummary, RustIndex};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::SymbolId;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasGenericParams, HasName},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct DeclarationSpan {
    pub(in crate::analysis) file: PathBuf,
    pub(in crate::analysis) start: usize,
    pub(in crate::analysis) end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct SelectedInherentCall {
    pub(in crate::analysis) test_name: String,
    pub(in crate::analysis) test_declaration: DeclarationSpan,
    pub(in crate::analysis) call_expression: DeclarationSpan,
    pub(in crate::analysis) callee: SymbolId,
    pub(in crate::analysis) receiver_declaration: DeclarationSpan,
    pub(in crate::analysis) helper: SymbolId,
    pub(in crate::analysis) helper_call_expression: DeclarationSpan,
}

/// Bind one selected test call through a plain inherent method to `helper`.
/// Multiple *other* callers of the helper do not select this test's path.
/// This is an additional identity receipt, not a replacement HelperChain.
pub(in crate::analysis) fn selected_inherent_calls(
    helper: &FunctionSummary,
    index: &RustIndex,
    workspace_complete: bool,
) -> Vec<SelectedInherentCall> {
    let resolve = || -> Option<Vec<SelectedInherentCall>> {
        if !workspace_complete
            || helper.source_role.is_evidence_role()
            || !super::callee_is_unique(&helper.name, index)
        {
            return None;
        }
        let files = index.files();
        let file = files.get(&helper.file)?;
        if file.used_lexical_fallback {
            return None;
        }
        let source = file.source.as_ref();
        let parsed = parse_clean_source_file(source)?;
        let tree = parsed.tree();
        let root = tree.syntax();
        // No import/expansion/type alias resolution is claimed. These guards
        // cover module and body scopes, including hoisted item declarations.
        if root.descendants().any(|node| {
            ast::Use::can_cast(node.kind())
                || ast::ExternCrate::can_cast(node.kind())
                || ast::TypeAlias::can_cast(node.kind())
                || ast::MacroRules::can_cast(node.kind())
                || ast::MacroDef::can_cast(node.kind())
        }) || root.descendants().filter_map(ast::Attr::cast).any(|attr| {
            !matches!(
                attr.simple_name().as_deref(),
                Some("test" | "allow" | "warn" | "deny" | "forbid" | "doc")
            )
        }) || root
            .descendants()
            .any(|node| ast::MacroCall::can_cast(node.kind()))
        {
            return None;
        }
        let helper_fn = indexed_fn(root, source, helper.start_line, &helper.name, &helper.body)?;
        if helper_fn.syntax().parent().as_ref() != Some(root)
            || !plain_sync(&helper_fn)
            || name_count(root, &helper.name) != 1
        {
            return None;
        }
        let mut receipts = Vec::new();
        for test in index.tests().iter().filter(|test| test.file == helper.file) {
            let Some(test_fn) = indexed_fn(root, source, test.start_line, &test.name, &test.body)
            else {
                continue;
            };
            if test_fn.syntax().parent().as_ref() != Some(root)
                || !plain_sync(&test_fn)
                || name_count(root, &test.name) != 1
                || !test_fn
                    .attrs()
                    .any(|attr| attr.simple_name().as_deref() == Some("test"))
            {
                continue;
            }
            let mut selected = Vec::new();
            for caller in index.functions().iter().filter(|caller| {
                caller.file == helper.file && !caller.source_role.is_evidence_role()
            }) {
                let Some(caller_fn) =
                    indexed_fn(root, source, caller.start_line, &caller.name, &caller.body)
                else {
                    continue;
                };
                if !plain_sync(&caller_fn) {
                    continue;
                }
                let Some(implementation) = caller_fn
                    .syntax()
                    .parent()
                    .and_then(|list| list.parent())
                    .and_then(ast::Impl::cast)
                else {
                    continue;
                };
                if implementation.syntax().parent().as_ref() != Some(root)
                    || implementation.trait_().is_some()
                    || implementation.generic_param_list().is_some()
                    || caller_fn
                        .param_list()
                        .is_none_or(|params| params.self_param().is_some())
                {
                    continue;
                }
                let Some(receiver_name) = implementation
                    .self_ty()
                    .map(|ty| ty.syntax().text().to_string())
                else {
                    continue;
                };
                if !super::is_identifier(&receiver_name) || name_count(root, &receiver_name) != 1 {
                    continue;
                }
                let Some(receiver) = root.children().filter_map(ast::Struct::cast).find(|item| {
                    item.name()
                        .is_some_and(|name| name.text() == receiver_name.as_str())
                }) else {
                    continue;
                };
                if receiver.generic_param_list().is_some() {
                    continue;
                }
                // Only one inherent declaration of this exact method on the
                // root type; same-name methods on other types are separate.
                let definitions = root
                    .children()
                    .filter_map(ast::Impl::cast)
                    .filter(|item| {
                        item.self_ty()
                            .is_some_and(|ty| ty.syntax().text() == receiver_name.as_str())
                    })
                    .flat_map(|item| {
                        item.syntax()
                            .descendants()
                            .filter_map(ast::Fn::cast)
                            .collect::<Vec<_>>()
                    })
                    .filter(|function| {
                        function
                            .name()
                            .is_some_and(|name| name.text() == caller.name.as_str())
                    })
                    .count();
                if definitions != 1 {
                    continue;
                }
                let Some(helper_call) =
                    sole_top_level_initializer_call(&caller_fn, &helper.name, root)
                else {
                    continue;
                };
                let qualified = format!("{receiver_name}::{}", caller.name);
                let Some(call) = sole_top_level_initializer_call(&test_fn, &qualified, root) else {
                    continue;
                };
                selected.push(SelectedInherentCall {
                    test_name: test.name.clone(),
                    test_declaration: span(&helper.file, test_fn.syntax()),
                    call_expression: span(&helper.file, call.syntax()),
                    callee: caller.id.clone(),
                    receiver_declaration: span(&helper.file, receiver.syntax()),
                    helper: helper.id.clone(),
                    helper_call_expression: span(&helper.file, helper_call.syntax()),
                });
            }
            // A receipt must select one path for this test, never conflate
            // multiple qualified caller paths just because the helper agrees.
            if selected.len() == 1 {
                receipts.extend(selected);
            }
        }
        Some(receipts)
    };
    resolve().unwrap_or_default()
}

fn span(file: &std::path::Path, node: &SyntaxNode) -> DeclarationSpan {
    DeclarationSpan {
        file: file.to_path_buf(),
        start: usize::from(node.text_range().start()),
        end: usize::from(node.text_range().end()),
    }
}

fn name_count(root: &SyntaxNode, name: &str) -> usize {
    root.descendants()
        .filter_map(ast::AnyHasName::cast)
        .filter_map(|item| item.name())
        .filter(|item| item.text().trim_start_matches("r#") == name.trim_start_matches("r#"))
        .count()
}

fn plain_sync(function: &ast::Fn) -> bool {
    function.async_token().is_none()
        && function.generic_param_list().is_none()
        && function.body().is_some()
        && function.syntax().descendants().all(|node| {
            !ast::ClosureExpr::can_cast(node.kind())
                && ast::BlockExpr::cast(node).is_none_or(|block| block.async_token().is_none())
        })
}

fn indexed_fn(
    root: &SyntaxNode,
    source: &str,
    line: usize,
    name: &str,
    body: &str,
) -> Option<ast::Fn> {
    let mut candidates = root
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            let Some(token) = function.fn_token() else {
                return false;
            };
            let start = usize::from(token.text_range().start());
            let end = usize::from(function.syntax().text_range().end());
            function.name().is_some_and(|item| item.text() == name)
                && source[..start]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1
                    == line
                && source.get(start..end) == Some(body)
        });
    let candidate = candidates.next()?;
    candidates.next().is_none().then_some(candidate)
}

fn sole_top_level_initializer_call(
    function: &ast::Fn,
    callee: &str,
    root: &SyntaxNode,
) -> Option<ast::CallExpr> {
    let body = function.body()?;
    // Type-qualified paths need a separate resolver; do not hide a competing
    // reference behind an unsupported `<Type>::method` spelling.
    if body
        .syntax()
        .descendants()
        .any(|node| ast::TypeAnchor::can_cast(node.kind()))
    {
        return None;
    }
    let expected: Vec<_> = callee.split("::").collect();
    let mut selected = None;
    for expression in body.syntax().descendants().filter_map(ast::PathExpr::cast) {
        let names = path_names(expression.path()?)?;
        let local = if matches!(names.first().map(String::as_str), Some("self" | "crate")) {
            &names[1..]
        } else {
            &names[..]
        };
        if local
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
        {
            if selected.replace(expression).is_some() {
                return None;
            }
        } else if names.len() > 1
            && (names.last().map(String::as_str) == expected.last().copied()
                || matches!(
                    names.first().map(String::as_str),
                    Some("super" | "self" | "crate")
                ))
            && !(expected.len() == 2
                && names.len() == 2
                && plain_other_root_struct(root, &names[0], expected[0]))
        {
            // A parent re-export can make `super::helper` the same function.
            // Unknown qualifications must not disappear from uniqueness;
            // only a declared distinct nominal root receiver is noncompeting.
            return None;
        }
    }
    // Count equivalent root spellings and function-value escapes before
    // admitting a supported direct call. Trivia is never identity authority.
    let reference = selected?;
    let path = reference.path()?;
    if path.syntax().descendants().any(|node| {
        ast::GenericArgList::can_cast(node.kind())
            || ast::NameRef::cast(node).is_some_and(|name| name.text().starts_with("r#"))
    }) || path_names(path.clone())?
        .iter()
        .map(String::as_str)
        .ne(expected.iter().copied())
        || path
            .syntax()
            .descendants()
            .filter_map(ast::Path::cast)
            .any(|part| {
                part.qualifier().is_none()
                    && (part.coloncolon_token().is_some()
                        || part
                            .segment()
                            .is_some_and(|segment| segment.coloncolon_token().is_some()))
            })
    {
        return None;
    }
    let call = reference.syntax().parent().and_then(ast::CallExpr::cast)?;
    if !call
        .expr()
        .is_some_and(|expr| expr.syntax() == reference.syntax())
    {
        return None;
    }
    // `?` evaluates this exact call before testing its result. Retain the call
    // identity only; successful Result payload transport is still unknown.
    let initializer = call
        .syntax()
        .parent()
        .and_then(ast::TryExpr::cast)
        .map_or_else(
            || call.syntax().clone(),
            |expression| expression.syntax().clone(),
        );
    let declaration = initializer.parent().and_then(ast::LetStmt::cast)?;
    // A discarded result supplies no selected caller-local binding. Further
    // observation/survival of a named binding remains unestablished here.
    let binding = declaration
        .pat()
        .and_then(|pat| ast::IdentPat::cast(pat.syntax().clone()))?;
    binding.name()?;
    (declaration
        .initializer()
        .is_some_and(|expr| expr.syntax() == &initializer)
        && declaration.syntax().parent() == body.stmt_list().map(|list| list.syntax().clone()))
    .then_some(call)
}

fn plain_other_root_struct(root: &SyntaxNode, name: &str, selected: &str) -> bool {
    name != selected
        && name_count(root, name) == 1
        && root.children().filter_map(ast::Struct::cast).any(|item| {
            item.generic_param_list().is_none()
                && item
                    .name()
                    .is_some_and(|declaration| declaration.text() == name)
        })
}

fn path_names(path: ast::Path) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut remaining = Some(path);
    while let Some(part) = remaining {
        let name = part.segment()?.name_ref()?.text().to_string();
        names.push(name.strip_prefix("r#").unwrap_or(&name).to_owned());
        remaining = part.qualifier();
    }
    names.reverse();
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::OwnedRustIndex;
    use std::path::Path;

    const SOURCE: &str = "struct Config { errors: Vec<String> }\nfn collect(flag: bool) -> Vec<String> { Vec::new() }\nimpl Config { fn parse(flag: bool) -> Self { let errors = collect(flag); Self { errors } } }\nfn sibling() { let ignored = collect(false); }\n#[test]\nfn direct_test() { let ignored = collect(false); }\n#[test]\nfn observes() { let malformed = Config::parse(true); if malformed.errors.len() != 0 { std::process::abort(); } }\n";

    fn index(source: &str) -> Result<RustIndex, String> {
        let facts = crate::analysis::syntax::ra::summarize_file_with_parser(
            Path::new("src/lib.rs"),
            source,
        )?;
        Ok(RustIndex::from_owned(OwnedRustIndex {
            functions: facts.functions.clone(),
            tests: facts.tests.clone(),
            files: [(PathBuf::from("src/lib.rs"), facts)].into_iter().collect(),
            ..Default::default()
        }))
    }

    fn receipts(source: &str) -> Result<Vec<SelectedInherentCall>, String> {
        let index = index(source)?;
        let helper = index
            .functions()
            .iter()
            .find(|function| function.name == "collect")
            .ok_or("missing helper")?;
        Ok(selected_inherent_calls(helper, &index, true))
    }

    #[test]
    fn binds_exact_test_call_and_receiver_despite_multiple_helper_callers() -> Result<(), String> {
        let index = index(SOURCE)?;
        let helper = index
            .functions()
            .iter()
            .find(|function| function.name == "collect")
            .ok_or("missing helper")?;
        let chain = super::super::resolve_chain("collect", &index, true, &[]);
        assert!(
            chain.hops.is_empty(),
            "singleton resolver must still refuse multiple callers"
        );
        let proofs = selected_inherent_calls(helper, &index, true);
        assert_eq!(proofs.len(), 1);
        let proof = &proofs[0];
        assert_eq!(proof.test_name, "observes");
        assert_eq!(proof.callee.0, "src/lib.rs::impl Config::parse");
        assert_eq!(proof.helper.0, "src/lib.rs::collect");
        assert_eq!(
            &SOURCE[proof.call_expression.start..proof.call_expression.end],
            "Config::parse(true)"
        );
        assert_eq!(
            &SOURCE[proof.receiver_declaration.start..proof.receiver_declaration.end],
            "struct Config { errors: Vec<String> }"
        );
        assert!(
            SOURCE[proof.test_declaration.start..proof.test_declaration.end]
                .starts_with("#[test]\nfn observes()")
        );
        assert_eq!(
            &SOURCE[proof.helper_call_expression.start..proof.helper_call_expression.end],
            "collect(flag)"
        );
        assert!(selected_inherent_calls(helper, &index, false).is_empty());
        Ok(())
    }

    #[test]
    fn refuses_trivia_equivalent_duplicate_selected_calls() -> Result<(), String> {
        for duplicate in [
            "Config :: parse(false)",
            "Config /* receiver */ :: /* method */ parse(false)",
            "Config\n::parse(false)",
            "Config::r#parse(false)",
            "r#Config::parse(false)",
            "self::Config::parse(false)",
            "crate::Config::parse(false)",
            "(Config::parse)(false)",
            "Config::parse",
            "<Config>::parse(false)",
        ] {
            let source = SOURCE.replace(
                "let malformed = Config::parse(true);",
                &format!("let malformed = Config::parse(true); let again = {duplicate};"),
            );
            assert!(receipts(&source)?.is_empty(), "duplicate {duplicate}");
        }
        Ok(())
    }

    #[test]
    fn refuses_equivalent_duplicate_helper_references() -> Result<(), String> {
        for duplicate in [
            "r#collect(false)",
            "self::collect(false)",
            "crate::collect(false)",
            "(collect)(false)",
            "collect",
        ] {
            let source = SOURCE.replace(
                "let errors = collect(flag);",
                &format!("let errors = collect(flag); let again = {duplicate};"),
            );
            assert!(receipts(&source)?.is_empty(), "duplicate {duplicate}");
        }
        Ok(())
    }

    #[test]
    fn refuses_parent_reexport_competing_helper_reference() -> Result<(), String> {
        for (export, reference) in [
            ("collect", "super::collect(false)"),
            ("collect as alias", "super::alias(false)"),
        ] {
            let parent = format!("mod child; pub(crate) use child::{export};");
            let child = SOURCE
                .replace("fn collect", "pub(crate) fn collect")
                .replace(
                    "let errors = collect(flag);",
                    &format!("let other = {reference}; let errors = collect(flag);"),
                );
            let mut facts = Vec::new();
            for (path, source) in [
                ("src/lib.rs", parent.as_str()),
                ("src/child.rs", child.as_str()),
            ] {
                facts.push((
                    PathBuf::from(path),
                    crate::analysis::syntax::ra::summarize_file_with_parser(
                        Path::new(path),
                        source,
                    )?,
                ));
            }
            let index = RustIndex::from_owned(OwnedRustIndex {
                functions: facts
                    .iter()
                    .flat_map(|(_, file)| file.functions.clone())
                    .collect(),
                tests: facts
                    .iter()
                    .flat_map(|(_, file)| file.tests.clone())
                    .collect(),
                files: facts.into_iter().collect(),
                ..Default::default()
            });
            let helper = index
                .functions()
                .iter()
                .find(|function| function.name == "collect")
                .ok_or("missing helper")?;
            assert!(selected_inherent_calls(helper, &index, true).is_empty());
        }
        Ok(())
    }

    #[test]
    fn binds_unique_calls_with_path_trivia() -> Result<(), String> {
        for spelling in [
            "Config :: parse(true)",
            "Config /* receiver */ :: /* method */ parse(true)",
            "Config\n::parse(true)",
        ] {
            let source = SOURCE.replace("Config::parse(true)", spelling);
            assert_eq!(receipts(&source)?.len(), 1, "unique {spelling}");
        }
        Ok(())
    }

    #[test]
    fn refuses_unresolved_competing_qualifications_but_retains_distinct_root_receiver()
    -> Result<(), String> {
        for duplicate in [
            "super::Config::parse(false)",
            "foreign::parse(false)",
            "crate::parent::Config::parse(false)",
        ] {
            let source = SOURCE.replace(
                "let malformed = Config::parse(true);",
                &format!("let other = {duplicate}; let malformed = Config::parse(true);"),
            );
            assert!(receipts(&source)?.is_empty(), "unresolved {duplicate}");
        }
        let source = format!(
            "struct Other; impl Other {{ fn parse(flag: bool) -> Self {{ Self }} }}\n{}",
            SOURCE.replace(
                "let malformed = Config::parse(true);",
                "let other = Other::parse(false); let malformed = Config::parse(true);"
            )
        );
        assert_eq!(
            receipts(&source)?.len(),
            1,
            "distinct nominal receiver must remain noncompeting"
        );
        Ok(())
    }

    #[test]
    fn refuses_unsupported_unique_path_spellings() -> Result<(), String> {
        for spelling in [
            "Config::r#parse(true)",
            "self::Config::parse(true)",
            "crate::Config::parse(true)",
            "::Config::parse(true)",
            "Config::parse::<bool>(true)",
            "(Config::parse)(true)",
            "<Config>::parse(true)",
        ] {
            let source = SOURCE.replace("Config::parse(true)", spelling);
            assert!(receipts(&source)?.is_empty(), "unsupported {spelling}");
        }
        Ok(())
    }

    #[test]
    fn refuses_ambiguous_or_unresolved_declaration_and_call_shapes() -> Result<(), String> {
        for (label, source) in [
            (
                "wrong qualification",
                SOURCE.replace("Config::parse(true)", "Other::parse(true)"),
            ),
            (
                "alias",
                format!(
                    "type Other = Config;\n{}",
                    SOURCE.replace("Config::parse(true)", "Other::parse(true)")
                ),
            ),
            (
                "competing alias call",
                format!(
                    "type Alias = Config;\n{}",
                    SOURCE.replace(
                        "let malformed",
                        "let again = Alias::parse(false); let malformed"
                    )
                ),
            ),
            (
                "shadowed type",
                SOURCE.replace("let malformed", "struct Config; let malformed"),
            ),
            (
                "local import",
                SOURCE.replace("let malformed", "use foreign::Config; let malformed"),
            ),
            ("glob", format!("use foreign::*;\n{SOURCE}")),
            ("super glob", format!("use super::*;\n{SOURCE}")),
            (
                "async function",
                SOURCE.replace("fn parse", "async fn parse"),
            ),
            (
                "async block",
                SOURCE.replace(
                    "let malformed = Config::parse(true);",
                    "let future = async { Config::parse(true) };",
                ),
            ),
            (
                "closure",
                SOURCE.replace("Config::parse(true)", "(|| Config::parse(true))()"),
            ),
            (
                "multiple calls",
                SOURCE.replace(
                    "let malformed",
                    "let other = Config::parse(false); let malformed",
                ),
            ),
            (
                "nested call",
                SOURCE.replace("Config::parse(true)", "other(Config::parse(true))"),
            ),
            (
                "trait impl",
                SOURCE.replace("impl Config", "impl Trait for Config"),
            ),
            (
                "generic impl",
                SOURCE.replace("impl Config", "impl<T> Config"),
            ),
            (
                "generic method",
                SOURCE.replace("fn parse(", "fn parse<T>("),
            ),
            (
                "generic type",
                SOURCE.replace("struct Config {", "struct Config<T> {"),
            ),
            (
                "cfg method",
                SOURCE.replace("fn parse", "#[cfg(feature = \"x\")] fn parse"),
            ),
            (
                "proc attr",
                SOURCE.replace("struct Config", "#[rewrite] struct Config"),
            ),
            ("item macro", format!("define_config!();\n{SOURCE}")),
            (
                "statement macro",
                SOURCE.replace("let malformed", "assert!(true); let malformed"),
            ),
            ("extern import", format!("extern crate external;\n{SOURCE}")),
            (
                "type namespace competitor",
                format!("mod nested {{ enum Config {{ A }} }}\n{SOURCE}"),
            ),
            (
                "evidence helper",
                SOURCE.replace("fn collect", "#[test]\nfn collect"),
            ),
            (
                "discard binding",
                SOURCE.replace(
                    "let malformed = Config::parse(true);",
                    "let _ = Config::parse(true);",
                ),
            ),
            (
                "multiple helper calls",
                SOURCE.replace(
                    "let errors = collect(flag);",
                    "let before = collect(false); let errors = collect(flag);",
                ),
            ),
            (
                "shadowed helper",
                SOURCE.replace("let errors", "let collect = |_| Vec::new(); let errors"),
            ),
            (
                "ignored qualified call",
                SOURCE.replace(
                    "let malformed = Config::parse(true);",
                    "Config::parse(true);",
                ),
            ),
        ] {
            assert!(receipts(&source)?.is_empty(), "accepted {label}");
        }
        Ok(())
    }

    #[test]
    fn retains_separate_test_identities_and_refuses_two_eligible_paths() -> Result<(), String> {
        let two_tests =
            format!("{SOURCE}\n#[test]\nfn another() {{ let other = Config::parse(false); }}\n");
        let proofs = receipts(&two_tests)?;
        assert_eq!(proofs.len(), 2);
        assert_ne!(proofs[0].test_declaration, proofs[1].test_declaration);
        assert_ne!(proofs[0].call_expression, proofs[1].call_expression);
        let two_paths = SOURCE.replace(
            "let malformed = Config::parse(true);",
            "let first = Config::parse(true); let second = Alternate::parse(true);",
        );
        let two_paths = format!(
            "struct Alternate;\nimpl Alternate {{ fn parse(flag: bool) -> Self {{ let errors = collect(flag); Self }} }}\n{two_paths}"
        );
        assert!(receipts(&two_paths)?.is_empty());
        Ok(())
    }

    #[test]
    fn question_mark_retains_only_the_exact_call_identity() -> Result<(), String> {
        let source = SOURCE
            .replace(
                "fn parse(flag: bool) -> Self",
                "fn parse(flag: bool) -> Result<Self, ()>",
            )
            .replace("Self { errors }", "Ok(Self { errors })")
            .replace("fn observes() {", "fn observes() -> Result<(), ()> {")
            .replace("Config::parse(true);", "Config::parse(true)?;")
            .replace(
                "std::process::abort(); } }",
                "std::process::abort(); } Ok(()) }",
            );
        let proofs = receipts(&source)?;
        assert_eq!(proofs.len(), 1);
        assert_eq!(
            &source[proofs[0].call_expression.start..proofs[0].call_expression.end],
            "Config::parse(true)"
        );
        Ok(())
    }
}
