//! Bounded AST-backed discrimination for direct two-boolean match inputs.
//!
//! This is a producer of discrimination evidence, not a class override. The
//! ordinary stage combiner still decides the finding. Unsupported source,
//! ownership, control flow, or oracle shapes leave existing evidence unchanged.

use crate::analysis::classify::{ProbeContext, file_imports_foreign_callee_name};
use crate::analysis::rust_index::find_file_facts;
use crate::domain::{Confidence, ProbeFamily, RelationReason, StageEvidence, StageState};
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxNode, ast};

struct ArmWitness {
    input: [bool; 2],
    result: String,
}

/// Refine only the missing discriminator; never repair reach or propagation.
pub(super) fn discrimination(
    context: &ProbeContext<'_>,
    observe: &StageEvidence,
    current: &StageEvidence,
) -> Option<StageEvidence> {
    if context.probe.family != ProbeFamily::MatchArm
        || observe.state != StageState::Yes
        || current.state != StageState::Weak
        || !current.summary.contains("observation_unverified")
        || !context.workspace_complete
    {
        return None;
    }
    let owner = context.owner_fn?;
    if context.probe.owner.as_ref() != Some(&owner.id)
        || context.index.functions.iter().filter(|f| f.name == owner.name).count() != 1
    {
        return None;
    }
    let facts = find_file_facts(context.index, &owner.file)?;
    let probe_facts = find_file_facts(context.index, &context.probe.location.file)?;
    if !std::ptr::eq(facts, probe_facts) || facts.used_lexical_fallback {
        return None;
    }
    let root = parsed(&facts.source)?;
    let function = named_function(&root, &owner.name)?;
    let witness = current_arm(&facts.source, &function, context)?;

    for (test, reason) in &context.related_tests {
        if *reason != RelationReason::DirectOwnerCall {
            continue;
        }
        let Some(test_facts) = find_file_facts(context.index, &test.file) else {
            continue;
        };
        if test_facts.used_lexical_fallback
            || file_imports_foreign_callee_name(
                &test_facts.source,
                &owner.name,
                &context.index.package_names,
            )
        {
            continue;
        }
        let Some(test_root) = parsed(&test_facts.source) else {
            continue;
        };
        if !test_namespace_is_plain(&test_root) {
            continue;
        }
        let Some(test_function) = named_function(&test_root, &test.name) else {
            continue;
        };
        if observed_equality(&test_function, &owner.name, &witness) {
            return Some(StageEvidence::new(
                StageState::Yes,
                Confidence::High,
                "Exact boolean tuple input selects this current arm; equality observes its changed literal result",
            ));
        }
    }
    None
}

fn parsed(source: &str) -> Option<ast::SourceFile> {
    let parse = SourceFile::parse(source, Edition::CURRENT);
    parse.errors().is_empty().then(|| parse.tree())
}

/// Only an unambiguous top-level free function is in this initial slice.
fn named_function(root: &ast::SourceFile, name: &str) -> Option<ast::Fn> {
    let mut functions = root.syntax().children().filter_map(ast::Fn::cast).filter(|f| {
        f.name().is_some_and(|n| n.text() == name)
    });
    let function = functions.next()?;
    functions.next().is_none().then_some(function)
}

fn current_arm(
    source: &str,
    function: &ast::Fn,
    context: &ProbeContext<'_>,
) -> Option<ArmWitness> {
    let match_expression = identity_match(function)?;
    let after = context.probe.after.as_deref()?;
    let before = context.probe.before.as_deref()?;
    let arms = match_expression.match_arm_list()?.arms().collect::<Vec<_>>();
    // Four distinct, unguarded literal patterns exclude earlier wildcard or
    // overlapping-arm selection without inventing general reachability facts.
    if arms.len() != 4 {
        return None;
    }
    let mut seen = Vec::new();
    let mut selected = None;
    for arm in arms {
        if arm.guard().is_some() || arm.attrs().next().is_some() {
            return None;
        }
        let input = bool_pattern(&arm.pat()?)?;
        if seen.contains(&input) {
            return None;
        }
        seen.push(input);
        let result = plain_string(&arm.expr()?)?;
        let start = u32::from(arm.syntax().text_range().start()) as usize;
        let line = source.get(..start)?.bytes().filter(|b| *b == b'\n').count() + 1;
        let text = arm.syntax().text().to_string();
        if line == context.probe.location.line
            && arm_text(&text) == arm_text(after)
            && arm_text(&text) == arm_text(&context.probe.expression)
        {
            if selected.is_some() {
                return None;
            }
            selected = Some(ArmWitness { input, result });
        }
    }
    let witness = selected?;
    let old = single_arm(before)?;
    if old.guard().is_some()
        || bool_pattern(&old.pat()?)? != witness.input
        || plain_string(&old.expr()?)? == witness.result
    {
        return None;
    }
    Some(witness)
}

fn arm_text(text: &str) -> &str {
    text.trim().trim_end_matches(',').trim_end()
}

/// No statements may intervene: shadowing, assignment, early returns, nested
/// closures and computed scrutinees do not establish this direct binding.
fn identity_match(function: &ast::Fn) -> Option<ast::MatchExpr> {
    if function.attrs().next().is_some() || function.async_token().is_some() {
        return None;
    }
    let list = function.param_list()?;
    if list.self_param().is_some() {
        return None;
    }
    let parameters = list.params().collect::<Vec<_>>();
    let [first, second] = parameters.as_slice() else {
        return None;
    };
    let names = [immutable_bool_name(first)?, immutable_bool_name(second)?];
    if names[0] == names[1] {
        return None;
    }
    let statements = function.body()?.stmt_list()?;
    if statements.statements().next().is_some() {
        return None;
    }
    let expression = ast::MatchExpr::cast(statements.tail_expr()?.syntax().clone())?;
    let tuple = ast::TupleExpr::cast(expression.expr()?.syntax().clone())?;
    let fields = tuple.syntax().children().filter_map(ast::Expr::cast).collect::<Vec<_>>();
    let [left, right] = fields.as_slice() else {
        return None;
    };
    if direct_name(left)? != names[0] || direct_name(right)? != names[1] {
        return None;
    }
    Some(expression)
}

fn immutable_bool_name(parameter: &ast::Param) -> Option<String> {
    if parameter.ty()?.syntax().text().to_string().trim() != "bool" {
        return None;
    }
    let pattern = ast::IdentPat::cast(parameter.pat()?.syntax().clone())?;
    let name = pattern.name()?.text().to_string();
    // Exact identifier syntax excludes mut/ref/@ bindings, not just spelling.
    (pattern.syntax().text().to_string().trim() == name).then_some(name)
}

fn direct_name(expression: &ast::Expr) -> Option<String> {
    let path = ast::PathExpr::cast(expression.syntax().clone())?.path()?;
    let text = path.syntax().text().to_string();
    let mut chars = text.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(text)
}

fn bool_pattern(pattern: &ast::Pat) -> Option<[bool; 2]> {
    let tuple = ast::TuplePat::cast(pattern.syntax().clone())?;
    let fields = tuple.syntax().children().filter_map(ast::Pat::cast).collect::<Vec<_>>();
    let [first, second] = fields.as_slice() else {
        return None;
    };
    Some([boolean(first.syntax())?, boolean(second.syntax())?])
}

fn boolean(node: &SyntaxNode) -> Option<bool> {
    match node.text().to_string().trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// This slice supports unescaped cooked results only. Other literal spellings
/// remain unverified; do not confuse source spelling with decoded Rust values.
fn plain_string(expression: &ast::Expr) -> Option<String> {
    let literal = ast::Literal::cast(expression.syntax().clone())?;
    let text = literal.syntax().text().to_string();
    let value = text.strip_prefix('"')?.strip_suffix('"')?;
    (!value.contains(['\\', '"', '\n', '\r'])).then(|| value.to_string())
}

fn single_arm(text: &str) -> Option<ast::MatchArm> {
    let source = format!("fn __arm() {{ match (false, false) {{ {text} }} }}");
    let root = parsed(&source)?;
    let function = named_function(&root, "__arm")?;
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    let expression = ast::MatchExpr::cast(body.tail_expr()?.syntax().clone())?;
    let list = expression.match_arm_list()?;
    let mut arms = list.arms();
    let arm = arms.next()?;
    arms.next().is_none().then_some(arm)
}

/// Keep macro and owner resolution conservative. No wildcard imports, macro
/// definitions, extern-crate macro imports, or nested test namespaces qualify.
fn test_namespace_is_plain(root: &ast::SourceFile) -> bool {
    root.syntax().children().all(|node| {
        if ast::Fn::can_cast(node.kind()) {
            return true;
        }
        if let Some(import) = ast::Use::cast(node) {
            let text = import.syntax().text().to_string();
            return !text.contains('*')
                && !text.contains("assert_eq")
                && !text.contains(" as ");
        }
        false
    })
}

fn observed_equality(function: &ast::Fn, owner: &str, witness: &ArmWitness) -> bool {
    observed_equality_inner(function, owner, witness).unwrap_or(false)
}

fn observed_equality_inner(
    function: &ast::Fn,
    owner: &str,
    witness: &ArmWitness,
) -> Option<bool> {
    let attributes = function.attrs().collect::<Vec<_>>();
    let [attribute] = attributes.as_slice() else {
        return None;
    };
    if attribute.syntax().text().to_string() != "#[test]"
        || function.async_token().is_some()
        || function.param_list()?.params().next().is_some()
    {
        return None;
    }
    let body = function.body()?.stmt_list()?;
    let statements = body.statements().collect::<Vec<_>>();
    let [ast::Stmt::ExprStmt(statement)] = statements.as_slice() else {
        return None;
    };
    if body.tail_expr().is_some() {
        return None;
    }
    let expression = ast::MacroExpr::cast(statement.expr()?.syntax().clone())?;
    let call = expression.macro_call()?;
    if call.path()?.syntax().text().to_string() != "assert_eq" {
        return None;
    }
    // Let the existing parser split actual expression operands. Diagnostic
    // arguments are parsed but never searched for owner calls or values.
    let tokens = call.token_tree()?.syntax().text().to_string();
    let inner = tokens.strip_prefix('(')?.strip_suffix(')')?;
    let source = format!("fn __operands() {{ ({inner}) }}");
    let root = parsed(&source)?;
    let function = named_function(&root, "__operands")?;
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    let tuple = ast::TupleExpr::cast(body.tail_expr()?.syntax().clone())?;
    let operands = tuple.syntax().children().filter_map(ast::Expr::cast).collect::<Vec<_>>();
    let [left, right, ..] = operands.as_slice() else {
        return None;
    };
    Some(
        (direct_input(left, owner) == Some(witness.input)
            && plain_string(right).as_ref() == Some(&witness.result))
            || (direct_input(right, owner) == Some(witness.input)
                && plain_string(left).as_ref() == Some(&witness.result)),
    )
}

fn direct_input(expression: &ast::Expr, owner: &str) -> Option<[bool; 2]> {
    let call = ast::CallExpr::cast(expression.syntax().clone())?;
    if direct_name(&call.expr()?)? != owner {
        return None;
    }
    let arguments = call.arg_list()?.args().collect::<Vec<_>>();
    let [first, second] = arguments.as_slice() else {
        return None;
    };
    Some([boolean(first.syntax())?, boolean(second.syntax())?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(source: &str, name: &str) -> Result<ast::Fn, String> {
        let root = parsed(source).ok_or_else(|| "invalid fixture syntax".to_string())?;
        named_function(&root, name).ok_or_else(|| "missing fixture function".to_string())
    }

    #[test]
    fn direct_binding_rejects_reordering_mutation_and_shadowing() -> Result<(), String> {
        let source = "fn route(a: bool, b: bool) { match (a, b) { _ => () } }";
        assert!(identity_match(&function(source, "route")?).is_some());
        for changed in [
            source.replace("(a, b)", "(b, a)"),
            source.replace("(a, b)", "(!a, b)"),
            source.replace("(a, b)", "(false, true)"),
            source.replace("a: bool", "mut a: bool"),
            source.replace("match", "let a = false; match"),
            source.replace("match", "return; match"),
            source.replace("match", "let _f = || (); match"),
        ] {
            assert!(
                identity_match(&function(&changed, "route")?).is_none(),
                "unsupported binding was admitted: {changed}"
            );
        }
        Ok(())
    }

    #[test]
    fn only_compared_direct_inputs_and_exact_results_observe() -> Result<(), String> {
        let witness = ArmWitness { input: [true, false], result: "new".to_string() };
        for (assertion, expected) in [
            ("assert_eq!(route(true, false), \"new\");", true),
            ("assert_eq!(\"new\", route(true, false));", true),
            ("assert_eq!(route(true, false), \"new\", \"message {}\", 1);", true),
            ("assert_eq!(route(false, true), \"new\");", false),
            ("assert_eq!(route(false, true), \"old\", \"{:?}\", route(true, false));", false),
            ("assert_eq!(route(true, false), \"old\");", false),
            ("assert_ne!(route(true, false), \"old\");", false),
            ("assert_eq!(other(true, false), \"new\");", false),
            ("assert_eq!(foreign::route(true, false), \"new\");", false),
            ("assert_eq!(route(if false { true } else { false }, false), \"new\");", false),
            ("assert_eq!(route(!false, false), \"new\");", false),
            ("assert_eq!({ route(true, false) }, \"new\");", false),
        ] {
            let source = format!("#[test]\nfn observes() {{ {assertion} }}");
            assert_eq!(
                observed_equality(&function(&source, "observes")?, "route", &witness),
                expected,
                "oracle: {assertion}"
            );
        }
        Ok(())
    }

    #[test]
    fn unsupported_pattern_shapes_and_literal_values_stay_absent() -> Result<(), String> {
        for (pattern, expected) in [
            ("(true, false)", Some([true, false])),
            ("(false, true)", Some([false, true])),
            ("(true, _)", None),
            ("(true, false, true)", None),
            ("((true, false), false)", None),
            ("(true, false) | (false, true)", None),
        ] {
            let arm = single_arm(&format!("{pattern} => \"new\","))
                .ok_or_else(|| "invalid arm fixture".to_string())?;
            let pattern = arm.pat().ok_or_else(|| "missing pattern".to_string())?;
            assert_eq!(bool_pattern(&pattern), expected);
        }
        for literal in [r#"r"new""#, r#""new\n""#] {
            let arm = single_arm(&format!("(true, false) => {literal},"))
                .ok_or_else(|| "invalid result fixture".to_string())?;
            let value = arm.expr().ok_or_else(|| "missing arm value".to_string())?;
            assert!(plain_string(&value).is_none());
        }
        Ok(())
    }
}
