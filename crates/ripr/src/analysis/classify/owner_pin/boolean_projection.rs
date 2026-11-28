//! Per-version equality for primitive Boolean projections and tuples.
//! Every element must agree on one version of the owner and the equality
//! must change across versions. A permanently wrong sibling masks the result.

use super::{
    FunctionContainer, FunctionSummary, OracleFact, OwnerPinSyntax, OwnerReturnPin, RustIndex,
    TestSummary, assertion_comparison_operands, builtin_type_unshadowed,
    function_declaration_is_async, is_plain_or_std_macro, is_whole_free_owner_call,
    parse_clean_source_file,
};
use crate::analysis::test_grip_evidence::fold_boolean_owner_call;
use ra_ap_syntax::{AstNode, ast};

mod conditional;
mod container;
mod encoding;

pub(super) fn has_mapped_operand(sides: [&str; 2]) -> bool {
    conditional::has_conditional_operand(sides)
        || sides
            .into_iter()
            .any(|side| encoding::recognizes(side) || container::recognizes(side))
}

/// Admit the established single-macro assertion, or the bare outer assertion
/// containing exactly one recognized standard value macro. The latter's
/// binding and its two complete operands are verified by the value reader.
pub(super) fn comparison_macro(text: &str, expected: &str) -> bool {
    if is_plain_or_std_macro(text, expected) {
        return true;
    }
    if !text.contains("format!") && !text.contains("vec!") {
        return false;
    }
    let Some(ast::Expr::MacroExpr(expression)) = operand(text.trim().trim_end_matches(';')) else {
        return false;
    };
    let Some(outer) = expression.macro_call() else {
        return false;
    };
    if !outer
        .path()
        .is_some_and(|path| path.syntax().text() == expected)
    {
        return false;
    }
    let masked = super::mask_comments_and_strings(text);
    let names = super::macro_invocations(&masked);
    matches!(names.as_slice(), [(head, _), ("format" | "vec", _)] if *head == expected)
        && encoding::macro_operands(&outer).is_some_and(|args| match args.as_slice() {
            [_, _] => true,
            [_, _, diagnostic, ..] => encoding::string_literal(diagnostic).is_some(),
            _ => false,
        })
}

fn operand(text: &str) -> Option<ast::Expr> {
    let text = text.trim();
    let expression = super::super::owner_value_sink::parse_expression(text)?;
    (expression.syntax().text() == text
        && !expression
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind())))
    .then_some(expression)
}

struct ComparedCall {
    call: String,
    when_false: bool,
    when_true: bool,
}

impl ComparedCall {
    fn equals(call: String, expected: bool) -> Self {
        Self {
            call,
            when_false: !expected,
            when_true: expected,
        }
    }

    fn matches(&self, value: bool) -> bool {
        if value {
            self.when_true
        } else {
            self.when_false
        }
    }
}

pub(in crate::analysis) fn boolean_projection_matches_versions(
    owner: &FunctionSummary,
    baseline: &FunctionSummary,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<(bool, bool)> {
    let unequal = comparison_macro(&assertion.text, "assert_ne");
    if !matches!(owner.item.container, FunctionContainer::Free)
        || !(comparison_macro(&assertion.text, "assert_eq") || unequal)
    {
        return None;
    }
    let compared = assertion_comparison_operands(&assertion.text)?;
    let [left, right] = compared;
    let (projection, expected) = if encoding::recognizes(left) || container::recognizes(left) {
        (left, right)
    } else {
        (right, left)
    };
    let cells = if encoding::recognizes(projection) {
        vec![encoding::comparison(
            projection, expected, test, assertion, index, syntax,
        )?]
    } else if container::recognizes(projection) {
        vec![container::comparison(
            projection, expected, test, assertion, index, syntax,
        )?]
    } else if conditional::has_conditional_operand(compared) {
        vec![conditional::comparison(
            compared, test, assertion, index, syntax,
        )?]
    } else if compared
        .iter()
        .all(|side| side.trim_start().starts_with('('))
    {
        tuple_cells(compared)?
    } else {
        let compared = if compared
            .iter()
            .all(|side| side.trim_start().starts_with("Some("))
        {
            // Standard Some(T) equality compares its payload. A rebound
            // constructor need not carry the argument into equality.
            if !standard_variant_in_scope("Some", test, assertion, index, syntax) {
                return None;
            }
            let [left, right] = compared;
            [some_payload(left)?, some_payload(right)?]
        } else {
            compared
        };
        let normalized = super::primitive_bool_negated_operands(compared)
            .or_else(|| super::bool_to_u8_operands(compared, index, syntax, test, assertion))
            .unwrap_or(compared);
        let [left, right] = normalized;
        let (call, expected) = match left.parse::<bool>() {
            Ok(expected) => (right, expected),
            Err(_) => (left, right.parse::<bool>().ok()?),
        };
        vec![ComparedCall::equals(call.to_string(), expected)]
    };
    if function_declaration_is_async(owner, index) != Some(false)
        || !(test.start_line..=test.end_line).contains(&assertion.line)
        || !syntax.admits(test, assertion, index)
        || !builtin_type_unshadowed("bool", index, syntax)
        || !primitive_parameters(owner, index, syntax)
    {
        return None;
    }
    let (mut before_matches, mut after_matches) = (true, true);
    for cell in cells {
        let call = &cell.call;
        if !is_whole_free_owner_call(call, &owner.name)
            || !OwnerReturnPin::field_call_binds_owner(owner, index, test, call, syntax)
        {
            return None;
        }
        let arguments = super::super::activation::call_arguments(call, &owner.name)?;
        let (Some(before), Some(after)) = (
            fold_boolean_owner_call(baseline, &arguments),
            fold_boolean_owner_call(owner, &arguments),
        ) else {
            return None;
        };
        before_matches &= cell.matches(before);
        after_matches &= cell.matches(after);
    }
    Some((before_matches != unequal, after_matches != unequal))
}

fn standard_variant_in_scope(
    name: &str,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    if test.generated.is_some()
        || syntax.macro_may_declare(name, index)
        || !syntax
            .option_prelude_available
            .get_or_init(|| option_prelude_available(index))
    {
        return false;
    }
    let Some(scopes) = index
        .source_syntax
        .get(index, &test.file)
        .and_then(|source| source.name_scopes(test))
    else {
        return false;
    };
    scopes.scopes.iter().all(|scope| {
        !scope.defines.contains(name)
            && !scope.block_binds(name)
            && scope.imports_of(name).next().is_none()
            && !scope.local_bindings.get(name).is_some_and(|sites| sites.iter().any(|site| {
                site.line <= assertion.line && site.end_line.is_none_or(|end| end >= assertion.line)
            }))
            && scope.globs.iter().all(|glob| {
                glob.as_ref().is_some_and(|path| matches!(path.as_slice(), [part] if part == "super" && !scope.modules.is_empty()))
            })
    })
}

// Method names may be introduced through local traits, including imports under aliases.
// Cache trait declarations once and evaluate their visibility at the actual test site.
fn standard_method_available(
    name: &str,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    let methods = syntax.projection_traits.get_or_init(|| {
        let mut methods = std::collections::BTreeMap::new();
        if !crate::analysis::classify::call_identity::closed_method_dispatch_imports(index) {
            return methods;
        }
        for method in ["from", "eq", "first", "contains"] {
            if syntax.macro_may_declare(method, index) {
                continue;
            }
            let mut traits = Some(std::collections::BTreeSet::new());
            for function in index
                .functions()
                .iter()
                .filter(|function| function.name == method)
            {
                match (&function.item.container, &mut traits) {
                    (FunctionContainer::Trait { trait_name }, Some(names)) => {
                        names.insert(trait_name.clone());
                    }
                    (FunctionContainer::Unknown, _) => {
                        traits = None;
                        break;
                    }
                    _ => {}
                }
            }
            methods.insert(method, traits);
        }
        methods
    });
    let Some(Some(traits)) = methods.get(name) else {
        return false;
    };
    if traits.is_empty() {
        return true;
    }
    let Some(scopes) = index
        .source_syntax
        .get(index, &test.file)
        .and_then(|source| source.name_scopes(test))
    else {
        return false;
    };
    traits
        .iter()
        .all(|name| standard_variant_in_scope(name, test, assertion, index, syntax))
        && scopes.scopes.iter().all(|scope| {
            scope.imports.is_empty()
                && scope.block_bindings.is_empty()
                && scope.block_rename_targets.is_empty()
                && !scope.block_globs
        })
}

fn option_prelude_available(index: &RustIndex) -> bool {
    index.files().iter().all(|(path, file)| {
        if file.used_lexical_fallback {
            return false;
        }
        if !file.source.contains("no_implicit_prelude") {
            return true;
        }
        let Some(tree) = index
            .source_syntax
            .get(index, path)
            .and_then(|source| source.tree())
        else {
            return false;
        };
        !tree
            .syntax()
            .descendants()
            .filter_map(ast::Attr::cast)
            .any(|attr| {
                attr.path()
                    .is_some_and(|path| path.syntax().text() == "no_implicit_prelude")
            })
    })
}

fn some_payload(operand: &str) -> Option<&str> {
    let text = operand.trim();
    let closing = super::constructor_call_span(text, "Some")?;
    Some(text.get("Some(".len()..closing)?.trim())
}

fn tuple_cells([left, right]: [&str; 2]) -> Option<Vec<ComparedCall>> {
    let (left, right) = (tuple(left)?, tuple(right)?);
    let (actual, expected) = if right.fields().all(|field| bool_literal(&field).is_some()) {
        (left, right)
    } else {
        (right, left)
    };
    // The standard tuple PartialEq implementations cover arities through 12.
    let arity = actual.fields().count();
    if !(1..=12).contains(&arity) || arity != expected.fields().count() {
        return None;
    }
    actual
        .fields()
        .zip(expected.fields())
        .map(|(call, literal)| {
            let ast::Expr::CallExpr(call) = call else {
                return None;
            };
            Some(ComparedCall::equals(
                call.syntax().text().to_string(),
                bool_literal(&literal)?,
            ))
        })
        .collect()
}

fn bool_literal(expr: &ast::Expr) -> Option<bool> {
    let ast::Expr::Literal(literal) = expr else {
        return None;
    };
    match literal.kind() {
        ast::LiteralKind::Bool(value) => Some(value),
        _ => None,
    }
}

fn tuple(text: &str) -> Option<ast::TupleExpr> {
    let source = format!("fn ripr_tuple() {{ {text} }}");
    super::tuple_tail(&source)
}

fn primitive_parameters(
    owner: &FunctionSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    let Some(function) = parse_clean_source_file(&owner.body)
        .and_then(|parse| parse.tree().syntax().children().find_map(ast::Fn::cast))
    else {
        return false;
    };
    function.param_list().is_some_and(|params| {
        params.params().all(|param| {
            param.ty().is_some_and(|ty| {
                builtin_type_unshadowed(&ty.syntax().text().to_string(), index, syntax)
            })
        })
    })
}
