fn macro_binding_ambiguities(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
    allowed_empty: &BTreeSet<String>,
    module_resolved: &dyn Fn(usize, &str) -> bool,
    drop_in_verified: &dyn Fn(&str) -> bool,
) -> Vec<(String, MacroBindingSite)> {
    let mut ambiguous = Vec::new();
    if !(trusted.contains(&"json") && source.contains("serde_json")
        || source.contains("macro")
        || source.contains("use")
        || source.contains("no_implicit_prelude")
        || source.contains('!'))
    {
        return ambiguous;
    }
    let all = |line: usize, kind: MacroBindingKind, crate_local: bool| {
        trusted
            .iter()
            .map(|name| {
                (
                    (*name).to_string(),
                    MacroBindingSite {
                        line,
                        kind: kind.clone(),
                        scope: None,
                        crate_local,
                    },
                )
            })
            .collect::<Vec<_>>()
    };
    let Some(parse) = parse_clean_source_file(source) else {
        return all(0, MacroBindingKind::Unparsed, false);
    };
    let tree = parse.tree();
    // `binding_free_local_wrapper` used to walk every descendant for each
    // untrusted macro. On a 20k-line test file that is macros × file size
    // and hung PR evidence after the unseen-reach cache landed.
    let binding_free = BindingFreeFileIndex::collect(tree.syntax());
    // Built only when a binding site is found, which most files lack.
    let lines = std::cell::OnceCell::new();
    let line_of = |node: &SyntaxNode| {
        lines
            .get_or_init(|| LineIndex::new(source))
            .line(node.text_range().start())
    };
    let scope_of = |item: Option<SyntaxNode>| {
        item.map(|item| {
            let range = item.text_range();
            let lines = lines.get_or_init(|| LineIndex::new(source));
            (
                lines.line(range.start()),
                lines.line_for_range_end(range.end()),
            )
        })
    };
    // A workspace package, a module of this file, or an alias (`use x as ..`,
    // `extern crate x as ..`) that takes a drop-in crate's name can export a
    // different `assert_eq!` under that path. A leading `::` names an extern
    // crate, which no module can shadow; an `extern crate` alias still can.
    let drop_in_shadowed = |root: &str, external: bool| {
        let named = |name: ast::Name| name.text().trim_start_matches("r#") == root;
        packages
            .iter()
            .any(|package| package.replace('-', "_") == root)
            || parse.tree().syntax().descendants().any(|node| {
                ast::Rename::cast(node.clone())
                    .and_then(|rename| rename.name())
                    .is_some_and(named)
                    || !external
                        && ast::Module::cast(node)
                            .and_then(|module| module.name())
                            .is_some_and(named)
            })
    };
    for node in parse.tree().syntax().descendants() {
        // An extern-crate alias or module in another file can defeat this
        // import's root even when that file never mentions `json` itself.
        if trusted.contains(&"json") && json_namespace_rebound(&node) {
            ambiguous.push((
                "json".to_string(),
                MacroBindingSite {
                    line: line_of(&node),
                    kind: MacroBindingKind::Import,
                    scope: None,
                    crate_local: false,
                },
            ));
        }
        let definition = ast::MacroRules::cast(node.clone())
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()));
        if let Some(name) = definition {
            let name = name.text().to_string();
            let name = name.trim_start_matches("r#");
            let admitted_declaration = allowed_empty.contains(name)
                && ast::MacroRules::cast(node.clone()).is_some_and(|item| empty_catch_all(&item));
            if trusted.contains(&name) && !admitted_declaration {
                let line = line_of(&node);
                let scope = scope_of(textual_scope(&node));
                ambiguous.push((
                    name.to_string(),
                    MacroBindingSite {
                        line,
                        kind: MacroBindingKind::Definition,
                        scope,
                        crate_local: !is_exported_macro(&node),
                    },
                ));
            }
        }
        if let Some(body) = macro_body_bindings(
            &node,
            &|name| trusted.contains(&name),
            // The same rule as a `use` item outside a macro: a verified,
            // unshadowed drop-in import keeps the assertion's meaning (#8149).
            &|root, name, external| {
                drop_in_crates(name).contains(&root)
                    && !drop_in_shadowed(root, external)
                    && drop_in_verified(root)
            },
        ) {
            let macro_name = ast::MacroRules::cast(node.clone())
                .and_then(|item| item.name())
                .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()))
                .map(|name| name.text().to_string())
                .unwrap_or_default();
            let fragment = body
                .binds_fragment
                .filter(|_| trusted.contains(&macro_name.trim_start_matches("r#")));
            if let Some(binding) = body.any.or(fragment) {
                // The expansion lands wherever the macro is in scope, so a
                // private macro's glob, `#[macro_use]` or
                // `no_implicit_prelude` body stays in its own target root,
                // like a direct site there; an exported macro, or one whose
                // body exports another, reaches other crates.
                let crate_local = !body.exports && !is_exported_macro(&node);
                return all(
                    line_of(&node),
                    MacroBindingKind::BodyBindsAny {
                        macro_name,
                        binding,
                    },
                    crate_local,
                );
            }
            // The expansion lands wherever the macro is invoked, so no
            // textual scope bounds it, and an exported macro or a
            // `#[macro_export]` in the body reaches other crates.
            let crate_local = !body.exports && !is_exported_macro(&node);
            for name in body.names {
                ambiguous.push((
                    name,
                    MacroBindingSite {
                        line: line_of(&node),
                        kind: MacroBindingKind::MacroBody(macro_name.clone()),
                        scope: None,
                        crate_local,
                    },
                ));
            }
        }
        if let Some(attr) = ast::Attr::cast(node.clone()) {
            let words: Vec<_> = attr
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .collect();
            if words
                .iter()
                .any(|token| token.text().trim_start_matches("r#") == "no_implicit_prelude")
            {
                ambiguous.extend(all(
                    line_of(&node),
                    MacroBindingKind::NoImplicitPrelude,
                    true,
                ));
                continue;
            }
            if words
                .iter()
                .any(|token| token.text().trim_start_matches("r#") == "macro_use")
            {
                let module = attr.syntax().parent().and_then(ast::Module::cast);
                let resolved = module.as_ref().is_some_and(|module| {
                    module.item_list().is_some()
                        || module
                            .mod_token()
                            .zip(module.name())
                            .is_some_and(|(token, name)| {
                                let line = lines
                                    .get_or_init(|| LineIndex::new(source))
                                    .line(token.text_range().start());
                                module_resolved(line, &format!("mod {};", name.text()))
                            })
                });
                if !resolved {
                    let item = attr
                        .syntax()
                        .parent()
                        .map(|parent| item_head(&parent))
                        .unwrap_or_default();
                    let line = attr
                        .syntax()
                        .parent()
                        .map_or_else(|| line_of(&node), |parent| item_line(&parent, source));
                    // `#[macro_use] extern crate` only imports into this crate. An
                    // unresolved `#[macro_use] mod` may `#[macro_export]` its macros,
                    // which other crates reach through a glob, so it stays
                    // workspace-wide.
                    let extern_crate = attr
                        .syntax()
                        .parent()
                        .is_some_and(|parent| ast::ExternCrate::can_cast(parent.kind()));
                    ambiguous.extend(all(line, MacroBindingKind::MacroUse(item), extern_crate));
                    continue;
                }
            }
        }
        if let Some(call) = ast::MacroCall::cast(node.clone())
            && let Some(path) = call.path()
            && !is_trusted_macro(&path.syntax().text().to_string(), trusted)
            && let Some(tree) = call.token_tree()
        {
            let tokens: Vec<_> = tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| !token.kind().is_trivia())
                .collect();
            // An attribute in the arguments is only tokens to the parser,
            // but the expansion may apply it to an item (`wrap! {
            // #[no_implicit_prelude] mod tests; }`).
            if let Some(attribute) = tokens.iter().find_map(|token| {
                let word = token.text().trim_start_matches("r#");
                matches!(word, "macro_use" | "no_implicit_prelude").then_some(word)
            }) {
                return all(
                    line_of(&node),
                    MacroBindingKind::ArgumentAttribute {
                        wrapper: path.syntax().text().to_string(),
                        attribute: attribute.to_string(),
                    },
                    // The expansion may export what it defines.
                    false,
                );
            }
            // Every mention counts, a plain `assert_eq!(..)` included: to the
            // macro it is only tokens, and `define!(assert_eq!(mod tests;))`
            // can emit `macro_rules! assert_eq` together with the module whose
            // tests then use it. The one exception is a wrapper whose only
            // definition this file shows and whose expansion cannot bind a
            // name at all (#6648).
            if binding_free_local_wrapper(&binding_free, &call, &tokens, trusted) {
                continue;
            }
            for token in &tokens {
                let name = token.text().trim_start_matches("r#");
                if trusted.contains(&name) {
                    let line = line_of(&node);
                    ambiguous.push((
                        name.to_string(),
                        MacroBindingSite {
                            line,
                            kind: MacroBindingKind::MacroArgument(path.syntax().text().to_string()),
                            scope: None,
                            crate_local: false,
                        },
                    ));
                }
            }
        }
        if let Some(import) = ast::Use::cast(node.clone()) {
            let Some(tree) = import.use_tree() else {
                return all(line_of(&node), MacroBindingKind::Unparsed, false);
            };
            let private = import.visibility().is_none();
            let root = tree
                .path()
                .map(|path| path.syntax().text().to_string())
                .unwrap_or_default();
            let external = root.trim_start().starts_with("::");
            let root = root
                .trim_start_matches("::")
                .split("::")
                .next()
                .unwrap_or("")
                .trim();
            let own = matches!(root, "crate" | "self" | "super")
                || packages
                    .iter()
                    .any(|package| package.replace('-', "_") == root);
            for item in tree.syntax().descendants().filter_map(ast::UseTree::cast) {
                if item.star_token().is_some() && !own {
                    let path = import.syntax().text().to_string();
                    let line = line_of(&node);
                    let Some(scope) = scope_of(import_scope(&import)) else {
                        ambiguous.extend(all(line, MacroBindingKind::ForeignGlob(path), private));
                        continue;
                    };
                    ambiguous.extend(trusted.iter().map(|name| {
                        (
                            (*name).to_string(),
                            MacroBindingSite {
                                line,
                                kind: MacroBindingKind::ForeignGlob(path.clone()),
                                scope: Some(scope),
                                crate_local: private,
                            },
                        )
                    }));
                    continue;
                }
                let name = if let Some(rename) = item.rename() {
                    rename.name().map(|name| name.text().to_string())
                } else if item.use_tree_list().is_none() {
                    item.path()
                        .and_then(|path| path.segment())
                        .and_then(|segment| segment.name_ref())
                        .map(|name| name.text().to_string())
                } else {
                    None
                };
                if let Some(name) = name {
                    let name = name.trim_start_matches("r#");
                    let kind = if !trusted.contains(&name) {
                        None
                    } else if !is_drop_in_assertion(&item, name) || drop_in_shadowed(root, external)
                    {
                        Some(MacroBindingKind::Import)
                    } else if !drop_in_verified(root) {
                        // Cargo can bind the crate name to another package.
                        Some(MacroBindingKind::UnverifiedDropIn(root.to_string()))
                    } else {
                        None
                    };
                    if let Some(kind) = kind {
                        let line = line_of(&node);
                        ambiguous.push((
                            name.to_string(),
                            MacroBindingSite {
                                line,
                                kind,
                                scope: scope_of(import_scope(&import)),
                                crate_local: private,
                            },
                        ));
                    }
                }
            }
        }
    }
    // A trusted name with a binding site is no longer a known macro, so its
    // arguments are only tokens too (#5941): `vec!(assert_eq!(mod tests;))`
    // beside `macro_rules! vec` can emit `macro_rules! assert_eq` and the
    // module whose tests then use it. The first pass skips trusted calls, so
    // run to a fixpoint: a name bound through one call's arguments makes its
    // own calls opaque in turn.
    let mut bound: BTreeSet<String> = ambiguous.iter().map(|(name, _)| name.clone()).collect();
    let mut reported = BTreeSet::new();
    // Calls spelled as tokens inside another call's tree (`dbg!(vec!(..))`)
    // are no `MacroCall` nodes: the parser keeps `vec ! (..)` flat beside a
    // nested tree. They count as calls of their bare name too (#8446).
    let mut sites = Vec::new();
    for node in parse.tree().syntax().descendants() {
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            if let (Some(path), Some(tree)) = (call.path(), call.token_tree()) {
                sites.push((
                    path.syntax().text().to_string(),
                    tree,
                    node.clone(),
                    Some(call),
                ));
            }
        } else if let Some(tree) = ast::TokenTree::cast(node.clone())
            && let Some(name) = nested_call_name(&tree)
        {
            sites.push((name, tree, node.clone(), None));
        }
    }
    loop {
        let before = bound.len();
        for (path, tree, node, call) in &sites {
            if !bound.contains(path.trim_start_matches("r#")) || !is_trusted_macro(path, trusted) {
                continue;
            }
            let tokens: Vec<_> = tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| !token.kind().is_trivia())
                .collect();
            // As for an untrusted wrapper: an attribute in the arguments may
            // be applied by the expansion to an item.
            if let Some(attribute) = tokens.iter().find_map(|token| {
                let word = token.text().trim_start_matches("r#");
                matches!(word, "macro_use" | "no_implicit_prelude").then_some(word)
            }) {
                return all(
                    line_of(node),
                    MacroBindingKind::ArgumentAttribute {
                        wrapper: path.clone(),
                        attribute: attribute.to_string(),
                    },
                    false,
                );
            }
            // As for an untrusted wrapper: a macro whose only definition
            // shows an expansion that cannot bind a name stays transparent.
            // A nested call has no node to locate that definition from, so
            // it never takes the exemption.
            if call.as_ref().is_some_and(|call| {
                binding_free_local_wrapper(&binding_free, call, &tokens, trusted)
            }) {
                continue;
            }
            for token in &tokens {
                let name = token.text().trim_start_matches("r#");
                if trusted.contains(&name) {
                    bound.insert(name.to_string());
                    let line = line_of(node);
                    if !reported.insert((name.to_string(), line, path.clone())) {
                        continue;
                    }
                    ambiguous.push((
                        name.to_string(),
                        MacroBindingSite {
                            line,
                            kind: MacroBindingKind::MacroArgument(path.clone()),
                            scope: None,
                            crate_local: false,
                        },
                    ));
                }
            }
        }
        if bound.len() == before {
            break;
        }
    }
    ambiguous
}

/// The bare name of a macro call written as `name ! (..)` tokens directly
/// before a nested token tree, if any. A top-level call is a `MacroCall`
/// node whose `!` follows a path node, so it never matches.
fn nested_call_name(tree: &ast::TokenTree) -> Option<String> {
    let mut previous = std::iter::successors(tree.syntax().prev_sibling_or_token(), |element| {
        element.prev_sibling_or_token()
    })
    .filter(|element| !matches!(element, ra_ap_syntax::NodeOrToken::Token(token) if token.kind().is_trivia()));
    let bang = previous.next()?.into_token()?;
    if bang.text() != "!" {
        return None;
    }
    let name = previous.next()?.into_token()?;
    (name.kind() == ra_ap_syntax::SyntaxKind::IDENT)
        .then(|| name.text().trim_start_matches("r#").to_string())
}
