//! Bounded discrimination for two derived Boolean locals inside one closure.
//!
//! This extends the direct-parameter tuple witness without becoming a Rust
//! interpreter. Every unsupported owner, closure, initializer, result-flow, or
//! oracle shape leaves the existing weak evidence unchanged.

use crate::analysis::classify::ProbeContext;
use crate::analysis::rust_index::find_file_facts;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Confidence, Probe, ProbeFamily, RelationReason, StageEvidence, StageState};
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, SyntaxNode, ast};
use std::path::{Component, Path, PathBuf};

struct DerivedWitness {
    input: [bool; 2],
    result: String,
    terminal: bool,
}

/// Refine only a still-missing discriminator for the bounded derived-local
/// closure shape. Reach, infection, propagation, observation, and final class
/// remain owned by the ordinary stage combiner.
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
        || context
            .index
            .functions()
            .iter()
            .filter(|function| function.name == owner.name)
            .count()
            != 1
    {
        return None;
    }

    let facts = find_file_facts(context.index, &owner.file)?;
    let same_source = find_file_facts(context.index, &context.probe.location.file)
        .is_some_and(|probe_facts| std::ptr::eq(facts.data(), probe_facts.data()))
        || same_current_file(
            context,
            &owner.file,
            &context.probe.location.file,
            &facts.source,
        );
    if !same_source || facts.used_lexical_fallback {
        return None;
    }

    let root = parsed(&facts.source)?;
    let function = named_function(&root, &owner.name)?;
    let witness = derived_witness(&facts.source, &function, context.probe)?;
    if witness.terminal && !super::derived_admission::terminal_namespace_unambiguous(context) {
        return None;
    }

    for (test, reason) in &context.related_tests {
        if *reason != RelationReason::DirectOwnerCall {
            continue;
        }
        let Some(test_facts) = find_file_facts(context.index, &test.file) else {
            continue;
        };
        if test_facts.used_lexical_fallback
            || !test_facts.role_provenance.edges.is_empty()
            || test_facts
                .role_provenance
                .earliest_unresolved_reason
                .is_some()
            || !assertion_namespace_is_standalone(&owner.file, &test.file)
            || context.test_file_imports_foreign_callee_name(
                &test.file,
                &test_facts.source,
                &owner.name,
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
        // `derived_admission` owns the projection oracle. Re-check its one
        // strict implementation here so this path keeps the same guarantees
        // even if the admission wiring in `tuple_match::discrimination` ever
        // changes; the former local weaker copy is deleted.
        let observes = if witness.terminal {
            super::derived_admission::terminal_projection_observes(
                &test_function,
                &owner.name,
                witness.result.as_str(),
                Some(witness.input),
            )
        } else {
            super::derived_admission::top_level_projection_observes(
                &test_function,
                &owner.name,
                witness.result.as_str(),
            )
        };
        if observes == Some(true) {
            return Some(StageEvidence::new(
                StageState::Yes,
                Confidence::High,
                "Exact derived Boolean tuple selects this current arm and the returned projection observes its unique literal result",
            ));
        }
    }

    None
}

fn derived_witness(source: &str, function: &ast::Fn, probe: &Probe) -> Option<DerivedWitness> {
    if function.attrs().next().is_some() || function.async_token().is_some() {
        return None;
    }
    if let Some(terminal_match) = super::derived_admission::terminal_owner_match(function) {
        let mut witness = derived_match_witness(source, &terminal_match, probe)?;
        witness.terminal = true;
        return Some(witness);
    }
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    let tail = body.tail_expr()?;
    let tail_text = compact(&tail.syntax().text().to_string());
    if !tail_text.contains(".iter().filter_map(") || !tail_text.ends_with(".collect()") {
        return None;
    }

    let mut selected = None;
    for match_expression in function
        .syntax()
        .descendants()
        .filter_map(ast::MatchExpr::cast)
    {
        let Some(candidate) = derived_match_witness(source, &match_expression, probe) else {
            continue;
        };
        if selected.is_some() {
            return None;
        }
        selected = Some(candidate);
    }
    selected
}

fn derived_match_witness(
    source: &str,
    match_expression: &ast::MatchExpr,
    probe: &Probe,
) -> Option<DerivedWitness> {
    let tuple = ast::TupleExpr::cast(match_expression.expr()?.syntax().clone())?;
    let tuple_fields = tuple
        .syntax()
        .children()
        .filter_map(ast::Expr::cast)
        .collect::<Vec<_>>();
    let [left, right] = tuple_fields.as_slice() else {
        return None;
    };
    let tuple_names = [direct_name(left)?, direct_name(right)?];
    if tuple_names[0] == tuple_names[1] {
        return None;
    }

    let relation_let = match_expression
        .syntax()
        .ancestors()
        .find_map(ast::LetStmt::cast)?;
    let initializer = relation_let.initializer()?;
    if initializer.syntax().text_range() != match_expression.syntax().text_range() {
        return None;
    }
    let relation_name = immutable_binding_name(&relation_let)?;
    if tuple_names.contains(&relation_name) {
        return None;
    }

    let closure = match_expression
        .syntax()
        .ancestors()
        .find_map(ast::ClosureExpr::cast)?;
    let closure_parameter = closure_parameter(&closure)?;
    let closure_body = ast::BlockExpr::cast(closure.body()?.syntax().clone())?;
    let statements = closure_body.stmt_list()?.statements().collect::<Vec<_>>();
    let [
        ast::Stmt::LetStmt(first),
        ast::Stmt::LetStmt(second),
        ast::Stmt::LetStmt(relation),
    ] = statements.as_slice()
    else {
        return None;
    };
    if relation.syntax().text_range() != relation_let.syntax().text_range()
        || immutable_binding_name(first)? != tuple_names[0]
        || immutable_binding_name(second)? != tuple_names[1]
        || immutable_binding_name(relation)? != relation_name
    {
        return None;
    }

    let first_initializer = first.initializer()?;
    let second_initializer = second.initializer()?;
    if !supported_request_membership(&first_initializer, &closure_parameter)
        || !supported_task_identity(&second_initializer, &closure_parameter)
    {
        return None;
    }

    let closure_tail = closure_body.stmt_list()?.tail_expr()?;
    if !returns_relation(&closure_tail, &closure_parameter, &relation_name) {
        return None;
    }

    current_arm(source, match_expression, probe)
}

fn current_arm(
    source: &str,
    match_expression: &ast::MatchExpr,
    probe: &Probe,
) -> Option<DerivedWitness> {
    let after = probe.after.as_deref()?;
    let arms = match_expression
        .match_arm_list()?
        .arms()
        .collect::<Vec<_>>();
    if arms.len() != 4 {
        return None;
    }

    let mut seen = Vec::new();
    let mut literal_results = Vec::new();
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

        if input == [false, false] {
            if !returns_none(&arm.expr()?) {
                return None;
            }
            continue;
        }

        let result = plain_string(&arm.expr()?)?;
        if literal_results.contains(&result) {
            return None;
        }
        literal_results.push(result.clone());

        let start = u32::from(arm.syntax().text_range().start()) as usize;
        let line = source
            .get(..start)?
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        if line == probe.location.line
            && arm_source_matches(&arm, after)?
            && arm_source_matches(&arm, &probe.expression)?
        {
            if selected.is_some() {
                return None;
            }
            selected = Some(DerivedWitness {
                input,
                result,
                terminal: false,
            });
        }
    }

    if ![[true, true], [true, false], [false, true], [false, false]]
        .iter()
        .all(|input| seen.contains(input))
    {
        return None;
    }

    let witness = selected?;
    if let Some(before) = probe.before.as_deref() {
        let old = single_arm(before)?;
        if old.guard().is_some()
            || old.attrs().next().is_some()
            || bool_pattern(&old.pat()?)? != witness.input
            || plain_string(&old.expr()?)? == witness.result
        {
            return None;
        }
    }
    Some(witness)
}

fn supported_request_membership(expression: &ast::Expr, closure_parameter: &str) -> bool {
    let text = compact(&expression.syntax().text().to_string());
    let receipt_lookup = format!(".get(&{closure_parameter}.id).is_some_and(");
    text.contains(&receipt_lookup)
        && text.contains(".iter().any(")
        && text.contains(".contains(")
        && !text.contains("!=")
        && !text.starts_with('!')
}

fn supported_task_identity(expression: &ast::Expr, closure_parameter: &str) -> bool {
    let text = compact(&expression.syntax().text().to_string());
    if text.contains("!=") || text.matches("==").count() != 1 {
        return false;
    }
    let Some((left, right)) = text.split_once("==") else {
        return false;
    };
    let receipt_id = format!("{closure_parameter}.id");
    (left == receipt_id && valid_identifier(right))
        || (right == receipt_id && valid_identifier(left))
}

fn returns_relation(expression: &ast::Expr, closure_parameter: &str, relation: &str) -> bool {
    let Some(call) = ast::CallExpr::cast(expression.syntax().clone()) else {
        return false;
    };
    let Some(callee) = call.expr() else {
        return false;
    };
    if direct_name(&callee).as_deref() != Some("Some") {
        return false;
    }
    let Some(argument_list) = call.arg_list() else {
        return false;
    };
    let arguments = argument_list.args().collect::<Vec<_>>();
    let [argument] = arguments.as_slice() else {
        return false;
    };
    let Some(tuple) = ast::TupleExpr::cast(argument.syntax().clone()) else {
        return false;
    };
    let fields = tuple
        .syntax()
        .children()
        .filter_map(ast::Expr::cast)
        .collect::<Vec<_>>();
    let [receipt, result] = fields.as_slice() else {
        return false;
    };
    direct_name(receipt).as_deref() == Some(closure_parameter)
        && direct_name(result).as_deref() == Some(relation)
}

fn returns_none(expression: &ast::Expr) -> bool {
    let Some(return_expression) = ast::ReturnExpr::cast(expression.syntax().clone()) else {
        return false;
    };
    return_expression
        .expr()
        .and_then(|value| direct_name(&value))
        .as_deref()
        == Some("None")
}

fn closure_parameter(closure: &ast::ClosureExpr) -> Option<String> {
    let text = closure.syntax().text().to_string();
    let rest = text.strip_prefix('|')?;
    let end = rest.find('|')?;
    let parameter = rest.get(..end)?.trim();
    valid_identifier(parameter).then(|| parameter.to_string())
}

fn immutable_binding_name(statement: &ast::LetStmt) -> Option<String> {
    let pattern = ast::IdentPat::cast(statement.pat()?.syntax().clone())?;
    let name = pattern.name()?.text().to_string();
    (pattern.syntax().text().to_string().trim() == name).then_some(name)
}

fn valid_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn direct_name(expression: &ast::Expr) -> Option<String> {
    let path = ast::PathExpr::cast(expression.syntax().clone())?.path()?;
    let text = path.syntax().text().to_string();
    valid_identifier(&text).then_some(text)
}

fn bool_pattern(pattern: &ast::Pat) -> Option<[bool; 2]> {
    let tuple = ast::TuplePat::cast(pattern.syntax().clone())?;
    let fields = tuple
        .syntax()
        .children()
        .filter_map(ast::Pat::cast)
        .collect::<Vec<_>>();
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

fn arm_text(text: &str) -> &str {
    text.trim().trim_end_matches(',').trim_end()
}

fn arm_source_matches(arm: &ast::MatchArm, claimed: &str) -> Option<bool> {
    let full = arm.syntax().text().to_string();
    let start = u32::from(arm.syntax().text_range().start());
    let end = u32::from(arm.fat_arrow_token()?.text_range().end());
    let boundary = end.checked_sub(start)? as usize;
    let pattern = full.get(..boundary)?;
    Some(arm_text(&full) == arm_text(claimed) || pattern.trim() == claimed.trim())
}

fn parsed(source: &str) -> Option<ast::SourceFile> {
    parse_clean_source_file(source).map(|parse| parse.tree())
}

fn named_function(root: &ast::SourceFile, name: &str) -> Option<ast::Fn> {
    let mut functions = root
        .syntax()
        .children()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.name().is_some_and(|value| value.text() == name));
    let function = functions.next()?;
    functions.next().is_none().then_some(function)
}

fn test_namespace_is_plain(root: &ast::SourceFile) -> bool {
    root.syntax().children().all(|node| {
        if ast::Fn::can_cast(node.kind()) {
            return true;
        }
        if let Some(import) = ast::Use::cast(node) {
            let text = import.syntax().text().to_string();
            return !text.contains('*') && !text.contains("assert_eq") && !text.contains(" as ");
        }
        false
    })
}

fn same_current_file(
    context: &ProbeContext<'_>,
    owner_file: &Path,
    probe_file: &Path,
    source: &str,
) -> bool {
    let Some(authority) = context.index.workspace_authority.as_ref() else {
        return false;
    };
    if !authority.validates_target(owner_file, owner_file, source) {
        return false;
    }
    let Ok(owner_path) = authority.root.join(owner_file).canonicalize() else {
        return false;
    };
    // Probe locations are `--root` as given joined with the changed path, so
    // they resolve from the working directory. Joining them onto the
    // canonical authority root again only works for an absolute or `.` root.
    let Ok(probe_path) = probe_file.canonicalize() else {
        return false;
    };
    owner_path == probe_path
}

fn assertion_namespace_is_standalone(owner_file: &Path, test_file: &Path) -> bool {
    if test_file == owner_file {
        return conventional_source_crate_root(test_file);
    }
    let Some(package_root) = conventional_package_root(owner_file) else {
        return false;
    };
    let relative_test = if package_root.as_os_str().is_empty() {
        test_file
    } else {
        let Ok(relative) = test_file.strip_prefix(&package_root) else {
            return false;
        };
        relative
    };
    let mut components = relative_test.components();
    let (Some(Component::Normal(directory)), Some(Component::Normal(file)), None) =
        (components.next(), components.next(), components.next())
    else {
        return false;
    };
    directory == "tests"
        && Path::new(file)
            .extension()
            .is_some_and(|extension| extension == "rs")
}

fn conventional_source_crate_root(file: &Path) -> bool {
    let Some(package_root) = conventional_package_root(file) else {
        return false;
    };
    let relative = if package_root.as_os_str().is_empty() {
        file
    } else {
        let Ok(relative) = file.strip_prefix(&package_root) else {
            return false;
        };
        relative
    };
    let mut components = relative.components();
    match (
        components.next(),
        components.next(),
        components.next(),
        components.next(),
    ) {
        (Some(Component::Normal(source)), Some(Component::Normal(file)), None, None) => {
            source == "src" && matches!(file.to_str(), Some("lib.rs" | "main.rs"))
        }
        (
            Some(Component::Normal(source)),
            Some(Component::Normal(bin)),
            Some(Component::Normal(file)),
            None,
        ) => {
            source == "src"
                && bin == "bin"
                && Path::new(file)
                    .extension()
                    .is_some_and(|extension| extension == "rs")
        }
        _ => false,
    }
}

fn conventional_package_root(owner_file: &Path) -> Option<PathBuf> {
    let components = owner_file.components().collect::<Vec<_>>();
    let source_index = components
        .iter()
        .rposition(|component| matches!(component, Component::Normal(name) if *name == "src"))?;
    if source_index + 1 >= components.len() {
        return None;
    }
    let mut root = PathBuf::new();
    for component in &components[..source_index] {
        let Component::Normal(name) = component else {
            return None;
        };
        root.push(name);
    }
    Some(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse `expression` as the initializer of a single `let` in a function.
    fn expr(expression: &str) -> Option<ast::Expr> {
        let source = format!("fn probe() {{ let value = {expression}; }}");
        let root = parsed(&source)?;
        let function = named_function(&root, "probe")?;
        let statement = function.body()?.stmt_list()?.statements().next()?;
        match statement {
            ast::Stmt::LetStmt(binding) => binding.initializer(),
            _ => None,
        }
    }

    fn standalone(owner: &str, test: &str) -> bool {
        assertion_namespace_is_standalone(Path::new(owner), Path::new(test))
    }

    #[test]
    fn identifiers_must_start_with_a_letter_or_underscore() {
        for accepted in ["receipt", "_receipt", "r2", "_"] {
            assert!(valid_identifier(accepted), "{accepted}");
        }
        for rejected in ["", "2r", "a-b", "a.b", "a b", "r#type", "é"] {
            assert!(!valid_identifier(rejected), "{rejected}");
        }
    }

    #[test]
    fn closure_parameter_requires_one_plain_identifier() {
        let parameter = |source: &str| {
            expr(source)
                .and_then(|value| ast::ClosureExpr::cast(value.syntax().clone()))
                .and_then(|closure| closure_parameter(&closure))
        };
        assert_eq!(parameter("|receipt| 1"), Some("receipt".to_string()));
        assert_eq!(parameter("| receipt | 1"), Some("receipt".to_string()));
        for source in [
            "|&receipt| 1",
            "|(a, b)| 1",
            "|a, b| 1",
            "|| 1",
            "|receipt: &R| 1",
        ] {
            assert_eq!(parameter(source), None, "{source}");
        }
    }

    #[test]
    fn task_identity_needs_exactly_one_positive_comparison_with_the_receipt_id() {
        let supported = |source: &str| {
            expr(source).is_some_and(|value| supported_task_identity(&value, "receipt"))
        };
        assert!(supported("receipt.id == task_id"));
        assert!(supported("task_id == receipt.id"));
        for source in [
            "receipt.id != task_id",
            "receipt.id == task_id && flag == other",
            "other.id == task_id",
            "receipt.id == task.id",
            "receipt.id",
        ] {
            assert!(expr(source).is_some(), "control must parse: {source}");
            assert!(!supported(source), "{source}");
        }
    }

    #[test]
    fn request_membership_needs_a_positive_receipt_lookup_over_the_request_set() {
        let supported = |source: &str| {
            expr(source).is_some_and(|value| supported_request_membership(&value, "receipt"))
        };
        assert!(supported(
            "requests.iter().any(|id| records.get(&receipt.id).is_some_and(|set| set.contains(id)))"
        ));
        for source in [
            // Negated, inequality, or lookup keyed by some other binding.
            "!requests.iter().any(|id| records.get(&receipt.id).is_some_and(|set| set.contains(id)))",
            "requests.iter().any(|id| records.get(&receipt.id).is_some_and(|set| set.contains(id) && id != other))",
            "requests.iter().any(|id| records.get(&other.id).is_some_and(|set| set.contains(id)))",
            "records.get(&receipt.id).is_some_and(|set| set.contains(id))",
        ] {
            assert!(expr(source).is_some(), "control must parse: {source}");
            assert!(!supported(source), "{source}");
        }
    }

    #[test]
    fn closure_tail_must_return_some_of_the_receipt_and_relation_pair() {
        let returns = |source: &str| {
            expr(source).is_some_and(|value| returns_relation(&value, "receipt", "relation"))
        };
        assert!(returns("Some((receipt, relation))"));
        for source in [
            "Some((relation, receipt))",
            "Some((receipt, other))",
            "Some((receipt, relation, extra))",
            "Some(receipt)",
            "Ok((receipt, relation))",
            "(receipt, relation)",
        ] {
            assert!(expr(source).is_some(), "control must parse: {source}");
            assert!(!returns(source), "{source}");
        }
    }

    #[test]
    fn none_arm_must_return_the_bare_none_path() {
        let none_arm = |source: &str| expr(source).is_some_and(|value| returns_none(&value));
        assert!(none_arm("return None"));
        for source in ["None", "return Some(1)", "return", "return other::None"] {
            assert!(!none_arm(source), "{source}");
        }
    }

    #[test]
    fn tuple_patterns_are_exactly_two_boolean_literals() {
        let pattern = |source: &str| {
            let text = format!("fn probe() {{ match (a, b) {{ {source} => 1, _ => 2 }} }}");
            let root = parsed(&text)?;
            let function = named_function(&root, "probe")?;
            let tail = function.body()?.stmt_list()?.tail_expr()?;
            let arm = ast::MatchExpr::cast(tail.syntax().clone())?
                .match_arm_list()?
                .arms()
                .next()?;
            bool_pattern(&arm.pat()?)
        };
        assert_eq!(pattern("(true, false)"), Some([true, false]));
        assert_eq!(pattern("(false, true)"), Some([false, true]));
        for source in [
            "(true, _)",
            "(true,)",
            "(true, false, true)",
            "true",
            "(1, 0)",
        ] {
            assert_eq!(pattern(source), None, "{source}");
        }
    }

    #[test]
    fn plain_strings_exclude_escapes_quotes_and_raw_or_non_string_literals() {
        let plain = |source: &str| expr(source).and_then(|value| plain_string(&value));
        assert_eq!(
            plain(r#""request_identity_v2""#),
            Some("request_identity_v2".to_string())
        );
        assert_eq!(plain(r#""""#), Some(String::new()));
        for source in [r#""a\nb""#, r##"r#"raw"#"##, "1", "true", "name"] {
            assert!(expr(source).is_some(), "control must parse: {source}");
            assert_eq!(plain(source), None, "{source}");
        }
    }

    #[test]
    fn derived_arm_text_matches_with_or_without_trailing_comma_or_by_pattern() {
        let selected = single_arm(r#"(true, false) => "join","#);
        assert!(selected.is_some());
        let matches = |claimed: &str| {
            selected
                .as_ref()
                .and_then(|value| arm_source_matches(value, claimed))
        };
        assert_eq!(matches(r#"(true, false) => "join""#), Some(true));
        assert_eq!(matches(r#"  (true, false) => "join" , "#), Some(true));
        // The pattern prefix alone is an accepted claim; a different arm is not.
        assert_eq!(matches("(true, false) =>"), Some(true));
        assert_eq!(matches(r#"(true, false) => "other""#), Some(false));
        assert_eq!(matches(r#"(false, true) => "join""#), Some(false));
        // `single_arm` rejects zero or several arms.
        assert!(single_arm("").is_none());
        assert!(single_arm(r#"(true, true) => "a", (true, false) => "b""#).is_none());
    }

    #[test]
    fn test_namespace_rejects_glob_alias_and_foreign_items() {
        let plain =
            |source: &str| parsed(source).is_some_and(|root| test_namespace_is_plain(&root));
        assert!(plain("use crate::terminalize_proof; #[test] fn t() {}"));
        for source in [
            "use crate::*; fn t() {}",
            "use crate::terminalize_proof as other; fn t() {}",
            "use std::assert_eq; fn t() {}",
            "macro_rules! m { () => {}; } fn t() {}",
            "struct Receipt; fn t() {}",
            "const X: u8 = 1; fn t() {}",
        ] {
            assert!(parsed(source).is_some(), "control must parse: {source}");
            assert!(!plain(source), "{source}");
        }
    }

    #[test]
    fn package_root_is_everything_before_the_last_src_component() {
        let root = |path: &str| conventional_package_root(Path::new(path));
        assert_eq!(
            root("crates/ripr/src/lib.rs"),
            Some(PathBuf::from("crates/ripr"))
        );
        assert_eq!(root("src/lib.rs"), Some(PathBuf::new()));
        // The last `src` wins, and a trailing `src` has no file under it.
        assert_eq!(root("a/src/b/src/x.rs"), Some(PathBuf::from("a/src/b")));
        assert_eq!(root("a/src"), None);
        assert_eq!(root("a/lib.rs"), None);
        assert_eq!(root("../a/src/lib.rs"), None);
        assert_eq!(root("/abs/src/lib.rs"), None);
    }

    #[test]
    fn crate_root_files_are_lib_main_or_direct_bin_sources_only() {
        let root = |path: &str| conventional_source_crate_root(Path::new(path));
        for path in [
            "src/lib.rs",
            "src/main.rs",
            "src/bin/tool.rs",
            "crates/ripr/src/lib.rs",
            "crates/ripr/src/bin/tool.rs",
        ] {
            assert!(root(path), "{path}");
        }
        for path in [
            "src/other.rs",
            "src/bin/tool.txt",
            "src/bin/nested/tool.rs",
            "src/nested/lib.rs",
            "lib.rs",
            "crates/ripr/lib.rs",
        ] {
            assert!(!root(path), "{path}");
        }
    }

    #[test]
    fn assertions_stay_in_the_owner_crate_root_or_its_flat_tests_directory() {
        // Same-file tests only count when the file is itself a crate root.
        assert!(standalone(
            "crates/ripr/src/lib.rs",
            "crates/ripr/src/lib.rs"
        ));
        assert!(!standalone(
            "crates/ripr/src/inner.rs",
            "crates/ripr/src/inner.rs"
        ));
        // Flat `tests/*.rs` of the same package are standalone.
        assert!(standalone(
            "crates/ripr/src/inner.rs",
            "crates/ripr/tests/derived.rs"
        ));
        assert!(standalone("src/inner.rs", "tests/derived.rs"));
        for test in [
            "crates/ripr/tests/nested/derived.rs",
            "crates/ripr/tests/derived.txt",
            "crates/ripr/tests",
            "crates/other/tests/derived.rs",
            "crates/ripr/src/derived.rs",
            "crates/ripr/benches/derived.rs",
        ] {
            assert!(!standalone("crates/ripr/src/inner.rs", test), "{test}");
        }
        // An owner outside any `src` directory has no package to anchor to.
        assert!(!standalone("scripts/inner.rs", "tests/derived.rs"));
    }
}
