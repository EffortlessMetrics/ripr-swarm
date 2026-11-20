//! A changed literal return in one arm of a literal-return lookup (#15306).
//!
//! `switch (n) { case 1: return 'one'; case 2: return 'two'; default: return
//! 'other'; }` returns a distinct literal from each arm. Changing
//! `return 'one'` changes the owner's result only for the inputs that reach
//! that arm, and the owner can return `'one'` from no other statement. An
//! exact assertion whose expected value is another arm's result (`'two'`)
//! passes on both versions, so reach plus a strong oracle does not
//! discriminate the change: only an assertion that expects `'one'`, or the
//! arm's new literal, can.
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
    let changed = line.checked_sub(owner.start_line)?;
    let returns = owner_returns(&owner.file, source)?;
    let mut changed_seen = false;
    let mut new_value_shared = false;
    for (return_line, value) in &returns {
        let value = value.as_ref()?;
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
    let Some(conditions) =
        enclosing_conditions(source, owner.start_line, change.line, &change.line_text)
    else {
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
            let new_result_of_another_arm = expected_value.as_ref() == Some(&change.new_value)
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
        "No exact assertion on `{owner_name}` expects `{}`, the result only the changed arm returned; every exact assertion on `{owner_name}` expects another arm's result ({}), so each passes on both versions.",
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
    match parse_arm_constant(text)? {
        ArmConstant::Member(_) => None,
        constant => Some(constant),
    }
}

/// Every return statement at the owner's own function level, as (0-based
/// line within `source`, its value when a scalar literal). `None` when the
/// owner does not parse, is a generator or a concise arrow, or declares no
/// function body this reader can locate.
fn owner_returns(file: &Path, source: &str) -> Option<Vec<(usize, Option<ArmConstant>)>> {
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
                depth: 0,
                owners: 0,
                refused: false,
                returns: Vec::new(),
            };
            oxc_ast_visit::Visit::visit_program(&mut collector, &parsed.program);
            if collector.refused || collector.owners != 1 {
                return None;
            }
            Some(
                collector
                    .returns
                    .into_iter()
                    .map(|(line, value)| (line.saturating_sub(prefix_lines), value))
                    .collect::<Vec<_>>(),
            )
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
    depth: usize,
    owners: usize,
    refused: bool,
    returns: Vec<(usize, Option<ArmConstant>)>,
}

impl ReturnCollector<'_> {
    fn line_of(&self, offset: u32) -> usize {
        self.text
            .get(..offset as usize)
            .map_or(0, |head| head.matches('\n').count())
    }
}

impl<'a> oxc_ast_visit::Visit<'a> for ReturnCollector<'_> {
    fn enter_node(&mut self, kind: oxc_ast::ast_kind::AstKind<'a>) {
        use oxc_ast::ast_kind::AstKind;
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
