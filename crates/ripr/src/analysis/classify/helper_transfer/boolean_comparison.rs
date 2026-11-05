//! One asserted test-local primitive equality, bound to its source and caller.
//!
//! This is an observer receipt, not unchanged return forwarding. Unsupported
//! helpers keep the ordinary helper-result limitation.

use crate::analysis::classify::OwnerPinSyntax;
use crate::analysis::classify::propagation_witness::normalize_semantic_text;
use crate::analysis::facts::{
    FunctionContainer, FunctionSourceRole, FunctionSummary, OracleFact, RustIndex, TestSummary,
};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{OracleKind, OracleStrength, Probe, ProbeFamily, SymbolId, ValueContext};
use crate::value_origins::LocatedValueFact;
use ra_ap_syntax::{
    AstNode,
    ast::{
        self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasLoopBody, HasModuleItem,
        HasName,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct BooleanComparisonReceipt {
    owner: SymbolId,
    helper: SymbolId,
    file: PathBuf,
    workspace_root: PathBuf,
    invocation_dir: PathBuf,
    owner_line: usize,
    expression: String,
    parameter: String,
    test: String,
    test_line: usize,
    assertion_line: usize,
    assertion: String,
    samples: Vec<u32>,
    source_hash: String,
    pub(in crate::analysis) semantic_digest: String,
    validation_digest: String,
}

fn digest(fields: &[String]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    format!("sha256:{:x}", hash.finalize())
}

impl BooleanComparisonReceipt {
    fn fields(&self) -> Vec<String> {
        vec![
            "asserted-u32-equality-v1".into(),
            self.owner.0.clone(),
            self.helper.0.clone(),
            self.file.to_string_lossy().into_owned(),
            self.expression.clone(),
            self.parameter.clone(),
            self.test.clone(),
            normalize_semantic_text(&self.assertion),
            format!("{:?}", self.samples),
        ]
    }

    fn validation_fields(&self) -> Vec<String> {
        let mut fields = self.fields();
        fields.extend([
            self.workspace_root.to_string_lossy().into_owned(),
            self.invocation_dir.to_string_lossy().into_owned(),
            self.source_hash.clone(),
            self.owner_line.to_string(),
            self.test_line.to_string(),
            self.assertion_line.to_string(),
            self.assertion.clone(),
        ]);
        fields
    }

    pub(in crate::analysis) fn valid_for(&self, probe: &Probe) -> bool {
        probe.family == ProbeFamily::ReturnValue
            && probe.owner.as_ref() == Some(&self.owner)
            && self.matches_source(&probe.location.file, probe.location.line, &probe.expression)
    }

    pub(in crate::analysis) fn matches_source(
        &self,
        file: &std::path::Path,
        line: usize,
        expression: &str,
    ) -> bool {
        crate::analysis::facts::WorkspaceRootAuthority::source_path_matches(
            &self.workspace_root,
            &self.invocation_dir,
            &self.file,
            file,
        ) && self.owner_line == line
            && self.expression == normalize_semantic_text(expression)
            && !self.samples.is_empty()
            && self.semantic_digest == digest(&self.fields())
            && self.validation_digest == digest(&self.validation_fields())
    }

    pub(in crate::analysis) fn admits(&self, test: &TestSummary, assertion: &OracleFact) -> bool {
        self.file == test.file
            && self.test == test.name
            && self.test_line == test.start_line
            && self.assertion_line == assertion.line
            && self.assertion == assertion.text
    }

    pub(in crate::analysis) fn for_test(&self, test: &TestSummary) -> bool {
        self.file == test.file && self.test == test.name && self.test_line == test.start_line
    }

    /// A source-derived observation supplied only by this receipt's closed
    /// execution proof. Raw extractor facts remain untouched.
    pub(in crate::analysis) fn oracle(&self) -> OracleFact {
        OracleFact {
            line: self.assertion_line,
            text: self.assertion.clone(),
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
            observed_tokens: crate::analysis::extract::extract_identifier_tokens(&self.assertion),
            ok_value_observed: None,
        }
    }

    pub(in crate::analysis) fn inputs(&self) -> Vec<LocatedValueFact> {
        self.samples
            .iter()
            .map(|sample| LocatedValueFact {
                file: Some(self.file.clone()),
                line: self.assertion_line,
                text: self.assertion.clone(),
                value: format!("{} = {:?}", self.parameter, sample.to_be_bytes()),
                context: ValueContext::FunctionArgument,
            })
            .collect()
    }

    pub(in crate::analysis) fn summary(&self) -> String {
        format!(
            "Owner result is compared whole by `{}` and its returned equality is asserted by `{}` (asserted_u32_helper_comparison)",
            self.helper.0, self.test
        )
    }
}

/// Reconstruct all premises from the parsed source. Public/copied oracle facts
/// cannot create the receipt: owner, helper, loop and exact assertion must agree
/// with the current indexed file. No generic operator or method trust is added.
pub(in crate::analysis) fn boolean_comparison_receipt(
    owner: &FunctionSummary,
    test: &TestSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<BooleanComparisonReceipt> {
    if !super::is_identifier(&owner.name)
        || !owner.body.contains("u32")
        || !test.body.contains("for")
        || !test.body.contains("assert")
        || owner.file != test.file
        || owner.source_role != FunctionSourceRole::Production
        || !matches!(owner.item.container, FunctionContainer::Free)
        || !super::callee_is_unique(&owner.name, index)
        || !index
            .functions()
            .iter()
            .any(|candidate| std::ptr::eq(candidate, owner))
        || index.workspace_authority.is_none()
    {
        return None;
    }
    let workspace = index.workspace_authority.as_ref()?;
    let invocation_dir = workspace.invocation_dir.as_ref()?;
    if crate::analysis::facts::WorkspaceRootAuthority::root_relative_source_path(
        &workspace.root,
        &owner.file,
    )
    .as_ref()
        != Some(&owner.file)
    {
        return None;
    }
    let facts = index.files().get(&owner.file)?;
    if facts.used_lexical_fallback {
        return None;
    }
    let parse = parse_clean_source_file(&facts.source)?;
    let file = parse.tree();
    // Sibling custom derives/attributes can emit a shadowing value item.
    // Only closed builtin attributes and no macro imports are admitted.
    if file
        .items()
        .any(|item| matches!(item, ast::Item::ExternCrate(_)))
        || file
            .syntax()
            .descendants()
            .filter_map(ast::Attr::cast)
            .any(|attr| !closed_attribute(&attr))
    {
        return None;
    }
    let macro_sites = super::super::macro_item_names::MacroSites::scan(&facts.source);
    if macro_sites.has_non_local_defining_invocation()
        || [owner.name.as_str(), "u32", "u8", "bool", "Debug", "PartialEq", "Eq"].iter().any(|name| macro_sites.may_define(name))
        // Nested macro expansion is outside the shared item-name scanner.
        // Refuse that unresolved producer rather than guessing its bindings.
        || file.syntax().descendants().filter_map(ast::MacroRules::cast).any(|rules| {
            rules.token_tree().is_none_or(|tree| tree.syntax().descendants_with_tokens()
                .filter_map(|element| element.into_token()).any(|token| matches!(token.text(), "!" | "use" | "extern")))
        })
    {
        return None;
    }
    if file
        .syntax()
        .descendants()
        .filter_map(ast::MacroRules::cast)
        .any(|rules| {
            rules.token_tree().is_none_or(|tree| {
                let tokens: Vec<_> = tree
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|element| element.into_token())
                    .filter(|token| !token.kind().is_trivia())
                    .map(|token| token.text().to_string())
                    .collect();
                tokens.iter().enumerate().any(|(at, token)| {
                    token == "#"
                        && tokens.get(at + 1..at + 4)
                            != Some(&["[".to_owned(), "test".to_owned(), "]".to_owned()][..])
                })
            })
        })
    {
        return None;
    }
    // The initial source-bound profile imports only its parent. Refuse forged
    // primitive types/imports even if the written parameter still says `u32`.
    if file
        .syntax()
        .descendants()
        .filter_map(ast::Name::cast)
        .any(|name| matches!(name.text().trim_start_matches("r#"), "u32" | "u8" | "bool"))
        || file.items().any(|item| matches!(item, ast::Item::Use(_)))
    {
        return None;
    }
    let owner_fn = file
        .items()
        .filter_map(|item| match item {
            ast::Item::Fn(function) => Some(function),
            _ => None,
        })
        .find(|function| {
            function
                .name()
                .is_some_and(|name| name.text() == owner.name.as_str())
        })?;
    if function_line(&facts.source, &owner_fn)? != owner.start_line
        || !plain_function(&owner_fn)
        || owner_fn.attrs().next().is_some()
        || return_type(&owner_fn)? != "u32"
    {
        return None;
    }
    let owner_parameters: Vec<_> = owner_fn.param_list()?.params().collect();
    let [owner_parameter] = owner_parameters.as_slice() else {
        return None;
    };
    if normalize_semantic_text(&owner_parameter.ty()?.syntax().text().to_string()) != "&[u8]" {
        return None;
    }
    let owner_parameter_name = identifier_pattern(owner_parameter.pat()?)?;
    let owner_tail = sole_tail(&owner_fn)?;
    let expression = normalize_semantic_text(&owner_tail.syntax().text().to_string());
    let owner_line = source_line(&facts.source, owner_tail.syntax());
    if !(owner.start_line..=owner.end_line).contains(&owner_line) {
        return None;
    }

    for module in file.items().filter_map(|item| match item {
        ast::Item::Module(module) => Some(module),
        _ => None,
    }) {
        let attrs: Vec<_> = module
            .attrs()
            .map(|attr| normalize_semantic_text(&attr.syntax().text().to_string()))
            .collect();
        if attrs != ["#[cfg(test)]"] {
            continue;
        }
        let items = module.item_list()?;
        if items
            .items()
            .filter_map(|item| match item {
                ast::Item::Use(item) => Some(item),
                _ => None,
            })
            .any(|item| !parent_glob(&item))
        {
            continue;
        }
        if !items
            .items()
            .any(|item| matches!(item, ast::Item::Use(item) if parent_glob(&item)))
        {
            continue;
        }
        let test_fn = items
            .items()
            .filter_map(|item| match item {
                ast::Item::Fn(function) => Some(function),
                _ => None,
            })
            .find(|function| {
                function
                    .name()
                    .is_some_and(|name| name.text() == test.name.as_str())
            })?;
        if function_line(&facts.source, &test_fn)? != test.start_line
            || !plain_function(&test_fn)
            || test_fn.param_list()?.params().next().is_some()
            || test_fn
                .attrs()
                .map(|attr| normalize_semantic_text(&attr.syntax().text().to_string()))
                .collect::<Vec<_>>()
                != ["#[test]"]
            || test_fn.ret_type().is_some()
            || test_fn
                .syntax()
                .descendants()
                .filter_map(ast::Attr::cast)
                .count()
                != 1
        {
            return None;
        }
        let ast::Expr::ForExpr(table) = one_expression(test_fn.body()?.stmt_list()?)? else {
            return None;
        };
        if table.label().is_some() || array_attributes_in(&table.iterable()?) {
            return None;
        }
        let row_name = identifier_pattern(table.pat()?)?;
        let ast::Expr::ArrayExpr(array) = table.iterable()? else {
            return None;
        };
        if array.semicolon_token().is_some() {
            return None;
        }
        let mut samples = Vec::new();
        for row in array.exprs() {
            // A dynamic/early-exiting row can prevent the whole table from
            // running. Only this closed primitive constant grammar is read.
            samples.push(u32_sample(&row)?);
        }
        if samples.is_empty() {
            return None;
        }
        let assertion_list = table.loop_body()?.stmt_list()?;
        let assertion_text = match (
            assertion_list.statements().next(),
            assertion_list.tail_expr(),
        ) {
            (Some(statement), None) => statement.syntax().text().to_string(),
            (None, Some(tail)) => tail.syntax().text().to_string(),
            _ => return None,
        };
        let ast::Expr::MacroExpr(assertion_expr) = one_expression(assertion_list)? else {
            return None;
        };
        let assertion_call = assertion_expr.macro_call()?;
        if assertion_call.path()?.syntax().text() != "assert" {
            return None;
        }
        let assertion_line = source_line(&facts.source, assertion_call.syntax());
        let assertion = OracleFact {
            line: assertion_line,
            text: assertion_text,
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        };
        if !syntax.admits_assertion_context(test, &assertion, index) {
            return None;
        }
        let condition = crate::analysis::extract::outer_assertion_condition(&assertion.text)?;
        let condition_source = format!("fn __condition() {{ {condition} }}");
        let condition_parse = parse_clean_source_file(&condition_source)?;
        let condition_fn = condition_parse.tree().items().find_map(|item| match item {
            ast::Item::Fn(function) => Some(function),
            _ => None,
        })?;
        let ast::Expr::CallExpr(helper_call) = sole_tail(&condition_fn)? else {
            return None;
        };
        let helper_name = helper_call.expr()?.syntax().text().to_string();
        let helper_arguments: Vec<_> = helper_call
            .arg_list()?
            .args()
            .map(|arg| arg.syntax().text().to_string())
            .collect();
        if normalize_semantic_text(&assertion_call.token_tree()?.syntax().text().to_string())
            != format!("({helper_name}({row_name}))")
        {
            return None;
        }
        if macro_sites.may_define(&helper_name) {
            return None;
        }
        if helper_name == row_name || helper_name == owner.name {
            return None;
        }
        if helper_arguments != [row_name]
            || !super::is_identifier(&helper_name)
            || !super::callee_is_unique(&helper_name, index)
        {
            return None;
        }
        let helper_fn = items
            .items()
            .filter_map(|item| match item {
                ast::Item::Fn(function) => Some(function),
                _ => None,
            })
            .find(|function| {
                function
                    .name()
                    .is_some_and(|name| name.text() == helper_name.as_str())
            })?;
        if !plain_function(&helper_fn)
            || helper_fn.attrs().next().is_some()
            || return_type(&helper_fn)? != "bool"
        {
            return None;
        }
        let params: Vec<_> = helper_fn.param_list()?.params().collect();
        let [param] = params.as_slice() else {
            return None;
        };
        if param.ty()?.syntax().text() != "u32" {
            return None;
        }
        let parameter = identifier_pattern(param.pat()?)?;
        if parameter == owner.name {
            return None;
        }
        let ast::Expr::BinExpr(comparison) = sole_tail(&helper_fn)? else {
            return None;
        };
        if comparison.op_kind() != Some(ast::BinaryOp::CmpOp(ast::CmpOp::Eq { negated: false }))
            || comparison.rhs()?.syntax().text() != parameter.as_str()
        {
            return None;
        }
        let ast::Expr::CallExpr(call) = comparison.lhs()? else {
            return None;
        };
        if call.expr()?.syntax().text() != owner.name.as_str() {
            return None;
        }
        let args: Vec<_> = call.arg_list()?.args().collect();
        let [arg] = args.as_slice() else {
            return None;
        };
        let ast::Expr::RefExpr(borrow) = arg else {
            return None;
        };
        if borrow.mut_token().is_some() || borrow.raw_token().is_some() {
            return None;
        }
        let ast::Expr::MethodCallExpr(convert) = borrow.expr()? else {
            return None;
        };
        if convert.receiver()?.syntax().text() != parameter.as_str()
            || convert.name_ref()?.text() != "to_be_bytes"
            || convert.generic_arg_list().is_some()
            || convert.arg_list()?.args().next().is_some()
        {
            return None;
        }
        // A local const/static/type/closure with the imported owner's name
        // changes resolution even when a unique root function still exists.
        for item in items.items() {
            if item
                .syntax()
                .children()
                .filter_map(ast::Name::cast)
                .any(|name| name.text().trim_start_matches("r#") == owner.name.as_str())
            {
                return None;
            }
        }
        let helper = index
            .functions()
            .iter()
            .find(|function| function.file == owner.file && function.name == helper_name)?;
        if helper.source_role != FunctionSourceRole::CfgTestModule
            || function_line(&facts.source, &helper_fn)? != helper.start_line
        {
            return None;
        }
        // Parsed #[test] syntax is not execution authority when Cargo uses a
        // custom harness or skips the library target. Pay for the shared
        // target inventory only after every source-profile premise holds.
        if !syntax.admits_ordinary_library_test_target(index, &owner.file) {
            return None;
        }
        let mut receipt = BooleanComparisonReceipt {
            owner: owner.id.clone(),
            helper: helper.id.clone(),
            file: owner.file.clone(),
            workspace_root: workspace.root.clone(),
            invocation_dir: invocation_dir.clone(),
            owner_line,
            expression,
            parameter: owner_parameter_name,
            test: test.name.clone(),
            test_line: test.start_line,
            assertion_line,
            assertion: assertion.text.clone(),
            samples,
            source_hash: format!("{:x}", Sha256::digest(facts.source.as_bytes())),
            semantic_digest: String::new(),
            validation_digest: String::new(),
        };
        receipt.semantic_digest = digest(&receipt.fields());
        receipt.validation_digest = digest(&receipt.validation_fields());
        return Some(receipt);
    }
    None
}

fn source_line(source: &str, node: &ra_ap_syntax::SyntaxNode) -> usize {
    source
        .get(..usize::from(node.text_range().start()))
        .map_or(0, |prefix| {
            1 + prefix.bytes().filter(|byte| *byte == b'\n').count()
        })
}

fn plain_function(function: &ast::Fn) -> bool {
    function.generic_param_list().is_none()
        && function.where_clause().is_none()
        && function.async_token().is_none()
        && function.unsafe_token().is_none()
        && function.abi().is_none()
        && function
            .param_list()
            .is_some_and(|params| params.self_param().is_none())
}

fn return_type(function: &ast::Fn) -> Option<String> {
    Some(function.ret_type()?.ty()?.syntax().text().to_string())
}

fn identifier_pattern(pattern: ast::Pat) -> Option<String> {
    let ast::Pat::IdentPat(pattern) = pattern else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    Some(pattern.name()?.text().to_string())
}

fn sole_tail(function: &ast::Fn) -> Option<ast::Expr> {
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    list.tail_expr()
}

fn one_expression(list: ast::StmtList) -> Option<ast::Expr> {
    let statements: Vec<_> = list.statements().collect();
    match (statements.as_slice(), list.tail_expr()) {
        ([], Some(tail)) => Some(tail),
        ([ast::Stmt::ExprStmt(statement)], None) => statement.expr(),
        _ => None,
    }
}

fn u32_sample(expression: &ast::Expr) -> Option<u32> {
    if let ast::Expr::BinExpr(binary) = expression {
        if binary.op_kind() == Some(ast::BinaryOp::ArithOp(ast::ArithOp::Sub))
            && binary.lhs()?.syntax().text() == "u32::MAX"
        {
            return u32::MAX.checked_sub(u32_literal(&binary.rhs()?)?);
        }
        return None;
    }
    u32_literal(expression)
}

fn u32_literal(expression: &ast::Expr) -> Option<u32> {
    let ast::Expr::Literal(literal) = expression else {
        return None;
    };
    let text = literal.syntax().text().to_string().replace('_', "");
    let text = text.strip_suffix("u32").unwrap_or(&text);
    if let Some(hex) = text.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else if text.bytes().all(|byte| byte.is_ascii_digit()) {
        text.parse().ok()
    } else {
        None
    }
}

fn array_attributes_in(expression: &ast::Expr) -> bool {
    expression
        .syntax()
        .descendants()
        .any(|node| ast::Attr::can_cast(node.kind()))
}

fn function_line(source: &str, function: &ast::Fn) -> Option<usize> {
    let token = function.fn_token()?;
    Some(
        1 + source
            .get(..usize::from(token.text_range().start()))?
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count(),
    )
}

fn parent_glob(item: &ast::Use) -> bool {
    item.attrs().next().is_none()
        && item.use_tree().is_some_and(|tree| {
            tree.star_token().is_some()
                && tree.rename().is_none()
                && tree.use_tree_list().is_none()
                && tree
                    .path()
                    .is_some_and(|path| path.syntax().text() == "super")
        })
}

fn closed_attribute(attribute: &ast::Attr) -> bool {
    let compact: String = attribute
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if matches!(compact.as_str(), "#[test]" | "#[cfg(test)]") {
        return true;
    }
    let Some(derive) = compact
        .strip_prefix("#[derive(")
        .and_then(|text| text.strip_suffix(")]"))
    else {
        return false;
    };
    !derive.is_empty()
        && derive
            .split(',')
            .filter(|name| !name.is_empty())
            .all(|name| matches!(name, "Debug" | "PartialEq" | "Eq"))
}
