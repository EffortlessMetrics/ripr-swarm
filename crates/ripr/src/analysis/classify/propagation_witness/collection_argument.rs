//! Exact call-site projection binding for the existing collection observer.

use crate::analysis::classify::OwnerPinSyntax;
use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::reveal::assertion_comparison_operands;
use crate::analysis::classify::targeted_free_call_reaches_owner;
use crate::analysis::rust_index::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList, HasAttrs, HasGenericParams, HasName},
};
use std::sync::{Mutex, PoisonError};

/// Access to the run-scoped [`OwnerPinSyntax`] for one projection check.
/// The classifier reads a shared instance directly; the seam walk (#8282)
/// locks its one shared instance per syntax query so the parse-heavy
/// segments between the queries stay parallel.
trait SyntaxAuthority {
    fn with_syntax<R>(&self, read: impl FnOnce(&OwnerPinSyntax) -> R) -> R;
}

impl SyntaxAuthority for OwnerPinSyntax {
    fn with_syntax<R>(&self, read: impl FnOnce(&OwnerPinSyntax) -> R) -> R {
        read(self)
    }
}

impl SyntaxAuthority for Mutex<OwnerPinSyntax> {
    fn with_syntax<R>(&self, read: impl FnOnce(&OwnerPinSyntax) -> R) -> R {
        let guard = self.lock().unwrap_or_else(PoisonError::into_inner);
        read(&guard)
    }
}

#[cfg(test)]
thread_local! {
    static COLLECTION_OWNER_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A narrow extension of the bare collection observer: a sole, unconditional
/// parameter push, called with an exact mutable field projection, followed by
/// an equality on that unchanged projection and an independent literal vec.
/// Aliases, nested execution, unresolved calls and intervening uses fail closed.
pub(in crate::analysis) fn assertion_observes_collection_argument(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &OwnerPinSyntax,
) -> bool {
    observed_projection(receiver, owner, index, test, assertion, syntax).is_some()
}

/// #8282: the same projection for the seam walk, reading the run's one
/// shared instance through its mutex. The lock is held per syntax query;
/// the parse-heavy segments between queries run in parallel.
pub(in crate::analysis) fn mutexed_assertion_observes_collection_argument(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &Mutex<OwnerPinSyntax>,
) -> bool {
    observed_projection(receiver, owner, index, test, assertion, syntax).is_some()
}

fn observed_projection<S: SyntaxAuthority>(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &S,
) -> Option<()> {
    if !assertion.text.trim_start().starts_with("assert_eq!") {
        return None;
    }
    let syntax_admits = syntax.with_syntax(|syntax| {
        syntax.admits(test, assertion, index)
            && syntax.targeted_macro_binding_is_unambiguous(test, assertion.line, "vec", index)
    });
    if !syntax_admits {
        return None;
    }
    let [subject, expected] = assertion_comparison_operands(&assertion.text)?;
    let subject = compact(subject);
    let (binding, _) = subject.split_once('.')?;
    if !super::is_bare_ident(binding)
        || !subject.split('.').skip(1).all(|field| {
            super::is_bare_ident(field)
                || (!field.is_empty() && field.chars().all(|c| c.is_ascii_digit()))
        })
        || !literal_vec(expected)
    {
        return None;
    }
    #[cfg(test)]
    COLLECTION_OWNER_PARSES.with(|count| count.set(count.get() + 1));
    let owner_tree = parse_clean_source_file(&owner.body)?.tree();
    let function = owner_tree.syntax().descendants().find_map(ast::Fn::cast)?;
    if function.async_token().is_some()
        || function.gen_token().is_some()
        || function.generic_param_list().is_some()
    {
        return None;
    }
    let parameters: Vec<_> = function.param_list()?.params().collect();
    let parameter = parameters.iter().position(|parameter| {
        let Some(ast::Pat::IdentPat(pattern)) = parameter.pat() else {
            return false;
        };
        let Some(ast::Type::RefType(ty)) = parameter.ty() else {
            return false;
        };
        pattern.name().is_some_and(|name| name.text() == receiver)
            && pattern.ref_token().is_none()
            && pattern.pat().is_none()
            && ty.mut_token().is_some()
    })?;
    let owner_statements = function.body()?.stmt_list()?;
    let expressions: Vec<_> = owner_statements
        .statements()
        .filter_map(statement_expression)
        .chain(owner_statements.tail_expr())
        .collect();
    if expressions.len() != 1 || owner_statements.statements().count() > 1 {
        return None;
    }
    let ast::Expr::MethodCallExpr(push) = &expressions[0] else {
        return None;
    };
    if push.attrs().next().is_some() {
        return None;
    }
    if push.name_ref()?.text() != "push" || push.receiver()?.syntax().text() != receiver {
        return None;
    }
    let tree = parse_clean_source_file(&test.body)?.tree();
    let function = tree.syntax().descendants().find_map(ast::Fn::cast)?;
    let statements: Vec<_> = function.body()?.stmt_list()?.statements().collect();
    // The exact recorded assertion must be one top-level statement on its line.
    let assertions: Vec<_> = statements
        .iter()
        .enumerate()
        .filter(|(_, statement)| {
            let at = usize::from(statement.syntax().text_range().start());
            test.start_line + test.body[..at].matches('\n').count() == assertion.line
                && compact(&statement.syntax().text().to_string()).trim_end_matches(';')
                    == compact(&assertion.text).trim_end_matches(';')
        })
        .map(|(at, _)| at)
        .collect();
    let [assertion_at] = assertions.as_slice() else {
        return None;
    };
    let visible = &statements[..*assertion_at];
    let mentions: Vec<_> = visible
        .iter()
        .enumerate()
        .filter(|(_, statement)| mentions_binding(statement.syntax(), binding))
        .collect();
    let [(binding_at, definition), (call_at, call_statement)] = mentions.as_slice() else {
        return None;
    };
    if binding_at >= call_at {
        return None;
    }
    let ast::Stmt::LetStmt(definition) = definition else {
        return None;
    };
    let ast::Pat::IdentPat(pattern) = definition.pat()? else {
        return None;
    };
    if pattern.name()?.text() != binding
        || pattern.ref_token().is_some()
        || pattern.pat().is_some()
        || definition.let_else().is_some()
    {
        return None;
    }
    // Construction owns a value; a borrowed or renamed binding is an alias.
    let initializer = definition.initializer()?;
    if initializer
        .syntax()
        .descendants()
        .any(|node| ast::RefExpr::can_cast(node.kind()))
        || !matches!(
            initializer,
            ast::Expr::CallExpr(_) | ast::Expr::RecordExpr(_)
        )
    {
        return None;
    }
    let constructor = match &initializer {
        ast::Expr::CallExpr(call) => {
            let ast::Expr::PathExpr(path) = call.expr()? else {
                return None;
            };
            compact(&path.syntax().text().to_string())
        }
        ast::Expr::RecordExpr(record) => compact(&record.path()?.syntax().text().to_string()),
        _ => return None,
    };
    let (_, field) = subject.split_once('.')?;
    let ast::Type::RefType(receiver_type) = parameters.get(parameter)?.ty()? else {
        return None;
    };
    let receiver_type_text = receiver_type.ty()?.syntax().text().to_string();
    if definition.ty().is_some()
        || !super::is_bare_ident(&constructor)
        || !syntax.with_syntax(|syntax| {
            crate::analysis::classify::owner_pin::projected_collection_has_value_equality(
                &constructor,
                field,
                &receiver_type_text,
                owner,
                test,
                index,
                syntax,
            )
        })
    {
        return None;
    }
    let ast::Expr::CallExpr(call) = statement_expression((*call_statement).clone())? else {
        return None;
    };
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    if call.attrs().next().is_some() {
        return None;
    }
    let spelled = compact(&path.syntax().text().to_string());
    let mut segments: Vec<String> = spelled.split("::").map(str::to_string).collect();
    if segments.pop()?.as_str() != owner.name {
        return None;
    }
    let prefix = (!segments.is_empty()).then(|| format!("{}::", segments.join("::")));
    let targeted_reaches = syntax.with_syntax(|syntax| {
        targeted_free_call_reaches_owner(test, owner, prefix.as_deref(), index, syntax)
    });
    if !targeted_reaches
        || shared_owner_call_identity(index, owner).is_some_and(|identity| {
            identity.resolve_call(test, index, &segments) != CallTarget::Owner
        })
    {
        return None;
    }
    let arguments: Vec<_> = call.arg_list()?.args().collect();
    if arguments.len() != parameters.len() {
        return None;
    }
    let ast::Expr::RefExpr(argument) = arguments.get(parameter)? else {
        return None;
    };
    if argument.mut_token().is_none() || argument.raw_token().is_some() {
        return None;
    }
    let ast::Expr::FieldExpr(projection) = argument.expr()? else {
        return None;
    };
    if compact(&projection.syntax().text().to_string()) != subject {
        return None;
    }
    // Other call arguments cannot use, mutate or alias this same root.
    if arguments
        .iter()
        .enumerate()
        .any(|(at, argument)| at != parameter && mentions_binding(argument.syntax(), binding))
    {
        return None;
    }
    Some(())
}

fn statement_expression(statement: ast::Stmt) -> Option<ast::Expr> {
    match statement {
        ast::Stmt::ExprStmt(statement) => statement.expr(),
        _ => None,
    }
}

fn mentions_binding(node: &SyntaxNode, binding: &str) -> bool {
    node.descendants_with_tokens()
        .filter_map(|node| node.into_token())
        .any(|token| {
            token.kind() == SyntaxKind::IDENT
                && token.text().strip_prefix("r#").unwrap_or(token.text()) == binding
        })
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn literal_vec(text: &str) -> bool {
    let text = compact(text);
    let Some(elements) = text
        .strip_prefix("vec![")
        .and_then(|text| text.strip_suffix(']'))
    else {
        return false;
    };
    elements.is_empty()
        || elements
            .trim_end_matches(',')
            .split(',')
            .all(|element| !element.is_empty() && element.chars().all(|ch| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::classify::{OwnerPinSyntax, WithheldMacroBindings};
    use crate::analysis::facts::build_index;
    use std::path::{Path, PathBuf};

    #[test]
    fn field_argument_observer_retains_withheld_macro_authority() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/collection_argument_tuple/input");
        let index = build_index(&root, &[PathBuf::from("src/lib.rs")])?;
        let owner = index
            .functions()
            .iter()
            .find(|f| f.name == "add")
            .ok_or("owner")?;
        let test = index.tests().first().ok_or("test")?;
        let assertion = test.assertions.first().ok_or("assertion")?;
        assert!(assertion_observes_collection_argument(
            "v",
            owner,
            &index,
            test,
            assertion,
            &OwnerPinSyntax::default()
        ));
        for name in ["assert_eq", "vec"] {
            let mut withheld = WithheldMacroBindings::default();
            withheld.absorb(
                Path::new("dep/src/lib.rs"),
                &format!("#[macro_export] macro_rules! {name} {{ ($($t:tt)*) => {{}}; }}"),
                index.macro_scope_crates(),
                &index.drop_in_manifests,
            );
            let syntax = OwnerPinSyntax::default().with_withheld(withheld);
            assert!(
                !assertion_observes_collection_argument(
                    "v", owner, &index, test, assertion, &syntax
                ),
                "a withheld {name} binding cannot be replaced by a fresh syntax authority"
            );
        }
        Ok(())
    }

    struct ProjectionFixtureRoot(PathBuf);

    impl Drop for ProjectionFixtureRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn assert_projection_case(
        named: bool,
        assertion_text: &str,
        expected: bool,
        expected_owner_parses: usize,
    ) -> Result<(), String> {
        let template = if named {
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/collection_argument_named/input/src/lib.rs"
            ))
        } else {
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/collection_argument_tuple/input/src/lib.rs"
            ))
        };
        let original = if named {
            "assert_eq!(b.items, vec![11]);"
        } else {
            "assert_eq!(b.0, vec![11]);"
        };
        assert_eq!(template.matches(original).count(), 1);
        let source = template.replace(original, assertion_text);
        let root = ProjectionFixtureRoot(crate::analysis::facts::drop_in::temp_workspace(
            "projection-shape",
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"projection-shape\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
                ),
                ("src/lib.rs", &source),
            ],
        )?);
        let index = build_index(&root.0, &[PathBuf::from("src/lib.rs")])?;
        let owner = index
            .functions()
            .iter()
            .find(|f| f.name == "add")
            .ok_or("owner")?;
        let test = index.tests().first().ok_or("test")?;
        let assertion = test.assertions.first().ok_or("assertion")?;
        assert_eq!(
            compact(&assertion.text).trim_end_matches(';'),
            compact(assertion_text).trim_end_matches(';')
        );
        let syntax = OwnerPinSyntax::default();
        // These are the exact unchanged gates before the reordered block.
        // A rejected admission must not manufacture a zero-parser-cost pass.
        assert!(
            syntax.admits(test, assertion, &index),
            "assertion authority: {assertion_text}"
        );
        assert!(
            syntax.targeted_macro_binding_is_unambiguous(test, assertion.line, "vec", &index),
            "vec authority: {assertion_text}"
        );
        let before = COLLECTION_OWNER_PARSES.with(std::cell::Cell::get);
        assert_eq!(
            assertion_observes_collection_argument("v", owner, &index, test, assertion, &syntax),
            expected,
            "full observer: {assertion_text}"
        );
        assert_eq!(
            COLLECTION_OWNER_PARSES.with(std::cell::Cell::get) - before,
            expected_owner_parses,
            "actual owner parse invocations: {assertion_text}"
        );
        Ok(())
    }

    #[test]
    fn projection_shape_rejects_admitted_non_candidates_before_owner_parse() -> Result<(), String> {
        for assertion in [
            "assert_eq!(11, 11);",
            "assert_eq!(b.0, [11]);",
            "assert_eq!(b.0, expected);",
            "assert_eq!(b.0.clone(), vec![11]);",
            "assert_eq!((b.0), vec![11]);",
            "assert_eq!(b.0, vec![x]);",
            "assert_eq!(b.0, vec![11u32]);",
        ] {
            assert_projection_case(false, assertion, false, 0)?;
        }
        Ok(())
    }

    #[test]
    fn projection_shape_preserves_full_named_and_tuple_observers() -> Result<(), String> {
        assert_projection_case(false, "assert_eq!(b.0, vec![11]);", true, 1)?;
        assert_projection_case(true, "assert_eq!(b.items, vec![11]);", true, 1)?;
        assert_projection_case(
            false,
            "assert_eq!(b.0, vec![11], \"literal vector\");",
            true,
            1,
        )?;
        Ok(())
    }

    #[test]
    fn projection_shape_does_not_replace_full_binding_admission() -> Result<(), String> {
        assert_projection_case(false, "assert_eq!(other.0, vec![11]);", false, 1)
    }
}
