//! RIPR-SPEC-0197 rule 3 short-circuit tails (#7969 grid cell): a bool
//! owner whose whole body is one `&&` chain or one `||` chain of
//! comparisons over its parameters.
//!
//! Rule 3 refuses `&&`/`||` because a pin may never evaluate the changed
//! part. `x == 0 && y == 1` pinned only at `(1, 5)` returns `false` from
//! `x == 0` alone; the changed `y == 1` never runs. This gate admits the
//! chain only when ripr can show that some pinned call evaluates every
//! operand: the test passes literal arguments (directly, through a
//! constant-row table, or through a check helper's calls), and on one of
//! those runs every operand but the last takes the value that does not
//! short-circuit (`true` for `&&`, `false` for `||`).
//!
//! Each operand is a bool parameter after any number of `!`, or one
//! comparison of two atoms with no leading `!` (`!x == 0` is `(!x) == 0`).
//! An atom is a parameter, a plain integer or bool literal, an unescaped
//! char literal, or a parameter `%` or `/` a nonzero integer literal. These operands cannot call, panic or
//! write, so skipping one changes nothing but the value. A chain mixing
//! `&&` and `||`, a call, an index, a field, a cast, a parenthesis or any
//! other text is not read (fail closed).

use super::closure_adaptor::{call_count, literal_boundary, top_level_arguments};
use super::whole_word_offsets;
use crate::analysis::extract::{mask_comments_and_strings, rust_identity_spans};
use crate::analysis::rust_index::TestSummary;
use crate::analysis::syntax::{HelperLoan, constant_table_column_around};

/// The admitted chain: its operator, the owner's parameters in order, and
/// its operands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) struct ShortCircuitTail {
    /// `&&` or `||`.
    operator: &'static str,
    parameters: Vec<String>,
    operands: Vec<Operand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Operand {
    negations: usize,
    kind: OperandKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OperandKind {
    /// A bool parameter.
    Flag(usize),
    Compare {
        left: Atom,
        operator: &'static str,
        right: Atom,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Atom {
    Parameter(usize),
    Literal(String),
    /// A parameter `%` or `/` a nonzero integer literal.
    Arithmetic {
        parameter: usize,
        operator: char,
        divisor: i128,
    },
}

const COMPARISONS: &[&str] = &["==", "!=", "<=", ">=", "<", ">"];

impl ShortCircuitTail {
    /// The chain when `changed` (the owner's whole tail) is one, and
    /// `function` (the owner's text from `fn`) declares plain identifier
    /// parameters with no receiver.
    pub(in crate::analysis::classify) fn parse(function: &str, changed: &str) -> Option<Self> {
        let parameters = parameter_names(function)?;
        // The chain must be the whole body: an earlier statement could
        // rebind a parameter (`let x = x + 1;`), and then the call's
        // arguments are not the values the chain reads.
        if !is_whole_body(function, changed) {
            return None;
        }
        let chain = Self::from_chain(parameters, changed)?;
        // #15030: a string literal compares by its text only against a plain
        // string parameter; another type may carry its own
        // `PartialEq<&str>`, so its `==` is not text equality.
        let types = parameter_types(function)?;
        chain
            .operands
            .iter()
            .all(|operand| operand.string_compare_is_plain(&types))
            .then_some(chain)
    }

    /// The owner's parameters in declaration order.
    pub(in crate::analysis::classify) fn parameters(&self) -> &[String] {
        &self.parameters
    }

    /// `chain` over the same parameters: the pre-edit chain of an edit that
    /// `self` is the post-edit chain of.
    pub(in crate::analysis::classify) fn with_chain(&self, chain: &str) -> Option<Self> {
        Self::from_chain(self.parameters.clone(), chain)
    }

    /// The chain `changed` over the named `parameters`, with no claim about
    /// where the chain sits in an owner's body.
    pub(in crate::analysis::classify) fn from_chain(
        parameters: Vec<String>,
        changed: &str,
    ) -> Option<Self> {
        // Split where the masked text holds the operator, so a `&&` inside
        // a string or comment is never one; read operands from the original
        // text, which masking keeps aligned byte for byte.
        let masked = mask_comments_and_strings(changed);
        if masked.len() != changed.len() || masked.contains("//") || masked.contains("/*") {
            return None;
        }
        let operator = match (masked.contains("&&"), masked.contains("||")) {
            (true, false) => "&&",
            (false, true) => "||",
            _ => return None,
        };
        let mut operands = Vec::new();
        let mut start = 0;
        for (at, _) in masked.match_indices(operator) {
            operands.push(parse_operand(&changed[start..at], &parameters)?);
            start = at + operator.len();
        }
        operands.push(parse_operand(&changed[start..], &parameters)?);
        (operands.len() >= 2).then_some(Self {
            operator,
            parameters,
            operands,
        })
    }

    /// #7970: whether `self` (the edited chain) differs from `before` in
    /// some operands and, on every run in `runs`, none of those operands is
    /// evaluated. `||` returns at the first `true` operand and `&&` at the
    /// first `false` one, so an operand is evaluated only when every earlier
    /// operand took the value that does not short-circuit.
    ///
    /// Fail closed toward `false` (the question stays open): `true` needs the
    /// same operator, parameters and operand count, at least one changed
    /// operand, at least one run, and on every run all arguments readable and
    /// some operand before the earliest changed one short-circuiting.
    pub(in crate::analysis::classify) fn changed_operands_never_evaluated(
        &self,
        before: &Self,
        runs: &[Vec<Option<String>>],
    ) -> bool {
        if self.operator != before.operator
            || self.parameters != before.parameters
            || self.operands.len() != before.operands.len()
            || runs.is_empty()
        {
            return false;
        }
        // The earliest changed operand is evaluated whenever any changed
        // one is, so the operands before it decide every run.
        let Some(first) =
            (0..self.operands.len()).find(|&at| self.operands[at] != before.operands[at])
        else {
            return false;
        };
        let continues = self.operator == "&&";
        runs.iter().all(|values| {
            if !values.iter().all(Option::is_some) {
                return false;
            }
            for operand in &self.operands[..first] {
                match operand.evaluate(values) {
                    Some(value) if value == continues => {}
                    Some(_) => return true,
                    None => return false,
                }
            }
            false
        })
    }

    /// #6488: whether `runs` (one per owner call, literal arguments) form a
    /// truth table that tells this all-bool chain from every one-step
    /// mutation of it: the other connective, the constant `true`, the
    /// constant `false`, and the chain with any one operand negated. Each
    /// needs a run on which its value differs from the chain's.
    ///
    /// Fail closed toward `false`: a comparison operand (its boundary is a
    /// literal question, not a truth-table one), no run, a run with an
    /// unreadable argument, or any mutation the runs cannot separate.
    pub(in crate::analysis::classify) fn runs_tell_apart_one_step_mutants(
        &self,
        runs: &[Vec<Option<String>>],
    ) -> bool {
        if runs.is_empty()
            || !self
                .operands
                .iter()
                .all(|operand| matches!(operand.kind, OperandKind::Flag(_)))
            || !runs.iter().all(|values| values.iter().all(Option::is_some))
        {
            return false;
        }
        let evaluate = |negated: Option<usize>, connective: &str, values: &[Option<String>]| {
            let mut operands = self.operands.iter().enumerate().map(|(at, operand)| {
                operand
                    .evaluate(values)
                    .map(|value| if negated == Some(at) { !value } else { value })
            });
            let mut folded = operands.next().flatten()?;
            for operand in operands {
                let operand = operand?;
                folded = if connective == "&&" {
                    folded && operand
                } else {
                    folded || operand
                };
            }
            Some(folded)
        };
        let other = if self.operator == "&&" { "||" } else { "&&" };
        let separates = |mutant: &dyn Fn(&[Option<String>]) -> Option<bool>| {
            runs.iter().any(|values| {
                match (evaluate(None, self.operator, values), mutant(values)) {
                    (Some(chain), Some(mutated)) => chain != mutated,
                    _ => false,
                }
            })
        };
        separates(&|values| evaluate(None, other, values))
            && separates(&|_| Some(true))
            && separates(&|_| Some(false))
            && (0..self.operands.len())
                .all(|at| separates(&|values| evaluate(Some(at), self.operator, values)))
    }

    /// Whether some run of the pinned call `call_text` (of `name`)
    /// evaluates every operand. Every argument the chain reads must be
    /// readable on that run; an unreadable run never counts.
    pub(in crate::analysis::classify) fn pinned_call_evaluates_every_operand(
        &self,
        test: &TestSummary,
        loan: Option<&HelperLoan>,
        call_text: &str,
        call_line: usize,
        name: &str,
    ) -> bool {
        let Some(arguments) = call_arguments(call_text, name) else {
            return false;
        };
        if arguments.len() != self.parameters.len() {
            return false;
        }
        let Some(runs) = argument_runs(test, loan, &arguments, call_line, name) else {
            return false;
        };
        let continues = self.operator == "&&";
        // A run counts only when every argument reads, the last operand's
        // included: the spec's run check reads the whole call.
        runs.iter().any(|values| {
            values.iter().all(Option::is_some)
                && self.operands[..self.operands.len() - 1]
                    .iter()
                    .all(|operand| operand.evaluate(values) == Some(continues))
        })
    }
}

/// #8361: an edit whose only difference is whole operands of one top-level
/// `&&` chain or one `||` chain (a dropped or an added guard), in a bool
/// owner whose whole body is that tail.
///
/// `g && x` and `x` return the same value on every input where `g` is
/// `true`, and `g || x` and `x` wherever `g` is `false`. A pinned call
/// whose literal arguments give every such operand that value runs both
/// versions to the same result, so no assertion on that call can notice the
/// edit. The operands that differ are read as [`ShortCircuitTail`] operands
/// (a bool parameter or one comparison over parameters and literals: they
/// cannot call, panic or write); the operands both versions keep are
/// compared as text and never read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) struct GuardOperandChange {
    /// `&&` or `||`.
    operator: &'static str,
    parameters: Vec<String>,
    /// The operands only one version holds.
    differing: Vec<Operand>,
}

impl GuardOperandChange {
    /// The change when `changed` (the owner's whole tail) and `before` (its
    /// pre-edit text) are the same chain but for whole top-level operands
    /// that only one of them holds. `None` for anything else: a rewritten
    /// operand, both operators, a differing operand inside a parenthesis or
    /// one this module cannot evaluate.
    pub(in crate::analysis::classify) fn read(
        function: &str,
        changed: &str,
        before: &str,
    ) -> Option<Self> {
        let parameters = parameter_names(function)?;
        // As for `ShortCircuitTail::parse`: an earlier statement could
        // rebind a parameter, so the call's arguments must be what the
        // operands read.
        if !is_whole_body(function, changed) {
            return None;
        }
        let before = before.trim().trim_end_matches(';').trim();
        let before = before.strip_prefix("return ").map_or(before, str::trim);
        // #14194 review: the direction is chosen by operand structure, not by
        // byte length. Layout whitespace in the retained operand can make the
        // shorter chain the longer string, which inverted the direction and
        // silently skipped the rule (fail-open). Trying both orders and keeping
        // the one that reads removes the byte-length guess entirely.
        let attempt = |longer: &str, shorter: &str| -> Option<(&'static str, Vec<Operand>)> {
            let masked = mask_comments_and_strings(longer);
            if masked.len() != longer.len() || masked.contains("//") || masked.contains("/*") {
                return None;
            }
            let operator = match (masked.contains("&&"), masked.contains("||")) {
                (true, false) => "&&",
                (false, true) => "||",
                _ => return None,
            };
            let longer_operands = top_level_operands(longer, operator)?;
            let shorter_operands = top_level_operands(shorter, operator)?;
            if shorter_operands.len() >= longer_operands.len() {
                return None;
            }
            // The shorter version's operands must be the longer one's, in
            // order, with nothing rewritten; what is left over differs.
            // They are compared as Rust token sequences (#15149): whitespace
            // between tokens is layout, but it still separates words
            // (`a as u8` is not `aasu8`), and inside a string literal it is
            // content, so `check(n, "a  b")` and `check(n, "a b")` are
            // different operands (#14194 review).
            let mut kept = shorter_operands
                .iter()
                .map(|text| operand_tokens(text))
                .peekable();
            let mut differing = Vec::new();
            for operand in &longer_operands {
                if kept
                    .peek()
                    .is_some_and(|next| *next == operand_tokens(operand))
                {
                    kept.next();
                } else {
                    differing.push(parse_operand(operand, &parameters)?);
                }
            }
            (kept.next().is_none() && !differing.is_empty()).then_some((operator, differing))
        };
        let (operator, differing) =
            attempt(before, changed).or_else(|| attempt(changed, before))?;
        Some(Self {
            operator,
            parameters,
            differing,
        })
    }

    /// Whether both versions provably return the same value on every run
    /// of the pinned call `call_text` (of `name`): every argument reads and
    /// every differing operand takes the value that leaves its chain
    /// unchanged. An unreadable run, or no run, answers `false` (the pin
    /// keeps its current reading).
    pub(in crate::analysis::classify) fn pinned_call_agrees(
        &self,
        test: &TestSummary,
        loan: Option<&HelperLoan>,
        call_text: &str,
        call_line: usize,
        name: &str,
    ) -> bool {
        let Some(arguments) = call_arguments(call_text, name) else {
            return false;
        };
        if arguments.len() != self.parameters.len() {
            return false;
        }
        let Some(runs) = argument_runs(test, loan, &arguments, call_line, name) else {
            return false;
        };
        self.runs_agree(&runs)
    }

    fn runs_agree(&self, runs: &[Vec<Option<String>>]) -> bool {
        let identity = self.operator == "&&";
        !runs.is_empty()
            && runs.iter().all(|values| {
                values.iter().all(Option::is_some)
                    && self
                        .differing
                        .iter()
                        .all(|operand| operand.evaluate(values) == Some(identity))
            })
    }
}

/// `text` as the sequence of words, literals and punctuation runs it holds.
/// Layout whitespace separates words but is not part of an operand's
/// identity, so `n % 2 == 0` and `n%2==0` are the same sequence. Word
/// boundaries stay significant: `a as u8` is three words and `aasu8` one, so
/// an edit that merges an identifier, keyword, type or number across a space
/// is a rewritten operand, not a retained one (#15149). Adjacent punctuation
/// is one run whatever the spacing inside it (`Vec<Vec<u8> >` and
/// `Vec<Vec<u8>>`, `> =` and `>=`), because splitting it into Rust tokens
/// would read some layout-only edits as rewrites and so drop the
/// withholding (#15537 review). Every literal is one entry copied verbatim,
/// since whitespace inside it is content. The literal ranges come from the
/// shared lexical scanner, so a quote inside a char literal (`'"'`) or a raw
/// string cannot flip the string tracking (#14194 review).
fn operand_tokens(text: &str) -> Vec<String> {
    let spans = rust_identity_spans(text);
    let mut tokens = Vec::new();
    let mut punctuation = String::new();
    let mut at = 0;
    for (start, end) in spans.literal_ranges {
        if start < at || end >= text.len() {
            continue;
        }
        push_code_tokens(&text[at..start], &mut punctuation, &mut tokens);
        flush_punctuation(&mut punctuation, &mut tokens);
        tokens.push(text[start..=end].to_string());
        at = end + 1;
    }
    push_code_tokens(&text[at..], &mut punctuation, &mut tokens);
    flush_punctuation(&mut punctuation, &mut tokens);
    tokens
}

fn flush_punctuation(punctuation: &mut String, tokens: &mut Vec<String>) {
    if !punctuation.is_empty() {
        tokens.push(std::mem::take(punctuation));
    }
}

/// The words of `code`, a stretch of operand text that holds no literal,
/// with the punctuation between them gathered into `punctuation` runs. A
/// word is a run of identifier characters, digits included; a `.` is always
/// punctuation, so `1.5` is `1`, `.`, `5`. Absorbing a decimal point into a
/// number read the `0.0` of `x.0.0` as one word but `x.0 .0` as two, so a
/// tuple-field spacing edit looked like a rewrite and dropped the withholding
/// (#15537 review). Whitespace inside a punctuation run is dropped, so the
/// run continues across it.
fn push_code_tokens(code: &str, punctuation: &mut String, tokens: &mut Vec<String>) {
    let is_word = |ch: char| ch == '_' || ch.is_alphanumeric();
    let mut rest = code;
    while let Some(first) = rest.chars().next() {
        if first.is_whitespace() {
            rest = &rest[first.len_utf8()..];
            continue;
        }
        if !is_word(first) {
            punctuation.push(first);
            rest = &rest[first.len_utf8()..];
            continue;
        }
        let end = rest.find(|ch: char| !is_word(ch)).unwrap_or(rest.len());
        flush_punctuation(punctuation, tokens);
        tokens.push(rest[..end].to_string());
        rest = &rest[end..];
    }
}

/// `text` split at each `operator` outside every parenthesis, bracket and
/// brace. `None` when `operator` also appears inside one (the chain is then
/// not one flat list of operands) or the delimiters do not balance.
fn top_level_operands<'a>(text: &'a str, operator: &str) -> Option<Vec<&'a str>> {
    let masked = mask_comments_and_strings(text);
    if masked.len() != text.len() {
        return None;
    }
    let bytes = masked.as_bytes();
    let mut operands = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            _ if bytes[at..].starts_with(operator.as_bytes()) => {
                if depth != 0 {
                    return None;
                }
                operands.push(&text[start..at]);
                at += operator.len();
                start = at;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    if depth != 0 {
        return None;
    }
    operands.push(&text[start..]);
    Some(operands)
}

impl Operand {
    /// Whether a comparison with a string literal compares a parameter
    /// declared as a plain string type (`&str`, `String`, `&String`) by
    /// `==` or `!=`.
    fn string_compare_is_plain(&self, types: &[String]) -> bool {
        let OperandKind::Compare {
            left,
            operator,
            right,
        } = &self.kind
        else {
            return true;
        };
        let is_string = |atom: &Atom| match atom {
            Atom::Literal(literal) => matches!(Value::read(literal), Some(Value::Str(_))),
            _ => false,
        };
        if !is_string(left) && !is_string(right) {
            return true;
        }
        // Only text equality is read; string ordering is not.
        matches!(*operator, "==" | "!=")
            && [left, right].into_iter().all(|atom| match atom {
                Atom::Parameter(parameter) => types
                    .get(*parameter)
                    .is_some_and(|ty| super::super::infection::is_plain_string_type(ty)),
                Atom::Literal(_) => true,
                Atom::Arithmetic { .. } => false,
            })
    }

    fn evaluate(&self, values: &[Option<String>]) -> Option<bool> {
        let value = match &self.kind {
            OperandKind::Flag(parameter) => match values.get(*parameter)?.as_deref()? {
                "true" => true,
                "false" => false,
                _ => return None,
            },
            OperandKind::Compare {
                left,
                operator,
                right,
            } => compare(&left.value(values)?, operator, &right.value(values)?)?,
        };
        Some(if self.negations.is_multiple_of(2) {
            value
        } else {
            !value
        })
    }
}

impl Atom {
    fn value(&self, values: &[Option<String>]) -> Option<Value> {
        match self {
            Self::Parameter(parameter) => Value::read(values.get(*parameter)?.as_deref()?),
            Self::Literal(literal) => Value::read(literal),
            Self::Arithmetic {
                parameter,
                operator,
                divisor,
            } => {
                let Value::Integer(value) = Value::read(values.get(*parameter)?.as_deref()?)?
                else {
                    return None;
                };
                Some(Value::Integer(match operator {
                    '%' => value.checked_rem(*divisor)?,
                    _ => value.checked_div(*divisor)?,
                }))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Integer(i128),
    Bool(bool),
    Char(String),
    /// #15030: an escape-free string literal's text.
    Str(String),
}

impl Value {
    /// A plain integer, bool or char literal; floats are not read, so no
    /// rounding question arises.
    fn read(text: &str) -> Option<Self> {
        if let Some(body) = plain_string_body(text) {
            return Some(Self::Str(body.to_string()));
        }
        let text = literal_boundary(text)?;
        match text.as_str() {
            "true" => return Some(Self::Bool(true)),
            "false" => return Some(Self::Bool(false)),
            _ => {}
        }
        if let Some(body) = text
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
        {
            // Only an unescaped char compares by its text: `'\n'` and a
            // decoded escape could name one char two ways.
            let mut chars = body.chars();
            return match (chars.next(), chars.next()) {
                (Some(character), None) if character != '\\' => {
                    Some(Self::Char(character.to_string()))
                }
                _ => None,
            };
        }
        if text.contains('.') {
            return None;
        }
        text.replace('_', "").parse().ok().map(Self::Integer)
    }
}

fn compare(left: &Value, operator: &str, right: &Value) -> Option<bool> {
    let ordering = match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => left.cmp(right),
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Char(left), Value::Char(right)) | (Value::Str(left), Value::Str(right))
            if matches!(operator, "==" | "!=") =>
        {
            left.cmp(right)
        }
        _ => return None,
    };
    Some(match operator {
        "==" => ordering.is_eq(),
        "!=" => ordering.is_ne(),
        "<" => ordering.is_lt(),
        "<=" => ordering.is_le(),
        ">" => ordering.is_gt(),
        ">=" => ordering.is_ge(),
        _ => return None,
    })
}

fn parse_operand(text: &str, parameters: &[String]) -> Option<Operand> {
    let mut text = text.trim();
    let mut negations = 0;
    while let Some(rest) = text.strip_prefix('!') {
        if rest.starts_with('=') {
            return None;
        }
        negations += 1;
        text = rest.trim_start();
    }
    // Find the operator outside string literals, so `f == "<"` splits at
    // its `==`.
    let masked = mask_comments_and_strings(text);
    if masked.len() != text.len() {
        return None;
    }
    for operator in COMPARISONS {
        if let Some(at) = masked.find(operator) {
            // `!x == 0` is `(!x) == 0`: the `!` binds to the atom, and on an
            // integer it is a bitwise NOT. Only a bare bool parameter takes
            // a leading `!`.
            if negations > 0 {
                return None;
            }
            let left = parse_atom(&text[..at], parameters)?;
            let right = parse_atom(&text[at + operator.len()..], parameters)?;
            // `a == b` of two literals, or no parameter at all, reads nothing
            // the test chooses.
            if matches!((&left, &right), (Atom::Literal(_), Atom::Literal(_))) {
                return None;
            }
            return Some(Operand {
                negations,
                kind: OperandKind::Compare {
                    left,
                    operator,
                    right,
                },
            });
        }
    }
    let parameter = parameters.iter().position(|name| name == text)?;
    Some(Operand {
        negations,
        kind: OperandKind::Flag(parameter),
    })
}

fn parse_atom(text: &str, parameters: &[String]) -> Option<Atom> {
    let text = text.trim();
    // A literal is read before the arithmetic split: a char literal like
    // `'/'` holds the operator character, and splitting it would reject the
    // operand before the literal reader (#14194 review).
    if let Some(literal) = literal_boundary(text) {
        return Value::read(&literal).map(|_| Atom::Literal(literal));
    }
    // #15030: an escape-free string literal is read by its text, before
    // the split too (`"a/b"`, `"100%"`; Devin review on #15160).
    if plain_string_body(text).is_some() {
        return Some(Atom::Literal(text.to_string()));
    }
    for operator in ['%', '/'] {
        if let Some((left, right)) = text.split_once(operator) {
            let parameter = parameters.iter().position(|name| name == left.trim())?;
            let right = right.trim();
            if !right
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'_')
            {
                return None;
            }
            let divisor: i128 = right.replace('_', "").parse().ok()?;
            return (divisor != 0).then_some(Atom::Arithmetic {
                parameter,
                operator,
                divisor,
            });
        }
    }
    parameters
        .iter()
        .position(|name| name == text)
        .map(Atom::Parameter)
}

/// The text of a plain string literal `"..."` with no escape and no inner
/// quote, so its text is its value. Raw and byte strings are not read.
fn plain_string_body(text: &str) -> Option<&str> {
    let body = text.trim().strip_prefix('"')?.strip_suffix('"')?;
    (!body.contains(['\\', '"'])).then_some(body)
}

/// The owner's parameter types in declaration order, as written.
fn parameter_types(function: &str) -> Option<Vec<String>> {
    let masked = mask_comments_and_strings(function);
    let name_end = masked.find("fn ")? + 3;
    let open = name_end + masked[name_end..].find('(')?;
    let close = super::matching_close(&masked, open, b'(', b')')?;
    top_level_arguments(&function[open + 1..close])
        .into_iter()
        .filter(|parameter| !parameter.trim().is_empty())
        .map(|parameter| {
            parameter
                .split_once(':')
                .map(|(_, ty)| ty.trim().to_string())
        })
        .collect()
}

/// Whether `changed` is all the owner's body holds between its braces,
/// ignoring comments and whitespace.
fn is_whole_body(function: &str, changed: &str) -> bool {
    let masked = mask_comments_and_strings(function);
    let Some(close) = masked.rfind('}') else {
        return false;
    };
    let Some(open) = super::body_block_open(&masked) else {
        return false;
    };
    let compact = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    open < close
        && compact(&masked[open + 1..close]) == compact(&mask_comments_and_strings(changed))
}

/// The owner's parameter names, when each is a plain identifier pattern
/// and there is no `self` receiver.
fn parameter_names(function: &str) -> Option<Vec<String>> {
    let masked = mask_comments_and_strings(function);
    let name_end = masked.find("fn ")? + 3;
    let open = name_end + masked[name_end..].find('(')?;
    if masked[name_end..open].contains('<') {
        return None;
    }
    let close = super::matching_close(&masked, open, b'(', b')')?;
    let inside = masked[open + 1..close].trim();
    if inside.is_empty() {
        return None;
    }
    top_level_arguments(inside)
        .into_iter()
        .filter(|parameter| !parameter.trim().is_empty())
        .map(|parameter| {
            let (pattern, _) = parameter.split_once(':')?;
            let pattern = pattern.trim();
            (super::is_plain_identifier(pattern) && pattern != "self" && pattern != "_")
                .then(|| pattern.to_string())
        })
        .collect()
}

/// The top-level argument texts of the pinned call, which must be the only
/// call of `name` in `call_text`.
fn call_arguments(call_text: &str, name: &str) -> Option<Vec<String>> {
    if call_count(call_text, name) != 1 {
        return None;
    }
    let masked = mask_comments_and_strings(call_text);
    let open = whole_word_offsets(&masked, name)
        .into_iter()
        .map(|offset| offset + name.len())
        .find(|end| masked[*end..].starts_with('('))?;
    let close = super::matching_close(&masked, open, b'(', b')')?;
    let inside = call_text[open + 1..close].trim();
    if inside.is_empty() {
        return Some(Vec::new());
    }
    Some(top_level_arguments(inside))
}

/// One value per argument for each run of the pinned call: `None` for an
/// argument ripr cannot read on that run. A literal argument has its value
/// on every run. A name is a check helper's parameter, read at each of the
/// test's eager calls of the helper, or else a column of a constant-row
/// `for` table, read per row. `None` when the runs cannot be aligned.
fn argument_runs(
    test: &TestSummary,
    loan: Option<&HelperLoan>,
    arguments: &[String],
    call_line: usize,
    name: &str,
) -> Option<Vec<Vec<Option<String>>>> {
    enum Source {
        Literal(String),
        Column(Vec<String>),
        Unread,
    }
    let mut sources = Vec::new();
    let mut rows: Option<usize> = None;
    for argument in arguments {
        let argument = argument.trim();
        let source = if let Some(literal) = literal_boundary(argument) {
            Source::Literal(literal)
        } else if plain_string_body(argument).is_some() {
            // #15030: only a literal written at the call reads as a string;
            // table and helper columns keep the non-string subset.
            Source::Literal(argument.to_string())
        } else if super::is_plain_identifier(argument) {
            let column = match loan {
                Some(loan) => helper_column(test, loan, argument),
                None => call_offset(test, call_line, name)
                    .and_then(|at| constant_table_column_around(&test.body, argument, at)),
            };
            match column {
                Some(column) => {
                    if rows.is_some_and(|rows| rows != column.len()) {
                        return None;
                    }
                    rows = Some(column.len());
                    Source::Column(column)
                }
                None => Source::Unread,
            }
        } else {
            Source::Unread
        };
        sources.push(source);
    }
    let rows = rows.unwrap_or(1);
    Some(
        (0..rows)
            .map(|row| {
                sources
                    .iter()
                    .map(|source| match source {
                        Source::Literal(literal) => Some(literal.clone()),
                        Source::Column(column) => {
                            column.get(row).and_then(|cell| literal_boundary(cell))
                        }
                        Source::Unread => None,
                    })
                    .collect()
            })
            .collect(),
    )
}

/// The texts `parameter` of the check helper takes at each of the test's
/// eager calls of it, or `None` when any call is unreadable.
/// The byte offset in `test.body` of the owner call on absolute line
/// `call_line`, so a table column counts only for a call inside its loop.
fn call_offset(test: &TestSummary, call_line: usize, name: &str) -> Option<usize> {
    let index = call_line.checked_sub(test.start_line)?;
    let mut offset = 0;
    for (number, line) in test.body.split_inclusive('\n').enumerate() {
        if number == index {
            let column = line
                .find(&format!("{name}("))
                .unwrap_or(line.len() - line.trim_start().len());
            return Some(offset + column);
        }
        offset += line.len();
    }
    None
}

fn helper_column(test: &TestSummary, loan: &HelperLoan, parameter: &str) -> Option<Vec<String>> {
    let index = loan.parameters.iter().position(|name| name == parameter)?;
    if loan.call_lines.is_empty() {
        return None;
    }
    loan.call_lines
        .iter()
        .map(|line| {
            let text = test.body.lines().nth(line.checked_sub(test.start_line)?)?;
            call_arguments(text, &loan.name)?.get(index).cloned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail(signature: &str, changed: &str) -> Option<ShortCircuitTail> {
        ShortCircuitTail::parse(&format!("{signature}\n    {changed}\n}}"), changed)
    }

    #[test]
    fn reads_homogeneous_comparison_chains_over_parameters() {
        for (function, changed) in [
            ("fn is_origin(x: i32, y: i32) -> bool {", "y == 0 && x == 0"),
            (
                "fn is_weekend(day: u8) -> bool {",
                "day % 7 == 6 || day % 7 == 0",
            ),
            ("fn f(a: bool, b: u8) -> bool {", "!a && b >= 3 && 10 > b"),
            ("fn f(c: char, n: i64) -> bool {", "c == 'x' || n / 2 != -1"),
        ] {
            assert!(tail(function, changed).is_some(), "{changed}");
        }
    }

    #[test]
    fn refuses_anything_that_can_call_panic_or_mix_operators() {
        for (function, changed) in [
            // Mixed operators need precedence.
            ("fn f(a: bool, b: bool, c: bool) -> bool {", "a || b && c"),
            // A call, index, field, cast, parenthesis or method.
            ("fn f(a: bool, b: u8) -> bool {", "a || check(b)"),
            ("fn f(a: bool, xs: &[u8]) -> bool {", "a && xs[0] == 1"),
            ("fn f(&self, a: bool) -> bool {", "a && self.on"),
            ("fn f(a: bool, b: u8) -> bool {", "a && b as u32 == 1"),
            ("fn f(a: bool, b: u8) -> bool {", "a && (b == 1)"),
            ("fn f(a: bool, s: &str) -> bool {", "a && s.is_empty()"),
            // Arithmetic that can overflow, or a division by a variable.
            ("fn f(a: u8, b: u8) -> bool {", "a == 0 || a + b == 3"),
            ("fn f(a: u8, b: u8) -> bool {", "b != 0 && a / b == 3"),
            ("fn f(a: u8) -> bool {", "a != 1 && a % 0 == 3"),
            // A constant, a float, two literals, a macro.
            ("fn f(a: u8) -> bool {", "a == LIMIT && a > 1"),
            ("fn f(a: f64) -> bool {", "a > 1.5 && a < 2.0"),
            ("fn f(a: u8) -> bool {", "1 == 1 && a > 1"),
            ("fn f(a: u8) -> bool {", "a > 1 && matches!(a, 3)"),
            // `!` binds to the atom, not the comparison.
            ("fn f(x: i32, y: i32) -> bool {", "!x == 0 && y == 0"),
            // An escaped char could name one char two ways.
            ("fn f(c: char, y: u8) -> bool {", "c == '\\n' || y == 0"),
            // A receiver or a pattern parameter.
            ("fn f(self, a: u8) -> bool {", "a > 1 && a < 3"),
            ("fn f((a, b): (u8, u8)) -> bool {", "a > 1 && b < 3"),
            // Generic signatures are not read.
            ("fn f<T: Into<u8>>(a: u8) -> bool {", "a > 1 && a < 3"),
            // A statement before the chain could rebind a parameter.
            (
                "fn f(x: i32, y: i32) -> bool {\n    let x = x + 1;",
                "x == 0 && y == 1",
            ),
        ] {
            assert!(tail(function, changed).is_none(), "{changed}");
        }
    }

    /// #15030: a plain string parameter compared with escape-free string
    /// literals reads by text; other types, escapes, raw strings and
    /// ordering comparisons stay unread.
    #[test]
    fn reads_plain_string_equality_chains_and_refuses_the_rest() -> Result<(), String> {
        for (function, changed) in [
            (
                "fn f(flag: &str) -> bool {",
                "flag == \"-v\" || flag == \"--verbose\"",
            ),
            (
                "fn f(flag: &'a str) -> bool {",
                "\"-v\" == flag || flag == \"<\"",
            ),
            (
                "fn f(flag: String, n: u8) -> bool {",
                "flag != \"x\" && n > 1",
            ),
            (
                "fn f(flag: &String) -> bool {",
                "flag == \"a||b\" || flag == \"c\"",
            ),
            (
                "fn f(flag: &str) -> bool {",
                "flag == \"a\" || flag == \"a/b\"",
            ),
            (
                "fn f(flag: &str) -> bool {",
                "flag == \"100%\" || flag == \"x\"",
            ),
        ] {
            assert!(tail(function, changed).is_some(), "{function} {changed}");
        }
        for (function, changed) in [
            // A type with its own `PartialEq<&str>`.
            (
                "fn f(flag: Flag) -> bool {",
                "flag == \"-v\" || flag == \"--verbose\"",
            ),
            (
                "fn f(flag: Cow<str>) -> bool {",
                "flag == \"-v\" || flag == \"x\"",
            ),
            // Escapes, raw and byte strings.
            (
                "fn f(flag: &str) -> bool {",
                "flag == \"\\n\" || flag == \"x\"",
            ),
            (
                "fn f(flag: &str) -> bool {",
                "flag == r\"x\" || flag == \"y\"",
            ),
            (
                "fn f(flag: &[u8]) -> bool {",
                "flag == b\"x\" || flag == b\"y\"",
            ),
            // Ordering on strings is not read.
            (
                "fn f(flag: &str) -> bool {",
                "flag < \"m\" || flag == \"z\"",
            ),
            // A string literal against an integer parameter.
            (
                "fn f(n: u8, flag: &str) -> bool {",
                "n == \"1\" || flag == \"x\"",
            ),
        ] {
            assert!(tail(function, changed).is_none(), "{function} {changed}");
        }
        let chain = tail(
            "fn f(flag: &str) -> bool {",
            "flag == \"-v\" || flag == \"--verbose\"",
        )
        .ok_or("chain")?;
        let value = |flag: &str| chain.operands[0].evaluate(&[Some(flag.to_string())]);
        assert_eq!(value("\"-v\""), Some(true));
        assert_eq!(value("\"--verbose\""), Some(false));
        // A non-string or escaped argument is unreadable, not unequal.
        assert_eq!(value("1"), None);
        assert_eq!(value("\"-\\x76\""), None);
        Ok(())
    }

    #[test]
    fn changed_operands_are_never_evaluated_only_when_every_run_short_circuits()
    -> Result<(), String> {
        let names = || vec!["x".to_string(), "y".to_string()];
        let run = |x: &str, y: &str| vec![Some(x.to_string()), Some(y.to_string())];
        let or = ShortCircuitTail::from_chain(names(), "x == 0 || y == 1").ok_or("or")?;
        let or_before =
            ShortCircuitTail::from_chain(names(), "x == 0 || y == 0").ok_or("before")?;
        // `x == 0` holds on every run, so `||` never reaches `y == 1`.
        assert!(or.changed_operands_never_evaluated(&or_before, &[run("0", "3"), run("0", "0")]));
        // One run with `x != 0` evaluates it; an unreadable run proves nothing.
        assert!(!or.changed_operands_never_evaluated(&or_before, &[run("0", "3"), run("1", "1")]));
        assert!(!or.changed_operands_never_evaluated(
            &or_before,
            &[run("0", "3"), vec![None, Some("1".to_string())]]
        ));
        assert!(!or.changed_operands_never_evaluated(&or_before, &[]));
        // The first operand is evaluated on every run.
        let first = ShortCircuitTail::from_chain(names(), "x == 1 || y == 0").ok_or("first")?;
        assert!(!first.changed_operands_never_evaluated(&or_before, &[run("0", "3")]));
        // `&&` short-circuits on `false`.
        let and = ShortCircuitTail::from_chain(names(), "x == 0 && y == 1").ok_or("and")?;
        let and_before = ShortCircuitTail::from_chain(names(), "x == 0 && y == 0").ok_or("ab")?;
        assert!(and.changed_operands_never_evaluated(&and_before, &[run("5", "1")]));
        assert!(!and.changed_operands_never_evaluated(&and_before, &[run("0", "1")]));
        // A different operator never compares.
        assert!(!or.changed_operands_never_evaluated(&and_before, &[run("0", "3")]));
        Ok(())
    }

    #[test]
    fn evaluates_operands_on_literal_values() -> Result<(), String> {
        let chain = tail("fn f(x: i32, y: i32) -> bool {", "x == 0 && y == 1").ok_or("chain")?;
        let value = |x: &str, y: &str| {
            chain.operands[0].evaluate(&[Some(x.to_string()), Some(y.to_string())])
        };
        assert_eq!(value("0", "9"), Some(true));
        assert_eq!(value("1", "9"), Some(false));
        assert_eq!(
            chain.operands[0].evaluate(&[None, Some("1".to_string())]),
            None
        );
        let weekend =
            tail("fn f(day: u8) -> bool {", "day % 7 == 6 || day % 7 == 0").ok_or("weekend")?;
        assert_eq!(
            weekend.operands[0].evaluate(&[Some("13".to_string())]),
            Some(true)
        );
        assert_eq!(
            weekend.operands[0].evaluate(&[Some("7".to_string())]),
            Some(false)
        );
        let negated = tail("fn f(a: bool, b: u8) -> bool {", "!!!a || b > 1").ok_or("negated")?;
        assert_eq!(
            negated.operands[0].evaluate(&[Some("true".to_string()), None]),
            Some(false)
        );
        Ok(())
    }

    #[test]
    fn truth_table_rows_must_separate_every_one_step_mutant() -> Result<(), String> {
        let and = tail("fn f(a: bool, b: bool) -> bool {", "a && b").ok_or("and")?;
        let run = |a: &str, b: &str| vec![Some(a.to_string()), Some(b.to_string())];
        let (tt, tf, ft, ff) = (
            run("true", "true"),
            run("true", "false"),
            run("false", "true"),
            run("false", "false"),
        );
        // The three-row pin separates the swap, both constants and each negation.
        assert!(and.runs_tell_apart_one_step_mutants(&[tt.clone(), tf.clone(), ft.clone()]));
        assert!(and.runs_tell_apart_one_step_mutants(&[tt.clone(), tf.clone(), ft, ff.clone()]));
        // No row where the operands differ: `a || b` agrees everywhere.
        assert!(!and.runs_tell_apart_one_step_mutants(&[tt.clone(), ff.clone()]));
        // `(true, false)` alone agrees with the constant `false` and with `!a && b`.
        assert!(!and.runs_tell_apart_one_step_mutants(std::slice::from_ref(&tf)));
        // `(true, true)` and `(true, false)` already separate every mutant.
        assert!(and.runs_tell_apart_one_step_mutants(&[tt.clone(), tf.clone()]));
        // A chain whose first operand is negated reads through `evaluate`.
        let not_a = tail("fn f(a: bool, b: bool) -> bool {", "!a && b").ok_or("not")?;
        assert!(not_a.runs_tell_apart_one_step_mutants(&[
            run("false", "true"),
            tf.clone(),
            ff.clone()
        ]));
        // Without the all-false row `!a || b` agrees with `!a && b`.
        assert!(!not_a.runs_tell_apart_one_step_mutants(&[run("false", "true"), tf.clone()]));
        // An unreadable argument, or no run at all, proves nothing.
        assert!(!and.runs_tell_apart_one_step_mutants(&[tt.clone(), vec![None, None]]));
        assert!(!and.runs_tell_apart_one_step_mutants(&[]));
        // `||` mirrors: the same table, read through its own connective.
        let or = tail("fn f(a: bool, b: bool) -> bool {", "a || b").ok_or("or")?;
        assert!(or.runs_tell_apart_one_step_mutants(&[ff, tf.clone(), run("false", "true")]));
        assert!(!or.runs_tell_apart_one_step_mutants(&[tt, tf]));
        // A comparison operand has a literal boundary, not a truth table.
        let compare = tail("fn f(a: bool, b: u8) -> bool {", "a && b > 1").ok_or("cmp")?;
        assert!(!compare.runs_tell_apart_one_step_mutants(&[
            vec![Some("true".to_string()), Some("2".to_string())],
            vec![Some("true".to_string()), Some("0".to_string())],
            vec![Some("false".to_string()), Some("2".to_string())],
        ]));
        Ok(())
    }

    const POW2: &str = "pub const fn is_pow2(n: u32) -> bool {\n    n & (n - 1) == 0\n}";

    fn run(values: &[&str]) -> Vec<Option<String>> {
        values.iter().map(|value| Some(value.to_string())).collect()
    }

    #[test]
    fn a_guard_operand_change_is_whole_top_level_operands_only() {
        // Dropped (the body holds the shorter chain) and added (the longer).
        let dropped =
            GuardOperandChange::read(POW2, "n & (n - 1) == 0", "n != 0 && n & (n - 1) == 0");
        assert!(dropped.is_some());
        assert_eq!(
            dropped.as_ref().map(|change| change.differing.len()),
            Some(1)
        );
        let added = "fn f(n: u32, on: bool) -> bool {\n    on || n > 3 || n == 0\n}";
        let change = GuardOperandChange::read(added, "on || n > 3 || n == 0", "    n > 3;");
        assert_eq!(change.as_ref().map(|change| change.operator), Some("||"));
        assert_eq!(
            change.as_ref().map(|change| change.differing.len()),
            Some(2)
        );
        for (function, changed, before) in [
            // A rewritten operand is not a dropped one.
            (POW2, "n & (n - 1) == 0", "n != 0 && n & (n - 2) == 0"),
            // Nothing dropped, or the same operand count.
            (POW2, "n & (n - 1) == 0", "n & (n - 1) == 0"),
            (POW2, "n & (n - 1) == 0", "n | (n - 1) == 0"),
            // The differing operand must be one this module evaluates.
            (POW2, "n & (n - 1) == 0", "check(n) && n & (n - 1) == 0"),
            (POW2, "n & (n - 1) == 0", "n - 1 != 0 && n & (n - 1) == 0"),
            // Both operators need precedence.
            (
                POW2,
                "n & (n - 1) == 0",
                "n != 0 && n & (n - 1) == 0 || n == 7",
            ),
            // An operator inside a parenthesis is not a flat chain.
            (
                "fn f(n: u32) -> bool {\n    g(n > 1)\n}",
                "g(n > 1)",
                "g(n != 0 && n > 1)",
            ),
            // An earlier statement may rebind the parameter.
            (
                "fn f(n: u32) -> bool {\n    let n = n + 1;\n    n > 1\n}",
                "n > 1",
                "n != 0 && n > 1",
            ),
            // A receiver or pattern parameter is not read.
            (
                "fn f(&self, n: u32) -> bool {\n    n > 1\n}",
                "n > 1",
                "n != 0 && n > 1",
            ),
        ] {
            assert_eq!(
                GuardOperandChange::read(function, changed, before),
                None,
                "{before} -> {changed}"
            );
        }
    }

    #[test]
    fn guard_operand_runs_agree_only_when_every_run_reads_the_identity_value() -> Result<(), String>
    {
        let and = GuardOperandChange::read(POW2, "n & (n - 1) == 0", "n != 0 && n & (n - 1) == 0")
            .ok_or("the dropped guard must read")?;
        assert!(and.runs_agree(&[run(&["4"]), run(&["6"])]));
        // `n == 0` is where the guard decided.
        assert!(!and.runs_agree(&[run(&["4"]), run(&["0"])]));
        // No run, or an unread argument, shows nothing.
        assert!(!and.runs_agree(&[]));
        assert!(!and.runs_agree(&[run(&["4"]), vec![None]]));
        let function = "fn f(n: u32, on: bool) -> bool {\n    n > 3\n}";
        let or = GuardOperandChange::read(function, "n > 3", "on || n > 3")
            .ok_or("the dropped `||` guard must read")?;
        // `||` is unchanged by a `false` operand.
        assert!(or.runs_agree(&[run(&["9", "false"])]));
        assert!(!or.runs_agree(&[run(&["9", "true"])]));
        Ok(())
    }

    #[test]
    fn a_wrapper_assertion_text_still_names_the_pinned_call() {
        // #14194 review: the guard gate reads the module's own reader, not
        // `activation::call_arguments`. Asserting the other reader can pass
        // while production rejects both wrappers, which is how the earlier
        // repair looked covered. The wrapper text here is the call text the
        // owner-pin arms derive, which is the plain call the reader must find.
        assert_eq!(
            super::call_arguments("is_pow2(4)", "is_pow2"),
            Some(vec!["4".to_string()])
        );
        assert_eq!(
            super::call_arguments("is_pow2(6)", "is_pow2"),
            Some(vec!["6".to_string()])
        );
        // The helper assertion a substituted pin reads from carries no call of
        // the owner, so it must not be mistaken for one.
        assert_eq!(
            super::call_arguments("assert_eq!(actual, true)", "is_pow2"),
            None
        );
        // And a text holding the owner twice is refused rather than guessed.
        assert_eq!(
            super::call_arguments("is_pow2(4) != is_pow2(6)", "is_pow2"),
            None
        );
    }

    #[test]
    fn layout_whitespace_cannot_invert_the_chain_direction() {
        // #14194 review: choosing the longer chain by byte length let a
        // retained operand padded with layout whitespace become the longer
        // string, so the direction inverted and the rule was skipped
        // (fail-open). The direction is now picked by operand count.
        let function = "fn f(n: u32) -> bool {\n    check(n, \"a b\")\n}";
        let padded = GuardOperandChange::read(
            function,
            "check(n,                              \"a b\")",
            "n != 0 && check(n, \"a b\")",
        );
        assert!(
            padded.is_some(),
            "a padded retained operand must still read as the shorter chain"
        );
        assert_eq!(
            padded.as_ref().map(|change| change.differing.len()),
            Some(1)
        );
        // The same edit with the padding on the other side reads the same.
        let padded_before = GuardOperandChange::read(
            function,
            "check(n, \"a b\")",
            "n != 0 &&   check(n,     \"a b\")",
        );
        assert_eq!(
            padded_before.as_ref().map(|change| change.differing.len()),
            Some(1)
        );
    }

    #[test]
    fn a_string_literal_edit_is_not_a_retained_operand() {
        // #14194 review: whitespace inside a string literal is content, not
        // layout. Collapsing it makes `check(n, "a  b")` and
        // `check(n, "a b")` compare equal, so the literal edit reads as a
        // retained operand and only the dropped guard decides the change.
        let function = "fn f(n: u32) -> bool {\n    check(n, \"a b\")\n}";
        assert_eq!(
            GuardOperandChange::read(
                function,
                "check(n, \"a b\")",
                "n != 0 && check(n, \"a  b\")"
            ),
            None
        );
        // Layout outside the literal is still ignored.
        let spaced = "fn f(n: u32) -> bool {\n    check(n, \"a b\")\n}";
        assert!(
            GuardOperandChange::read(
                spaced,
                "check(n, \"a b\")",
                "n != 0 &&   check(n,   \"a b\")"
            )
            .is_some()
        );
    }

    #[test]
    fn layout_whitespace_does_not_hide_a_retained_operand() {
        // #14194 review: the operand comparison kept the presence of a space, so
        // `n % 2 == 0` and `n%2==0` compared unequal and the dropped guard
        // was not read (fail-open). Layout whitespace is not part of the
        // operand's identity.
        let compact = "fn f(n: u32) -> bool {\n    n%2==0\n}";
        let change = GuardOperandChange::read(compact, "n%2==0", "n != 0 && n % 2 == 0");
        assert_eq!(
            change.as_ref().map(|change| change.differing.len()),
            Some(1)
        );
        // The same edit with the spacing on the other side reads the same.
        let spaced = "fn f(n: u32) -> bool {\n    n % 2 == 0\n}";
        let change = GuardOperandChange::read(spaced, "n % 2 == 0", "n != 0 && n%2==0");
        assert_eq!(
            change.as_ref().map(|change| change.differing.len()),
            Some(1)
        );
    }

    #[test]
    fn a_space_that_separates_words_is_part_of_the_retained_operand() {
        // #15149: stripping every space read `a as u8 == 0` and `aasu8 == 0`
        // as one retained operand, so a rewrite looked like a dropped guard.
        // They are different token sequences; the rewritten cast operand is
        // not an item 1 operand, so the edit is not read.
        let merged = "fn f(n: u32, a: u32, aasu8: u8) -> bool {\n    aasu8 == 0\n}";
        assert_eq!(
            GuardOperandChange::read(merged, "aasu8 == 0", "n != 0 && a as u8 == 0"),
            None
        );
        // The layout-only spelling of the same cast is still retained.
        let spaced = "fn f(n: u32, a: u32) -> bool {\n    a as u8 == 0\n}";
        assert_eq!(
            GuardOperandChange::read(spaced, "a as u8 == 0", "n != 0 && a  as  u8==0")
                .map(|change| change.differing.len()),
            Some(1)
        );
    }

    #[test]
    fn a_layout_only_generic_close_keeps_the_dropped_guard_read() -> Result<(), String> {
        // #15537 review: splitting `>>` and `> >` into different tokens read
        // this layout-only edit of the retained operand as a rewrite, so the
        // withholding was dropped (fail-open toward exposed).
        let function = "fn f(n: u32, a: usize) -> bool {\n    a == size_of::<Vec<Vec<u8>>>()\n}";
        let change = GuardOperandChange::read(
            function,
            "a == size_of::<Vec<Vec<u8>>>()",
            "n != 0 && a == size_of::<Vec<Vec<u8> > >()",
        )
        .ok_or("the layout-only retained operand must read as retained")?;
        assert_eq!(change.differing.len(), 1);
        // Every run leaves the dropped `n != 0` true: the pin is withheld.
        assert!(change.runs_agree(&[run(&["1", "24"]), run(&["7", "0"])]));
        Ok(())
    }

    #[test]
    fn a_tuple_field_spacing_edit_keeps_the_dropped_guard_read() -> Result<(), String> {
        // #15537 review: `0.0` absorbed a decimal point, so `((n,),).0 .0`
        // and `((n,),).0.0` compared unequal and the dropped guard was not
        // read (fail-open toward exposed). Both spacing directions.
        let tight = "fn f(n: u32) -> bool {\n    ((n,),).0.0 == 4\n}";
        let change =
            GuardOperandChange::read(tight, "((n,),).0.0 == 4", "n != 0 && ((n,),).0 .0 == 4")
                .ok_or("the spaced tuple field must read as the retained operand")?;
        assert_eq!(change.differing.len(), 1);
        assert!(change.runs_agree(&[run(&["4"]), run(&["5"])]));
        let spaced = "fn f(n: u32) -> bool {\n    ((n,),).0 .0 == 4\n}";
        let change =
            GuardOperandChange::read(spaced, "((n,),).0 .0 == 4", "n != 0 && ((n,),).0.0 == 4")
                .ok_or("the tight tuple field must read as the retained operand")?;
        assert_eq!(change.differing.len(), 1);
        Ok(())
    }

    #[test]
    fn operand_tokens_split_words_and_merge_punctuation_runs() {
        assert_eq!(operand_tokens("a as u8"), ["a", "as", "u8"]);
        assert_eq!(operand_tokens("aasu8"), ["aasu8"]);
        assert_ne!(operand_tokens("a as u8"), operand_tokens("aasu8"));
        assert_eq!(operand_tokens("a%2"), operand_tokens("a % 2"));
        assert_eq!(operand_tokens("x == -1"), ["x", "==-", "1"]);
        assert_eq!(operand_tokens("x==-1"), operand_tokens("x == -1"));
        assert_eq!(operand_tokens("x&&y"), ["x", "&&", "y"]);
        // Spacing inside a punctuation run is layout only (#15537 review):
        // reading these as different would drop the withholding.
        assert_eq!(operand_tokens("x & &y"), operand_tokens("x&&y"));
        assert_eq!(operand_tokens("a > = b"), operand_tokens("a >= b"));
        assert_eq!(operand_tokens("a < < 2"), operand_tokens("a << 2"));
        assert_eq!(
            operand_tokens("Vec<Vec<u8> >"),
            operand_tokens("Vec<Vec<u8>>")
        );
        assert_eq!(operand_tokens("v < 1.5"), ["v", "<", "1", ".", "5"]);
        // A tuple-field chain reads the same whatever the spacing between
        // its fields (#15537 review), in both directions.
        assert_eq!(operand_tokens("x.0.0"), operand_tokens("x.0 .0"));
        assert_eq!(operand_tokens("x.0 .0"), operand_tokens("x.0.0"));
        assert_eq!(operand_tokens("x.0.0"), ["x", ".", "0", ".", "0"]);
        assert_eq!(
            operand_tokens("check(n, \"a  b\")"),
            ["check", "(", "n", ",", "\"a  b\"", ")"]
        );
    }

    #[test]
    fn a_char_literal_operator_is_not_an_arithmetic_split() -> Result<(), String> {
        // #14194 review: `parse_atom` split `'/'` as a division operator, so
        // the dropped `c != '/'` guard was refused and the rule was skipped.
        let function = "fn f(c: char) -> bool {\n    c != 'x'\n}";
        let change = GuardOperandChange::read(function, "c != 'x'", "c != '/' && c != 'x'")
            .ok_or("the dropped char guard must read")?;
        assert_eq!(change.differing.len(), 1);
        // `'y'` is neither `'/'` nor `'x'`, so both versions return true;
        // `'/'` is where the dropped guard decided.
        assert!(change.runs_agree(&[run(&["'y'"])]));
        assert!(!change.runs_agree(&[run(&["'/'"])]));
        Ok(())
    }

    #[test]
    fn a_quote_char_literal_does_not_flip_the_literal_scan() {
        // #14194 review: a `'"'` char literal flipped the quote-and-escape
        // scan, so whitespace inside the following string collapsed and the
        // literal edit read as a retained operand.
        let function = "fn f(n: u32) -> bool {\n    check(n, '\"', \"a b\")\n}";
        assert_eq!(
            GuardOperandChange::read(
                function,
                "check(n, '\"', \"a b\")",
                "n != 0 && check(n, '\"', \"a  b\")"
            ),
            None
        );
        // A raw string is literal data too.
        let raw = "fn f(n: u32) -> bool {\n    check(n, r#\"a b\"#)\n}";
        assert_eq!(
            GuardOperandChange::read(
                raw,
                "check(n, r#\"a b\"#)",
                "n != 0 && check(n, r#\"a  b\"#)"
            ),
            None
        );
    }
}
