//! A bounded, assertion-local path through a hand-written trait recorder.
//! Reach, oracle strength and execution/macro admission remain with their owners.
use super::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::classify::{OwnerPinSyntax, reveal::assertion_comparison_operands};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{OracleKind, Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasName},
};
use std::collections::BTreeMap;

pub(in crate::analysis) struct RecordingDouble {
    owner: String,
    file: std::path::PathBuf,
    argument: usize,
    doubles: BTreeMap<String, String>,
}
fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
fn text(node: &impl AstNode) -> String {
    compact(&node.syntax().text().to_string())
}
fn ident(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with(|c: char| c.is_ascii_digit())
        && value.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
}
fn function(body: &str) -> Option<ast::Fn> {
    parse_clean_source_file(body)?
        .tree()
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
}
fn statements(function: &ast::Fn) -> Option<Vec<ast::Stmt>> {
    let list = function.body()?.stmt_list()?;
    if list.tail_expr().is_some() {
        return None;
    }
    Some(list.statements().collect())
}
fn method(statement: &ast::Stmt) -> Option<ast::MethodCallExpr> {
    let ast::Stmt::ExprStmt(statement) = statement else {
        return None;
    };
    let ast::Expr::MethodCallExpr(call) = statement.expr()? else {
        return None;
    };
    Some(call)
}
fn simple_signature(function: &ast::Fn) -> bool {
    function.attrs().next().is_none()
        && function.async_token().is_none()
        && function.ret_type().is_none()
        && !function.syntax().children().any(|n| {
            matches!(
                n.kind(),
                ra_ap_syntax::SyntaxKind::GENERIC_PARAM_LIST
                    | ra_ap_syntax::SyntaxKind::WHERE_CLAUSE
            )
        })
}
fn root(node: &SyntaxNode) -> bool {
    node.parent()
        .is_some_and(|p| p.kind() == ra_ap_syntax::SyntaxKind::SOURCE_FILE)
}
impl RecordingDouble {
    pub(super) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
        complete: bool,
    ) -> Option<Self> {
        if !complete
            || !matches!(
                probe.family,
                ProbeFamily::CallDeletion | ProbeFamily::SideEffect
            )
            || !owner.body.contains("&mut dyn ")
            || owner.item.has_self_param
        {
            return None;
        }
        let declaration = function(&owner.body)?;
        if !simple_signature(&declaration) {
            return None;
        }
        // The changed call itself must be an unconditional top-level statement.
        let owner_statements = statements(&declaration)?;
        let changed = function(&format!(
            "fn __changed() {{ {}; }}",
            probe.expression.trim_end_matches(';')
        ))?;
        let changed_statements = statements(&changed)?;
        let [changed_statement] = changed_statements.as_slice() else {
            return None;
        };
        let changed_call = method(changed_statement)?;
        if changed_call.arg_list()?.args().count() != 1 {
            return None;
        }
        let (at, call) = owner_statements.iter().enumerate().find_map(|(at, s)| {
            let call = method(s)?;
            let offset = usize::from(call.syntax().text_range().start());
            (owner.start_line + owner.body.get(..offset)?.matches('\n').count()
                == probe.location.line
                && call
                    .receiver()
                    .zip(changed_call.receiver())
                    .is_some_and(|(a, b)| text(&a) == text(&b))
                && call
                    .name_ref()
                    .zip(changed_call.name_ref())
                    .is_some_and(|(a, b)| a.text() == b.text()))
            .then_some((at, call))
        })?;
        if call.generic_arg_list().is_some() || call.arg_list()?.args().count() != 1 {
            return None;
        }
        let sink = text(&call.receiver()?);
        if !ident(&sink) {
            return None;
        }
        let method_name = call.name_ref()?.text().to_string();
        let body = declaration.body()?;
        let masked = mask_comments_and_strings(&body.syntax().text().to_string());
        let sink_mentions = masked
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|v| *v == sink)
            .count();
        let sink_calls = body
            .syntax()
            .descendants()
            .filter_map(ast::MethodCallExpr::cast)
            .filter(|c| {
                c.receiver().is_some_and(|r| text(&r) == sink)
                    && c.name_ref().is_some_and(|n| n.text() == method_name)
            })
            .count();
        if sink_mentions != sink_calls
            || body.syntax().descendants().any(|n| {
                matches!(
                    n.kind(),
                    ra_ap_syntax::SyntaxKind::CALL_EXPR
                        | ra_ap_syntax::SyntaxKind::LOOP_EXPR
                        | ra_ap_syntax::SyntaxKind::WHILE_EXPR
                        | ra_ap_syntax::SyntaxKind::FOR_EXPR
                        | ra_ap_syntax::SyntaxKind::CLOSURE_EXPR
                        | ra_ap_syntax::SyntaxKind::AWAIT_EXPR
                ) || ast::MacroCall::cast(n)
                    .is_some_and(|m| m.path().is_none_or(|p| text(&p) != "format"))
            })
        {
            return None;
        }
        for parameter in declaration.param_list()?.params() {
            let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
                return None;
            };
            if pattern.name()?.text() != sink {
                let ty = text(&parameter.ty()?);
                if !matches!(
                    ty.as_str(),
                    "u8" | "u16"
                        | "u32"
                        | "u64"
                        | "usize"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "isize"
                        | "&[u32]"
                        | "&[u64]"
                ) {
                    return None;
                }
            }
        }
        let (argument, trait_name) =
            declaration
                .param_list()?
                .params()
                .enumerate()
                .find_map(|(at, parameter)| {
                    let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
                        return None;
                    };
                    if pattern.name()?.text() != sink
                        || pattern.ref_token().is_some()
                        || pattern.pat().is_some()
                    {
                        return None;
                    }
                    let ty = text(&parameter.ty()?);
                    let trait_name = ty.strip_prefix("&mutdyn")?;
                    ident(trait_name).then_some((at, trait_name.to_string()))
                })?;
        // No earlier branch, opaque call or sink hand-off can skip the write.
        for statement in &owner_statements[..at] {
            let ast::Stmt::LetStmt(binding) = statement else {
                return None;
            };
            let ast::Pat::IdentPat(pattern) = binding.pat()? else {
                return None;
            };
            if pattern.name()?.text() == sink {
                return None;
            }
            let initializer = binding.initializer()?;
            let value = text(&initializer);
            if value.contains(&sink)
                || initializer.syntax().descendants().any(|n| {
                    matches!(
                        n.kind(),
                        ra_ap_syntax::SyntaxKind::IF_EXPR
                            | ra_ap_syntax::SyntaxKind::MATCH_EXPR
                            | ra_ap_syntax::SyntaxKind::LOOP_EXPR
                            | ra_ap_syntax::SyntaxKind::FOR_EXPR
                            | ra_ap_syntax::SyntaxKind::WHILE_EXPR
                            | ra_ap_syntax::SyntaxKind::CLOSURE_EXPR
                            | ra_ap_syntax::SyntaxKind::CALL_EXPR
                            | ra_ap_syntax::SyntaxKind::MACRO_CALL
                            | ra_ap_syntax::SyntaxKind::TRY_EXPR
                            | ra_ap_syntax::SyntaxKind::AWAIT_EXPR
                    )
                })
            {
                return None;
            }
            // The retained corpus sums a primitive slice before sending it.
            for call in initializer
                .syntax()
                .descendants()
                .filter_map(ast::MethodCallExpr::cast)
            {
                if !matches!(call.name_ref()?.text(), "iter" | "sum")
                    || call.arg_list()?.args().next().is_some()
                {
                    return None;
                }
            }
        }
        // Resolve the root trait and same-file doubles, including an inline test module.
        // Imports, expansion and standard-type/read-method lookalikes refuse.
        let facts = index.files().get(&owner.file)?;
        if facts.used_lexical_fallback {
            return None;
        }
        let tree = parse_clean_source_file(&facts.source)?.tree();
        if tree.syntax().descendants().any(|node| {
            ast::ExternCrate::cast(node.clone()).is_some()
                || ast::Attr::cast(node.clone())
                    .is_some_and(|a| !matches!(text(&a).as_str(), "#[cfg(test)]" | "#[test]"))
                || ast::MacroCall::cast(node.clone())
                    .is_some_and(|m| !m.syntax().ancestors().any(|n| ast::Fn::cast(n).is_some()))
                || ast::MacroRules::cast(node.clone()).is_some()
                || ast::MacroDef::cast(node.clone()).is_some()
                || ast::TypeAlias::cast(node.clone()).is_some()
                || ast::Fn::cast(node.clone()).is_some_and(|f| {
                    f.name()
                        .is_some_and(|n| matches!(n.text(), "clone" | "to_string"))
                })
                || ast::Use::cast(node.clone()).is_some_and(|u| {
                    !u.syntax()
                        .ancestors()
                        .any(|n| ast::Module::cast(n).is_some())
                        || (text(&u) != "usesuper::*;" && text(&u) != "useself::*;")
                })
                || node
                    .children()
                    .filter_map(ast::Name::cast)
                    .any(|n| matches!(n.text(), "Vec" | "String"))
        }) {
            return None;
        }
        let sources = [(owner.file.clone(), tree)];
        let (_, tree) = sources.iter().find(|(path, _)| path == &owner.file)?;
        let owners: Vec<_> = index
            .functions()
            .iter()
            .filter(|f| f.name == owner.name)
            .collect();
        if owners.len() != 1 {
            return None;
        }
        let traits: Vec<_> = sources
            .iter()
            .flat_map(|(_, t)| t.syntax().descendants().filter_map(ast::Trait::cast))
            .filter(|t| t.name().is_some_and(|n| n.text() == trait_name))
            .collect();
        let [trait_item] = traits.as_slice() else {
            return None;
        };
        if !root(trait_item.syntax())
            || trait_item.attrs().next().is_some()
            || !tree
                .syntax()
                .descendants()
                .any(|n| n == *trait_item.syntax())
        {
            return None;
        }
        let mut doubles = BTreeMap::new();
        for implementation in tree.syntax().descendants().filter_map(ast::Impl::cast) {
            if implementation.attrs().next().is_some()
                || implementation
                    .trait_()
                    .is_none_or(|t| text(&t) != trait_name)
            {
                continue;
            }
            let Some(ty) = implementation
                .self_ty()
                .map(|t| text(&t))
                .filter(|t| ident(t))
            else {
                continue;
            };
            let Some(items) = implementation.assoc_item_list() else {
                continue;
            };
            let functions: Vec<_> = items
                .syntax()
                .children()
                .filter_map(ast::Fn::cast)
                .collect();
            let [writer] = functions.as_slice() else {
                continue;
            };
            if writer.name().is_none_or(|n| n.text() != method_name) || !simple_signature(writer) {
                continue;
            }
            let Some(parameters) = writer.param_list() else {
                continue;
            };
            if parameters
                .self_param()
                .is_none_or(|p| text(&p) != "&mutself")
            {
                continue;
            }
            let parameters: Vec<_> = parameters.params().collect();
            let [parameter] = parameters.as_slice() else {
                continue;
            };
            if parameter.ty().is_none_or(|t| text(&t) != "String") {
                continue;
            }
            let Some(ast::Pat::IdentPat(pattern)) = parameter.pat() else {
                continue;
            };
            if pattern.ref_token().is_some()
                || pattern.mut_token().is_some()
                || pattern.pat().is_some()
            {
                continue;
            }
            let Some(parameter) = pattern.name().map(|n| n.text().to_string()) else {
                continue;
            };
            let Some(body) = statements(writer) else {
                continue;
            };
            let [statement] = body.as_slice() else {
                continue;
            };
            let Some(push) = method(statement) else {
                continue;
            };
            if push.name_ref().is_none_or(|n| n.text() != "push")
                || push.generic_arg_list().is_some()
            {
                continue;
            }
            let Some(ast::Expr::FieldExpr(field)) = push.receiver() else {
                continue;
            };
            if field.expr().is_none_or(|e| text(&e) != "self") {
                continue;
            }
            let Some(field) = field.name_ref().map(|n| n.text().to_string()) else {
                continue;
            };
            let Some(arguments) = push.arg_list() else {
                continue;
            };
            let arguments: Vec<_> = arguments.args().collect();
            if arguments.len() != 1 || text(arguments.first()?) != parameter {
                continue;
            }
            let definitions: Vec<_> = sources
                .iter()
                .flat_map(|(_, t)| t.syntax().descendants().filter_map(ast::Struct::cast))
                .filter(|s| s.name().is_some_and(|n| n.text() == ty))
                .collect();
            let [definition] = definitions.as_slice() else {
                continue;
            };
            if definition.syntax().parent() != implementation.syntax().parent()
                || definition.attrs().next().is_some()
                || definition
                    .syntax()
                    .ancestors()
                    .filter_map(ast::Module::cast)
                    .any(|m| m.attrs().any(|a| text(&a) != "#[cfg(test)]"))
                || !tree
                    .syntax()
                    .descendants()
                    .any(|n| n == *definition.syntax())
            {
                continue;
            }
            let Some(ast::FieldList::RecordFieldList(fields)) = definition.field_list() else {
                continue;
            };
            if !fields.fields().any(|f| {
                f.name().is_some_and(|n| n.text() == field)
                    && f.ty().is_some_and(|t| text(&t) == "Vec<String>")
            }) {
                continue;
            }
            // Competing inherent/trait methods or another impl cannot supply this push.
            let implementations = sources
                .iter()
                .flat_map(|(_, t)| t.syntax().descendants().filter_map(ast::Impl::cast))
                .filter(|i| i.self_ty().is_some_and(|t| text(&t) == ty))
                .count();
            if implementations != 1 {
                continue;
            }
            doubles.insert(ty, field);
        }
        (!doubles.is_empty()).then_some(Self {
            owner: owner.name.clone(),
            file: owner.file.clone(),
            argument,
            doubles,
        })
    }

    pub(in crate::analysis) fn admits(
        &self,
        test: &TestSummary,
        assertion: &OracleFact,
        index: &RustIndex,
        syntax: &OwnerPinSyntax,
    ) -> bool {
        if test.file != self.file
            || !matches!(
                assertion.kind,
                OracleKind::ExactValue | OracleKind::WholeObjectEquality
            )
            || !syntax.admits(test, assertion, index)
            || !["assert_eq", "vec", "format"].into_iter().all(|name| {
                syntax.targeted_macro_binding_is_unambiguous(test, assertion.line, name, index)
            })
        {
            return false;
        }
        self.admits_inner(test, assertion).is_some()
    }
    fn admits_inner(&self, test: &TestSummary, assertion: &OracleFact) -> Option<()> {
        let function = function(&test.body)?;
        let statements = statements(&function)?;
        let relative = assertion.line.checked_sub(test.start_line)?;
        let at = statements.iter().position(|s| {
            let ast::Stmt::ExprStmt(s) = s else {
                return false;
            };
            let Some(ast::Expr::MacroExpr(m)) = s.expr() else {
                return false;
            };
            let Some(call) = m.macro_call() else {
                return false;
            };
            let offset = usize::from(call.syntax().text_range().start());
            test.body
                .get(..offset)
                .is_some_and(|p| p.matches('\n').count() == relative)
                && text(&call) == compact(assertion.text.trim_end_matches(';'))
        })?;
        if at != 2 {
            return None;
        }
        let ast::Stmt::ExprStmt(statement) = statements.get(at.checked_sub(1)?)? else {
            return None;
        };
        let ast::Expr::CallExpr(call) = statement.expr()? else {
            return None;
        };
        if text(&call.expr()?) != self.owner {
            return None;
        }
        let arguments: Vec<_> = call.arg_list()?.args().collect();
        for (at, argument) in arguments.iter().enumerate() {
            if at != self.argument {
                let value = text(argument);
                let value = value
                    .strip_prefix("&[")
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or(&value);
                if !value
                    .split(',')
                    .all(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
                {
                    return None;
                }
            }
        }
        let argument = text(arguments.get(self.argument)?);
        let local = argument.strip_prefix("&mut")?;
        if !ident(local) {
            return None;
        }
        let ast::Stmt::LetStmt(binding) = statements.get(at.checked_sub(2)?)? else {
            return None;
        };
        let ast::Pat::IdentPat(pattern) = binding.pat()? else {
            return None;
        };
        if pattern.name()?.text() != local
            || pattern.mut_token().is_none()
            || pattern.ref_token().is_some()
            || pattern.pat().is_some()
            || binding.ty().is_some()
        {
            return None;
        }
        let ast::Expr::RecordExpr(record) = binding.initializer()? else {
            return None;
        };
        let field = self.doubles.get(&text(&record.path()?))?;
        let fields = record.record_expr_field_list()?;
        if fields.spread().is_some()
            || !fields
                .fields()
                .all(|f| f.expr().is_some_and(|e| text(&e) == "vec![]"))
        {
            return None;
        }
        let [left, right] = assertion_comparison_operands(&assertion.text)?;
        let expected = match (compact(left), compact(right)) {
            (left, right)
                if left == format!("{local}.{field}")
                    || left == format!("{local}.{field}.clone()") =>
            {
                right
            }
            (left, right)
                if right == format!("{local}.{field}")
                    || right == format!("{local}.{field}.clone()") =>
            {
                left
            }
            _ => return None,
        };
        // Independent nonempty literal strings only; count/len, self-computed
        // expectations and user equality/Display implementations grant no credit.
        let values = expected.strip_prefix("vec![")?.strip_suffix(']')?;
        if values.is_empty()
            || !values.split(',').all(|value| {
                value
                    .strip_suffix(".to_string()")
                    .and_then(|v| v.strip_prefix('"'))
                    .and_then(|v| v.strip_suffix('"'))
                    .is_some_and(|v| !v.contains(['"', '\\', '{', '}']))
            })
        {
            return None;
        }
        Some(())
    }
}
