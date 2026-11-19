//! A changed literal return in one arm of a literal-return lookup (#15306).
//!
//! `switch (n) { case 1: return 'one'; case 2: return 'two'; default: return
//! 'other'; }` returns a distinct literal from each arm. Changing
//! `return 'one'` changes the owner's result only for the inputs that reach
//! that arm, and the owner can return `'one'` from no other statement. An
//! exact assertion whose expected value is another arm's result (`'two'`)
//! passes on both versions, so reach plus a strong oracle does not
//! discriminate the change: only an assertion that expects `'one'` can.
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
    for (return_line, value) in &returns {
        let value = value.as_ref()?;
        if *return_line == changed {
            if changed_seen || *value != new_value {
                return None;
            }
            changed_seen = true;
        } else if *value == old_value {
            return None;
        }
    }
    (changed_seen && returns.len() >= 2).then(|| LiteralArmReturn {
        old_value,
        old_text: old_text.to_string(),
    })
}

/// What the related assertions say about the changed arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LiteralArmWitness {
    /// Some strong assertion expects the arm's old literal, or one this
    /// reader cannot rule out: keep the existing verdict.
    Kept,
    /// Every strong assertion on an owner call expects a readable literal and
    /// none is the arm's old result; the readable expected values, sorted.
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
            // An assertion anywhere that expects the old result, alone or
            // inside a wrapping object, array or expression, may observe the
            // arm through a local or a wrapper: keep the verdict.
            if expected_value.as_ref() == Some(&change.old_value)
                || [
                    assertion.expected_value_or_variant.as_deref(),
                    assertion.dynamic_expected_text.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|text| mentions_constant(text, &change.old_value))
            {
                return LiteralArmWitness::Kept;
            }
            let observes_owner_call = ts_observed_owner_call_arguments(
                candidate.test,
                assertion,
                &names,
                owner,
                &receivers,
            )
            .is_some();
            if !observes_owner_call {
                // A member, transform or wrapper of the owner's result, or a
                // local that may hold it, can observe the arm without
                // spelling its old result: only an observation naming none
                // of those is unrelated.
                if may_observe_owner_indirectly(
                    candidate.test,
                    assertion.observed_expression.as_deref(),
                    &names,
                    &receivers,
                ) {
                    return LiteralArmWitness::Kept;
                }
                continue;
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

/// Whether an assertion's observed side, not itself a whole owner call, may
/// still carry the owner's result: it is unreadable, or it names the owner,
/// one of its call names or receivers, or a local the test body declares.
fn may_observe_owner_indirectly(
    test: &TypeScriptTest,
    observed: Option<&str>,
    names: &[&str],
    receivers: &[String],
) -> bool {
    let Some(observed) = observed else {
        return true;
    };
    let masked = ts_mask_literals_and_comments(observed);
    masked
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '$')))
        .filter(|token| {
            token
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, '_' | '$'))
        })
        .any(|token| {
            names.contains(&token)
                || receivers.iter().any(|receiver| receiver == token)
                || local_identifier_declared_in_test_body(&test.body_text, token)
        })
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
