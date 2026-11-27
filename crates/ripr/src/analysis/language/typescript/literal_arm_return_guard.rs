//! A changed literal return in one arm of a literal-return lookup (#15306).
//!
//! `switch (n) { case 1: return 'one'; case 2: return 'two'; default: return
//! 'other'; }` returns a distinct literal from each arm. Changing
//! `return 'one'` changes the owner's result only for the inputs that reach
//! that arm, and the owner can return `'one'` from no other statement. An
//! exact assertion whose expected value is another arm's result (`'two'`)
//! passes on both versions, so reach plus a strong oracle does not
//! discriminate the change: only an assertion that expects `'one'`, or the
//! arm's new literal, can. When another arm also returns the new literal, an
//! assertion expecting it is that arm's result only when its call's literal
//! inputs provably leave the changed arm closed (a readable `if` condition or
//! lone `case` label); a stacked label, `default` or unreadable selector
//! keeps the verdict.
//!
//! The #8569 guard-entry rule withdraws credit only when every call
//! provably leaves the arm untaken, which a computed discriminant
//! (`Math.round(bearing / 22.5)`) or an object argument never proves. This
//! rule reads the expected side instead. It only withdraws credit, and only
//! on proof: every return of the owner must be a scalar literal, the changed
//! arm's old literal must be unique among them, and every strong assertion
//! that observes an owner call must expect a readable literal. Anything else
//! keeps the existing verdict.

use super::*;

/// Wrappers that let an owner's recorded source parse as a program: a
/// function or arrow declaration as-is, a class member inside a class body.
const WRAPPERS: [(&str, &str); 2] = [("", ""), ("class __ripr_owner__ {\n", "\n}")];

/// The changed arm: the literal it returned before the edit, which no other
/// return of the owner yields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LiteralArmReturn {
    pub(crate) old_value: ArmConstant,
    /// The old literal as written on the base side (`'NNE'`).
    pub(crate) old_text: String,
    /// The literal the changed arm returns after the edit. An exact
    /// assertion that expects it distinguishes the two versions when the
    /// call takes the changed arm.
    pub(crate) new_value: ArmConstant,
    /// Another arm also returns the new literal, so an assertion expecting
    /// it may observe that unchanged arm instead: it counts as another arm's
    /// result only when its call provably leaves the changed arm closed.
    pub(crate) new_value_shared: bool,
    /// The changed line and its text, to read the conditions that select it.
    pub(crate) line: usize,
    pub(crate) line_text: String,
}

/// `Some` when the changed line `line` is a one-line `return <literal>`
/// (semicolon optional) that returned another literal before the edit, the
/// owner returns only scalar literals from at least two return statements at
/// its own function level, and the old literal equals none of the other
/// returns' literals.
pub(crate) fn literal_arm_return(
    owner: &TypeScriptOwner,
    line: usize,
    line_text: &str,
    old_line_text: Option<&str>,
) -> Option<LiteralArmReturn> {
    let old_text = returned_literal_text(old_line_text?)?;
    let new_text = returned_literal_text(line_text)?;
    let old_value = scalar_constant(old_text)?;
    let new_value = scalar_constant(new_text)?;
    // `undefined` is also what a body that falls off its end returns.
    if old_value == new_value || old_value == ArmConstant::Undefined {
        return None;
    }
    let source = owner.source_text.as_deref()?;
    let changed = line.checked_sub(owner.source_start_line())?;
    // The recorded text must hold the changed line at that offset; an owner
    // span that disagrees with its text names another line.
    if source.lines().nth(changed).map(str::trim) != Some(line_text.trim()) {
        return None;
    }
    let (returns, binds_undefined) = owner_returns(&owner.file, source)?;
    // `undefined` is an ordinary identifier: a parameter or a top-level
    // declaration may bind it, and the recorded source cannot prove which
    // one a `return undefined` reads. Decline only when such a binding is
    // established; otherwise an `undefined` return (or a bare `return;`) is an
    // unreadable other arm, not a scalar that disables the changed arm.
    if binds_undefined {
        return None;
    }
    let mut changed_seen = false;
    let mut new_value_shared = false;
    for (return_line, value) in &returns {
        let value = value.as_ref()?;
        // A bare `return;` or a global `return undefined;` yields no literal,
        // so it cannot be the source of an assertion expecting one and does
        // not make the changed arm's old literal ambiguous.
        if *value == ArmConstant::Undefined {
            continue;
        }
        if *return_line == changed {
            if changed_seen || *value != new_value {
                return None;
            }
            changed_seen = true;
        } else if *value == old_value {
            return None;
        } else if *value == new_value {
            new_value_shared = true;
        }
    }
    (changed_seen && returns.len() >= 2).then(|| LiteralArmReturn {
        old_value,
        old_text: old_text.to_string(),
        new_value,
        new_value_shared,
        line,
        line_text: line_text.to_string(),
    })
}

/// Whether a call with `arguments` provably leaves the changed arm closed:
/// some `if` / lone `case` condition the arm needs is provably unmet by the
/// call's literal inputs (the #8569 reader). Anything unreadable is `false`.
fn call_leaves_changed_arm(
    change: &LiteralArmReturn,
    owner: &TypeScriptOwner,
    arguments: &[String],
) -> bool {
    use super::guard_entry_guard::{
        CallInputs, call_excludes_arm, enclosing_conditions, parameter_defaults, parameter_shapes,
    };
    let Some(source) = owner.source_text.as_deref() else {
        return false;
    };
    let Some(conditions) = enclosing_conditions(
        source,
        owner.source_start_line(),
        change.line,
        &change.line_text,
    ) else {
        return false;
    };
    let (params, defaults, options) = if owner.params.is_empty() {
        let Some(shapes) = parameter_shapes(source) else {
            return false;
        };
        (shapes.names, shapes.defaults, shapes.options)
    } else {
        let Some(defaults) = parameter_defaults(source, &owner.params) else {
            return false;
        };
        (owner.params.clone(), defaults, Vec::new())
    };
    let inputs = CallInputs {
        owner_source: source,
        params: &params,
        defaults: &defaults,
        options: &options,
        arguments,
    };
    call_excludes_arm(&inputs, &conditions)
}

/// What the related assertions say about the changed arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LiteralArmWitness {
    /// Some strong assertion expects the arm's old or new literal, or one this
    /// reader cannot rule out: keep the existing verdict.
    Kept,
    /// Every strong assertion on an owner call expects a readable literal and
    /// none is the arm's old or new result; the readable expected values, sorted.
    Unpinned(Vec<String>),
}

/// Whether some strong related assertion could pin the changed arm. Only
/// direct owner-call relations are read: a wrapper or entry could carry the
/// arm's result to an assertion under another expected value.
pub(crate) fn ts_literal_arm_witness(
    change: &LiteralArmReturn,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> LiteralArmWitness {
    let mut expected_values: Vec<String> = Vec::new();
    // A recursive or re-exported owner can be re-entered with another input,
    // so a call that leaves the changed arm closed can still reach it through
    // the owner's own recursion. The shared-new-result exemption needs the
    // owner named once in its module, as `ts_unentered_guard` requires.
    let owner_reentrant = workspace_root
        .is_none_or(|root| !super::classifier::owner_module_names_owner_once(owner, root));
    for candidate in candidates.iter().filter(|c| c.relation.uses_oracle()) {
        if !matches!(
            candidate.relation,
            TypeScriptRelationKind::DirectOwnerCall
                | TypeScriptRelationKind::ImportAliasOwnerCall
                | TypeScriptRelationKind::ImportedOwnerCall
        ) || local_identifier_declared_in_test_body(&candidate.test.body_text, &owner.name)
        {
            return LiteralArmWitness::Kept;
        }
        let receivers =
            unshadowed_owner_namespace_receivers(candidate.test, owner, alias_map, workspace_root);
        let names = ts_owner_call_names(candidate, owner, alias_map, workspace_root);
        for assertion in &candidate.test.assertions {
            if assertion.oracle_strength.rank() < OracleStrength::Strong.rank() {
                continue;
            }
            let expected = assertion
                .expected_value_or_variant
                .as_deref()
                .filter(|_| !assertion.has_dynamic_matcher_arg);
            let expected_value = expected.and_then(scalar_constant);
            let observed_arguments = ts_observed_owner_call_arguments(
                candidate.test,
                assertion,
                &names,
                owner,
                &receivers,
            );
            // An exact scalar expectation of the new result on an owner call
            // is another arm's result only when another arm shares it and
            // the call provably leaves the changed arm closed.
            let new_result_of_another_arm = !owner_reentrant
                && expected_value.as_ref() == Some(&change.new_value)
                && change.new_value_shared
                && observed_arguments
                    .as_deref()
                    .is_some_and(|arguments| call_leaves_changed_arm(change, owner, arguments));
            // An assertion anywhere that expects the old or the new result,
            // alone or inside a wrapping object, array or expression, may
            // observe the changed arm directly or through a local or a
            // wrapper, and then tells the versions apart: keep the verdict.
            let pinned = |value: &ArmConstant| {
                expected_value.as_ref() == Some(value)
                    || [
                        assertion.expected_value_or_variant.as_deref(),
                        assertion.dynamic_expected_text.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|text| mentions_constant(text, value))
            };
            if pinned(&change.old_value)
                || (!new_result_of_another_arm && pinned(&change.new_value))
            {
                return LiteralArmWitness::Kept;
            }
            if observed_arguments.is_none() {
                // A member, transform or wrapper of the owner's result, or a
                // local (destructured, module-level or hook-assigned) that
                // may hold it, can observe the arm without spelling its old
                // result; this reader cannot rule that out, so any strong
                // assertion that is not a whole owner call keeps the verdict.
                return LiteralArmWitness::Kept;
            }
            // A snapshot, a structural matcher or a dynamic expected side on
            // the owner's result may hold the arm's result unread.
            let (Some(expected), Some(_)) = (expected, expected_value) else {
                return LiteralArmWitness::Kept;
            };
            if assertion.oracle_kind != OracleKind::ExactValue {
                return LiteralArmWitness::Kept;
            }
            expected_values.push(expected.trim().to_string());
        }
    }
    if expected_values.is_empty() {
        return LiteralArmWitness::Kept;
    }
    expected_values.sort();
    expected_values.dedup();
    LiteralArmWitness::Unpinned(expected_values)
}

/// The finding line naming the unpinned arm: what the arm returned and the
/// results the exact assertions expect instead.
pub(crate) fn ts_literal_arm_unpinned_line(
    owner_name: &str,
    change: &LiteralArmReturn,
    expected: &[String],
) -> String {
    format!(
        "No exact assertion on `{owner_name}` expects `{}`, the result only the changed arm returned; the exact assertions on `{owner_name}` expect other arms' results ({}).",
        change.old_text,
        expected
            .iter()
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Whether expected-side source text spells `constant` anywhere: a quoted
/// string with its exact content, or a number, boolean or `null` token.
fn mentions_constant(text: &str, constant: &ArmConstant) -> bool {
    match constant {
        ArmConstant::Text(inner) => ['\'', '"', '`']
            .iter()
            .any(|quote| text.contains(&format!("{quote}{inner}{quote}"))),
        ArmConstant::Number(_) | ArmConstant::Boolean(_) | ArmConstant::Null => text
            .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '$')))
            .filter_map(scalar_constant)
            .any(|token| token == *constant),
        ArmConstant::Undefined | ArmConstant::Member(_) => false,
    }
}

/// The returned literal text of a one-line `return <literal>` (an optional
/// trailing `;`), or `None`.
fn returned_literal_text(line_text: &str) -> Option<&str> {
    let rest = line_text.trim().strip_prefix("return ")?.trim();
    let rest = rest.strip_suffix(';').unwrap_or(rest).trim_end();
    (!rest.is_empty()).then_some(rest)
}

/// A scalar literal (string, number, boolean, `null`, `undefined`), never a
/// member path whose value is unknown without the declaration.
fn scalar_constant(text: &str) -> Option<ArmConstant> {
    // A BigInt (`5n`) reads as the number 5, but `5n !== 5`: its identity is
    // unproven here, so it is not a readable scalar.
    let trimmed = text.trim();
    if trimmed.ends_with('n') && trimmed.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        return None;
    }
    match parse_arm_constant(text)? {
        ArmConstant::Member(_) => None,
        constant => Some(constant),
    }
}

/// The owner's own returns, each as (0-based line within `source`, its value
/// when a scalar literal), and whether a declaration or parameter in scope
/// binds the name `undefined`.
type OwnerReturns = (Vec<(usize, Option<ArmConstant>)>, bool);

/// Every return statement at the owner's own function level, as (0-based
/// line within `source`, its value when a scalar literal), and whether a
/// top-level declaration binds the name `undefined`. `None` when the owner
/// does not parse, is a generator or a concise arrow, or declares no
/// function body this reader can locate.
fn owner_returns(file: &Path, source: &str) -> Option<OwnerReturns> {
    for (prefix, suffix) in WRAPPERS {
        let wrapped = format!("{prefix}{source}{suffix}");
        let prefix_lines = prefix.matches('\n').count();
        let found = parse_on_worker(file, &wrapped, move |file, text, allocator| {
            let parsed = Parser::new(allocator, text, source_type_for(file)).parse();
            if parsed.panicked || !parsed.errors.is_empty() {
                return None;
            }
            let mut collector = ReturnCollector {
                text,
                newlines: text.match_indices('\n').map(|(offset, _)| offset).collect(),
                depth: 0,
                owners: 0,
                refused: false,
                returns: Vec::new(),
                binds_undefined: false,
            };
            oxc_ast_visit::Visit::visit_program(&mut collector, &parsed.program);
            if collector.refused || collector.owners != 1 {
                return None;
            }
            Some((
                collector
                    .returns
                    .into_iter()
                    .map(|(line, value)| (line.saturating_sub(prefix_lines), value))
                    .collect::<Vec<_>>(),
                collector.binds_undefined,
            ))
        })
        .ok()
        .flatten();
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Collects the returns of the one outermost function (the owner), never
/// those of a nested function or class method, whose returns are its own.
struct ReturnCollector<'t> {
    text: &'t str,
    /// Byte offsets of every `\n`, built once so locating a return's line is
    /// a binary search instead of a rescan of the whole source per return.
    newlines: Vec<usize>,
    depth: usize,
    owners: usize,
    refused: bool,
    returns: Vec<(usize, Option<ArmConstant>)>,
    /// A declaration or parameter in scope binds the name `undefined`, so a
    /// `return undefined` may read it rather than the global.
    binds_undefined: bool,
}

impl ReturnCollector<'_> {
    fn line_of(&self, offset: u32) -> usize {
        self.newlines
            .partition_point(|&newline| newline < offset as usize)
    }

    /// Whether a declaration or parameter in scope binds the name `undefined`.
    /// The AST sees through comments and formatting, so `const /* shadow */
    /// undefined` is caught where a source substring scan missed it.
    fn note_binding(&mut self, kind: oxc_ast::ast_kind::AstKind<'_>, owner_level: bool) {
        use oxc_ast::ast::ImportDeclarationSpecifier;
        use oxc_ast::ast_kind::AstKind;
        match kind {
            AstKind::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    if pattern_binds_undefined(&declarator.id) {
                        self.binds_undefined = true;
                    }
                }
            }
            AstKind::Function(function) => {
                if function
                    .id
                    .as_ref()
                    .is_some_and(|id| id.name.as_str() == "undefined")
                {
                    self.binds_undefined = true;
                }
                // Only the owner's own parameters shadow a return at its level.
                if owner_level
                    && function
                        .params
                        .items
                        .iter()
                        .any(|param| pattern_binds_undefined(&param.pattern))
                {
                    self.binds_undefined = true;
                }
            }
            AstKind::ArrowFunctionExpression(arrow)
                if owner_level
                    && arrow
                        .params
                        .items
                        .iter()
                        .any(|param| pattern_binds_undefined(&param.pattern)) =>
            {
                self.binds_undefined = true;
            }
            AstKind::Class(class)
                if class
                    .id
                    .as_ref()
                    .is_some_and(|id| id.name.as_str() == "undefined") =>
            {
                self.binds_undefined = true;
            }
            AstKind::CatchClause(catch) => {
                if let Some(param) = &catch.param
                    && pattern_binds_undefined(&param.pattern)
                {
                    self.binds_undefined = true;
                }
            }
            AstKind::ImportDeclaration(import) => {
                for specifier in import.specifiers.iter().flatten() {
                    let local = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => &specifier.local,
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                            &specifier.local
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            &specifier.local
                        }
                    };
                    if local.name.as_str() == "undefined" {
                        self.binds_undefined = true;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Whether a binding pattern binds the name `undefined`, through a plain
/// identifier, destructuring, a default or a rest element.
fn pattern_binds_undefined(pattern: &oxc_ast::ast::BindingPattern<'_>) -> bool {
    use oxc_ast::ast::BindingPattern;
    match pattern {
        BindingPattern::BindingIdentifier(id) => id.name.as_str() == "undefined",
        BindingPattern::ObjectPattern(object) => {
            object
                .properties
                .iter()
                .any(|property| pattern_binds_undefined(&property.value))
                || object
                    .rest
                    .as_ref()
                    .is_some_and(|rest| pattern_binds_undefined(&rest.argument))
        }
        BindingPattern::ArrayPattern(array) => {
            array.elements.iter().flatten().any(pattern_binds_undefined)
                || array
                    .rest
                    .as_ref()
                    .is_some_and(|rest| pattern_binds_undefined(&rest.argument))
        }
        BindingPattern::AssignmentPattern(assignment) => pattern_binds_undefined(&assignment.left),
    }
}

impl<'a> oxc_ast_visit::Visit<'a> for ReturnCollector<'_> {
    fn enter_node(&mut self, kind: oxc_ast::ast_kind::AstKind<'a>) {
        use oxc_ast::ast_kind::AstKind;
        if self.depth <= 1 {
            self.note_binding(kind, self.depth == 0);
        }
        let refused = match kind {
            AstKind::Function(function) => function.generator || function.body.is_none(),
            AstKind::ArrowFunctionExpression(arrow) => arrow.expression,
            _ => return,
        };
        if self.depth == 0 {
            self.owners += 1;
            self.refused |= refused;
        }
        self.depth += 1;
    }

    fn leave_node(&mut self, kind: oxc_ast::ast_kind::AstKind<'a>) {
        use oxc_ast::ast_kind::AstKind;
        if matches!(
            kind,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            self.depth = self.depth.saturating_sub(1);
        }
    }

    fn visit_return_statement(&mut self, it: &oxc_ast::ast::ReturnStatement<'a>) {
        if self.depth != 1 {
            return;
        }
        let line = self.line_of(it.span.start);
        // A return spanning several lines is not a one-line literal arm.
        let value = match &it.argument {
            None => Some(ArmConstant::Undefined),
            Some(argument) => {
                let span = argument.span();
                let one_line = self.line_of(span.start) == self.line_of(span.end);
                self.text
                    .get(span.start as usize..span.end as usize)
                    .filter(|_| one_line)
                    .and_then(scalar_constant)
            }
        };
        self.returns.push((line, value));
    }
}
