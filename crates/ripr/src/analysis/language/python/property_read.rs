//! Plain builtin `@property` reads (#15490).
//!
//! A test that asserts a property's value never calls the getter by name:
//! `assert Box([1, 2, 3]).total == 6` reads the attribute, and the builtin
//! `property` runs the getter. This module recognizes only that shape and
//! refuses everything it cannot read statically:
//!
//! - [`property_read_relation`] relates a pytest test that reads
//!   `Cls(...).<name>` or `v.<name>` (with `v` bound once to `Cls(...)`) for
//!   an owner decorated exactly `@property`, where `Cls` reaches the test
//!   through one import of the owner's class from the owner's module. A call,
//!   write or `del` of the attribute, any other use of the class or the bound
//!   local, any other call, and any patch or mock marker refuse the relation.
//! - [`property_read_credit`] credits the relation only when the owner module
//!   declares a plain builtin property (no rebinding of `property`, no
//!   same-name setter, deleter or class attribute, no attribute hooks, no
//!   class-name mention or property store inside the class), the getter is
//!   straight-line with one `return` and the changed line is that return,
//!   and the row-selected strong
//!   assertion is a top-level `assert <read> == <literal>`. Only that credit
//!   lifts the owner's `decorator_indirection` limit (`static_limits.rs`); a
//!   bare `assert v.<name>` or any other shape keeps it.

use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::bool_oracle::count_bindings;
use super::module_constants::visit_exprs_in_statements;
use super::owner_result_field::{
    class_level_binds, dedent, is_literal, names_identifier, plain_identifier, text_of,
};
use super::related_tests::{
    PythonRelatedCandidate, PythonRelationKind, owner_class_locals, owner_module_paths,
};
use super::source_facts::parse_module_result;
use super::{PythonAssertion, PythonFramework, PythonOwner, PythonTest};
use crate::domain::{OracleStrength, OwnerKind, ProbeFamily};
use rustpython_parser::ast::{self, Expr, ExprContext, Mod, Stmt};
use std::path::Path;

/// The one decorator spelling this module reads: the builtin name.
const PROPERTY: &str = "property";

/// Test-text markers that can replace the property, the getter or the class,
/// or read the attribute by name.
const TEST_REFUSAL_MARKERS: [&str; 15] = [
    "patch",
    "mock",
    "Mock",
    "monkeypatch",
    "getattr",
    "setattr",
    "delattr",
    "__",
    "vars(",
    "globals(",
    "locals(",
    "exec(",
    "eval(",
    "builtins",
    "fget",
];

/// Owner-module markers that can replace or intercept an attribute read
/// without naming the class or the property.
const MODULE_REFUSAL_MARKERS: [&str; 14] = [
    "__getattr",
    "__setattr",
    "__delattr",
    "__dict__",
    "__class__",
    "__new__",
    "__init_subclass__",
    "__set_name__",
    "setattr(",
    "delattr(",
    "vars(",
    "metaclass",
    "builtins",
    "import *",
];

/// The strong property-read assertion credited for a changed getter line.
#[derive(Clone, Copy, Debug)]
pub(super) struct PropertyReadCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
}

/// What a test does with the owner's property, when the shape is readable.
#[derive(Debug, Default)]
struct PropertyReads {
    /// Trimmed text of each top-level `assert <read> == <literal>`.
    exact_assertions: Vec<String>,
}

/// The owner-side gate that needs no source: a method of a top-level plain
/// class without bases, decorated with exactly the bare `property` name, in a
/// module that neither rebinds nor imports that name.
fn is_property_owner(owner: &PythonOwner) -> bool {
    owner.owner_kind == Some(OwnerKind::Method)
        && owner.decorators.len() == 1
        && owner
            .decorators
            .first()
            .is_some_and(|name| name == PROPERTY)
        && !owner.unplain_decorators.iter().any(|name| name == PROPERTY)
        && !owner
            .imports
            .iter()
            .any(|import| import.alias == PROPERTY || import.imported == "*")
        && !owner.name.starts_with("__")
        && plain_identifier(&owner.class_path)
        && owner.qualified_name == format!("{}.{}", owner.class_path, owner.name)
        && owner.class_facts.plain
        && owner.class_facts.no_bases
}

/// RIPR-SPEC-0238 R5a: the `property_read` relation.
pub(super) fn property_read_relation(test: &PythonTest, owner: &PythonOwner) -> bool {
    is_property_owner(owner) && property_reads(test, owner).is_some()
}

/// The credit for a changed getter line, or `None` whenever any part of the
/// shape is not statically established.
pub(super) fn property_read_credit<'a>(
    owner: &PythonOwner,
    owner_source: &str,
    line_text: &str,
    focus: &PythonAssertionFocus,
    candidates: &[PythonRelatedCandidate<'a>],
) -> Option<PropertyReadCredit<'a>> {
    if *focus.family() != ProbeFamily::ReturnValue
        || !line_text.trim_start().starts_with("return ")
        || !is_property_owner(owner)
        || !candidates
            .iter()
            .any(|candidate| candidate.relation == PythonRelationKind::PropertyRead)
        || !owner_declares_plain_property(owner, owner_source, line_text)
    {
        return None;
    }
    candidates
        .iter()
        .filter(|candidate| candidate.relation == PythonRelationKind::PropertyRead)
        .find_map(|candidate| {
            let test = candidate.test;
            let reads = property_reads(test, owner)?;
            // Only the assertion the finding's row shows can carry the
            // credit: another strong assertion of the test cannot donate it.
            let selected = select_relevant_assertion(&test.assertions, Some(focus)).assertion()?;
            (selected.oracle_strength == OracleStrength::Strong
                && !selected.helper_origin.is_helper()
                && reads
                    .exact_assertions
                    .iter()
                    .any(|text| text == selected.text.trim()))
            .then_some(PropertyReadCredit {
                test,
                assertion: selected,
            })
        })
}

/// Whether the owner module declares the owner as a plain builtin property:
/// the class is bound once, has no bases, keywords or decorators, is not
/// named anywhere else in the module, binds the owner name once (no
/// `@<name>.setter` / `.deleter` / `.getter` sibling or class attribute of
/// that name), and the module never spells `property` except as the bare
/// decorator line. The getter runs straight through to its one `return`,
/// and `line_text` is that return: a change under a branch, or to a line
/// before the return, may not reach the read the test compares.
fn owner_declares_plain_property(owner: &PythonOwner, source: &str, line_text: &str) -> bool {
    // Python folds identifiers by NFKC: ASCII owner modules only.
    if !source.is_ascii()
        || MODULE_REFUSAL_MARKERS
            .iter()
            .any(|marker| source.contains(marker))
        || names_identifier(source, "type")
        || source
            .lines()
            .any(|line| names_identifier(line, PROPERTY) && line.trim() != "@property")
    {
        return false;
    }
    let Ok(Mod::Module(module)) = parse_module_result(&owner.file, source) else {
        return false;
    };
    let class_name = owner.class_path.as_str();
    let mut classes = module.body.iter().filter_map(|stmt| match stmt {
        Stmt::ClassDef(class) if class.name.as_str() == class_name => Some(class),
        _ => None,
    });
    let (Some(class), None) = (classes.next(), classes.next()) else {
        return false;
    };
    if count_bindings(&module.body, class_name) != 1
        || count_bindings(&module.body, PROPERTY) != 0
        || !class.bases.is_empty()
        || !class.keywords.is_empty()
        || !class.decorator_list.is_empty()
    {
        return false;
    }
    let (Some(before), Some(after)) = (
        source.get(..usize::from(class.range.start())),
        source.get(usize::from(class.range.end())..),
    ) else {
        return false;
    };
    if names_identifier(before, class_name) || names_identifier(after, class_name) {
        return false;
    }
    let plain_statement = |stmt: &Stmt| match stmt {
        Stmt::FunctionDef(_) => true,
        Stmt::Assign(assign) => matches!(assign.value.as_ref(), Expr::Constant(_)),
        Stmt::AnnAssign(assign) => assign
            .value
            .as_deref()
            .is_none_or(|value| matches!(value, Expr::Constant(_))),
        Stmt::Expr(expr) => matches!(expr.value.as_ref(), Expr::Constant(_)),
        Stmt::Pass(_) => true,
        _ => false,
    };
    if !class.body.iter().all(plain_statement)
        || class
            .body
            .iter()
            .filter(|stmt| class_level_binds(stmt, &owner.name))
            .count()
            != 1
    {
        return false;
    }
    // No decorator in the class may name the owner (`@total.setter def
    // set_total`), and the owner's one decorator is the bare builtin name.
    let mut getter = None;
    for stmt in &class.body {
        let Stmt::FunctionDef(function) = stmt else {
            continue;
        };
        if function.name.as_str() == "__getattribute__" || function.name.as_str() == "__getattr__" {
            return false;
        }
        for decorator in &function.decorator_list {
            let Some(text) = text_of(source, ast::Ranged::range(decorator)) else {
                return false;
            };
            if names_identifier(text, &owner.name) {
                return false;
            }
        }
        if function.name.as_str() == owner.name {
            getter = Some(function);
        }
    }
    let Some(getter) = getter else {
        return false;
    };
    if !matches!(getter.decorator_list.as_slice(),
        [Expr::Name(name)] if name.id.as_str() == PROPERTY)
    {
        return false;
    }
    // Nothing in the class names the class (`Box.total = 6` in `__init__`)
    // or stores or deletes the property (`self.total = 0`, `del self.total`):
    // either can replace what the read runs.
    let class_mentions = class.body.iter().any(|stmt| {
        text_of(source, ast::Ranged::range(stmt))
            .is_none_or(|text| names_identifier(text, class_name))
    });
    let mut property_stores = 0usize;
    visit_exprs_in_statements(&class.body, &mut |expr| {
        if let Expr::Attribute(attribute) = expr
            && attribute.attr.as_str() == owner.name
            && attribute.ctx != ExprContext::Load
        {
            property_stores += 1;
        }
    });
    if class_mentions || property_stores != 0 {
        return false;
    }
    straight_line_getter_returns(getter, source, line_text)
}

/// Whether the getter body is an optional docstring, simple statements and
/// one final `return`, and `line_text` is that return's only line, written
/// once in the getter.
fn straight_line_getter_returns(
    getter: &ast::StmtFunctionDef,
    source: &str,
    line_text: &str,
) -> bool {
    let Some((Stmt::Return(ret), before)) = getter.body.split_last() else {
        return false;
    };
    let simple = |stmt: &Stmt| match stmt {
        Stmt::Assign(_) | Stmt::AnnAssign(_) | Stmt::Pass(_) => true,
        Stmt::Expr(expr) => matches!(expr.value.as_ref(), Expr::Constant(_)),
        _ => false,
    };
    if !before.iter().all(simple) {
        return false;
    }
    let (Some(return_text), Some(getter_text)) =
        (text_of(source, ret.range), text_of(source, getter.range))
    else {
        return false;
    };
    let changed = line_text.trim();
    !return_text.contains('\n')
        && return_text.trim() == changed
        && getter_text
            .lines()
            .filter(|line| line.trim() == changed)
            .count()
            == 1
}

/// The test's reads of the owner property, when every use of the attribute,
/// the class and the bound locals is one this module reads; `None` refuses
/// the test.
fn property_reads(test: &PythonTest, owner: &PythonOwner) -> Option<PropertyReads> {
    if test.framework != PythonFramework::Pytest
        || !test.decorators.is_empty()
        || !test.fixtures.is_empty()
        || test.parametrized
        || test.qualified_name != test.name
        || !test.body_text.is_ascii()
        || !names_identifier(&test.body_text, &owner.name)
        || TEST_REFUSAL_MARKERS
            .iter()
            .any(|marker| test.body_text.contains(marker))
    {
        return None;
    }
    let class = owner.class_path.as_str();
    // Nothing outside the test body reaches the class first: `Box = Other`,
    // a module-level `Box.total = 6`, a namespace writer anywhere in the
    // module (`setattr`, `patch`, `sys.modules`, even in a sibling test), an
    // autouse fixture, a setup hook, a definition-time call or a fixture that
    // substitutes the owner.
    let rebinding = &test.constant_rebinding;
    if rebinding.rebinds(class)
        || rebinding.module_opaque
        || rebinding.assigns_attribute(class)
        || rebinding.assigns_attribute(&owner.name)
        || rebinding.assigns_dunder_attribute()
        || rebinding.implicit_reach
        || rebinding.header_reach
        || rebinding.fixture_substitution
        || super::test_activation::activation_control(test).is_some()
    {
        return None;
    }
    // The class reaches the test through exactly one `from <owner module>
    // import <Class>` under its own name: a same-named class of another
    // module is not the owner's.
    let owner_modules = owner_module_paths(&owner.file);
    let mut class_imports = test
        .imports
        .iter()
        .filter(|import| import.imported == class || import.alias == class);
    let (Some(class_import), None) = (class_imports.next(), class_imports.next()) else {
        return None;
    };
    if class_import.alias != class
        || class_import.imported != class
        || !owner_modules.contains(&class_import.source_module)
        || !owner_class_locals(test, owner, class)
            .iter()
            .any(|local| local == class)
    {
        return None;
    }
    let source = dedent(&test.body_text);
    if !source.trim_start().starts_with("def ") {
        return None;
    }
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<python-test-body>"), &source)
    else {
        return None;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return None;
    };
    let args = &function.args;
    if !args.posonlyargs.is_empty()
        || !args.args.is_empty()
        || !args.kwonlyargs.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || !function.decorator_list.is_empty()
    {
        return None;
    }
    let body = &function.body;
    // Straight-line statements only: no branch, loop, context manager,
    // handler, `del`, augmented assignment, import or nested scope.
    if !body.iter().all(|stmt| {
        matches!(
            stmt,
            Stmt::Assign(_) | Stmt::Expr(_) | Stmt::Assert(_) | Stmt::Pass(_)
        )
    }) {
        return None;
    }
    // Locals bound to a construction of the class, each bound exactly once.
    let mut bindings: Vec<String> = Vec::new();
    for stmt in body {
        let Stmt::Assign(assign) = stmt else {
            continue;
        };
        if !is_construction(&assign.value, class) {
            continue;
        }
        let [Expr::Name(target)] = assign.targets.as_slice() else {
            return None;
        };
        let name = target.id.to_string();
        if name == class || bindings.contains(&name) || count_bindings(body, &name) != 1 {
            return None;
        }
        bindings.push(name);
    }
    let mut uses = PropertyUses::default();
    visit_exprs_in_statements(body, &mut |expr| {
        uses.record(expr, class, &owner.name, &bindings);
    });
    if uses.refused
        || uses.reads == 0
        || uses.class_loads != uses.constructions
        || uses.binding_loads != uses.binding_reads
    {
        return None;
    }
    let exact_assertions = body
        .iter()
        .filter_map(|stmt| exact_read_assertion(stmt, class, &owner.name, &bindings))
        .filter_map(|stmt| text_of(&source, ast::Ranged::range(stmt)))
        .map(|text| text.trim().to_string())
        .collect();
    Some(PropertyReads { exact_assertions })
}

/// `Cls(...)` for the class local.
fn is_construction(expr: &Expr, class: &str) -> bool {
    matches!(expr, Expr::Call(call)
        if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == class))
}

/// `Cls(...).<name>` or `<binding>.<name>`, loaded.
fn is_property_read(expr: &Expr, class: &str, name: &str, bindings: &[String]) -> bool {
    let Expr::Attribute(attribute) = expr else {
        return false;
    };
    attribute.attr.as_str() == name
        && attribute.ctx == ExprContext::Load
        && (is_construction(&attribute.value, class)
            || matches!(attribute.value.as_ref(),
                Expr::Name(local) if bindings.iter().any(|bound| bound == local.id.as_str())))
}

/// `stmt` when it is `assert <read> == <literal>` (either side), with no
/// message.
fn exact_read_assertion<'a>(
    stmt: &'a Stmt,
    class: &str,
    name: &str,
    bindings: &[String],
) -> Option<&'a Stmt> {
    let Stmt::Assert(assert) = stmt else {
        return None;
    };
    if assert.msg.is_some() {
        return None;
    }
    let Expr::Compare(compare) = assert.test.as_ref() else {
        return None;
    };
    let ([ast::CmpOp::Eq], [right]) = (compare.ops.as_slice(), compare.comparators.as_slice())
    else {
        return None;
    };
    let left = compare.left.as_ref();
    let read = |expr: &Expr| is_property_read(expr, class, name, bindings);
    ((read(left) && is_literal(right)) || (is_literal(left) && read(right))).then_some(stmt)
}

/// Counts of how the test body uses the property, the class and the bound
/// locals.
#[derive(Default)]
struct PropertyUses {
    refused: bool,
    reads: usize,
    constructions: usize,
    class_loads: usize,
    binding_loads: usize,
    binding_reads: usize,
}

impl PropertyUses {
    fn record(&mut self, expr: &Expr, class: &str, name: &str, bindings: &[String]) {
        match expr {
            // A deferred or rebinding expression can run or bind out of order.
            Expr::Lambda(_)
            | Expr::NamedExpr(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::GeneratorExp(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::YieldFrom(_)
            | Expr::Starred(_) => self.refused = true,
            Expr::Call(call) => {
                // The construction is the only code the test may run: another
                // call could replace the property, and `v.<name>()` calls the
                // getter's result.
                if is_construction(expr, class) {
                    self.constructions += 1;
                } else {
                    self.refused = true;
                }
                if let Expr::Attribute(attribute) = call.func.as_ref()
                    && attribute.attr.as_str() == name
                {
                    self.refused = true;
                }
            }
            Expr::Attribute(attribute) if attribute.attr.as_str() == name => {
                if is_property_read(expr, class, name, bindings) {
                    self.reads += 1;
                    if matches!(attribute.value.as_ref(), Expr::Name(_)) {
                        self.binding_reads += 1;
                    }
                } else {
                    // A write, `del`, class access (`Cls.<name>`) or a read
                    // on a receiver this module does not tie to the class.
                    self.refused = true;
                }
            }
            Expr::Name(local) if local.id.as_str() == class => {
                if local.ctx == ExprContext::Load {
                    self.class_loads += 1;
                } else {
                    self.refused = true;
                }
            }
            Expr::Name(local)
                if local.ctx == ExprContext::Load
                    && bindings.iter().any(|bound| bound == local.id.as_str()) =>
            {
                self.binding_loads += 1;
            }
            _ => {}
        }
    }
}
