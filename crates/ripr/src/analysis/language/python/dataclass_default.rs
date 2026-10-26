//! A default edit of a plain `@dataclass` field, read through the assertions
//! that depend on it (#9619).
//!
//! `tax_rate: float = 0.2` -> `0.25` has no callable owner of its own: the
//! edit sits in a class body, where the dataclass decorator generates the
//! constructor that applies it. A test observes it by building the class
//! without that field and asserting on a method or attribute that reads it.
//! This pass credits exactly that, with the module-value evaluator
//! (`module_value.rs`) extended by floats and `round` for this read only:
//!
//! - the class is a top-level `@dataclass` or `@dataclasses.dataclass` with no
//!   arguments, bases or keywords, bound once, imported from `dataclasses`;
//! - its body holds only annotated fields, a docstring, `pass` and methods, no
//!   `__init__`, `__post_init__`, `__new__` or attribute hook, and no field is
//!   `ClassVar`, `InitVar`, keyword-only or `init=False`;
//! - the edited line is the file's only changed line, the same field with the
//!   same annotation and a different literal default;
//! - a straight-line `assert` of a test builds the class from literals leaving
//!   the field unset, and reads it through a one-`return` method or attribute;
//! - the assertion holds under the old default and fails under the new one.
//!
//! Anything the evaluator cannot decide leaves the `decorator_indirection`
//! abstention in place.

use super::bool_oracle::count_bindings;
use super::module_constants::ModuleBindingFacts;
use super::module_value::{
    ModuleEdit, ModuleValueCredit, Reader, Scope, Value, assertion_expression, constant_spellings,
    evaluate, keep_unshadowed, straight_line_assertion, with_line_replaced,
};
use super::source_facts::parse_module_result;
use super::{PythonOwner, PythonTest, expr_full_name};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

/// Methods that run on construction or on every attribute access, so a field
/// default is no longer what the instance holds.
const CONSTRUCTION_HOOKS: [&str; 8] = [
    "__init__",
    "__post_init__",
    "__new__",
    "__setattr__",
    "__getattr__",
    "__getattribute__",
    "__set_name__",
    "__init_subclass__",
];

struct Field {
    name: String,
    /// Whether construction may leave the field out.
    has_default: bool,
    /// The default when it is a literal the evaluator reads.
    default: Option<Value>,
}

struct Method {
    name: String,
    receiver: String,
    reader: Reader,
}

/// The `@dataclass` class as a test sees it: the fields in constructor order
/// and the methods the evaluator can run.
pub(super) struct ClassModel {
    spellings: Vec<String>,
    fields: Vec<Field>,
    methods: Vec<Method>,
}

impl ClassModel {
    /// The field values of `Class(...)` built from this call: arguments bound
    /// by position and name, the rest from their defaults. `None` when the call
    /// is not a certain construction of the class.
    pub(super) fn instance(
        &self,
        call: &ast::ExprCall,
        scope: &Scope<'_>,
    ) -> Option<Vec<(String, Option<Value>)>> {
        let callee = expr_full_name(&call.func)?;
        if !self.spellings.contains(&callee) {
            return None;
        }
        let mut bound: Vec<Option<Option<Value>>> = vec![None; self.fields.len()];
        if call.args.len() > self.fields.len() {
            return None;
        }
        for (index, argument) in call.args.iter().enumerate() {
            bound[index] = Some(Some(evaluate(argument, scope)?));
        }
        for keyword in &call.keywords {
            let name = keyword.arg.as_ref()?;
            let index = self
                .fields
                .iter()
                .position(|field| field.name == name.as_str())?;
            if bound[index].is_some() {
                return None;
            }
            bound[index] = Some(Some(evaluate(&keyword.value, scope)?));
        }
        self.fields
            .iter()
            .zip(bound)
            .map(|(field, value)| match value {
                Some(value) => Some((field.name.clone(), value)),
                None if field.has_default => Some((field.name.clone(), field.default.clone())),
                None => None,
            })
            .collect()
    }

    /// The result of `instance.method(arguments)`.
    pub(super) fn run_method(
        &self,
        name: &str,
        args: &[Expr],
        keywords: &[ast::Keyword],
        instance: &[(String, Option<Value>)],
        scope: &Scope<'_>,
    ) -> Option<Value> {
        let method = self.methods.iter().find(|method| method.name == name)?;
        if !keywords.is_empty() || args.len() != method.reader.parameters.len() {
            return None;
        }
        let arguments: Vec<(String, Value)> = method
            .reader
            .parameters
            .iter()
            .zip(args)
            .map(|(parameter, argument)| Some((parameter.clone(), evaluate(argument, scope)?)))
            .collect::<Option<_>>()?;
        let fields: Vec<(String, Value)> = instance
            .iter()
            .filter_map(|(field, value)| Some((field.clone(), value.clone()?)))
            .collect();
        let unused = Value::Bool(false);
        let inner = Scope {
            constant: "",
            value: &unused,
            parameters: Some(&arguments),
            constant_spellings: &[],
            reader_spellings: &[],
            reader: None,
            model: None,
            receiver: Some((method.receiver.as_str(), &fields)),
            floats: true,
        };
        evaluate(&method.reader.expression, &inner)
    }
}

/// The credits for a default edit of a plain dataclass field. Empty when the
/// edit is not a certain literal swap or no assertion distinguishes the values.
pub(super) fn dataclass_default_credits<'a>(
    edit: &ModuleEdit<'_>,
    class_owner: &PythonOwner,
    module_owner: Option<&PythonOwner>,
    all_tests: &'a [PythonTest],
) -> Vec<ModuleValueCredit<'a>> {
    let Some(module_owner) = module_owner else {
        return Vec::new();
    };
    credits(edit, class_owner, module_owner, all_tests).unwrap_or_default()
}

fn credits<'a>(
    edit: &ModuleEdit<'_>,
    class_owner: &PythonOwner,
    module_owner: &PythonOwner,
    all_tests: &'a [PythonTest],
) -> Option<Vec<ModuleValueCredit<'a>>> {
    let ModuleEdit {
        file,
        line,
        line_text,
        old_line_text,
        new_source,
    } = *edit;
    let old_line_text = old_line_text?;
    if class_owner.owner_kind.is_some()
        || class_owner.is_module_owner()
        || !matches!(
            class_owner.decorators.as_slice(),
            [decorator] if decorator == "dataclass" || decorator == "dataclasses.dataclass"
        )
    {
        return None;
    }
    let class = class_owner.name.as_str();
    let (field, new_value) = field_line(line_text)?;
    let (old_field, old_value) = field_line(old_line_text)?;
    // The target and annotation are evaluated at class creation, so a changed
    // one is not a default swap.
    if field != old_field
        || old_value == new_value
        || binding_header(line_text) != binding_header(old_line_text)
    {
        return None;
    }
    // Another workspace file or test file that rewrites the class or the field
    // changes it for every test.
    if module_owner.foreign_writes.may_rebind(class, file)
        || module_owner.foreign_writes.may_rebind(&field, file)
        || all_tests.iter().any(|test| {
            test.constant_rebinding
                .attributes
                .contains(&class.to_string())
                || test.constant_rebinding.attributes.contains(&field)
        })
    {
        return None;
    }
    let old_source = with_line_replaced(new_source, line, old_line_text)?;
    let (Mod::Module(new_module), Mod::Module(old_module)) = (
        parse_module_result(file, new_source).ok()?,
        parse_module_result(file, &old_source).ok()?,
    ) else {
        return None;
    };
    for (source, module) in [
        (new_source, &new_module),
        (old_source.as_str(), &old_module),
    ] {
        let facts = ModuleBindingFacts::of(source, &module.body);
        if !facts.bound_once(class) {
            return None;
        }
    }

    let mut out = Vec::new();
    for test in all_tests {
        if super::test_activation::activation_control(test).is_some()
            || test.constant_rebinding.edited_in_diff
            || test.constant_rebinding.assigns_attribute(&field)
            || test.constant_rebinding.assigns_attribute(class)
            || keep_unshadowed(test, vec!["round".to_string()]).is_empty()
        {
            continue;
        }
        let spellings = constant_spellings(test, class_owner, class);
        if spellings.is_empty() {
            continue;
        }
        let (Some(old_model), Some(new_model)) = (
            class_model(&old_module.body, class, &spellings),
            class_model(&new_module.body, class, &spellings),
        ) else {
            continue;
        };
        // The edit moves only the one field's default.
        if !same_fields(&old_model, &new_model, &field, &old_value, &new_value) {
            continue;
        }
        for assertion in &test.assertions {
            let Some(expression) = assertion_expression(assertion) else {
                continue;
            };
            if !straight_line_assertion(test, assertion) {
                continue;
            }
            let Some(preceding) = test
                .assertions
                .iter()
                .filter(|prior| prior.line < assertion.line)
                .map(assertion_expression)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let outcome = |model: &ClassModel| {
                let unused = Value::Bool(false);
                let scope = Scope {
                    constant: "",
                    value: &unused,
                    parameters: None,
                    constant_spellings: &[],
                    reader_spellings: &[],
                    reader: None,
                    model: Some(model),
                    receiver: None,
                    floats: true,
                };
                if !preceding
                    .iter()
                    .all(|prior| evaluate(prior, &scope).is_some_and(|value| value.truthy()))
                {
                    return None;
                }
                evaluate(&expression, &scope).map(|result| result.truthy())
            };
            if outcome(&old_model) == Some(true) && outcome(&new_model) == Some(false) {
                let reader = old_model
                    .methods
                    .iter()
                    .find(|method| assertion.text.contains(&format!(".{}(", method.name)))
                    .map(|method| method.name.clone());
                out.push(ModuleValueCredit {
                    test,
                    assertion,
                    constant: format!("{class}.{field}"),
                    old: old_value.render(),
                    new: new_value.render(),
                    reader,
                });
                break;
            }
        }
    }
    Some(out)
}

/// The text before the `=` of a one-line field: its name and annotation.
fn binding_header(line: &str) -> &str {
    line.split_once('=')
        .map_or(line, |(header, _)| header)
        .trim()
}

/// `name: annotation = <literal>` on one line.
fn field_line(text: &str) -> Option<(String, Value)> {
    let source = text.trim();
    let Mod::Module(module) = parse_module_result(Path::new("line.py"), source).ok()? else {
        return None;
    };
    let [Stmt::AnnAssign(assign)] = module.body.as_slice() else {
        return None;
    };
    let Expr::Name(name) = assign.target.as_ref() else {
        return None;
    };
    if !assign.simple {
        return None;
    }
    let value = assign.value.as_deref()?;
    if !matches!(value, Expr::Constant(_)) {
        return None;
    }
    let unused = Value::Bool(false);
    let scope = Scope {
        constant: "",
        value: &unused,
        parameters: None,
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
        model: None,
        receiver: None,
        floats: true,
    };
    Some((name.id.to_string(), evaluate(value, &scope)?))
}

fn same_fields(
    old: &ClassModel,
    new: &ClassModel,
    field: &str,
    old_value: &Value,
    new_value: &Value,
) -> bool {
    old.fields.len() == new.fields.len()
        && old.fields.iter().zip(&new.fields).all(|(before, after)| {
            before.name == after.name
                && before.has_default == after.has_default
                && if before.name == field {
                    before.default.as_ref() == Some(old_value)
                        && after.default.as_ref() == Some(new_value)
                } else {
                    before.default == after.default
                }
        })
}

/// The plain `@dataclass` class `class` of `module` as a model, when nothing
/// in it can change how a default reaches an instance.
fn class_model(module: &[Stmt], class: &str, spellings: &[String]) -> Option<ClassModel> {
    if count_bindings(module, class) != 1 || count_bindings(module, "round") != 0 {
        return None;
    }
    let definition = module.iter().find_map(|stmt| match stmt {
        Stmt::ClassDef(definition) if definition.name.as_str() == class => Some(definition),
        _ => None,
    })?;
    if !definition.bases.is_empty()
        || !definition.keywords.is_empty()
        || !plain_dataclass_decorator(module, &definition.decorator_list)
    {
        return None;
    }
    let mut fields: Vec<Field> = Vec::new();
    let mut methods: Vec<(String, Option<Method>)> = Vec::new();
    for (index, stmt) in definition.body.iter().enumerate() {
        match stmt {
            Stmt::Pass(_) => {}
            Stmt::Expr(docstring)
                if index == 0 && matches!(docstring.value.as_ref(), Expr::Constant(_)) => {}
            Stmt::AnnAssign(assign) => {
                if !assign.simple {
                    return None;
                }
                let Expr::Name(name) = assign.target.as_ref() else {
                    return None;
                };
                let annotation = format!("{:?}", assign.annotation);
                if ["ClassVar", "InitVar", "KW_ONLY"]
                    .iter()
                    .any(|marker| annotation.contains(marker))
                    || fields.iter().any(|field| field.name == name.id.as_str())
                {
                    return None;
                }
                let (has_default, default) = match assign.value.as_deref() {
                    None => (false, None),
                    Some(Expr::Constant(_)) => (true, field_value(assign.value.as_deref()?)),
                    Some(Expr::Call(call)) => {
                        // `field(init=False)` and `field(kw_only=True)` change
                        // how the constructor binds the field.
                        if call.keywords.iter().any(|keyword| {
                            keyword
                                .arg
                                .as_ref()
                                .is_some_and(|arg| matches!(arg.as_str(), "init" | "kw_only"))
                        }) {
                            return None;
                        }
                        (true, None)
                    }
                    Some(_) => (true, None),
                };
                fields.push(Field {
                    name: name.id.to_string(),
                    has_default,
                    default,
                });
            }
            Stmt::FunctionDef(function) => {
                if CONSTRUCTION_HOOKS.contains(&function.name.as_str()) {
                    return None;
                }
                let method = method_of(function);
                match methods
                    .iter_mut()
                    .find(|(name, _)| name == function.name.as_str())
                {
                    // Defined twice: no certain definition.
                    Some(entry) => entry.1 = None,
                    None => methods.push((function.name.to_string(), method)),
                }
            }
            Stmt::AsyncFunctionDef(function) => {
                if CONSTRUCTION_HOOKS.contains(&function.name.as_str()) {
                    return None;
                }
                match methods
                    .iter_mut()
                    .find(|(name, _)| name == function.name.as_str())
                {
                    Some(entry) => entry.1 = None,
                    None => methods.push((function.name.to_string(), None)),
                }
            }
            _ => return None,
        }
    }
    // A method named like a field replaces the field's default on the class.
    if methods
        .iter()
        .any(|(name, _)| fields.iter().any(|field| &field.name == name))
    {
        return None;
    }
    Some(ClassModel {
        spellings: spellings.to_vec(),
        fields,
        methods: methods
            .into_iter()
            .filter_map(|(_, method)| method)
            .collect(),
    })
}

fn field_value(value: &Expr) -> Option<Value> {
    let unused = Value::Bool(false);
    let scope = Scope {
        constant: "",
        value: &unused,
        parameters: None,
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
        model: None,
        receiver: None,
        floats: true,
    };
    evaluate(value, &scope)
}

/// Exactly one bare `@dataclass` / `@dataclasses.dataclass` that the module
/// imports from `dataclasses` and binds nowhere else.
fn plain_dataclass_decorator(module: &[Stmt], decorators: &[Expr]) -> bool {
    let [decorator] = decorators else {
        return false;
    };
    match decorator {
        Expr::Name(name) if name.id.as_str() == "dataclass" => {
            count_bindings(module, "dataclass") == 1
                && module.iter().any(|stmt| match stmt {
                    Stmt::ImportFrom(import) => {
                        import.level.is_none_or(|level| level.to_u32() == 0)
                            && import
                                .module
                                .as_ref()
                                .is_some_and(|m| m.as_str() == "dataclasses")
                            && import.names.iter().any(|alias| {
                                alias.name.as_str() == "dataclass" && alias.asname.is_none()
                            })
                    }
                    _ => false,
                })
        }
        Expr::Attribute(attribute) if attribute.attr.as_str() == "dataclass" => {
            matches!(attribute.value.as_ref(), Expr::Name(base) if base.id.as_str() == "dataclasses")
                && count_bindings(module, "dataclasses") == 1
                && module.iter().any(|stmt| match stmt {
                    Stmt::Import(import) => import.names.iter().any(|alias| {
                        alias.name.as_str() == "dataclasses" && alias.asname.is_none()
                    }),
                    _ => false,
                })
        }
        _ => false,
    }
}

/// An undecorated method whose body is one `return` expression (after an
/// optional docstring), over a receiver and positional parameters.
fn method_of(function: &ast::StmtFunctionDef) -> Option<Method> {
    let args = &function.args;
    if !function.decorator_list.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || !args.kwonlyargs.is_empty()
        || args
            .posonlyargs
            .iter()
            .chain(args.args.iter())
            .any(|arg| arg.default.is_some())
    {
        return None;
    }
    let mut names = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|arg| arg.def.arg.to_string());
    let receiver = names.next()?;
    let parameters: Vec<String> = names.collect();
    if parameters.contains(&receiver) {
        return None;
    }
    let body = match function.body.as_slice() {
        [Stmt::Expr(docstring), rest @ ..]
            if matches!(docstring.value.as_ref(), Expr::Constant(_)) && !rest.is_empty() =>
        {
            rest
        }
        all => all,
    };
    let [Stmt::Return(ret)] = body else {
        return None;
    };
    Some(Method {
        name: function.name.to_string(),
        receiver,
        reader: Reader {
            parameters,
            expression: ret.value.as_deref()?.clone(),
        },
    })
}
