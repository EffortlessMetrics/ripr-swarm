    let mut scopes = ModuleItemScopes::default();
    for function in source.syntax().descendants().filter_map(ast::Fn::cast) {
        let (Some(name), Some(fn_token)) = (function.name(), function.fn_token()) else {
            continue;
        };
        let key = (
            line_index.line(fn_token.text_range().start()),
            name.text().to_string(),
        );
        let body = function.body();
        let body_range = body.as_ref().map(|body| body.syntax().text_range());
        let mut has_cfg = false;
        let mut bound = Vec::new();
        let mut local_use = false;
        let mut deferred = body.is_some() && function.async_token().is_some();
        let mut called = Vec::new();
        // Keep descendant (not just immediate-child) semantics: nested
        // attributes and bindings still belong to the enclosing summary.
        // Body-only facts exclude signature nodes. Direct calls retain the
        // existing nested-fn/closure/async boundary check below.
        for node in function.syntax().descendants() {
            if !has_cfg && let Some(attribute) = ast::Attr::cast(node.clone()) {
                let compact: String = attribute
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect();
                has_cfg = compact.starts_with("#[cfg") || compact.starts_with("#![cfg");
            }
            if &node != function.syntax()
                && let Some(name) = ast::AnyHasName::cast(node.clone()).and_then(|named| named.name())
            {
                bound.push(name.text().to_string());
            }
            if body_range.is_none_or(|range| !range.contains_range(node.text_range())) {
                continue;
            }
            local_use |= ast::Use::can_cast(node.kind());
            if !deferred {
                deferred = ast::ClosureExpr::can_cast(node.kind())
                    || ast::BlockExpr::cast(node.clone())
                        .is_some_and(|block| block.async_token().is_some());
            }
            if let (Some(body), Some(call)) = (&body, ast::CallExpr::cast(node))
                && !enclosed_by_nested_fn_like(call.syntax(), body)
                && let Some(ast::Expr::PathExpr(path)) = call.expr()
                && let Some(path) = path.path()
                && path.qualifier().is_none()
                && let Some(name) = path.segment().and_then(|segment| segment.name_ref())
            {
                called.push(name.text().to_string());
            }
        }
        if has_cfg {
            scopes.fns_with_cfg.insert(key.clone());
        }
        scopes.bound_names.insert(key.clone(), bound.into_iter().collect());
        if body.is_some() {
            if local_use {
                scopes.fns_with_local_use.insert(key.clone());
            }
            if deferred {
                scopes.fns_with_deferred_code.insert(key.clone());
            }
            scopes.direct_calls.insert(key.clone(), called.into_iter().collect());
        }
