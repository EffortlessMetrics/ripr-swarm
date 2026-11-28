//! A class-only `pytest.raises` can distinguish a changed raised type (#12952,
//! builtin classes #13869).
//!
//! This is a finding-specific reading, not a stronger grade for arbitrary
//! exception names. For local classes, two inert sibling classes must directly
//! extend the unshadowed builtin `Exception`, and the test must import the new
//! class and owner without aliases from the owner's module. For builtin
//! classes, the two must be disjoint in the builtin hierarchy, the context may
//! name either one, and neither name may be imported, bound or shadowed in the
//! owner or test module. Its own `raises` context must call the owner with
//! literal inputs that reach the exact edited raise.
//! Unknown bindings, class behavior, test activation or function flow grant
//! no credit. The shared repair validator is unchanged.

use super::assertion_selection::PythonAssertionFocus;
use super::message_pin::is_strong_error_variant;
use super::module_constants::{
    ModuleBindingFacts, function_bound_names, mentions_identifier, visit_exprs_in_statements,
};
use super::module_value::{
    ModuleEdit, Scope, Value, comparison, evaluate, mentions_reflection, names_may_differ,
    with_line_replaced,
};
use super::related_tests::{
    PythonRelatedCandidate, import_source_module_matches_owner, module_name_identifies_owner_for,
    owner_module_paths,
};
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_start};
use super::{PythonAssertion, PythonFramework, PythonOracleShape, PythonOwner, PythonTest};
use crate::analysis::stale_raised_type_pin::StaleRaisedTypePin;
use crate::domain::{OracleKind, OwnerKind};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::BTreeSet;
use std::path::Path;

/// A local module or package can implement a different `pytest.raises`.
/// Reuse the workspace's bounded import roots and compiled-module index;
/// source entries are checked physically even when excluded from source facts.
pub(super) fn pytest_import_unshadowed(
    root: &Path,
    import_roots: &BTreeSet<&Path>,
    extension_names: Option<&BTreeSet<String>>,
) -> bool {
    let module = Path::new("pytest.py");
    import_roots.contains(Path::new(""))
        && extension_names
            .is_some_and(|names| !super::module_value::shadowed_by_extension(names, module))
        && !super::module_value::compiled_package_shadow(root, import_roots, module)
        && import_roots.iter().all(|directory| {
            matches!(
                std::fs::symlink_metadata(root.join(directory).join(module)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            )
        })
}

pub(super) struct ExceptionVariantCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
    pub(super) old_type: String,
    pub(super) new_type: String,
    /// The context names the type raised before the edit (it fails after);
    /// otherwise it names the new type (it fails before).
    pub(super) pins_old: bool,
}

pub(super) fn exception_variant_credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    module_owner: &PythonOwner,
    candidates: &[PythonRelatedCandidate<'a>],
    imported_class_sole_site: bool,
) -> Vec<ExceptionVariantCredit<'a>> {
    credits(edit, owner, module_owner, candidates)
        .filter(|credits| !credits.is_empty())
        .or_else(|| {
            imported_class_sole_site
                .then(|| imported_class_credits(edit, owner, module_owner, candidates))?
        })
        .unwrap_or_default()
}

/// Class-only credit for a raise swapped to an imported workspace class
/// (#15356): `raise Old(args)` -> `raise New(args)` with `Old` an untouched
/// builtin and `New` bound by the owner module's one plain `from M import
/// New`. The caller has established ([`imported_raise_class_sole_site`]) that
/// `New` is a plain exception class of one unshadowed workspace module and
/// that no other production source names it. Here the owner module names it
/// only in that import and on the changed line, so before the edit nothing
/// the owner runs can raise `New`: a `with pytest.raises(New): owner(...)`
/// whose `New` is the same import and whose arguments are constants cannot
/// pass against the old raise, whatever constants it calls with. A context naming the old class is not read, because
/// `New` may derive from it.
fn imported_class_credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    module_owner: &PythonOwner,
    candidates: &[PythonRelatedCandidate<'a>],
) -> Option<Vec<ExceptionVariantCredit<'a>>> {
    if owner.owner_kind != Some(OwnerKind::Function)
        || owner.name != owner.qualified_name
        || !owner.decorators.is_empty()
    {
        return None;
    }
    let old_line = edit.old_line_text?;
    let old_type = raised_type_name(old_line)?;
    let new_type = raised_type_name(edit.line_text)?;
    let import = plain_class_import(&owner.imports, &new_type)?;
    // The text after the class name (the arguments) and the indentation are
    // unchanged, so the edit swaps the class and nothing else.
    let rest = |line: &'_ str, name: &str| {
        let (indent, statement) = line.split_at(line.len() - line.trim_start().len());
        let arguments = statement
            .strip_prefix("raise ")?
            .trim_start()
            .strip_prefix(name)?;
        Some((indent.to_string(), arguments.trim().to_string()))
    };
    if old_type == new_type
        || rest(old_line, &old_type)? != rest(edit.line_text, &new_type)?
        || edit.new_source.lines().nth(edit.line.checked_sub(1)?)? != edit.line_text
        || identifier_count(edit.new_source, &new_type) != 2
    {
        return None;
    }
    let old_source = with_line_replaced(edit.new_source, edit.line, old_line)?;
    let Mod::Module(new_module) = parse_module_result(edit.file, edit.new_source).ok()? else {
        return None;
    };
    let Mod::Module(old_module) = parse_module_result(edit.file, &old_source).ok()? else {
        return None;
    };
    let new_facts = ModuleBindingFacts::of(edit.new_source, &new_module.body);
    let old_facts = ModuleBindingFacts::of(&old_source, &old_module.body);
    let names = [old_type.as_str(), new_type.as_str(), owner.name.as_str()];
    if !module_bindings_intact(edit, owner, module_owner, &names, &new_facts, &old_facts)
        || builtin_exception_ancestors(&old_type).is_none()
        || !new_facts.unbound(&old_type)
        || !old_facts.unbound(&old_type)
        || !new_facts.bound_once(&new_type)
        || !old_facts.bound_once(&new_type)
    {
        return None;
    }
    let source = SourceText::new(edit.new_source);
    let function = new_module.body.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(function)
            if function.name.as_str() == owner.name
                && line_for_range_start(&source, function.range) == owner.start_line =>
        {
            Some(function)
        }
        _ => None,
    })?;
    let local_names = function_bound_names(&function.args, &function.body, edit.new_source)?;
    if names
        .iter()
        .any(|name| local_names.iter().any(|local| local.as_str() == *name))
        || deferred_body(&function.body)
    {
        return None;
    }
    Some(context_credits(
        candidates,
        owner,
        &old_type,
        &new_type,
        |test, expected, call| {
            (expected == new_type
                && constant_arguments(call)
                && test_bindings_intact(
                    test,
                    owner,
                    &old_type,
                    &new_type,
                    expected,
                    ClassKind::Imported(import),
                ))
            .then_some(false)
        },
    ))
}

/// Whether a call passes only constants. Any other argument (a callback, an
/// object with a hook such as `__bool__`, an iterator) can run test-side code
/// inside the owner and raise the expected class before the changed line.
fn constant_arguments(call: &ast::ExprCall) -> bool {
    call.args
        .iter()
        .chain(call.keywords.iter().map(|keyword| &keyword.value))
        .all(|argument| matches!(argument, Expr::Constant(_)))
        && call.keywords.iter().all(|keyword| keyword.arg.is_some())
}

/// The one import that binds `name` in a module, when it is a plain
/// `from M import name`.
fn plain_class_import<'a>(
    imports: &'a [super::PythonImport],
    name: &str,
) -> Option<&'a super::PythonImport> {
    let mut bindings = imports.iter().filter(|import| import.alias == name);
    let import = bindings.next()?;
    (bindings.next().is_none() && import.imported == name && !import.source_module.is_empty())
        .then_some(import)
}

/// How many times `name` is spelled as a whole identifier in `text`, strings
/// and comments included.
fn identifier_count(text: &str, name: &str) -> usize {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| *word == name)
        .count()
}

/// Whether the class a changed `raise Name(...)` constructs is a plain
/// exception class that only its defining module and the owner module name
/// (#15356). The owner module binds `Name` by one plain `from M import Name`;
/// exactly one workspace source is module `M`, `admitted` grants it the
/// unshadowed-import fact with its foreign-write view, and it defines `Name`
/// once as `class Name(Base): pass` over `Exception` or an untouched builtin
/// exception, naming it nowhere else. No other production source or
/// `conftest.py` names it. Test files may name it; the credit reads each
/// test on its own.
pub(super) fn imported_raise_class_sole_site(
    line_text: &str,
    owner_file: &Path,
    owner_imports: &[super::PythonImport],
    sources: &std::collections::HashMap<std::path::PathBuf, String>,
    admitted: impl Fn(&Path) -> Option<super::module_value::ForeignWrites>,
) -> bool {
    let Some(name) = raised_type_name(line_text) else {
        return false;
    };
    let Some(import) = plain_class_import(owner_imports, &name) else {
        return false;
    };
    if name == "BaseException" || builtin_exception_parent(&name).is_some() {
        return false;
    }
    let mut defining = sources
        .iter()
        .filter(|(file, _)| owner_module_paths(file).contains(&import.source_module));
    let (Some((file, source)), None) = (defining.next(), defining.next()) else {
        return false;
    };
    if file == owner_file
        || sources.iter().any(|(other, text)| {
            other != file
                && other != owner_file
                && (!super::source_utils::is_test_file(other)
                    || other.file_name().is_some_and(|name| name == "conftest.py"))
                && mentions_identifier(text, &name)
        })
    {
        return false;
    }
    admitted(file).is_some_and(|foreign| sole_plain_exception_class(file, source, &name, &foreign))
}

/// Whether `source` defines `name` once as `class name(Base): pass` (a
/// docstring allowed) over `Exception` or an untouched builtin exception whose
/// constructor takes any arguments, names it nowhere else, and neither the
/// module nor another workspace file can rebind it or change what it builds.
fn sole_plain_exception_class(
    file: &Path,
    source: &str,
    name: &str,
    foreign: &super::module_value::ForeignWrites,
) -> bool {
    let Ok(Mod::Module(module)) = parse_module_result(file, source) else {
        return false;
    };
    let facts = ModuleBindingFacts::of(source, &module.body);
    let base = module.body.iter().find_map(|statement| match statement {
        Stmt::ClassDef(class) if class.name.as_str() == name => match class.bases.as_slice() {
            [Expr::Name(base)]
                if class.keywords.is_empty()
                    && class.decorator_list.is_empty()
                    && class.type_params.is_empty()
                    && class.body.iter().all(|statement| {
                        matches!(statement, Stmt::Pass(_)) || docstring(statement)
                    }) =>
            {
                Some(base.id.as_str())
            }
            _ => None,
        },
        _ => None,
    });
    base.is_some_and(|base| {
        (base == "Exception" || builtin_exception_ancestors(base).is_some())
            && facts.unbound(base)
            && facts.unbound("Exception")
            && !foreign.may_rebind(base, file)
    }) && facts.bound_once(name)
        && identifier_count(source, name) == 1
        && !mentions_identifier(source, "__builtins__")
        && !mentions_reflection(source)
        && !names_may_differ(source)
        && !facts.assigns_dunder()
        && !foreign.reflection_elsewhere()
        && !foreign.names_may_differ_elsewhere()
        && !foreign.writes_dunder()
        && !foreign.may_rebind(name, file)
        && !foreign.may_rebind("Exception", file)
}

fn credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    module_owner: &PythonOwner,
    candidates: &[PythonRelatedCandidate<'a>],
) -> Option<Vec<ExceptionVariantCredit<'a>>> {
    if owner.owner_kind != Some(OwnerKind::Function)
        || owner.name != owner.qualified_name
        || !owner.decorators.is_empty()
    {
        return None;
    }
    let old_line = edit.old_line_text?;
    let (old_type, old_args) = raised_type(old_line)?;
    let (new_type, new_args) = raised_type(edit.line_text)?;
    let old_indent = &old_line[..old_line.len() - old_line.trim_start().len()];
    let new_indent = &edit.line_text[..edit.line_text.len() - edit.line_text.trim_start().len()];
    if old_type == new_type
        || old_args != new_args
        || old_indent != new_indent
        || edit.new_source.lines().nth(edit.line.checked_sub(1)?)? != edit.line_text
    {
        return None;
    }
    let old_source = with_line_replaced(edit.new_source, edit.line, old_line)?;
    let Mod::Module(new_module) = parse_module_result(edit.file, edit.new_source).ok()? else {
        return None;
    };
    let Mod::Module(old_module) = parse_module_result(edit.file, &old_source).ok()? else {
        return None;
    };
    let new_facts = ModuleBindingFacts::of(edit.new_source, &new_module.body);
    let old_facts = ModuleBindingFacts::of(&old_source, &old_module.body);
    let names = [old_type.as_str(), new_type.as_str(), owner.name.as_str()];
    if !module_bindings_intact(edit, owner, module_owner, &names, &new_facts, &old_facts) {
        return None;
    }
    // Each raised class is either a plain sibling of `Exception` defined once
    // in the module, or an untouched builtin whose hierarchy is fixed.
    let kinds = [old_type.as_str(), new_type.as_str()].map(|name| {
        let builtin = builtin_exception_ancestors(name).is_some()
            && new_facts.unbound(name)
            && old_facts.unbound(name);
        let local = new_facts.bound_once(name)
            && old_facts.bound_once(name)
            && [&new_module, &old_module]
                .iter()
                .all(|module| plain_exception(&module.body, name));
        if builtin {
            Some(ClassKind::Builtin)
        } else if local {
            Some(ClassKind::Local)
        } else {
            None
        }
    });
    let [Some(old_kind), Some(new_kind)] = kinds else {
        return None;
    };
    if !disjoint(&old_type, old_kind, &new_type, new_kind) {
        return None;
    }
    let source = SourceText::new(edit.new_source);
    let function = new_module.body.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(function)
            if function.name.as_str() == owner.name
                && line_for_range_start(&source, function.range) == owner.start_line =>
        {
            Some(function)
        }
        _ => None,
    })?;
    let local_names = function_bound_names(&function.args, &function.body, edit.new_source)?;
    if names
        .iter()
        .any(|name| local_names.iter().any(|local| local.as_str() == *name))
        || deferred_body(&function.body)
        || !plain_parameters(function)
    {
        return None;
    }
    let len_intact = new_facts.unbound("len")
        && old_facts.unbound("len")
        && !local_names.iter().any(|name| name == "len")
        && !module_owner.foreign_writes.may_rebind("len", edit.file);
    Some(context_credits(
        candidates,
        owner,
        &old_type,
        &new_type,
        |test, expected, call| {
            let (pins_old, expected_kind) = if expected == new_type {
                (false, new_kind)
            } else if expected == old_type {
                (true, old_kind)
            } else {
                return None;
            };
            if !test_bindings_intact(test, owner, &old_type, &new_type, expected, expected_kind) {
                return None;
            }
            let parameters = literal_arguments(function, call)?;
            let mut budget = 128;
            let path = RaisePath {
                source: &source,
                line: edit.line,
                parameters: &parameters,
                len_intact,
            };
            (path.walk(&function.body, 0, &mut budget) == Some(Flow::Target)).then_some(pins_old)
        },
    ))
}

/// The module-scope conditions both class-only readings need: nothing in the
/// owner module or another workspace file can rebind `names`, `Exception` or
/// the framework, reflect on the module, or change what a class builds.
fn module_bindings_intact(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    module_owner: &PythonOwner,
    names: &[&str],
    new_facts: &ModuleBindingFacts,
    old_facts: &ModuleBindingFacts,
) -> bool {
    // The caller requires the current per-file module-import admission.
    // That producer checks raw module identity; the shared owner below
    // retains the legacy configured-path flag for other module-value passes.
    let foreign = &module_owner.foreign_writes;
    new_facts.unbound("Exception")
        && old_facts.unbound("Exception")
        && !mentions_identifier(edit.new_source, "__builtins__")
        && !mentions_reflection(edit.new_source)
        && !names_may_differ(edit.new_source)
        && !new_facts.assigns_dunder()
        && !old_facts.assigns_dunder()
        && !foreign.reflection_elsewhere()
        && !foreign.names_may_differ_elsewhere()
        && !foreign.writes_dunder()
        && !foreign.may_replace_reader_code()
        && ["Exception", "pytest", "raises"]
            .iter()
            .chain(names)
            .all(|name| !foreign.may_rebind(name, edit.file))
        && new_facts.bound_once(&owner.name)
        && old_facts.bound_once(&owner.name)
}

/// One credit per related test whose first statement that is not an inert
/// prefix is `with pytest.raises(Name): owner(...)`, with no keyword and a
/// body of that one call, when `admit` accepts the test, the expected name
/// and the call. `admit` returns whether the context pins the old class.
fn context_credits<'a>(
    candidates: &[PythonRelatedCandidate<'a>],
    owner: &PythonOwner,
    old_type: &str,
    new_type: &str,
    admit: impl Fn(&PythonTest, &str, &ast::ExprCall) -> Option<bool>,
) -> Vec<ExceptionVariantCredit<'a>> {
    let mut credits = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let test = candidate.test;
        let Ok(Mod::Module(module)) = parse_module_result(&test.file, &test.body_text) else {
            continue;
        };
        let [Stmt::FunctionDef(test_function)] = module.body.as_slice() else {
            continue;
        };
        if deferred_body(&test_function.body) {
            continue;
        }
        for statement in &test_function.body {
            let Stmt::With(context) = statement else {
                // Only a demonstrated passing assertion or inert prefix can precede
                // the context. An earlier failing assertion prevents its call.
                if inert_prefix(statement) {
                    continue;
                }
                break;
            };
            let [item] = context.items.as_slice() else {
                break;
            };
            let Expr::Call(raises) = &item.context_expr else {
                break;
            };
            if !pytest_raises(raises) || !raises.keywords.is_empty() {
                break;
            }
            let [Expr::Name(expected)] = raises.args.as_slice() else {
                break;
            };
            let [Stmt::Expr(body)] = context.body.as_slice() else {
                break;
            };
            let Expr::Call(call) = body.value.as_ref() else {
                break;
            };
            if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == owner.name) {
                break;
            }
            let Some(pins_old) = admit(test, expected.id.as_str(), call) else {
                break;
            };
            let Some(assertion) = test.assertions.iter().find(|assertion| {
                assertion.source_range == Some(item.context_expr.range())
                    && assertion.oracle_shape == PythonOracleShape::ExceptionAssertion
                    && assertion.oracle_kind == OracleKind::BroadError
            }) else {
                break;
            };
            credits.push(ExceptionVariantCredit {
                test,
                assertion,
                old_type: old_type.to_string(),
                new_type: new_type.to_string(),
                pins_old,
            });
            break;
        }
    }
    credits
}

/// A raised-type replacement (#13622) whose strong related error-variant
/// assertions all still expect the OLD class exactly, while the new class is
/// neither that class nor a statically known subclass of it. Such a test fails
/// against the new raise. The fact is advisory evidence only: the exposure
/// class is unchanged, because a pre-existing old-type assertion does notice
/// the edit (RIPR-SPEC-0238 example 14). The gap-bound repair closure reads it
/// so a test written during the repair is not reported as closing the gap.
///
/// Fails open (no fact) for any shape it cannot read exactly: a raise that is
/// not one `raise Name(...)` or `raise Name` line (a dotted `mod.Exc` is never
/// read), an expected class that is dotted, a tuple with any member other than
/// the old class, a related test that names the new class anywhere, an assertion expecting another
/// class, or a class hierarchy it cannot resolve within the module and the
/// builtin exception table.
pub(super) fn stale_raised_type_pins(
    line_text: &str,
    old_line_text: Option<&str>,
    module_source: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Vec<StaleRaisedTypePin> {
    stale_pins(
        line_text,
        old_line_text,
        module_source,
        related_candidates,
        focus,
    )
    .unwrap_or_default()
}

fn stale_pins(
    line_text: &str,
    old_line_text: Option<&str>,
    module_source: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<Vec<StaleRaisedTypePin>> {
    let old_type = raised_type_name(old_line_text?)?;
    let new_type = raised_type_name(line_text)?;
    if old_type == new_type {
        return None;
    }
    let oracle_tests = related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle());
    // A related test that names the new type anywhere in its body may tell the
    // new raise apart (`excinfo.type is New`, `isinstance(exc.value, New)`, or
    // a plain `pytest.raises(New)`); those forms are not evaluated here.
    if oracle_tests
        .clone()
        .any(|candidate| mentions_identifier(&candidate.test.body_text, &new_type))
    {
        return None;
    }
    let mut pins = Vec::new();
    for candidate in oracle_tests {
        for assertion in candidate
            .test
            .assertions
            .iter()
            .filter(|assertion| is_strong_error_variant(assertion) && focus.admits(assertion))
        {
            if !expected_classes(&assertion.text)?
                .iter()
                .all(|class| *class == old_type)
            {
                return None;
            }
            // The test module may rebind the expected name
            // (`from builtins import TypeError as ValueError`), so a renamed
            // import, or any import of a builtin class name, is not read.
            // A test that rebinds the expected name (`ValueError = Exception`)
            // may accept the new class: a builtin name bound anywhere in the
            // test module, or a workspace name bound other than by one plain
            // import, is not read.
            let rebinding = &candidate.test.constant_rebinding;
            if rebinding.module_opaque || rebinding.rebinds(&old_type) {
                return None;
            }
            let builtin =
                old_type == "BaseException" || builtin_exception_parent(&old_type).is_some();
            if candidate
                .test
                .imports
                .iter()
                .any(|import| import.alias == old_type && (builtin || import.imported != old_type))
            {
                return None;
            }
            let plainly_imported = candidate
                .test
                .imports
                .iter()
                .any(|import| import.alias == old_type);
            if rebinding.module_bound.contains(&old_type) && (builtin || !plainly_imported) {
                return None;
            }
            pins.push(StaleRaisedTypePin {
                assertion: assertion
                    .text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                old_type: old_type.clone(),
                new_type: new_type.clone(),
                test: candidate.test.name.clone(),
            });
        }
    }
    if pins.is_empty() || !known_not_subclass(module_source?, &new_type, &old_type)? {
        return None;
    }
    pins.sort_by(|left, right| (&left.test, &left.assertion).cmp(&(&right.test, &right.assertion)));
    pins.dedup();
    Some(pins)
}

/// Whether an assertion expecting exactly `new` fails against the OLD raise
/// (#15356): the old class is neither `new` nor one of its subclasses, so
/// `pytest.raises(new)` passes after the edit but not before. A resolvable
/// `new` (builtin or defined once in the module) needs the old class provably
/// outside its subtree. An imported `new` cannot be read here; an untouched
/// builtin old class cannot derive from a user class, which leaves only an
/// alias of that builtin in the class's own module (`New = ValueError`) as a
/// documented residual.
pub(super) fn old_class_outside(
    source: Option<&str>,
    old: &str,
    new: &str,
    new_imported: bool,
    owner_imports: &[super::PythonImport],
) -> bool {
    if old == new {
        return false;
    }
    let Some(source) = source else {
        // No module text: only two builtins have a fixed hierarchy.
        return !new_imported
            && builtin_chain(old).is_some_and(|chain| !chain.contains(&new))
            && builtin_chain(new).is_some();
    };
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<module>"), source) else {
        return false;
    };
    let facts = ModuleBindingFacts::of(source, &module.body);
    if new_imported {
        // A builtin name imported from elsewhere (`from compat import KeyError`)
        // is not the builtin class.
        return builtin_exception_parent(new).is_none()
            && builtin_exception_parent(old).is_some()
            && owner_imports.iter().all(|import| import.alias != old)
            && facts.unbound(old);
    }
    // Both names must read as the one class they spell: an untouched builtin
    // or a class defined once in the module.
    let reads = |name: &str| facts.unbound(name) || facts.bound_once(name);
    reads(old)
        && reads(new)
        && exception_ancestry(&module.body, &facts, new, 0).is_some()
        && exception_ancestry(&module.body, &facts, old, 0)
            .is_some_and(|ancestry| !ancestry.iter().any(|name| name == new))
}

/// Whether no name in `names` is bound in the owner function's own scope (a
/// parameter, a local assignment, an `except .. as` target), so each raised
/// name reads as the module-level class it spells (#15356 review: a local
/// `ValueError = InvalidVersion` makes both raises construct one class). An
/// owner whose `def` cannot be found at module or class level, or whose scope
/// is unreadable (a nested scope or a `lambda`), refuses. With no module text
/// nothing local is known, and the caller's own hierarchy rule decides.
pub(super) fn owner_scope_leaves_unbound(
    source: Option<&str>,
    owner: &PythonOwner,
    names: &[&str],
) -> bool {
    let Some(source) = source else {
        return true;
    };
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<module>"), source) else {
        return false;
    };
    let text = SourceText::new(source);
    let is_owner = |function: &ast::StmtFunctionDef| {
        function.name.as_str() == owner.name
            && line_for_range_start(&text, function.range) == owner.start_line
    };
    let function = module.body.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(function) if is_owner(function) => Some(function),
        Stmt::ClassDef(class) => class.body.iter().find_map(|inner| match inner {
            Stmt::FunctionDef(function) if is_owner(function) => Some(function),
            _ => None,
        }),
        _ => None,
    });
    // The owner's own text: a `lambda` elsewhere in the module does not hide
    // this scope's bindings.
    function
        .and_then(|function| {
            let text = source
                .get(usize::from(function.range.start())..usize::from(function.range.end()))?;
            function_bound_names(&function.args, &function.body, text)
        })
        .is_some_and(|locals| {
            names
                .iter()
                .all(|name| !locals.iter().any(|local| local.as_str() == *name))
        })
}

/// The builtin class and its ancestors up to `BaseException`.
fn builtin_chain(name: &str) -> Option<Vec<&str>> {
    let mut chain = vec![name];
    let mut current = name;
    while current != "BaseException" {
        current = builtin_exception_parent(current)?;
        chain.push(current);
    }
    Some(chain)
}

/// `Some(true)` only when `new` is statically known not to be `old` or a
/// subclass of it; `None` when either hierarchy cannot be read exactly.
pub(super) fn known_not_subclass(source: &str, new: &str, old: &str) -> Option<bool> {
    let Mod::Module(module) = parse_module_result(Path::new("<module>"), source).ok()? else {
        return None;
    };
    let facts = ModuleBindingFacts::of(source, &module.body);
    // `old` must itself resolve, so an alias such as `IOError` (the same class
    // as `OSError`) or an imported name never reads as unrelated.
    exception_ancestry(&module.body, &facts, old, 0)?;
    // The raised name must read as the class it names everywhere in the
    // module: a function-local `TypeError = ValueError`, a parameter or an
    // import rebinds it (#13622 review).
    let class_headers = usize::from(!facts.unbound(new));
    if !only_raised_or_declared(source, new, class_headers) {
        return None;
    }
    let ancestry = exception_ancestry(&module.body, &facts, new, 0)?;
    // Constructors whose arguments choose or reject the class: `OSError(2, "m")`
    // constructs `FileNotFoundError`, and the Unicode errors, `SyntaxError` and
    // exception groups raise `TypeError` for the wrong argument shape.
    if ancestry
        .iter()
        .any(|name| CONSTRUCTOR_CHECKED_EXCEPTIONS.contains(&name.as_str()))
    {
        return None;
    }
    Some(!ancestry.iter().any(|name| name == old))
}

/// `true` only when `new` is an untouched builtin exception and `old` a class
/// the module defines once, so the builtin's fixed ancestry cannot contain it
/// (a narrowing `raise Sub(..)` -> `raise Base(..)`; #15356 review).
/// [`known_not_subclass`] refuses here because the builtin is spelled in the
/// subclass's header; a `global` statement naming it refuses, and the
/// caller checks the owner's own scope.
pub(super) fn builtin_new_outside_local_old(source: &str, new: &str, old: &str) -> bool {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<module>"), source) else {
        return false;
    };
    let facts = ModuleBindingFacts::of(source, &module.body);
    let global_names_new = source.lines().any(|line| {
        line.trim_start()
            .strip_prefix("global ")
            .is_some_and(|names| mentions_identifier(names, new))
    });
    !global_names_new
        && facts.unbound(new)
        && facts.bound_once(old)
        && exception_ancestry(&module.body, &facts, old, 0).is_some()
        && builtin_chain(new).is_some_and(|chain| {
            !chain.contains(&old)
                && !chain
                    .iter()
                    .any(|name| CONSTRUCTOR_CHECKED_EXCEPTIONS.contains(name))
        })
}

/// Builtin exceptions whose constructor maps or validates its arguments, so a
/// raise of one (or of a class derived from one) may raise another class.
const CONSTRUCTOR_CHECKED_EXCEPTIONS: [&str; 6] = [
    "OSError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeTranslateError",
    "SyntaxError",
    "BaseExceptionGroup",
];

/// Whether `name` occurs in `source` (outside string literals) only after
/// `raise` or `except`, or after `class` at most `class_headers` times. Any
/// other occurrence (an assignment, a parameter, an import, a nested class)
/// may rebind it somewhere the raise can see.
fn only_raised_or_declared(source: &str, name: &str, class_headers: usize) -> bool {
    let blanked = super::sink_alignment::blank_quoted_strings(source);
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut headers = 0;
    for (start, _) in blanked.match_indices(name) {
        let before = blanked[..start].chars().next_back();
        let after = blanked[start + name.len()..].chars().next();
        if before.is_some_and(is_word) || after.is_some_and(is_word) {
            continue;
        }
        let preceding = blanked[..start].trim_end();
        let keyword = preceding
            .rsplit(|ch: char| !is_word(ch))
            .next()
            .unwrap_or_default();
        match keyword {
            "raise" | "except" => {}
            "class" => headers += 1,
            _ => return false,
        }
    }
    headers <= class_headers
}

/// `name` and every class it derives from, resolved through single top-level
/// single-base class definitions that do not override `__new__`, and the
/// builtin exception table.
fn exception_ancestry(
    body: &[Stmt],
    facts: &ModuleBindingFacts,
    name: &str,
    depth: usize,
) -> Option<Vec<String>> {
    if depth > 32 {
        return None;
    }
    if facts.unbound(name) {
        let mut chain = vec![name.to_string()];
        let mut current = name;
        while current != "BaseException" {
            current = builtin_exception_parent(current)?;
            chain.push(current.to_string());
        }
        return Some(chain);
    }
    if !facts.bound_once(name) {
        return None;
    }
    let class = body.iter().find_map(|statement| match statement {
        Stmt::ClassDef(class) if class.name.as_str() == name => Some(class),
        _ => None,
    })?;
    // One plain base only: several bases could repeat shared ancestors at
    // every level, and a class that overrides construction (`__new__` or `__init__`) can
    // return an instance of another class (#13622 review).
    let ([Expr::Name(base)], true, true) = (
        class.bases.as_slice(),
        class.keywords.is_empty()
            && class.type_params.is_empty()
            && class.decorator_list.is_empty(),
        !overrides_construction(&class.body),
    ) else {
        return None;
    };
    let mut chain = vec![name.to_string()];
    chain.extend(exception_ancestry(
        body,
        facts,
        base.id.as_str(),
        depth + 1,
    )?);
    Some(chain)
}

/// Whether a class body may change construction. Only a plain body is read:
/// `pass`, `...`, docstrings and methods other than `__new__` and `__init__`. Any other
/// statement (an assignment, an `if` or `try` that could define `__new__`, a
/// decorator-built binding) is treated as overriding construction.
fn overrides_construction(class_body: &[Stmt]) -> bool {
    !class_body.iter().all(|statement| match statement {
        Stmt::Pass(_) => true,
        Stmt::Expr(expression) => matches!(expression.value.as_ref(), Expr::Constant(_)),
        Stmt::FunctionDef(function) => {
            !matches!(function.name.as_str(), "__new__" | "__init__")
                && function.decorator_list.is_empty()
        }
        Stmt::AsyncFunctionDef(function) => {
            !matches!(function.name.as_str(), "__new__" | "__init__")
                && function.decorator_list.is_empty()
        }
        _ => false,
    })
}

/// The single base of a builtin exception class (CPython 3 hierarchy). Names
/// with several bases (`ExceptionGroup`) and aliases (`IOError`,
/// `EnvironmentError`) are absent, so they never resolve.
pub(super) fn builtin_exception_parent(name: &str) -> Option<&'static str> {
    Some(match name {
        "SystemExit" | "KeyboardInterrupt" | "GeneratorExit" | "Exception"
        | "BaseExceptionGroup" => "BaseException",
        "ArithmeticError" | "AssertionError" | "AttributeError" | "BufferError" | "EOFError"
        | "ImportError" | "LookupError" | "MemoryError" | "NameError" | "OSError"
        | "ReferenceError" | "RuntimeError" | "StopIteration" | "StopAsyncIteration"
        | "SyntaxError" | "SystemError" | "TypeError" | "ValueError" | "Warning" => "Exception",
        "FloatingPointError" | "OverflowError" | "ZeroDivisionError" => "ArithmeticError",
        "ModuleNotFoundError" => "ImportError",
        "IndexError" | "KeyError" => "LookupError",
        "UnboundLocalError" => "NameError",
        "BlockingIOError" | "ChildProcessError" | "ConnectionError" | "FileExistsError"
        | "FileNotFoundError" | "InterruptedError" | "IsADirectoryError" | "NotADirectoryError"
        | "PermissionError" | "ProcessLookupError" | "TimeoutError" => "OSError",
        "BrokenPipeError"
        | "ConnectionAbortedError"
        | "ConnectionRefusedError"
        | "ConnectionResetError" => "ConnectionError",
        "NotImplementedError" | "RecursionError" | "PythonFinalizationError" => "RuntimeError",
        "IndentationError" => "SyntaxError",
        "TabError" => "IndentationError",
        "UnicodeError" => "ValueError",
        "UnicodeDecodeError" | "UnicodeEncodeError" | "UnicodeTranslateError" => "UnicodeError",
        "DeprecationWarning"
        | "PendingDeprecationWarning"
        | "RuntimeWarning"
        | "SyntaxWarning"
        | "UserWarning"
        | "FutureWarning"
        | "ImportWarning"
        | "UnicodeWarning"
        | "BytesWarning"
        | "ResourceWarning"
        | "EncodingWarning" => "Warning",
        _ => return None,
    })
}

/// The class names a `raises` / `assertRaises*` call expects, read exactly: the
/// first positional argument or the `expected_exception=` keyword (never both),
/// each a plain name or a non-empty tuple of plain names. A dotted name, a
/// starred or computed element, or a duplicated keyword is unreadable.
pub(super) fn expected_classes(assertion_text: &str) -> Option<Vec<String>> {
    let Ok(Mod::Module(module)) =
        parse_module_result(Path::new("<expected-class>"), assertion_text.trim())
    else {
        return None;
    };
    let [Stmt::Expr(statement)] = module.body.as_slice() else {
        return None;
    };
    let Expr::Call(call) = statement.value.as_ref() else {
        return None;
    };
    let callee = super::expr_full_name(&call.func)?;
    if !matches!(
        callee.rsplit('.').next()?,
        "raises" | "assertRaises" | "assertRaisesRegex"
    ) {
        return None;
    }
    let mut keywords = call.keywords.iter().filter(|keyword| {
        keyword
            .arg
            .as_ref()
            .is_some_and(|arg| arg.as_str() == "expected_exception")
    });
    let expected = match (call.args.first(), keywords.next()) {
        (Some(expected), None) => expected,
        (None, Some(keyword)) => &keyword.value,
        _ => return None,
    };
    if keywords.next().is_some() {
        return None;
    }
    let plain = |expression: &Expr| match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        _ => None,
    };
    match expected {
        Expr::Tuple(tuple) if !tuple.elts.is_empty() => tuple.elts.iter().map(plain).collect(),
        other => Some(vec![plain(other)?]),
    }
}

/// The imported class a sole-line raised-type replacement now constructs
/// (#15356): `raise Old(args)` -> `raise New(args)` where `New` is bound by
/// a plain `from M import New` of the owner module, and the arguments are the
/// same inert text. The call of `New` is the exception constructor the edit
/// swaps, not an imported implementation the adapter must follow, so the
/// imported-symbol abstention does not apply. Nothing here resolves `New`'s
/// hierarchy, so it grants no exposure credit.
pub(super) fn imported_raised_class_swap(
    line_text: &str,
    old_line_text: Option<&str>,
    imports: &[super::PythonImport],
) -> bool {
    let (Some(old_line), Some(new_type), true) = (
        old_line_text,
        raised_type_name(line_text),
        line_text.trim_start().starts_with("raise "),
    ) else {
        return false;
    };
    let Some(old_type) = raised_type_name(old_line) else {
        return false;
    };
    let arguments = |line: &str, name: &str| {
        line.trim()
            .strip_prefix("raise ")
            .and_then(|rest| rest.trim_start().strip_prefix(name))
            .map(|rest| rest.trim().to_string())
    };
    let (Some(old_arguments), Some(new_arguments)) = (
        arguments(old_line, &old_type),
        arguments(line_text, &new_type),
    ) else {
        return false;
    };
    // A class by PEP 8 naming; a `snake_case` factory may build anything.
    old_type != new_type
        && new_type.starts_with(|ch: char| ch.is_ascii_uppercase())
        && old_arguments == new_arguments
        && old_arguments.starts_with('(')
        // Only the class is exempt: an imported call in the arguments still
        // needs the import graph (a `from cause` never reaches this check).
        && !super::static_limits::line_uses_imported_symbol(&new_arguments, imports)
        && imports
            .iter()
            .filter(|import| import.alias == new_type)
            .count()
            == 1
        && imports.iter().any(|import| {
            import.alias == new_type
                && import.imported == new_type
                && !import.source_module.is_empty()
        })
        && imports.iter().all(|import| import.alias != old_type)
}

/// The class a one-line `raise Name(...)` or bare `raise Name` constructs. A
/// dotted `mod.Exc` is not read: its hierarchy is not resolvable here, so it
/// could alias an ancestor of the other class. The arguments, keywords
/// and cause do not change which class is raised, so a message built with an
/// f-string or a variable still names its type (#13622 review).
pub(super) fn raised_type_name(line: &str) -> Option<String> {
    let Mod::Module(module) = parse_module_result(Path::new("raise.py"), line.trim()).ok()? else {
        return None;
    };
    let [Stmt::Raise(raise)] = module.body.as_slice() else {
        return None;
    };
    // A cause (`from fail()`) is evaluated before the raise and could raise
    // another class first.
    if raise.cause.is_some() {
        return None;
    }
    let call = match raise.exc.as_deref()? {
        // A bare class is instantiated with no arguments; whether the name is
        // a class at all is settled by the hierarchy resolution.
        Expr::Name(name) => return Some(name.id.to_string()),
        Expr::Call(call) => call,
        _ => return None,
    };
    let Expr::Name(name) = call.func.as_ref() else {
        return None;
    };
    // Keywords are refused: `**value` may not be a mapping, and exception
    // constructors reject named keywords, so the call could raise first.
    let inert = call.keywords.is_empty() && call.args.iter().all(inert_raise_argument);
    inert.then(|| name.id.to_string())
}

/// An argument whose evaluation cannot itself raise before the exception is
/// built: a constant, a plain name, or an f-string formatting only those.
/// A call, operator or subscript (`f"{1 / 0}"`) could raise some other class
/// first, so the raised class would not be the constructor's (#13622 review).
fn inert_raise_argument(argument: &Expr) -> bool {
    match argument {
        Expr::Constant(_) | Expr::Name(_) => true,
        Expr::JoinedStr(joined) => joined.values.iter().all(|part| match part {
            Expr::Constant(_) => true,
            Expr::FormattedValue(formatted) => {
                formatted.format_spec.is_none()
                    && matches!(formatted.value.as_ref(), Expr::Constant(_) | Expr::Name(_))
            }
            _ => false,
        }),
        _ => false,
    }
}

/// Only the constructor's name changes; evaluating its arguments runs no code.
fn raised_type(line: &str) -> Option<(String, Vec<ast::Constant>)> {
    let Mod::Module(module) = parse_module_result(Path::new("raise.py"), line.trim()).ok()? else {
        return None;
    };
    let [Stmt::Raise(raise)] = module.body.as_slice() else {
        return None;
    };
    if raise.cause.is_some() {
        return None;
    }
    let Expr::Call(call) = raise.exc.as_deref()? else {
        return None;
    };
    let Expr::Name(name) = call.func.as_ref() else {
        return None;
    };
    if !call.keywords.is_empty() {
        return None;
    }
    let arguments = call
        .args
        .iter()
        .map(|argument| match argument {
            Expr::Constant(constant)
                if matches!(
                    constant.value,
                    ast::Constant::Str(_)
                        | ast::Constant::Int(_)
                        | ast::Constant::Bool(_)
                        | ast::Constant::None
                ) =>
            {
                Some(constant.value.clone())
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((name.id.to_string(), arguments))
}

fn plain_exception(body: &[Stmt], name: &str) -> bool {
    body.iter().any(|statement| {
        let Stmt::ClassDef(class) = statement else {
            return false;
        };
        class.name.as_str() == name
            && matches!(
                class.bases.as_slice(),
                [Expr::Name(base)] if base.id.as_str() == "Exception"
            )
            && class.keywords.is_empty()
            && class.decorator_list.is_empty()
            && class.type_params.is_empty()
            && class
                .body
                .iter()
                .all(|statement| matches!(statement, Stmt::Pass(_)) || docstring(statement))
    })
}

fn docstring(statement: &Stmt) -> bool {
    let Stmt::Expr(statement) = statement else {
        return false;
    };
    let Expr::Constant(constant) = statement.value.as_ref() else {
        return false;
    };
    matches!(constant.value, ast::Constant::Str(_))
}

fn inert_prefix(statement: &Stmt) -> bool {
    match statement {
        Stmt::Pass(_) => true,
        Stmt::Assert(assertion) => {
            evaluate(&assertion.test, &scope(&[])).is_some_and(|value| value.truthy())
        }
        _ => docstring(statement),
    }
}

fn plain_parameters(function: &ast::StmtFunctionDef) -> bool {
    function.args.vararg.is_none()
        && function.args.kwarg.is_none()
        && function.args.kwonlyargs.is_empty()
        && function.type_params.is_empty()
        && function.returns.is_none()
        && function
            .args
            .posonlyargs
            .iter()
            .chain(&function.args.args)
            .all(|argument| argument.default.is_none() && argument.def.annotation.is_none())
}

fn deferred_body(body: &[Stmt]) -> bool {
    let mut deferred = false;
    visit_exprs_in_statements(body, &mut |expression| {
        deferred |= matches!(
            expression,
            Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
        );
    });
    deferred
}

fn test_bindings_intact(
    test: &PythonTest,
    owner: &PythonOwner,
    old_type: &str,
    new_type: &str,
    expected: &str,
    expected_kind: ClassKind<'_>,
) -> bool {
    let rebinding = &test.constant_rebinding;
    let owner_modules = owner_module_paths(&owner.file);
    test.framework == PythonFramework::Pytest
        && test.name == test.qualified_name
        && test.decorators.is_empty()
        && test.fixtures.is_empty()
        && !test.parametrized
        && !rebinding.implicit_reach
        && !rebinding.header_reach
        && !rebinding.fixture_substitution
        && super::test_activation::activation_control(test).is_none()
        && !mentions_identifier(&test.body_text, "__builtins__")
        && !mentions_reflection(&test.body_text)
        && !names_may_differ(&test.body_text)
        && [
            owner.name.as_str(),
            old_type,
            new_type,
            "pytest",
            "raises",
            "Exception",
            "len",
            "__builtins__",
        ]
        .iter()
        .all(|name| !rebinding.rebinds(name) && !rebinding.assigns_attribute(name))
        && rebinding
            .attributes
            .iter()
            .chain(rebinding.foreign_attributes.iter())
            .all(|attribute| !(attribute.starts_with("__") && attribute.ends_with("__")))
        && test.imports.iter().any(|import| {
            import.imported == "pytest"
                && import.alias == "pytest"
                && import.source_module.is_empty()
        })
        && test.imports.iter().any(|import| {
            import.imported == owner.name
                && import.alias == owner.name
                && owner_modules.contains(&import.source_module)
                && import_source_module_matches_owner(import, owner, &test.file)
                && match expected_kind {
                    // A local class is the owner module's own definition.
                    ClassKind::Local => test.imports.iter().any(|class_import| {
                        class_import.imported == expected
                            && class_import.alias == expected
                            && class_import.source_module == import.source_module
                    }),
                    // A builtin is read only when the test module binds
                    // neither name, even once (`TypeError = Exception`).
                    ClassKind::Builtin => {
                        !rebinding.module_opaque
                            && !rebinding
                                .module_bound
                                .iter()
                                .any(|bound| bound == old_type || bound == new_type)
                            && test.imports.iter().all(|other| {
                                other.alias != old_type
                                    && other.alias != new_type
                                    && other.imported != "*"
                            })
                    }
                    // The test binds the class by the owner module's own
                    // import and nothing else, and names it once: in the
                    // context itself.
                    ClassKind::Imported(owner_import) => {
                        !rebinding.module_opaque
                            && plain_class_import(&test.imports, expected).is_some_and(|theirs| {
                                theirs.source_module == owner_import.source_module
                                    && same_import_root(
                                        &owner_modules,
                                        &import.source_module,
                                        &theirs.source_module,
                                    )
                                    && module_name_identifies_owner_for(
                                        owner,
                                        &theirs.source_module,
                                        &test.file,
                                    )
                            })
                            && test.imports.iter().all(|other| other.imported != "*")
                            && identifier_count(&test.body_text, expected) == 1
                    }
                }
        })
}

/// Whether a test that imports the owner as `owner_spelling` can be naming
/// the same module object when it imports a class from `class_spelling`.
/// When the owner is imported under a layout root (`verlib.core` for
/// `src/verlib/core.py`), a class module spelled with that root
/// (`src.verlib.errors`) is loaded a second time under another name, and its
/// class is not the one the owner raises.
fn same_import_root(owner_modules: &[String], owner_spelling: &str, class_spelling: &str) -> bool {
    !owner_modules.iter().any(|full| {
        full.strip_suffix(owner_spelling)
            .is_some_and(|root| root.ends_with('.') && class_spelling.starts_with(root))
    })
}

#[derive(Clone, Copy)]
enum ClassKind<'a> {
    /// `class Name(Exception): pass`, defined once in the owner module.
    Local,
    /// An unshadowed builtin from `builtin_exception_ancestors`.
    Builtin,
    /// A plain exception class the owner module binds by this import
    /// ([`imported_class_credits`]).
    Imported(&'a super::PythonImport),
}

/// Whether neither raised class is a subclass of the other, so a class-only
/// context naming one rejects the other. A local plain class has exactly the
/// ancestors `Exception` and `BaseException`, which no listed builtin is.
fn disjoint(old: &str, old_kind: ClassKind<'_>, new: &str, new_kind: ClassKind<'_>) -> bool {
    match (old_kind, new_kind) {
        // The class-only reading of an imported class has its own producer.
        (ClassKind::Imported(_), _) | (_, ClassKind::Imported(_)) => false,
        (ClassKind::Local, ClassKind::Local) => old != new,
        (ClassKind::Local, ClassKind::Builtin) | (ClassKind::Builtin, ClassKind::Local) => true,
        (ClassKind::Builtin, ClassKind::Builtin) => {
            let (Some(old_ancestors), Some(new_ancestors)) = (
                builtin_exception_ancestors(old),
                builtin_exception_ancestors(new),
            ) else {
                return false;
            };
            old != new && !old_ancestors.contains(&new) && !new_ancestors.contains(&old)
        }
    }
}

/// The builtin ancestors (below `Exception`) of builtin exception classes whose
/// constructor accepts any positional constants and returns an instance of
/// exactly that class. `OSError` and its subclasses are left out (an `errno`
/// argument selects a different subclass), as are the `Unicode*Error` classes
/// (fixed constructor arity), `StopIteration` and `SyntaxError`.
fn builtin_exception_ancestors(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "ArithmeticError" | "AssertionError" | "AttributeError" | "BufferError" | "EOFError"
        | "ImportError" | "LookupError" | "NameError" | "RuntimeError" | "TypeError"
        | "ValueError" => &[],
        "FloatingPointError" | "OverflowError" | "ZeroDivisionError" => &["ArithmeticError"],
        "ModuleNotFoundError" => &["ImportError"],
        "IndexError" | "KeyError" => &["LookupError"],
        "UnboundLocalError" => &["NameError"],
        "NotImplementedError" | "RecursionError" => &["RuntimeError"],
        _ => return None,
    })
}

fn pytest_raises(call: &ast::ExprCall) -> bool {
    matches!(call.func.as_ref(), Expr::Attribute(attribute)
        if attribute.attr.as_str() == "raises"
            && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == "pytest"))
}

fn literal_arguments(
    function: &ast::StmtFunctionDef,
    call: &ast::ExprCall,
) -> Option<Vec<(String, Value)>> {
    let arguments = function.args.posonlyargs.iter().chain(&function.args.args);
    if !call.keywords.is_empty() || arguments.clone().count() != call.args.len() {
        return None;
    }
    arguments
        .zip(&call.args)
        .map(|(parameter, argument)| {
            let literal = matches!(argument, Expr::Constant(_))
                || matches!(argument, Expr::UnaryOp(unary)
                    if matches!(unary.op, ast::UnaryOp::USub | ast::UnaryOp::UAdd)
                        && matches!(unary.operand.as_ref(), Expr::Constant(_)));
            if !literal {
                return None;
            }
            Some((
                parameter.def.arg.to_string(),
                evaluate(argument, &scope(&[]))?,
            ))
        })
        .collect()
}

static UNBOUND_VALUE: Value = Value::Bool(false);

fn scope(parameters: &[(String, Value)]) -> Scope<'_> {
    Scope {
        constant: "",
        value: &UNBOUND_VALUE,
        parameters: Some(parameters),
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
        model: None,
        receiver: None,
        floats: false,
        callee: None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Next,
    Target,
    Exit,
}

struct RaisePath<'a> {
    source: &'a SourceText<'a>,
    line: usize,
    parameters: &'a [(String, Value)],
    len_intact: bool,
}

impl RaisePath<'_> {
    fn walk(&self, body: &[Stmt], depth: usize, budget: &mut usize) -> Option<Flow> {
        if depth > 16 {
            return None;
        }
        for statement in body {
            *budget = budget.checked_sub(1)?;
            match statement {
                Stmt::If(branch) => {
                    let body = if self.value(&branch.test, 0)?.truthy() {
                        &branch.body
                    } else {
                        &branch.orelse
                    };
                    match self.walk(body, depth + 1, budget)? {
                        Flow::Next => {}
                        exit => return Some(exit),
                    }
                }
                Stmt::Raise(raise) => {
                    return Some(
                        if line_for_range_start(self.source, raise.range) == self.line {
                            Flow::Target
                        } else {
                            Flow::Exit
                        },
                    );
                }
                Stmt::Return(_) => return Some(Flow::Exit),
                Stmt::Pass(_) => {}
                _ if docstring(statement) => {}
                _ => return None,
            }
        }
        Some(Flow::Next)
    }

    fn value(&self, expression: &Expr, depth: usize) -> Option<Value> {
        if depth > 16 {
            return None;
        }
        match expression {
            Expr::Constant(_) | Expr::Name(_) => evaluate(expression, &scope(self.parameters)),
            Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::Not => Some(Value::Bool(
                !self.value(&unary.operand, depth + 1)?.truthy(),
            )),
            Expr::Compare(compare) => {
                let mut left = self.value(&compare.left, depth + 1)?;
                for (operator, right) in compare.ops.iter().zip(&compare.comparators) {
                    let right = self.value(right, depth + 1)?;
                    if !comparison(operator, &left, &right, false)? {
                        return Some(Value::Bool(false));
                    }
                    left = right;
                }
                Some(Value::Bool(true))
            }
            Expr::Call(call)
                if self.len_intact
                    && call.keywords.is_empty()
                    && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "len") =>
            {
                let [argument] = call.args.as_slice() else {
                    return None;
                };
                let Value::Str(value) = self.value(argument, depth + 1)? else {
                    return None;
                };
                Some(Value::Int(value.chars().count().try_into().ok()?))
            }
            _ => None,
        }
    }
}
