//! Bounded exception-type identity for rule 23. Unknown is not unrelated.

use super::super::module_constants::{
    ModuleBindingFacts, class_scope_names, function_bound_names, visit_statements,
};
use super::super::source_facts::parse_module_result;
use super::super::source_utils::{SourceText, text_for_range};
use rustpython_parser::ast::{self, Expr, Stmt};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn catches(kind: &Expr, source: &SourceText<'_>) -> Option<bool> {
    let names = source.trusted_exception_names(trusted_names);
    match kind {
        Expr::Name(name) if names.contains(name.id.as_str()) => {
            Some(catching_name(name.id.as_str()))
        }
        Expr::Attribute(attr)
            if matches!(attr.value.as_ref(), Expr::Name(root) if root.id.as_str() == "builtins")
                && names.contains(&format!("builtins.{}", attr.attr)) =>
        {
            Some(catching_name(attr.attr.as_str()))
        }
        Expr::Tuple(tuple) => {
            // Check every element: a known class beside an unknown one does
            // not establish that evaluating/matching the tuple is bounded.
            let mut catches = false;
            for element in &tuple.elts {
                if matches!(element, Expr::Tuple(_)) {
                    return None;
                }
                catches |= self::catches(element, source)?;
            }
            Some(catches)
        }
        _ => None,
    }
}

fn catching_name(name: &str) -> bool {
    matches!(name, "Exception" | "BaseException" | "AssertionError")
}

const BUILTIN_EXCEPTIONS: &[&str] = &[
    "Exception",
    "BaseException",
    "AssertionError",
    "ArithmeticError",
    "AttributeError",
    "ImportError",
    "IndexError",
    "KeyError",
    "LookupError",
    "NameError",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "RuntimeError",
    "StopIteration",
    "TypeError",
    "UnicodeError",
    "ValueError",
    "ZeroDivisionError",
];

fn trusted_names(source: &str) -> BTreeSet<String> {
    // These namespace escapes can replace even an otherwise unbound builtin.
    if [
        "__builtins__",
        "__globals__",
        "f_globals",
        "f_locals",
        "f_builtins",
    ]
    .iter()
    .any(|name| source.contains(name))
    {
        return BTreeSet::new();
    }
    let Ok(ast::Mod::Module(module)) = parse_module_result(Path::new("<handler-types>"), source)
    else {
        return BTreeSet::new();
    };
    let module_bindings = ModuleBindingFacts::of(source, &module.body);
    let mut locals = BTreeSet::new();
    let mut opaque = false;
    // Conservative whole-file custody: a binding in another function may
    // refuse this read too. Reuse the established scope/binding owner, including
    // parameters, handler aliases, imports, globals, match and walrus guards.
    visit_statements(&module.body, &mut |stmt| {
        let bound = match stmt {
            Stmt::FunctionDef(function) => function_bound_names(
                &function.args,
                &function.body,
                &text_for_range(source, function.range),
            ),
            Stmt::AsyncFunctionDef(function) => function_bound_names(
                &function.args,
                &function.body,
                &text_for_range(source, function.range),
            ),
            Stmt::ClassDef(class) => class_scope_names(&class.body),
            _ => return,
        };
        match bound {
            Some(names) => locals.extend(names),
            None => opaque = true,
        }
    });
    if opaque {
        return BTreeSet::new();
    }
    let builtins_imported = module.body.iter().any(|stmt| {
        matches!(stmt,
        Stmt::Import(import) if import.names.iter().any(|alias|
            alias.name.as_str() == "builtins" && alias.asname.is_none()))
    });
    let escaped = builtins_escapes(source);
    let builtins_stable = builtins_imported
        && module_bindings.bound_once("builtins")
        && !locals.contains("builtins")
        && !escaped;
    let mut trusted = BTreeSet::new();
    for &name in BUILTIN_EXCEPTIONS {
        if module_bindings.unbound(name) && !locals.contains(name) && !escaped {
            trusted.insert(name.to_string());
        }
        if builtins_stable && !module_bindings.may_assign_attribute(name) {
            trusted.insert(format!("builtins.{name}"));
        }
    }
    trusted
}

// Passing/aliasing the builtins module can hide a write. Only a plain import
// and direct attribute access keep custody; dynamic namespace access is
// separately rejected by ModuleBindingFacts and the namespace guard above.
fn builtins_escapes(source: &str) -> bool {
    use rustpython_parser::{Mode, Tok, lexer::lex};
    let Ok(tokens) = lex(source, Mode::Module).collect::<Result<Vec<_>, _>>() else {
        return true;
    };
    tokens.iter().enumerate().any(|(index, (token, _))| {
        matches!(token, Tok::Name { name } if matches!(name.as_str(), "__import__" | "import_module"))
            || (matches!(token, Tok::Name { name } if name == "builtins")
                && !matches!(index.checked_sub(1).and_then(|previous| tokens.get(previous)), Some((Tok::Import, _)))
                && !(matches!(tokens.get(index + 1), Some((Tok::Dot, _)))
                    && matches!(tokens.get(index + 2), Some((Tok::Name { name }, _)) if BUILTIN_EXCEPTIONS.contains(&name.as_str()))))
    }) || tokens.windows(2).any(|pair| {
        matches!(&pair[0].0, Tok::Name { name } if name == "builtins")
            && matches!(pair[1].0, Tok::As)
    })
}
