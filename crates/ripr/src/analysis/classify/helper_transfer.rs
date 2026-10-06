//! Bounded helper-call transfer (#3296, P4 of #3215).
//!
//! When the changed behavior sits inside a helper that related tests
//! reach only through a short, statically resolvable call chain
//! (`test -> classify -> is_word_start`), the exact input literals the
//! tests provide never cross the helper edge: the helper-owned probe
//! reports no static path and the transitive-reach limitation can only
//! hint that a caller "may lead here".
//!
//! This module is the one helper-transfer authority. It resolves the
//! bounded chain once and is consumed by the relation (test
//! relatedness) and activation (exact input rows) stages, so every
//! surface sees the same decision.
//!
//! V1 transfers only:
//!
//! - direct same-crate calls with a **unique callee identity** (the
//!   #2971 workspace-complete uniqueness rule);
//! - **positional argument-to-parameter binding** where each bound
//!   argument is a literal or the caller's own parameter (resolved
//!   through the caller's rows);
//! - chains of at most [`MAX_HELPER_HOPS`] hops, acyclic, with a
//!   single caller and a single call site per hop.
//!
//! Everything else stops at a named edge: recursion, an ambiguous or
//! unknown callee, a computed (non-literal, non-parameter) argument,
//! multi-caller or multi-site binding, and hop exhaustion. Call reach
//! alone never establishes propagation or discrimination — the oracle
//! still has to observe the sink through the ordinary evidence stages.

use super::value_transfer::ExactInputs;
use crate::analysis::facts::{CallFact, FunctionSummary, RustIndex};

/// The configured hop bound for helper transfer. Exceeding it yields a
/// typed limitation naming the chain's stop, never a silent drop.
pub(crate) const MAX_HELPER_HOPS: usize = 3;

/// Bounded evaluation context for helper returns whose bodies call
/// other helpers (#3296 recursive row). `depth` counts helper-return
/// evaluations on the current path — the same explicit
/// [`MAX_HELPER_HOPS`] bound the caller-side chain enforces — and
/// `states` records each `(helper, bound inputs)` state already
/// evaluated: a repeated state is a true cycle (the same inputs take
/// the same arm), while distinct inputs unroll within the bound.
pub(crate) struct HelperEval<'a> {
    index: &'a RustIndex,
    workspace_complete: bool,
    depth: usize,
    states: Vec<String>,
}

impl<'a> HelperEval<'a> {
    pub(crate) fn root(index: &'a RustIndex, workspace_complete: bool) -> Self {
        HelperEval {
            index,
            workspace_complete,
            depth: 0,
            states: Vec::new(),
        }
    }

    /// Enter the evaluation of `helper` over `inputs`: refuse a state
    /// already on the path (a cycle) or a path at the hop bound, and
    /// return the advanced context for the body's evaluation.
    fn enter(&self, helper: &FunctionSummary, inputs: &ExactInputs) -> Option<HelperEval<'a>> {
        if self.depth >= MAX_HELPER_HOPS {
            return None;
        }
        let key = format!(
            "{}|{}",
            helper.name,
            inputs
                .iter()
                .map(|(parameter, value)| format!("{parameter}={value}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        if self.states.iter().any(|state| state == &key) {
            return None;
        }
        let mut states = self.states.clone();
        states.push(key);
        Some(HelperEval {
            index: self.index,
            workspace_complete: self.workspace_complete,
            depth: self.depth + 1,
            states,
        })
    }
}

/// Resolve one nested direct call inside a helper body (`label_of("a")`
/// as a match arm value) over the calling helper's bound inputs. The
/// decomposition, uniqueness, and splittability checks are the shared
/// direct-call authority; argument binding stays strict (a literal or
/// a parameter bound in `inputs`). A cycle or the hop bound refuses
/// (fail closed).
pub(crate) fn nested_call_value(
    call: &str,
    inputs: &ExactInputs,
    eval: &HelperEval<'_>,
) -> Option<super::value_transfer::TypedValue> {
    if !eval.workspace_complete {
        return None;
    }
    let super::activation::ResolvedOperand::Call { callee, arguments } =
        super::activation::resolve_direct_call(call, eval.index, eval.workspace_complete)?
    else {
        return None;
    };
    let mut bound = ExactInputs::new();
    let parameters = super::activation::function_parameters(&callee);
    for (index, argument) in arguments.iter().enumerate() {
        let parameter = parameters
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("arg{index}"));
        let value = strict_literal(argument).or_else(|| inputs.get(argument.trim()).cloned())?;
        bound.insert(parameter, value);
    }
    helper_return_value(&callee, &bound, eval)
}

/// One hop of a resolved helper chain: the caller, the call site, and
/// the positional argument texts at that site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HelperHop {
    pub(crate) caller: FunctionSummary,
    pub(crate) call_text: String,
    pub(crate) arguments: Vec<String>,
}

/// The resolved chain for one helper. `stop_above` names the first
/// unsupported edge above the resolved hops (an upper bound reached, a
/// recursion, or an ambiguous grand-caller); it does not invalidate the
/// hops below it, which may still carry direct test rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HelperChain {
    pub(crate) hops: Vec<HelperHop>,
    pub(crate) stop_above: Option<String>,
}

impl HelperChain {
    /// The function a related test can call directly: the topmost
    /// resolved hop's caller.
    #[cfg(test)]
    pub(crate) fn entry_name(&self) -> Option<&str> {
        self.hops.last().map(|hop| hop.caller.name.as_str())
    }
}

/// Whether `callee_name` names exactly one function in the index (the
/// #2971 uniqueness rule; the caller must already have established
/// workspace completeness — a partial index would make a same-named
/// function in an unindexed file invisible and the name falsely
/// unique).
pub(crate) fn callee_is_unique(callee_name: &str, index: &RustIndex) -> bool {
    !callee_name.is_empty()
        && index
            .functions()
            .iter()
            .filter(|function| function.name == callee_name)
            .count()
            == 1
}

/// The functions that call `callee_name` directly. Recursion
/// (`function.name == callee_name`) is excluded here and named by the
/// resolver — a self-recursive helper is a bound, not a transfer.
pub(crate) fn direct_callers<'a>(
    callee_name: &str,
    index: &'a RustIndex,
) -> Vec<&'a FunctionSummary> {
    index
        .functions()
        .iter()
        .filter(|function| {
            function.name != callee_name
                && function.calls.iter().any(|call| call.name == callee_name)
        })
        .collect()
}

/// The call sites in `caller` that invoke `callee_name`, with their
/// split argument texts. `None` when a call text cannot be split.
pub(crate) fn call_sites(
    caller: &FunctionSummary,
    callee_name: &str,
) -> Option<Vec<(String, Vec<String>)>> {
    let mut sites = Vec::new();
    for call in &caller.calls {
        if call.name != callee_name {
            continue;
        }
        // #3296 review B1: a method call (`word.validate()`) or a
        // path-qualified invocation (`internal::inner(..)`) is recorded
        // under the bare callee name, and name identity is not callee
        // identity. Only a direct free-function call site may bind.
        if !is_direct_call_site(&call.text, callee_name) {
            continue;
        }
        let arguments = split_call_arguments_text(&call.text, callee_name)?;
        sites.push((call.text.clone(), arguments));
    }
    (!sites.is_empty()).then_some(sites)
}

/// Byte index of the `(` that opens a direct free-function call of
/// `callee_name`: the first `callee(` occurrence at a token boundary that
/// is preceded by neither a receiver (`.`), a path qualifier (`:`), nor an
/// identifier character. Later occurrences are examined when an earlier one
/// is shadowed (e.g. `my_inner(2); inner(1)` still resolves the direct
/// `inner(1)` site). `None` when no occurrence qualifies.
fn direct_call_paren(text: &str, callee_name: &str) -> Option<usize> {
    let needle = format!("{callee_name}(");
    let mut search = 0usize;
    while let Some(relative) = text[search..].find(&needle) {
        let at = search + relative;
        let direct = match text[..at].chars().next_back() {
            None => true,
            Some(before) => {
                !before.is_ascii_alphanumeric() && before != '_' && before != '.' && before != ':'
            }
        };
        if direct {
            return Some(at);
        }
        // Step by the first char's width so a non-ASCII callee name never
        // leaves `search` inside a multibyte char.
        search = at + needle.chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// Whether `text` invokes `callee_name` as a direct free-function call:
/// the callee occurrence is at a token boundary and is preceded by
/// neither a receiver (`.`) nor a path qualifier (`::`).
pub(crate) fn is_direct_call_site(text: &str, callee_name: &str) -> bool {
    direct_call_paren(text, callee_name).is_some()
}

/// Split a call's argument texts, quote- and nesting-aware.
pub(crate) fn split_call_arguments_text(text: &str, callee_name: &str) -> Option<Vec<String>> {
    let open = direct_call_paren(text, callee_name)?;
    let after = &text[open + callee_name.len() + 1..];
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut literal: Option<char> = None;
    let mut escaped = false;
    let mut start = 0usize;
    for (at, character) in after.char_indices() {
        if let Some(quote) = literal {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                literal = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => literal = Some(character),
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth == 0 {
                    let last = after[start..at].trim();
                    if !last.is_empty() {
                        arguments.push(last.to_string());
                    }
                    return Some(arguments);
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                let argument = after[start..at].trim();
                if !argument.is_empty() {
                    arguments.push(argument.to_string());
                }
                start = at + 1;
            }
            _ => {}
        }
    }
    None
}

/// Resolve the helper chain above `callee_name`, one hop at a time.
/// `visited` carries the chain's names so a cycle stops with the
/// recursion edge named. The result always names either the resolved
/// hops or the exact stop edge.
pub(crate) fn resolve_chain(
    callee_name: &str,
    index: &RustIndex,
    workspace_complete: bool,
    visited: &[String],
) -> HelperChain {
    let stopped = |edge: String| HelperChain {
        hops: Vec::new(),
        stop_above: Some(edge),
    };
    if !workspace_complete {
        return stopped(
            "helper chain requires a workspace-complete index (the analysis mode indexed a subset)"
                .to_string(),
        );
    }
    if visited.len() >= MAX_HELPER_HOPS {
        return stopped(format!(
            "helper chain from `{callee_name}` exceeds the {MAX_HELPER_HOPS}-hop bound"
        ));
    }
    if !callee_is_unique(callee_name, index) {
        return stopped(format!(
            "callee `{callee_name}` is not a unique function in the workspace"
        ));
    }
    if visited.contains(&callee_name.to_string()) {
        return stopped(format!(
            "recursion through helper `{callee_name}` at the transfer bound"
        ));
    }
    let callers = direct_callers(callee_name, index);
    if callers.is_empty() {
        return stopped(format!("no static caller found for helper `{callee_name}`"));
    }
    if callers.len() > 1 {
        return stopped(format!(
            "callee `{callee_name}` has {} callers; multi-caller binding is not transferred at this bound",
            callers.len()
        ));
    }
    let caller = callers[0];
    let Some(sites) = call_sites(caller, callee_name) else {
        return stopped(format!(
            "call site for `{callee_name}` in `{}` is not statically splittable",
            caller.name
        ));
    };
    if sites.len() > 1 {
        return stopped(format!(
            "helper `{callee_name}` is called {} times in `{}`; multi-site binding is not transferred at this bound",
            sites.len(),
            caller.name
        ));
    }
    let (call_text, arguments) = sites[0].clone();
    let hop = HelperHop {
        caller: caller.clone(),
        call_text,
        arguments,
    };
    let mut next_visited = visited.to_vec();
    next_visited.push(callee_name.to_string());
    let HelperChain {
        mut hops,
        stop_above,
    } = resolve_chain(&caller.name, index, workspace_complete, &next_visited);
    // #3296 review M2: tests are ordinary functions in the index, so
    // the chain must never climb into a test body — the entry a test
    // can call directly is the topmost production caller.
    while hops
        .last()
        .is_some_and(|top| top.caller.source_role.is_evidence_role())
    {
        hops.pop();
    }
    let mut resolved = vec![hop];
    resolved.append(&mut hops);
    HelperChain {
        hops: resolved,
        stop_above,
    }
}

/// Whether a test reaches `callee_name` through a bounded helper chain
/// (the relation stage's question): the test calls some resolved hop's
/// caller directly. The relation stage resolves the chain once per
/// probe and checks the calls itself (#3296 review M1); this query
/// stays as the unit-test surface for the reach question.
#[cfg_attr(not(test), allow(dead_code, reason = "unit-test query surface"))]
pub(crate) fn test_reaches_through_chain(
    test_calls: &[CallFact],
    callee_name: &str,
    index: &RustIndex,
    workspace_complete: bool,
) -> Option<HelperChain> {
    let chain = resolve_chain(callee_name, index, workspace_complete, &[]);
    if chain.hops.is_empty() {
        return None;
    }
    let reached = test_calls
        .iter()
        .any(|call| chain.hops.iter().any(|hop| hop.caller.name == call.name));
    reached.then_some(chain)
}

/// Evaluate a helper's return value over bound exact inputs (#3296,
/// the boolean/return predicate family). V1 supports a body whose
/// final expression is a bare binding established by simple single-line
/// `let` statements: the binding's initializer evaluates through the
/// #3295 families. A body with loops, inner calls, early returns, or
/// any other shape fails closed to `None`.
pub(crate) fn helper_return_value(
    helper: &FunctionSummary,
    inputs: &ExactInputs,
    eval: &HelperEval<'_>,
) -> Option<super::value_transfer::TypedValue> {
    // The bounded context refuses a repeated (helper, inputs) state (a
    // true cycle) and a path at the hop bound before any body shape is
    // considered.
    let eval = eval.enter(helper, inputs)?;
    // #3296: the scanner authority owns the literal-driven state-loop
    // shape first; the literal match-arm authority owns the string
    // `match` tail expression (and may resolve nested direct calls in
    // arm values through the same context); the let-chain evaluator
    // below keeps every other body (and stays the authority for
    // non-scanner tails).
    if let Some(value) = super::scanner_transfer::scanner_return_value(helper, inputs) {
        return Some(value);
    }
    if let Some(value) = super::match_transfer::match_return_value(helper, inputs, &eval) {
        return Some(value);
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&helper.body);
    let raw_lines: Vec<&str> = helper.body.lines().collect();
    let masked_lines: Vec<&str> = masked.lines().collect();
    if raw_lines.len() != masked_lines.len() {
        return None;
    }
    let mut bindings: Vec<(String, String)> = Vec::new();
    let mut tail: Option<String> = None;
    for (raw_line, masked_line) in raw_lines.iter().zip(masked_lines.iter()) {
        let trimmed = masked_line.trim();
        if trimmed.starts_with("let ") {
            if !trimmed.ends_with(';') {
                return None;
            }
            let (declared, _) = crate::analysis::language::changed_let_binding(trimmed)?;
            let (declared_raw, initializer) =
                crate::analysis::language::changed_let_binding(raw_line.trim())?;
            if declared != declared_raw {
                return None;
            }
            bindings.push((declared.to_string(), initializer.to_string()));
            continue;
        }
        let cleaned = trimmed.trim_end_matches(';').trim();
        if cleaned.is_empty()
            || cleaned.starts_with("//")
            || cleaned.starts_with('#')
            || cleaned.starts_with("pub ")
            || cleaned.starts_with("pub(")
            || cleaned.starts_with("fn ")
            || cleaned.starts_with("async ")
            || cleaned.starts_with("const ")
            || cleaned.starts_with("unsafe ")
            || cleaned.starts_with('}')
            || cleaned.starts_with("return ")
        {
            continue;
        }
        if tail.is_some() {
            return None;
        }
        tail = Some(raw_line.trim().trim_end_matches(';').trim().to_string());
    }
    let tail = tail?;
    let is_identifier = !tail.is_empty()
        && tail
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && tail
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !is_identifier {
        return None;
    }
    let initializer = bindings
        .iter()
        .rev()
        .find(|(name, _)| *name == tail)
        .map(|(_, initializer)| initializer.clone())?;
    match super::value_transfer::evaluate_initializer(&initializer, inputs) {
        super::value_transfer::EvalOutcome::Exact { value, .. } => Some(value),
        _ => None,
    }
}

/// The single-quote character, named so the strict literal parser does
/// not need an escaped char constant.
const CHAR_QUOTE: char = '\'';

fn char_quote() -> char {
    CHAR_QUOTE
}

/// Parse a call-site argument as a strict literal (#3296 review B2):
/// only a whole-token string, char, integer, or boolean literal counts.
/// An identifier like `a2` is a parameter reference, never the literal
/// `2` — the substring scanner must not fabricate values.
pub(crate) fn strict_literal(argument: &str) -> Option<String> {
    let trimmed = argument.trim();
    if trimmed.is_empty() {
        return None;
    }
    let is_string = trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2;
    let is_char =
        trimmed.starts_with(char_quote()) && trimmed.ends_with(char_quote()) && trimmed.len() >= 3;
    let is_integer = trimmed.chars().all(|ch| ch.is_ascii_digit());
    let is_boolean = trimmed == "true" || trimmed == "false";
    if !(is_string || is_char || is_integer || is_boolean) {
        return None;
    }
    Some(trimmed.to_string())
}

/// Stop token named when the owner is reached only through a chain whose
/// hops do not hand the owner's result to the entry's return (#6780).
pub(crate) const HELPER_RESULT_NOT_FORWARDED: &str = "helper_result_not_forwarded";

/// True when some related test reaches the owner through the helper chain
/// and none calls it directly: every oracle then observes a hop caller's
/// result, never the owner's own.
pub(crate) fn helper_only_reach(
    related_tests: &[(
        &crate::analysis::facts::TestSummary,
        crate::domain::RelationReason,
    )],
) -> bool {
    use crate::domain::RelationReason;
    related_tests
        .iter()
        .any(|(_, reason)| *reason == RelationReason::HelperOwnerCall)
        && !related_tests
            .iter()
            .any(|(_, reason)| *reason == RelationReason::DirectOwnerCall)
}

/// Whether every hop of `chain` hands the result of its call straight to
/// its caller's return (#6694, #6672), so an exact oracle on the entry's
/// result can stand for an oracle on the owner's result.
///
/// Each hop's caller body must have no early `return` and no `?`, and its
/// tail expression must be either the hop call itself (`capped(value,
/// 10)`) or `if <call> { A } else { B }` / `if !<call> { A } else { B }`
/// with textually distinct branches. Any other shape — a discarded or
/// let-bound result, arithmetic on the result, an `else if` chain, a call
/// inside one branch, a `match` — answers `false` (fail closed): reach
/// still holds, but the entry's oracle is not paired with the owner's
/// boundary.
pub(crate) fn chain_forwards_owner_result(owner_name: &str, chain: &HelperChain) -> bool {
    !chain.hops.is_empty()
        && chain.hops.iter().enumerate().all(|(step, hop)| {
            let callee = match step.checked_sub(1) {
                None => owner_name,
                Some(below) => match chain.hops.get(below) {
                    Some(lower) => lower.caller.name.as_str(),
                    None => return false,
                },
            };
            caller_tail_forwards_call(&hop.caller.body, callee)
                && hop.arguments.iter().all(|argument| {
                    !is_identifier(argument.trim())
                        || !caller_rebinds_parameter(&hop.caller.body, argument.trim())
                })
        })
}

fn caller_tail_forwards_call(body: &str, callee: &str) -> bool {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(body);
    if masked.len() != body.len() || callee.is_empty() {
        return false;
    }
    let (Some(open), Some(close)) = (masked.find('{'), masked.rfind('}')) else {
        return false;
    };
    if close <= open {
        return false;
    }
    let inner = &masked[open + 1..close];
    if inner.contains('?') || contains_word(inner, "return") {
        return false;
    }
    // The tail starts after the last depth-0 `;`.
    let mut depth = 0usize;
    let mut tail_start = 0usize;
    for (at, character) in inner.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            ';' if depth == 0 => tail_start = at + 1,
            _ => {}
        }
    }
    if depth != 0 {
        return false;
    }
    let Some(raw_inner) = body.get(open + 1..close) else {
        return false;
    };
    let tail = inner[tail_start..].trim();
    let lead = inner[tail_start..].len() - inner[tail_start..].trim_start().len();
    let Some(raw_tail) = raw_inner.get(tail_start + lead..tail_start + lead + tail.len()) else {
        return false;
    };
    if is_exact_call(tail, callee) {
        return true;
    }
    let Some(after_if) = tail.strip_prefix("if ") else {
        return false;
    };
    let base = tail.len() - after_if.len();
    let Some(then_open) = after_if.find('{') else {
        return false;
    };
    let condition = after_if[..then_open].trim();
    let condition = condition.strip_prefix('!').unwrap_or(condition).trim();
    if !is_exact_call(condition, callee) {
        return false;
    }
    let then_open = base + then_open;
    let Some(then_close) = matching_close(tail, then_open) else {
        return false;
    };
    let rest = &tail[then_close + 1..];
    let Some(after_else) = rest.trim_start().strip_prefix("else") else {
        return false;
    };
    let else_open_rel = after_else.len() - after_else.trim_start().len();
    if !after_else[else_open_rel..].starts_with('{') {
        return false;
    }
    let else_open = tail.len() - after_else.len() + else_open_rel;
    let Some(else_close) = matching_close(tail, else_open) else {
        return false;
    };
    if !tail[else_close + 1..].trim().is_empty() {
        return false;
    }
    match (
        raw_tail.get(then_open + 1..then_close),
        raw_tail.get(else_open + 1..else_close),
    ) {
        // Both branches must be distinct literals (#6780 review B3): a
        // computed branch (`qty / 2`) can equal the other at the boundary.
        (Some(then_raw), Some(else_raw)) => {
            match (strict_literal(then_raw), strict_literal(else_raw)) {
                (Some(then_value), Some(else_value)) => then_value != else_value,
                _ => false,
            }
        }
        _ => false,
    }
}

/// Whether `body` (a caller's full text) rebinds or assigns `parameter`
/// after its signature (#6780 review B1): a `let` pattern, a closure
/// parameter list, a `for` pattern, a match-arm or `@` pattern, or a plain
/// or compound assignment naming it. A rebound parameter no longer holds
/// the caller's input, so the test's argument cannot bind through it.
pub(crate) fn caller_rebinds_parameter(body: &str, parameter: &str) -> bool {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(body);
    let Some(open) = masked.find('{') else {
        return true;
    };
    let inner = &masked[open + 1..];
    let word_at = |text: &str, at: usize| {
        let before = text[..at].chars().next_back();
        let after = text[at + parameter.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    };
    let mentions = |segment: &str| {
        segment
            .match_indices(parameter)
            .any(|(at, _)| word_at(segment, at))
    };
    for (at, _) in inner.match_indices(parameter) {
        if !word_at(inner, at) {
            continue;
        }
        let after = inner[at + parameter.len()..].trim_start();
        let assigns =
            (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"))
                || ["+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>="]
                    .iter()
                    .any(|op| after.starts_with(op))
                || after.starts_with('@');
        if assigns {
            return true;
        }
    }
    // `let` patterns: the text between `let` and its `=` or `;`.
    for (at, _) in inner.match_indices("let") {
        if inner[..at]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            continue;
        }
        let rest = &inner[at + 3..];
        let end = rest.find(['=', ';']).unwrap_or(rest.len());
        if mentions(&rest[..end]) {
            return true;
        }
    }
    // `for <pattern> in`.
    for (at, _) in inner.match_indices("for ") {
        let rest = &inner[at + 4..];
        if let Some(end) = rest.find(" in ")
            && mentions(&rest[..end])
        {
            return true;
        }
    }
    // Match-arm patterns: the text before each `=>` back to the enclosing
    // `{` or a top-level `,`, cut at a top-level `if` guard. A guard only
    // reads the parameter (`n if n > qty =>`); the pattern before it can
    // bind it (`Some(qty) if .. =>`, `Foo { qty } =>`).
    for (at, _) in inner.match_indices("=>") {
        let start = match_arm_start(&inner[..at]);
        if mentions(match_arm_pattern(&inner[start..at])) {
            return true;
        }
    }
    // Closure parameter lists `|..|` on one line (`||` is not one).
    for line in inner.lines() {
        let pipes: Vec<usize> = line
            .match_indices('|')
            .map(|(index, _)| index)
            .filter(|index| !line[..*index].ends_with('|') && !line[index + 1..].starts_with('|'))
            .collect();
        for pair in pipes.chunks(2) {
            if let [left, right] = pair
                && mentions(&line[left + 1..*right])
            {
                return true;
            }
        }
    }
    false
}

/// Where the match arm ending at the end of `before` starts: after the
/// nearest unmatched opening bracket or top-level `,`, scanning backward so
/// a struct pattern's own braces (`Foo { qty }`) stay inside the arm. A
/// preceding block arm without a comma is kept in the arm text, which can
/// only report a rebinding, never hide one.
fn match_arm_start(before: &str) -> usize {
    let mut depth = 0usize;
    for (at, character) in before.char_indices().rev() {
        match character {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth == 0 => return at + 1,
            '(' | '[' | '{' => depth -= 1,
            ',' if depth == 0 => return at + 1,
            _ => {}
        }
    }
    0
}

/// The pattern part of a match arm: `arm` before its last `if` keyword
/// outside any bracket. Patterns cannot contain `if`, so the last top-level
/// one opens the guard; without a guard the whole arm is the pattern.
fn match_arm_pattern(arm: &str) -> &str {
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let mut depth = 0usize;
    let mut guard = None;
    for (at, character) in arm.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            'i' if depth == 0
                && arm[at..].starts_with("if")
                && !arm[..at].chars().next_back().is_some_and(is_word)
                && !arm[at + 2..].chars().next().is_some_and(is_word) =>
            {
                guard = Some(at);
            }
            _ => {}
        }
    }
    guard.map_or(arm, |at| &arm[..at])
}

/// `text` is exactly one direct call of `callee` (nothing before or after).
fn is_exact_call(text: &str, callee: &str) -> bool {
    if direct_call_paren(text, callee) != Some(0) {
        return false;
    }
    matching_close(text, callee.len()).is_some_and(|close| close + 1 == text.len())
}

/// Index of the bracket closing the one opened at `open` (masked text).
fn matching_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, character) in text.char_indices().skip_while(|(at, _)| *at < open) {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::facts::{CallFact, FunctionSummary};
    use crate::domain::SymbolId;
    use std::path::PathBuf;

    fn function(file: &str, name: &str, calls: &[(&str, &str)]) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("{file}::{name}")),
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 3,
            body: format!("pub fn {name}(input: &str) -> bool {{ true }}"),
            calls: calls
                .iter()
                .map(|(callee, text)| CallFact {
                    line: 2,
                    name: callee.to_string(),
                    text: text.to_string(),
                })
                .collect(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn index(functions: Vec<FunctionSummary>) -> RustIndex {
        RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions,
            ..Default::default()
        })
    }

    #[test]
    fn one_hop_chain_resolves_with_arguments() -> Result<(), String> {
        let owner = function("src/lib.rs", "is_word_start", &[]);
        let caller = function(
            "src/lib.rs",
            "classify",
            &[("is_word_start", "is_word_start(input, 0)")],
        );
        let idx = index(vec![owner, caller]);
        let chain = resolve_chain("is_word_start", &idx, true, &[]);
        if chain.hops.is_empty() {
            return Err(format!(
                "expected a resolved hop, stopped: {:?}",
                chain.stop_above
            ));
        }
        assert_eq!(chain.hops[0].caller.name, "classify");
        assert_eq!(
            chain.hops[0].arguments,
            vec!["input".to_string(), "0".to_string()]
        );
        assert_eq!(chain.entry_name(), Some("classify"));
        Ok(())
    }

    #[test]
    fn non_unique_callee_stops_by_name() -> Result<(), String> {
        let a = function("src/a.rs", "helper", &[]);
        let b = function("src/b.rs", "helper", &[]);
        let idx = index(vec![a, b]);
        let chain = resolve_chain("helper", &idx, true, &[]);
        assert!(chain.hops.is_empty());
        assert!(
            chain
                .stop_above
                .as_ref()
                .is_some_and(|edge| edge.contains("not a unique function"))
        );
        Ok(())
    }

    #[test]
    fn incomplete_workspace_stops_by_name() -> Result<(), String> {
        let owner = function("src/lib.rs", "helper", &[]);
        let idx = index(vec![owner]);
        let chain = resolve_chain("helper", &idx, false, &[]);
        assert!(chain.hops.is_empty());
        assert!(
            chain
                .stop_above
                .as_ref()
                .is_some_and(|edge| edge.contains("workspace-complete"))
        );
        Ok(())
    }

    #[test]
    fn recursion_stops_at_the_bound() -> Result<(), String> {
        let a = function("src/a.rs", "a", &[("b", "b()")]);
        let b = function("src/b.rs", "b", &[("a", "a()")]);
        let idx = index(vec![a, b]);
        let chain = resolve_chain("a", &idx, true, &[]);
        assert!(
            chain
                .stop_above
                .as_ref()
                .is_some_and(|edge| edge.contains("recursion"))
        );
        Ok(())
    }

    #[test]
    fn multiple_callers_stop_the_binding() -> Result<(), String> {
        let owner = function("src/lib.rs", "helper", &[]);
        let first = function("src/lib.rs", "first", &[("helper", "helper()")]);
        let second = function("src/lib.rs", "second", &[("helper", "helper()")]);
        let idx = index(vec![owner, first, second]);
        let chain = resolve_chain("helper", &idx, true, &[]);
        assert!(chain.hops.is_empty());
        assert!(
            chain
                .stop_above
                .as_ref()
                .is_some_and(|edge| edge.contains("2 callers"))
        );
        Ok(())
    }

    // #3296 review B1: a method call site (`word.validate()`) shares
    // the callee's bare name but is not callee identity — it must
    // never bind a hop.
    #[test]
    fn method_call_sites_never_bind() -> Result<(), String> {
        assert!(!is_direct_call_site("word.validate()", "validate"));
        assert!(!is_direct_call_site("internal::inner(a, b)", "inner"));
        // #3713 review (splitter-occurrence thread): the shared
        // occurrence rule looks past the shadowed `outer_inner(` prefix to
        // the direct `inner(2)` site.
        assert!(is_direct_call_site("outer_inner(inner(2))", "inner"));
        assert!(is_direct_call_site(
            "is_word_start(input, 0)",
            "is_word_start"
        ));
        assert!(is_direct_call_site("let x = helper();", "helper"));
        Ok(())
    }

    fn one_hop_chain(caller_body: &str) -> HelperChain {
        let mut caller = function("src/lib.rs", "wrapper", &[]);
        caller.body = caller_body.to_string();
        HelperChain {
            hops: vec![HelperHop {
                caller,
                call_text: String::new(),
                arguments: Vec::new(),
            }],
            stop_above: None,
        }
    }

    // #6694 / #6672: an exact oracle on the wrapper's result stands for the
    // helper's result only when the wrapper hands that result to its return.
    #[test]
    fn chain_forwards_owner_result_accepts_tail_call_and_if_condition() {
        for body in [
            "pub fn capped_score(value: u32) -> u32 {\n    capped(value, 10)\n}",
            "pub fn order_discount(qty: u32) -> u32 {\n    if is_bulk(qty) {\n        5\n    } else {\n        0\n    }\n}",
            "pub fn order_discount(qty: u32) -> u32 {\n    // a comment; with a semicolon\n    if !is_bulk(qty) { 0 } else { 5 }\n}",
            "pub fn w(qty: u32) -> u32 {\n    let _unused = 3;\n    capped(qty, 10)\n}",
        ] {
            let callee = if body.contains("is_bulk") {
                "is_bulk"
            } else {
                "capped"
            };
            assert!(
                chain_forwards_owner_result(callee, &one_hop_chain(body)),
                "{body}"
            );
        }
    }

    #[test]
    fn chain_forwards_owner_result_refuses_shapes_that_drop_or_transform_the_result() {
        for body in [
            // discarded result
            "pub fn w(qty: u32) -> u32 {\n    let _ = is_bulk(qty);\n    5\n}",
            "pub fn w(qty: u32) -> u32 {\n    is_bulk(qty);\n    5\n}",
            // let-bound result (not followed in V1)
            "pub fn w(qty: u32) -> u32 {\n    let bulk = is_bulk(qty);\n    if bulk { 5 } else { 0 }\n}",
            // the call sits in one branch, behind another condition
            "pub fn w(qty: u32) -> bool {\n    if qty == 10 { true } else { is_bulk(qty) }\n}",
            // equal branches never reveal the helper's result
            "pub fn w(qty: u32) -> u32 {\n    if is_bulk(qty) { 5 } else { 5 }\n}",
            // compound condition
            "pub fn w(qty: u32) -> u32 {\n    if is_bulk(qty) && qty > 99 { 5 } else { 0 }\n}",
            // else-if chain
            "pub fn w(qty: u32) -> u32 {\n    if is_bulk(qty) { 5 } else if qty > 3 { 1 } else { 0 }\n}",
            // arithmetic on the result
            "pub fn w(qty: u32) -> bool {\n    is_bulk(qty) || true\n}",
            // early exit before the tail
            "pub fn w(qty: u32) -> bool {\n    if qty == 10 { return true; }\n    is_bulk(qty)\n}",
            "pub fn w(qty: u32) -> Option<bool> {\n    let q = Some(qty)?;\n    Some(is_bulk(q))\n}",
            // method call with the same name is not the hop
            "pub fn w(qty: u32) -> bool {\n    self.is_bulk(qty)\n}",
        ] {
            assert!(
                !chain_forwards_owner_result("is_bulk", &one_hop_chain(body)),
                "{body}"
            );
        }
        assert!(!chain_forwards_owner_result(
            "is_bulk",
            &HelperChain {
                hops: Vec::new(),
                stop_above: None,
            }
        ));
    }

    // #6780 review B1 / B3: rebinding the forwarded parameter or a computed
    // branch value refuses forwarding.
    #[test]
    fn chain_forwards_owner_result_refuses_rebinding_and_computed_branches() {
        let chain_with_args = |body: &str| {
            let mut chain = one_hop_chain(body);
            if let Some(hop) = chain.hops.first_mut() {
                hop.arguments = vec!["qty".to_string()];
            }
            chain
        };
        assert!(chain_forwards_owner_result(
            "is_bulk",
            &chain_with_args(
                "pub fn w(qty: u32) -> u32 {\n    if is_bulk(qty) { 5 } else { 0 }\n}"
            )
        ));
        for body in [
            "pub fn w(qty: u32) -> u32 {\n    let qty = qty * 2;\n    if is_bulk(qty) { 5 } else { 0 }\n}",
            "pub fn w(mut qty: u32) -> u32 {\n    qty = qty + 1;\n    if is_bulk(qty) { 5 } else { 0 }\n}",
            "pub fn w(mut qty: u32) -> u32 {\n    qty <<= 1;\n    if is_bulk(qty) { 5 } else { 0 }\n}",
            "pub fn w(qty: u32) -> u32 {\n    if is_bulk(qty) { qty / 2 } else { 5 }\n}",
            "pub fn w(qty: u32) -> u32 {\n    if is_bulk(qty) { FIVE } else { 0 }\n}",
        ] {
            assert!(
                !chain_forwards_owner_result("is_bulk", &chain_with_args(body)),
                "{body}"
            );
        }
    }

    #[test]
    fn caller_rebinds_parameter_finds_patterns_and_assignments() {
        for body in [
            "fn w(qty: u32) -> u32 { let qty = 2; qty }",
            "fn w(qty: u32) -> u32 { let (a, qty) = (1, 2); qty }",
            "fn w(mut qty: u32) -> u32 { qty -= 1; qty }",
            "fn w(qty: u32) -> u32 { for qty in 0..3 {} 1 }",
            "fn w(qty: Option<u32>) -> u32 { match qty { Some(qty) => qty, None => 0 } }",
            "fn w(qty: u32) -> u32 { [1].iter().map(|qty| qty + 1).sum() }",
            "fn w(qty: u32) -> u32 { match qty { n @ 1..=3 => n, qty @ _ => qty } }",
            // A guarded arm whose pattern binds the parameter still rebinds.
            "fn w(qty: Option<u32>) -> u32 { match qty { Some(qty) if qty > 3 => qty, _ => 0 } }",
            "fn w(qty: Line) -> u32 { match qty { Line { qty } => qty } }",
            "fn w(qty: u32, n: Line) -> u32 { match n { Line { qty, .. } if qty > 1 => qty, _ => 0 } }",
        ] {
            assert!(caller_rebinds_parameter(body, "qty"), "{body}");
        }
        for body in [
            "fn w(qty: u32) -> u32 { if is_bulk(qty) { 5 } else { 0 } }",
            "fn w(qty: u32) -> bool { qty <= 3 || qty == 9 || qty >= 7 }",
            "fn w(qty: u32) -> u32 { let other = qty + 1; other }",
            // CodeRabbit (#6780): a match guard only reads the parameter.
            "fn w(qty: u32, n: u32) -> u32 { match n { n if n > qty => 1, _ => 0 } }",
            "fn w(qty: u32, n: u32) -> u32 { match n { 0 => 0, m if is_bulk(qty) => m, _ => 1 } }",
            // A guard-like word inside the pattern is not a guard.
            "fn w(qty: u32, n: Diff) -> u32 { match n { Diff { iff } if iff > qty => 1, _ => 0 } }",
        ] {
            assert!(!caller_rebinds_parameter(body, "qty"), "{body}");
        }
    }

    #[test]
    fn chain_forwards_owner_result_checks_every_hop() {
        let mut lower = function("src/lib.rs", "middle", &[]);
        lower.body = "fn middle(qty: u32) -> bool {\n    is_bulk(qty)\n}".to_string();
        let mut upper = function("src/lib.rs", "entry", &[]);
        upper.body =
            "pub fn entry(qty: u32) -> u32 {\n    if middle(qty) { 5 } else { 0 }\n}".to_string();
        let hop = |caller: FunctionSummary| HelperHop {
            caller,
            call_text: String::new(),
            arguments: Vec::new(),
        };
        let chain = HelperChain {
            hops: vec![hop(lower.clone()), hop(upper)],
            stop_above: None,
        };
        assert!(chain_forwards_owner_result("is_bulk", &chain));
        let mut dropping = function("src/lib.rs", "entry", &[]);
        dropping.body =
            "pub fn entry(qty: u32) -> u32 {\n    let _ = middle(qty);\n    5\n}".to_string();
        let chain = HelperChain {
            hops: vec![hop(lower), hop(dropping)],
            stop_above: None,
        };
        assert!(!chain_forwards_owner_result("is_bulk", &chain));
    }

    // #3296 review B2: only whole-token literals bind; an identifier
    // like `a2` is never the literal `2`.
    #[test]
    fn strict_literals_reject_identifiers() -> Result<(), String> {
        assert_eq!(strict_literal("\"ab\"").as_deref(), Some("\"ab\""));
        assert_eq!(strict_literal("'x'").as_deref(), Some("'x'"));
        assert_eq!(strict_literal("42").as_deref(), Some("42"));
        assert_eq!(strict_literal("true").as_deref(), Some("true"));
        assert_eq!(strict_literal("a2"), None);
        assert_eq!(strict_literal("input"), None);
        assert_eq!(strict_literal("input.trim()"), None);
        Ok(())
    }

    #[test]
    fn test_reaches_through_the_entry_hop() -> Result<(), String> {
        let owner = function("src/lib.rs", "is_word_start", &[]);
        let caller = function(
            "src/lib.rs",
            "classify",
            &[("is_word_start", "is_word_start(input, 0)")],
        );
        let idx = index(vec![owner.clone(), caller]);
        let test_calls = vec![CallFact {
            line: 1,
            name: "classify".to_string(),
            text: "classify(\" x\")".to_string(),
        }];
        assert!(test_reaches_through_chain(&test_calls, "is_word_start", &idx, true).is_some());
        let unrelated = vec![CallFact {
            line: 1,
            name: "other".to_string(),
            text: "other()".to_string(),
        }];
        assert!(test_reaches_through_chain(&unrelated, "is_word_start", &idx, true).is_none());
        Ok(())
    }

    #[test]
    fn helper_return_value_evaluates_identity_tails_and_fails_closed_otherwise()
    -> Result<(), String> {
        let mut inputs = super::super::value_transfer::ExactInputs::new();
        inputs.insert("input".to_string(), "\" x\"".to_string());
        // An identity closure evaluates through the #3295 families.
        let evaluated = FunctionSummary {
            body: "pub fn first_char(input: &str) -> char {
    let prev = input.chars().next().map_or('?', |c| c);
    prev
}
"
            .to_string(),
            start_line: 1,
            ..function("src/lib.rs", "first_char", &[])
        };
        let empty = RustIndex::default();
        let eval = HelperEval::root(&empty, true);
        match helper_return_value(&evaluated, &inputs, &eval) {
            Some(value) => assert_eq!(value.render(), "' '"),
            None => return Err("expected the identity tail to evaluate".to_string()),
        }
        // A non-identity closure is an unsupported edge: the return
        // fails closed instead of guessing a boolean.
        let closed = FunctionSummary {
            body: "pub fn is_word_start(input: &str) -> bool {
    let prev = input.chars().next().map_or(true, |c| c == ' ');
    prev
}
"
            .to_string(),
            start_line: 1,
            ..function("src/lib.rs", "is_word_start", &[])
        };
        assert!(helper_return_value(&closed, &inputs, &eval).is_none());
        Ok(())
    }

    #[test]
    fn shadowed_first_occurrence_still_resolves_direct_site() -> Result<(), String> {
        // #3713 review: the matcher and the splitter share one
        // occurrence-resolution rule, so a shadowed first occurrence no
        // longer hides a direct call later on the same line.
        if !is_direct_call_site("my_inner(2); inner(1)", "inner") {
            return Err("expected the second occurrence to qualify".to_string());
        }
        match split_call_arguments_text("my_inner(2); inner(1)", "inner") {
            Some(arguments) if arguments == vec!["1".to_string()] => Ok(()),
            other => Err(format!("expected [\"1\"], got {other:?}")),
        }
    }

    #[test]
    fn non_ascii_callee_after_shadowed_occurrence_does_not_panic() -> Result<(), String> {
        // A shadowed first occurrence of a callee whose name starts with a
        // multibyte char must advance by that char's width, not one byte.
        if is_direct_call_site("my_заказ(2);", "заказ") {
            return Err("a prefixed occurrence must not qualify".to_string());
        }
        match split_call_arguments_text("my_заказ(2); заказ(1)", "заказ") {
            Some(arguments) if arguments == vec!["1".to_string()] => Ok(()),
            other => Err(format!("expected [\"1\"], got {other:?}")),
        }
    }

    #[test]
    fn nested_call_splits_inner_arguments() -> Result<(), String> {
        match split_call_arguments_text("foo(inner(2))", "inner") {
            Some(arguments) if arguments == vec!["2".to_string()] => Ok(()),
            other => Err(format!("expected [\"2\"], got {other:?}")),
        }
    }

    #[test]
    fn receiver_path_and_spaced_calls_stay_rejected() -> Result<(), String> {
        for (text, callee) in [
            ("word.validate()", "validate"),
            ("internal::inner(..)", "inner"),
            ("inner (2)", "inner"),
            ("my_inner(2)", "inner"),
        ] {
            if is_direct_call_site(text, callee) {
                return Err(format!("expected no direct site in {text:?}"));
            }
            if split_call_arguments_text(text, callee).is_some() {
                return Err(format!("expected no split in {text:?}"));
            }
        }
        Ok(())
    }
}
