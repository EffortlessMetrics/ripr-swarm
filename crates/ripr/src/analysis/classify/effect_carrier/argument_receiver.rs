//! Assertion-local identity for a concrete effect receiver passed to a free owner.
//! This establishes the call-site binding, not strength or runtime adequacy.
use super::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::reveal::assertion_comparison_operands;
use crate::analysis::classify::{
    OwnerPinSyntax, impl_self_type_name, targeted_free_call_reaches_owner,
};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasName},
};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::analysis) struct ArgumentReceiver {
    parameter: usize,
    fields: BTreeSet<String>,
    receiver_type: String,
    projection_imports_closed: bool,
}
fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
fn bare_path(expr: ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(expr) = expr else {
        return None;
    };
    let path = expr.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    Some(path.segment()?.name_ref()?.text().to_string())
}
fn parse_function(source: &str) -> Option<ast::Fn> {
    parse_clean_source_file(source)?
        .tree()
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
}
fn expression(statement: &ast::Stmt) -> Option<ast::Expr> {
    let ast::Stmt::ExprStmt(statement) = statement else {
        return None;
    };
    statement.expr()
}
fn mentions(node: &ra_ap_syntax::SyntaxNode, binding: &str) -> bool {
    node.descendants().any(|node| {
        ast::IdentPat::cast(node.clone())
            .and_then(|pattern| pattern.name())
            .is_some_and(|name| name.text() == binding)
            || ast::PathExpr::cast(node).is_some_and(|path| {
                bare_path(ast::Expr::PathExpr(path)).as_deref() == Some(binding)
            })
    })
}
// A trait on Ref<Vec<_>> can intercept first/last before dereferencing to
// the slice, even when its method declaration is outside the index. The
// workspace import closure is deliberately conservative: an unread external
// import (including a re-export in another indexed file) withholds this
// projection witness. Ordinary field identity and closed standard imports
// keep their existing admission.
fn projection_imports_closed(index: &RustIndex) -> bool {
    // MemberCrates admits only a witnessed library target at src/lib.rs.
    // A globally familiar crate name is not its binding in this importer:
    // Cargo can bind the package's own name, or even `alloc`, to an outside
    // dependency. Resolve each nonstandard root through manifest authority.
    // The value-equality witness separately certifies the std/core roots.
    let library_roots: Vec<_> = index
        .files()
        .iter()
        .map(|(path, _)| path)
        .filter(|path| path.ends_with("src/lib.rs"))
        .collect();
    index.files().iter().all(|(file, facts)| {
        let known = |root: &str| {
            matches!(root, "std" | "core" | "crate" | "self" | "super")
                || library_roots.iter().any(|library| {
                    index
                        .member_crates
                        .import_names(file, library)
                        .iter()
                        .any(|bound| bound == root)
                })
        };
        if crate::analysis::classify::reveal::flattened_use_paths(&facts.source)
            .iter()
            .any(|import| {
                !known(
                    import
                        .path
                        .trim_start_matches("::")
                        .split("::")
                        .next()
                        .unwrap_or_default(),
                )
            })
        {
            return false;
        }
        if !facts.source.contains("extern") {
            return true;
        }
        let Some(file) = parse_clean_source_file(&facts.source) else {
            return false;
        };
        file.tree()
            .syntax()
            .descendants()
            .filter_map(ast::ExternCrate::cast)
            .all(|item| item.name_ref().is_some_and(|name| known(name.text())))
    })
}
fn field_root(
    mut expression: ast::Expr,
    index: &RustIndex,
    closed_imports: bool,
) -> Option<(String, String, bool)> {
    let mut projected = false;
    for _ in 0..super::MAX_DEPTH {
        expression = match expression {
            ast::Expr::PrefixExpr(prefix) => prefix.expr()?,
            ast::Expr::RefExpr(reference) => reference.expr()?,
            ast::Expr::ParenExpr(paren) => paren.expr()?,
            ast::Expr::MethodCallExpr(call)
                if call.name_ref()?.text() == "map"
                    && call
                        .arg_list()?
                        .args()
                        .map(|arg| compact(&arg.syntax().text().to_string()))
                        .collect::<Vec<_>>()
                        == ["String::as_str"]
                    && !crate::analysis::classify::owner_pin::wrapper_may_be_rebound(
                        "String", index,
                    ) =>
            {
                projected = true;
                call.receiver()?
            }
            ast::Expr::MethodCallExpr(call)
                if closed_imports
                    && matches!(call.name_ref()?.text(), "first" | "last")
                    && call.arg_list()?.args().next().is_none()
                    && !crate::analysis::classify::owner_pin::wrapper_may_be_rebound(
                        call.name_ref()?.text(),
                        index,
                    ) =>
            {
                projected = true;
                call.receiver()?
            }
            ast::Expr::MethodCallExpr(call)
                if call.arg_list()?.args().next().is_none()
                    && call.name_ref().is_some_and(|name| {
                        matches!(
                            name.text(),
                            "borrow" | "as_ref" | "as_slice" | "as_str" | "clone"
                        )
                    }) =>
            {
                call.receiver()?
            }
            ast::Expr::FieldExpr(field) => {
                return Some((
                    bare_path(field.expr()?)?,
                    field.name_ref()?.text().to_string(),
                    projected,
                ));
            }
            _ => return None,
        };
    }
    None
}
// Payload identity is a narrower question than the carrier's may-write set.
// Bound only direct parameter stores and collection writes, with no hidden
// initializer, parameter rebinding, or later unrelated access to that field.
fn method_expression(source: &str) -> Option<ast::MethodCallExpr> {
    let function = parse_function(&format!("fn probe() {{ {}; }}", source))?;
    let statements: Vec<_> = function.body()?.stmt_list()?.statements().collect();
    let [statement] = statements.as_slice() else {
        return None;
    };
    let ast::Expr::MethodCallExpr(call) = expression(statement)? else {
        return None;
    };
    Some(call)
}
fn changed_parameters(
    before: &str,
    after: &ast::MethodCallExpr,
    callee: &FunctionSummary,
) -> Option<BTreeSet<String>> {
    let before = method_expression(before)?;
    if before.name_ref()?.text() != after.name_ref()?.text()
        || compact(&before.receiver()?.syntax().text().to_string())
            != compact(&after.receiver()?.syntax().text().to_string())
    {
        return None;
    }
    let previous: Vec<_> = before.arg_list()?.args().collect();
    let current: Vec<_> = after.arg_list()?.args().collect();
    let function = parse_function(&callee.body)?;
    let parameters: Vec<_> = function.param_list()?.params().collect();
    if current.len() != previous.len() || current.len() != parameters.len() {
        return None;
    }
    let mut changed = BTreeSet::new();
    for (at, (old, new)) in previous.iter().zip(&current).enumerate() {
        if crate::analysis::classify::propagation_witness::normalize_semantic_text(
            &old.syntax().text().to_string(),
        ) == crate::analysis::classify::propagation_witness::normalize_semantic_text(
            &new.syntax().text().to_string(),
        ) {
            continue;
        }
        let ast::Pat::IdentPat(pattern) = parameters[at].pat()? else {
            return None;
        };
        let name = pattern.name()?.text().to_string();
        if crate::analysis::classify::parameter_rebound(&callee.body, &name) {
            return None;
        }
        changed.insert(name);
    }
    (!changed.is_empty()).then_some(changed)
}
// One standard format string with named primitive captures only. No Display
// implementation, explicit argument expression or hidden macro executes here.
fn primitive_format_captures(
    call: &ast::MacroCall,
    primitive: &BTreeSet<String>,
) -> Option<BTreeSet<String>> {
    if call.path()?.syntax().text().to_string() != "format" {
        return None;
    }
    let tokens = call.token_tree()?.syntax().text().to_string();
    let inner = tokens.strip_prefix('(')?.strip_suffix(')')?;
    let parsed = parse_function(&format!("fn args() {{ argument({inner}) }}"))?;
    let ast::Expr::CallExpr(arguments) = parsed.body()?.stmt_list()?.tail_expr()? else {
        return None;
    };
    let values: Vec<_> = arguments.arg_list()?.args().collect();
    let [ast::Expr::Literal(literal)] = values.as_slice() else {
        return None;
    };
    let ast::LiteralKind::String(literal) = literal.kind() else {
        return None;
    };
    let template = literal.value().ok()?;
    let chars: Vec<_> = template.chars().collect();
    let mut at = 0;
    let mut captures = BTreeSet::new();
    while at < chars.len() {
        match chars[at] {
            '{' if chars.get(at + 1) == Some(&'{') => at += 2,
            '}' if chars.get(at + 1) == Some(&'}') => at += 2,
            '{' => {
                let start = at + 1;
                let end = start + chars.get(start..)?.iter().position(|c| *c == '}')?;
                let name: String = chars[start..end].iter().collect();
                if !primitive.contains(&name) {
                    return None;
                }
                captures.insert(name);
                at = end + 1;
            }
            '}' => return None,
            _ => at += 1,
        }
    }
    Some(captures)
}
fn parameter_value(
    mut expression: ast::Expr,
    parameters: &BTreeSet<String>,
    primitive_format: &BTreeSet<String>,
) -> bool {
    for _ in 0..super::MAX_DEPTH {
        let next = match expression {
            ast::Expr::MacroExpr(expression) => {
                return expression
                    .macro_call()
                    .and_then(|call| primitive_format_captures(&call, primitive_format))
                    .is_some_and(|captures| captures.iter().any(|name| parameters.contains(name)));
            }
            ast::Expr::PathExpr(path) => {
                return bare_path(ast::Expr::PathExpr(path))
                    .is_some_and(|name| parameters.contains(&name));
            }
            ast::Expr::ParenExpr(paren) => paren.expr(),
            ast::Expr::RefExpr(reference) if reference.mut_token().is_none() => reference.expr(),
            ast::Expr::MethodCallExpr(call)
                if call.name_ref().is_some_and(|name| {
                    matches!(name.text(), "to_string" | "to_owned" | "clone")
                }) && call.generic_arg_list().is_none()
                    && call
                        .arg_list()
                        .is_some_and(|args| args.args().next().is_none()) =>
            {
                call.receiver()
            }
            _ => return false,
        };
        let Some(next) = next else { return false };
        expression = next;
    }
    false
}
fn storage_field(mut expression: ast::Expr) -> Option<String> {
    for _ in 0..super::MAX_DEPTH {
        expression = match expression {
            ast::Expr::MethodCallExpr(call)
                if call.name_ref()?.text() == "borrow_mut"
                    && call.arg_list()?.args().next().is_none() =>
            {
                call.receiver()?
            }
            ast::Expr::FieldExpr(field) if bare_path(field.expr()?).as_deref() == Some("self") => {
                return Some(field.name_ref()?.text().to_string());
            }
            _ => return None,
        };
    }
    None
}
fn parameter_write(
    expression: ast::Expr,
    parameters: &BTreeSet<String>,
    primitive_format: &BTreeSet<String>,
) -> Option<String> {
    if expression.attrs().next().is_some() {
        return None;
    }
    match expression {
        ast::Expr::BinExpr(assignment)
            if assignment.op_kind() == Some(ast::BinaryOp::Assignment { op: None })
                && parameter_value(assignment.rhs()?, parameters, primitive_format) =>
        {
            storage_field(assignment.lhs()?)
        }
        ast::Expr::MethodCallExpr(call) => {
            let name = call.name_ref()?;
            if !matches!(
                name.text(),
                "push" | "push_back" | "push_front" | "push_str" | "set"
            ) || call.generic_arg_list().is_some()
            {
                return None;
            }
            let arity = 1;
            let arguments: Vec<_> = call.arg_list()?.args().collect();
            if arguments.len() != arity
                || !arguments
                    .into_iter()
                    .any(|arg| parameter_value(arg, parameters, primitive_format))
            {
                return None;
            }
            storage_field(call.receiver()?)
        }
        _ => None,
    }
}
fn parameter_fields(
    callee: &FunctionSummary,
    parameters: &BTreeSet<String>,
    index: &RustIndex,
) -> Option<BTreeSet<String>> {
    let function = parse_function(&callee.body)?;
    let body = function.body()?;
    let standard_format =
        !crate::analysis::classify::activation::macro_names_rebound(index, &["format"]);
    let primitive_format: BTreeSet<_> = if !standard_format {
        BTreeSet::new()
    } else {
        function
            .param_list()?
            .params()
            .filter_map(|parameter| {
                let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
                    return None;
                };
                let ty = compact(&parameter.ty()?.syntax().text().to_string());
                let ty = ty.strip_prefix('&').unwrap_or(&ty);
                if !matches!(
                    ty,
                    "str"
                        | "bool"
                        | "char"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                        | "f32"
                        | "f64"
                ) || crate::analysis::classify::owner_pin::wrapper_may_be_rebound(ty, index)
                {
                    return None;
                }
                Some(pattern.name()?.text().to_string())
            })
            .collect()
    };
    if function.attrs().next().is_some()
        || body.syntax().descendants().any(|node| {
            ast::CallExpr::can_cast(node.kind())
                || ast::MacroCall::cast(node).is_some_and(|call| {
                    !standard_format
                        || primitive_format_captures(&call, &primitive_format).is_none()
                })
        })
    {
        return None;
    }
    let mut mentions = BTreeMap::<String, usize>::new();
    for field in body.syntax().descendants().filter_map(ast::FieldExpr::cast) {
        if field.expr().and_then(bare_path).as_deref() == Some("self") {
            *mentions
                .entry(field.name_ref()?.text().to_string())
                .or_default() += 1;
        }
    }
    let all_parameters: BTreeSet<_> = function
        .param_list()?
        .params()
        .filter_map(|parameter| match parameter.pat()? {
            ast::Pat::IdentPat(pattern) => Some(pattern.name()?.text().to_string()),
            _ => None,
        })
        .collect();
    let statements = body.stmt_list()?;
    let mut writes = BTreeMap::<String, usize>::new();
    let mut admitted_methods = 0;
    for expression in statements
        .statements()
        .filter_map(|statement| expression(&statement))
        .chain(statements.tail_expr())
    {
        // Unchanged arguments may still be written to other fields. They
        // establish only bounded execution, never the selected payload carrier.
        if parameter_write(expression.clone(), &all_parameters, &primitive_format).is_some() {
            admitted_methods += expression
                .syntax()
                .descendants()
                .filter(|node| ast::MethodCallExpr::can_cast(node.kind()))
                .count();
        }
        if let Some(field) = parameter_write(expression, parameters, &primitive_format) {
            *writes.entry(field).or_default() += 1;
        }
    }
    // A helper or another method could replace or alias the stored state.
    // Do not borrow a carrier from that opaque execution.
    if body
        .syntax()
        .descendants()
        .filter(|node| ast::MethodCallExpr::can_cast(node.kind()))
        .count()
        != admitted_methods
    {
        return None;
    }
    Some(
        writes
            .into_iter()
            .filter_map(|(field, count)| (mentions.get(&field) == Some(&count)).then_some(field))
            .collect(),
    )
}

impl ArgumentReceiver {
    pub(in crate::analysis) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
        complete: bool,
    ) -> Option<Self> {
        if !complete
            || owner.item.has_self_param
            || !matches!(
                probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            )
            || index.files().get(&owner.file)?.used_lexical_fallback
        {
            return None;
        }
        let probe_fn = parse_function(&format!("fn probe() {{ {}; }}", probe.expression))?;
        let statements: Vec<_> = probe_fn.body()?.stmt_list()?.statements().collect();
        let [statement] = statements.as_slice() else {
            return None;
        };
        let ast::Expr::MethodCallExpr(call) = expression(statement)? else {
            return None;
        };
        let receiver = bare_path(call.receiver()?)?;
        let method = call.name_ref()?.text().to_string();
        let function = parse_function(&owner.body)?;
        let parameters: Vec<_> = function.param_list()?.params().collect();
        let parameter = parameters.iter().position(|parameter| {
            matches!(parameter.pat(), Some(ast::Pat::IdentPat(pattern)) if pattern.name().is_some_and(|name| name.text() == receiver))
        })?;
        let ast::Type::RefType(ty) = parameters.get(parameter)?.ty()? else {
            return None;
        };
        let ast::Type::PathType(ty) = ty.ty()? else {
            return None;
        };
        let receiver_ty = ty.path()?.segment()?.name_ref()?.text().to_string();
        // Rebinding the owner parameter would invalidate the call-site projection.
        if function
            .body()?
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .any(|pattern| pattern.name().is_some_and(|name| name.text() == receiver))
        {
            return None;
        }
        let candidates: Vec<_> = index
            .functions()
            .iter()
            .filter(|candidate| candidate.name == method)
            .collect();
        let [callee] = candidates.as_slice() else {
            return None;
        };
        if impl_self_type_name(&callee.id.0).as_deref() != Some(receiver_ty.as_str()) {
            return None;
        }
        // Reuse the carrier's write attribution. Merely reading self.status
        // must not identify an observer of the self.sent write.
        let uses = super::function_field_use(callee)?;
        let mut fields = uses.writes;
        if matches!(probe.family, ProbeFamily::SideEffect)
            && let Some(before) = probe.before.as_deref()
        {
            if uses.whole_self || !uses.self_calls.is_empty() {
                return None;
            }
            let parameters = changed_parameters(before, &call, callee)?;
            let carrying = parameter_fields(callee, &parameters, index)?;
            fields.retain(|field| carrying.contains(field));
        }
        (!fields.is_empty()).then_some(Self {
            parameter,
            fields,
            receiver_type: receiver_ty,
            projection_imports_closed: projection_imports_closed(index),
        })
    }
    pub(in crate::analysis) fn admits(
        &self,
        owner: &FunctionSummary,
        index: &RustIndex,
        test: &TestSummary,
        assertion: &OracleFact,
        syntax: &OwnerPinSyntax,
    ) -> bool {
        self.bound(owner, index, test, assertion, syntax).is_some()
    }
    /// A first/last element projection can identify this call's payload
    /// only on a concrete, freshly empty std vector field. Reuse canonical
    /// receiver typing; do not credit an element seeded before the owner call.
    fn starts_empty(
        &self,
        index: &RustIndex,
        test: &TestSummary,
        owner: &FunctionSummary,
        root: &str,
        field: &str,
        definition: &ast::LetStmt,
    ) -> bool {
        use crate::analysis::classify::owner_pin::{
            OwnerScope, test_receiver_named_type, wrapper_may_be_rebound,
        };
        if ["String", "Vec", "RefCell"]
            .iter()
            .any(|name| wrapper_may_be_rebound(name, index))
            || test_receiver_named_type(
                test,
                root,
                OwnerScope::new(&owner.name, owner.start_line, &owner.file),
                index,
            )
            .as_deref()
                != Some(self.receiver_type.as_str())
        {
            return false;
        }
        let Some(ast::Expr::CallExpr(call)) = definition.initializer() else {
            return false;
        };
        if call
            .arg_list()
            .is_none_or(|args| args.args().next().is_some())
        {
            return false;
        }
        let Some(ast::Expr::PathExpr(path)) = call.expr() else {
            return false;
        };
        let path = compact(&path.syntax().text().to_string());
        let Some((ty, ctor)) = path.split_once("::") else {
            return false;
        };
        if ty != self.receiver_type || ctor.contains("::") {
            return false;
        }
        let Some(body) =
            crate::analysis::classify::builder_override::inherent_method_body(index, ty, ctor)
        else {
            return false;
        };
        let Some(function) = parse_function(body) else {
            return false;
        };
        let Some(list) = function.body().and_then(|body| body.stmt_list()) else {
            return false;
        };
        if list.statements().next().is_some() {
            return false;
        }
        let Some(ast::Expr::RecordExpr(record)) = list.tail_expr() else {
            return false;
        };
        let record_type = record
            .path()
            .map(|path| compact(&path.syntax().text().to_string()));
        if record_type.as_deref() != Some("Self") && record_type.as_deref() != Some(ty) {
            return false;
        }
        let empty = record.record_expr_field_list().is_some_and(|fields| {
            fields.fields().any(|entry| {
                entry.field_name().is_some_and(|name| name.text() == field)
                    && entry.expr().is_some_and(|expr| {
                        compact(&expr.syntax().text().to_string()) == "RefCell::new(Vec::new())"
                    })
            })
        });
        empty && index.files().values().any(|facts| {
            let Some(parse)=parse_clean_source_file(&facts.source) else { return false };
            parse.tree().syntax().descendants().filter_map(ast::Struct::cast).any(|item| {
                item.name().is_some_and(|name|name.text()==ty)
                    && matches!(item.field_list(),Some(ast::FieldList::RecordFieldList(fields)) if fields.fields().any(|entry|entry.name().is_some_and(|name|name.text()==field) && entry.ty().is_some_and(|ty|compact(&ty.syntax().text().to_string())=="RefCell<Vec<String>>")))
            })
        })
    }
    fn bound(
        &self,
        owner: &FunctionSummary,
        index: &RustIndex,
        test: &TestSummary,
        assertion: &OracleFact,
        syntax: &OwnerPinSyntax,
    ) -> Option<()> {
        if !syntax.admits_effect_assertion(test, assertion, index) {
            return None;
        }
        let operands = assertion_comparison_operands(&assertion.text)
            .map(|operands| operands.map(str::to_string).to_vec())
            .or_else(|| {
                crate::analysis::classify::owner_pin::equality_condition_operands(&assertion.text)
                    .map(|operands| operands.to_vec())
            })
            .unwrap_or_else(|| {
                crate::analysis::extract::assertion_observed_operand(&assertion.text)
                    .into_iter()
                    .collect()
            });
        let (root, field, projected) = operands.iter().find_map(|operand| {
            let function = parse_function(&format!("fn oracle() {{ {operand}; }}"))?;
            let statements: Vec<_> = function.body()?.stmt_list()?.statements().collect();
            let [statement] = statements.as_slice() else {
                return None;
            };
            let (root, field, projected) = field_root(
                expression(statement)?,
                index,
                self.projection_imports_closed,
            )?;
            self.fields
                .contains(&field)
                .then_some((root, field, projected))
        })?;
        let function = parse_function(&test.body)?;
        let statements: Vec<_> = function.body()?.stmt_list()?.statements().collect();
        let assertion_at = statements.iter().position(|statement| {
            let at = usize::from(statement.syntax().text_range().start());
            test.start_line + test.body[..at].matches('\n').count() == assertion.line
                && compact(&statement.syntax().text().to_string()).trim_end_matches(';')
                    == compact(&assertion.text).trim_end_matches(';')
        })?;
        let visible: Vec<_> = statements[..assertion_at]
            .iter()
            .filter(|statement| mentions(statement.syntax(), &root))
            .collect();
        let [definition, statement] = visible.as_slice() else {
            return None;
        };
        let ast::Stmt::LetStmt(definition) = definition else {
            return None;
        };
        let ast::Pat::IdentPat(pattern) = definition.pat()? else {
            return None;
        };
        if projected && !self.starts_empty(index, test, owner, &root, &field, definition) {
            return None;
        }

        if pattern.name()?.text() != root || definition.let_else().is_some() {
            return None;
        }
        let ast::Expr::CallExpr(call) = expression(statement)? else {
            return None;
        };
        let ast::Expr::PathExpr(callee) = call.expr()? else {
            return None;
        };
        let path = compact(&callee.syntax().text().to_string());
        let mut segments: Vec<String> = path.split("::").map(str::to_string).collect();
        if segments.pop()?.as_str() != owner.name {
            return None;
        }
        let prefix = (!segments.is_empty()).then(|| format!("{}::", segments.join("::")));
        if !targeted_free_call_reaches_owner(test, owner, prefix.as_deref(), index, syntax)
            || shared_owner_call_identity(index, owner).is_some_and(|identity| {
                identity.resolve_call(test, index, &segments) != CallTarget::Owner
            })
        {
            return None;
        }
        let argument = call.arg_list()?.args().nth(self.parameter)?;
        let argument = match argument {
            ast::Expr::RefExpr(reference) => reference.expr()?,
            other => other,
        };
        (bare_path(argument)?.as_str() == root).then_some(())
    }
}
