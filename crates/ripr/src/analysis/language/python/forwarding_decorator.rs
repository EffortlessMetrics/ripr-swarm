//! A decorator whose wrapper forwards the call unchanged (#9619).
//!
//! `@logged def double(x): ...` calls `double(4)` through
//! `inner(*args)`, which runs a side effect and then `return fn(*args)`. The
//! decorated function's return value reaches the caller untouched, so a test
//! that asserts on `double(4)` observes an edit to the body the same way it
//! would without the decorator. Everything else a decorator can do (change
//! the arguments, swallow or retry the call, return something else, replace
//! the function, be rebound or come from another module) keeps the
//! `decorator_indirection` limit.
//!
//! The recognizer is deliberately narrow and reads only the owner's own file:
//! the decorator is a bare name defined once at module scope as
//! `def NAME(fn): [@functools.wraps(fn)] def inner(*args[, **kwargs]): <side
//! effects>; return fn(*args[, **kwargs])` followed by `return inner`.

use super::PythonOwner;
use super::module_constants::ModuleBindingFacts;
use super::source_facts::parse_module_result;
use super::static_limits::{has_identifier_boundary, is_simple_python_identifier};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::path::Path;

/// Whether `decorator` is a workspace-local forwarding wrapper defined in
/// `owner_source`. An unparsable file, an empty source or any unfamiliar shape
/// is not forwarding (fail closed).
pub(super) fn is_forwarding_decorator(
    decorator: &str,
    owner: &PythonOwner,
    owner_source: &str,
) -> bool {
    // `owner.decorators` names `@logged(arg)` as `logged`; the call form runs
    // the decorator with other arguments, so only the bare spelling counts
    // (`owner.unplain_decorators` also holds every locally defined name, so it
    // cannot tell the two apart). A
    // method's class body may bind the name too, which this module-level read
    // cannot see.
    if owner_source.is_empty()
        || !is_simple_python_identifier(decorator)
        || !owner.class_path.is_empty()
        || decorated_by_call(owner_source, decorator)
    {
        return false;
    }
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<owner>"), owner_source) else {
        return false;
    };
    let facts = ModuleBindingFacts::of(owner_source, &module.body);
    if !facts.bound_once(decorator) {
        return false;
    }
    let Some(definition) = module.body.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(function) if function.name.as_str() == decorator => Some(function),
        _ => None,
    }) else {
        return false;
    };
    let plain_functools = module.body.iter().any(|stmt| match stmt {
        Stmt::Import(import) => import
            .names
            .iter()
            .any(|alias| alias.name.as_str() == "functools" && alias.asname.is_none()),
        _ => false,
    }) && facts.bound_once("functools");
    forwards_unchanged(definition, owner_source, plain_functools)
}

/// Whether any decorator line in the file mentions `decorator` other than as
/// the whole bare expression `@name` (so `@name(...)`, `@(name)(...)`,
/// `@name.attr` and `@wrap(name)` all count). File-wide and lexical on
/// purpose: it is fail-closed and costs only credit.
fn decorated_by_call(source: &str, decorator: &str) -> bool {
    source.lines().any(|line| {
        let Some(expression) = line.trim_start().strip_prefix('@') else {
            return false;
        };
        let expression = expression.split('#').next().unwrap_or("").trim();
        expression != decorator && mentions(expression, decorator)
    })
}

fn forwards_unchanged(
    definition: &ast::StmtFunctionDef,
    source: &str,
    plain_functools: bool,
) -> bool {
    let args = &definition.args;
    let [parameter] = args.args.as_slice() else {
        return false;
    };
    if !definition.decorator_list.is_empty()
        || !args.posonlyargs.is_empty()
        || !args.kwonlyargs.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || parameter.default.is_some()
    {
        return false;
    }
    let function = parameter.def.arg.as_str();
    if function.is_empty() {
        return false;
    }
    let mut body: Vec<&Stmt> = definition.body.iter().collect();
    if matches!(
        body.first(),
        Some(Stmt::Expr(expr)) if matches!(expr.value.as_ref(), Expr::Constant(_))
    ) {
        body.remove(0);
    }
    let [Stmt::FunctionDef(inner), Stmt::Return(returned)] = body.as_slice() else {
        return false;
    };
    let returns_inner = matches!(
        returned.value.as_deref(),
        Some(Expr::Name(name)) if name.id.as_str() == inner.name.as_str()
    );
    returns_inner && inner_forwards(inner, function, source, plain_functools)
}

fn inner_forwards(
    inner: &ast::StmtFunctionDef,
    function: &str,
    source: &str,
    plain_functools: bool,
) -> bool {
    let args = &inner.args;
    if !args.posonlyargs.is_empty()
        || !args.args.is_empty()
        || !args.kwonlyargs.is_empty()
        || inner.returns.is_some()
    {
        return false;
    }
    let Some(vararg) = args.vararg.as_ref().map(|arg| arg.arg.as_str()) else {
        return false;
    };
    let kwarg = args.kwarg.as_ref().map(|arg| arg.arg.as_str());
    // A wrapper name or parameter spelled like the wrapped function shadows it,
    // so `fn(*args)` would not reach the decorated owner.
    if inner.name.as_str() == function || vararg == function || kwarg == Some(function) {
        return false;
    }
    // `functools.wraps(fn)` only copies metadata; any other decorator may
    // replace the call.
    let wraps_ok = match inner.decorator_list.as_slice() {
        [] => true,
        [decorator] => plain_functools && is_wraps_of(decorator, function),
        _ => false,
    };
    if !wraps_ok {
        return false;
    }
    let Some((last, effects)) = inner.body.split_last() else {
        return false;
    };
    if !is_forwarding_return(last, function, vararg, kwarg) {
        return false;
    }
    effects.iter().all(|stmt| {
        let Stmt::Expr(expr) = stmt else {
            return false;
        };
        if !matches!(expr.value.as_ref(), Expr::Call(_)) {
            return false;
        }
        let text = source
            .get(usize::from(stmt.range().start())..usize::from(stmt.range().end()))
            .unwrap_or("");
        // A side effect may name the wrapped function (`fn.__name__`) but
        // never call it, and never touches the forwarded arguments.
        !text.is_empty()
            && !mentions(text, vararg)
            && kwarg.is_none_or(|name| !mentions(text, name))
            && !mentions(text, "yield")
            && !mentions(text, "await")
            && !mentions_other_than_safe_attribute(text, function)
    })
}

fn is_wraps_of(decorator: &Expr, function: &str) -> bool {
    let Expr::Call(call) = decorator else {
        return false;
    };
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return false;
    };
    let receiver_is_functools =
        matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == "functools");
    receiver_is_functools
        && attribute.attr.as_str() == "wraps"
        && call.keywords.is_empty()
        && matches!(
            call.args.as_slice(),
            [Expr::Name(name)] if name.id.as_str() == function
        )
}

/// `return fn(*args)` / `return fn(*args, **kwargs)` and nothing else.
fn is_forwarding_return(stmt: &Stmt, function: &str, vararg: &str, kwarg: Option<&str>) -> bool {
    let Stmt::Return(returned) = stmt else {
        return false;
    };
    let Some(Expr::Call(call)) = returned.value.as_deref() else {
        return false;
    };
    let calls_function =
        matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == function);
    let spreads_args = matches!(
        call.args.as_slice(),
        [Expr::Starred(starred)]
            if matches!(starred.value.as_ref(), Expr::Name(name) if name.id.as_str() == vararg)
    );
    let spreads_kwargs = match (kwarg, call.keywords.as_slice()) {
        (None, []) => true,
        (Some(expected), [keyword]) => {
            keyword.arg.is_none()
                && matches!(&keyword.value, Expr::Name(name) if name.id.as_str() == expected)
        }
        _ => false,
    };
    calls_function && spreads_args && spreads_kwargs
}

fn mentions(text: &str, identifier: &str) -> bool {
    text.match_indices(identifier)
        .any(|(idx, _)| has_identifier_boundary(text, idx, identifier.len()))
}

/// Whether `identifier` appears other than as the receiver of a plain read of
/// a metadata attribute. `fn.__call__(0)`, `fn.cache_clear()` and chained
/// access run or reset the wrapped function.
fn mentions_other_than_safe_attribute(text: &str, identifier: &str) -> bool {
    const SAFE: [&str; 4] = [".__name__", ".__qualname__", ".__module__", ".__doc__"];
    text.match_indices(identifier).any(|(idx, _)| {
        if !has_identifier_boundary(text, idx, identifier.len()) {
            return false;
        }
        let rest = &text[idx + identifier.len()..];
        !SAFE.iter().any(|attribute| {
            rest.strip_prefix(attribute).is_some_and(|after| {
                !after.starts_with(|c: char| c.is_alphanumeric() || c == '_')
                    && !matches!(after.trim_start().chars().next(), Some('(' | '.'))
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGGED: &str = "import functools\n\nLOG = []\n\n\ndef logged(fn):\n    @functools.wraps(fn)\n    def inner(*args):\n        LOG.append(fn.__name__)\n        return fn(*args)\n\n    return inner\n";

    fn owner() -> PythonOwner {
        super::super::static_limits::tests::owner_with(&["logged"], Vec::new())
    }

    fn forwarding(source: &str) -> bool {
        is_forwarding_decorator("logged", &owner(), source)
    }

    #[test]
    fn logging_wrapper_that_returns_the_call_forwards() {
        assert!(forwarding(LOGGED));
    }

    #[test]
    fn kwargs_forwarding_wrapper_forwards() {
        let source = LOGGED.replace("(*args)", "(*args, **kwargs)");
        assert!(forwarding(&source));
    }

    #[test]
    fn wrapper_without_functools_wraps_forwards() {
        let source = LOGGED.replace("    @functools.wraps(fn)\n", "");
        assert!(forwarding(&source));
    }

    #[test]
    fn transforming_wrapper_does_not_forward() {
        let source = LOGGED.replace("return fn(*args)", "return fn(*args) + 1");
        assert!(!forwarding(&source));
    }

    #[test]
    fn argument_rewriting_wrapper_does_not_forward() {
        let source = LOGGED.replace("return fn(*args)", "return fn(*args[::-1])");
        assert!(!forwarding(&source));
    }

    #[test]
    fn swallowing_wrapper_does_not_forward() {
        let source = "import functools\n\n\ndef logged(fn):\n    @functools.wraps(fn)\n    def inner(*args):\n        try:\n            return fn(*args)\n        except ValueError:\n            return None\n\n    return inner\n";
        assert!(!forwarding(source));
    }

    #[test]
    fn side_effect_that_calls_the_function_does_not_forward() {
        let source = LOGGED.replace("LOG.append(fn.__name__)", "LOG.append(fn(0))");
        assert!(!forwarding(&source));
    }

    #[test]
    fn side_effect_that_touches_the_arguments_does_not_forward() {
        let source = LOGGED.replace("LOG.append(fn.__name__)", "LOG.append(args)");
        assert!(!forwarding(&source));
    }

    #[test]
    fn extra_decorator_on_the_wrapper_does_not_forward() {
        let source = LOGGED.replace("    @functools.wraps(fn)\n", "    @other(fn)\n");
        assert!(!forwarding(&source));
    }

    #[test]
    fn wrapper_returning_something_else_does_not_forward() {
        let source = LOGGED.replace("    return inner\n", "    return fn\n");
        assert!(!forwarding(&source));
    }

    #[test]
    fn rebound_decorator_does_not_forward() {
        let source = format!("{LOGGED}\nlogged = other\n");
        assert!(!forwarding(&source));
    }

    #[test]
    fn decorator_defined_elsewhere_does_not_forward() {
        let source = "from helpers import logged\n";
        assert!(!forwarding(source));
        assert!(!is_forwarding_decorator("logged", &owner(), ""));
    }

    #[test]
    fn dotted_or_called_decorators_do_not_forward() {
        assert!(!is_forwarding_decorator("mod.logged", &owner(), LOGGED));
        assert!(!is_forwarding_decorator("logged(3)", &owner(), LOGGED));
    }

    #[test]
    fn unparsable_source_does_not_forward() {
        assert!(!forwarding("def logged(fn:\n"));
    }

    #[test]
    fn aliased_functools_wraps_does_not_forward() {
        let source = LOGGED.replace("import functools", "import functools as ft");
        assert!(!forwarding(&source));
    }

    #[test]
    fn dunder_call_on_the_function_does_not_forward() {
        let source = LOGGED.replace("LOG.append(fn.__name__)", "LOG.append(fn.__call__(0))");
        assert!(!forwarding(&source));
        let source = LOGGED.replace("LOG.append(fn.__name__)", "LOG.append(fn.__name__.upper())");
        assert!(!forwarding(&source));
    }

    #[test]
    fn yield_in_a_side_effect_does_not_forward() {
        let source = LOGGED.replace("LOG.append(fn.__name__)", "LOG.append((yield))");
        assert!(!forwarding(&source));
    }

    #[test]
    fn called_decorator_spelling_does_not_forward() {
        let called = format!("{LOGGED}\n\n@logged(transform)\ndef double(x):\n    return x\n");
        assert!(!is_forwarding_decorator("logged", &owner(), &called));
        let bare = format!("{LOGGED}\n\n@logged\ndef double(x):\n    return x\n");
        assert!(is_forwarding_decorator("logged", &owner(), &bare));
    }

    #[test]
    fn method_owner_does_not_forward() {
        let mut method = owner();
        method.class_path = "C".to_string();
        assert!(!is_forwarding_decorator("logged", &method, LOGGED));
    }

    #[test]
    fn grouped_or_wrapped_decorator_calls_do_not_forward() {
        for spelling in [
            "@(logged)(1)",
            "@logged.attr",
            "@wrap(logged)",
            "@ logged (1)",
        ] {
            let source = format!("{LOGGED}\n\n{spelling}\ndef double(x):\n    return x\n");
            assert!(!forwarding(&source), "{spelling}");
        }
        let commented = format!("{LOGGED}\n\n@logged  # note\ndef double(x):\n    return x\n");
        assert!(forwarding(&commented));
    }

    #[test]
    fn wrapper_shadowing_the_wrapped_function_does_not_forward() {
        let nested = "import functools\n\n\ndef logged(fn):\n    def fn(*args):\n        return fn(*args)\n\n    return fn\n";
        assert!(!forwarding(nested));
        let star = LOGGED
            .replace("inner(*args)", "inner(*fn)")
            .replace("fn(*args)", "fn(*fn)");
        assert!(!forwarding(&star));
        let double_star = LOGGED
            .replace("inner(*args)", "inner(*args, **fn)")
            .replace("fn(*args)", "fn(*args, **fn)");
        assert!(!forwarding(&double_star));
    }
}
