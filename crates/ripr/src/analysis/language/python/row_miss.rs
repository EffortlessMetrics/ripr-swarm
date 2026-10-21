//! Row-local `RelatedTestMiss` for Python related tests (#5491,
//! RIPR-SPEC-0224 language-neutral contract).
//!
//! `miss` explains one related-test row and is evidence, not a verdict: this
//! pass reads the finished class and the retained row evidence and writes only
//! `RelatedTest::miss`. It never changes the class, stages, confidence,
//! ordering, repair placement or identity.
//!
//! Emitted (only on a `weakly_exposed` finding, and only from facts the row
//! owns; a finding-wide fact never lends a row its reason):
//!
//! - `no_call_path`: a name-or-location-only relation (`same_stem`,
//!   `test_name_similarity`, `fixture_name`). `dunder_protocol` is excluded:
//!   the test builds the owner's class and reaches the method through an
//!   implicit protocol call, so "no call to the changed code" would be false.
//!   A body that uses the owner name or an import alias of it in any position
//!   other than `callable(owner)`, `owner.__name__` (and `__qualname__`,
//!   `__module__`, `__doc__`) or a lone `ident = owner` never read again may
//!   still invoke it through a shape the call extractor does not model
//!   (`map(owner, ...)`, `f = owner; f(0)`), so it keeps no miss.
//! - `weak_assertion`: the family-selected assertion (#5572) is weak or smoke.
//! - `observation_unconfirmed`: a strong family-selected assertion that this
//!   row alone does not credit as observing the changed sink. Python's
//!   alignment is a credit-only model, so no credit is an unresolved
//!   observation edge, never an affirmative mismatch.
//!
//! Deliberately not emitted, so the row stays `miss: None`:
//!
//! - `no_assertion`: `extraction_complete_no_assertion` (#5571) still has the
//!   cross-file blind spots of #6657 (conftest autouse fixtures, plugins, ini
//!   `filterwarnings`, bare sibling imports); until they are accounted for the
//!   state is not an established absence.
//! - `assertion_not_observing`: alignment cannot tell an affirmative sibling
//!   sink from an unresolved one, and the value also marks a row as
//!   examined-only (`RelatedTest::is_unmatched`), which decisional consumers
//!   (`oracle_related_tests`) read, so emitting it would move decisions.
//! - `assertion_not_credited`: no typed established refusal exists (#5446);
//!   skip / xfail rows stay `activation_limited` with no oracle credit.
//! - a local stand-in row (`fee = Mock(...)` shadowing the imported owner) keeps
//!   its `observation_unconfirmed`: stand-in detection is textual and errs
//!   toward withholding credit, so it cannot support an affirmative no-call claim.
//! - `missing_input`: pending follow-up (#5491). The row-owned omission needs
//!   import-resolved owner calls and per-row operand resolution; until that
//!   lands a strong row that observes nothing falls to
//!   `observation_unconfirmed`, which claims less.
//! - `missing_exact_assertion`: no row-owned exact-discriminator fact exists;
//!   `missing_discriminators` is finding-wide.

use super::related_tests::{PythonRelatedCandidate, PythonRelationKind};
use super::sink_alignment::{AlignmentPrep, classify_sink_alignment_prepared};
use super::{PythonOwner, PythonTest};
use crate::domain::{ExposureClass, OracleStrength, OwnerKind, RelatedTest, RelatedTestMiss};

/// The changed line, as the classifier saw it.
pub(super) struct RowMissChange<'a> {
    pub(super) owner: &'a PythonOwner,
    pub(super) line_text: &'a str,
    pub(super) old_line_text: Option<&'a str>,
}

/// Annotate `rows` (one per `candidates` entry, in order) in place.
pub(super) fn annotate_row_misses(
    rows: &mut [RelatedTest],
    candidates: &[PythonRelatedCandidate<'_>],
    class: &ExposureClass,
    has_static_limit: bool,
    change: &RowMissChange<'_>,
    all_tests: &[PythonTest],
) {
    // Class gate: only a concrete supported gap class carries row misses.
    if has_static_limit || !matches!(class, ExposureClass::WeaklyExposed) {
        return;
    }
    // Rows are built one per candidate, in order (`classify.rs`), and nothing
    // reorders them before this pass. Fail closed if that ever stops holding:
    // a row must never take another test's reason.
    if rows.len() != candidates.len()
        || rows.iter().zip(candidates).any(|(row, candidate)| {
            row.name != candidate.test.name
                || row.file != candidate.test.file
                || row.line != candidate.test.line
        })
    {
        return;
    }
    // Built once per finding; each strong row is still aligned on its own.
    let prep = AlignmentPrep::new(change.owner, all_tests);
    for (row, candidate) in rows.iter_mut().zip(candidates) {
        row.miss = row_miss(row, candidate, change, &prep);
    }
}

fn row_miss(
    row: &RelatedTest,
    candidate: &PythonRelatedCandidate<'_>,
    change: &RowMissChange<'_>,
    prep: &AlignmentPrep<'_>,
) -> Option<RelatedTestMiss> {
    if !candidate.relation.uses_oracle() {
        return match candidate.relation {
            PythonRelationKind::SameStem
            | PythonRelationKind::TestNameSimilarity
            | PythonRelationKind::FixtureName => {
                let mut names = vec![change.owner.name.as_str()];
                names.extend(
                    candidate
                        .test
                        .imports
                        .iter()
                        .filter(|import| import.imported == change.owner.name)
                        .map(|import| import.alias.as_str()),
                );
                (callable_owner(change.owner)
                    && !header_mentions_owner(candidate.test, &names)
                    && !body_may_invoke_owner(&candidate.test.body_text, &names))
                .then_some(RelatedTestMiss::NoCallPath)
            }
            // A fixture or same-module caller does call the owner, a dunder
            // protocol row reaches it implicitly, and a skip / xfail row is
            // `activation_limited`: no path claim.
            _ => None,
        };
    }
    // No selected assertion (none recognized, or none family-relevant):
    // admission is the authority and nothing is emitted from it here.
    row.oracle.as_ref()?;
    match row.oracle_strength {
        OracleStrength::Weak | OracleStrength::Smoke => Some(RelatedTestMiss::WeakAssertion),
        OracleStrength::Strong => strong_row_miss(row, change, prep),
        _ => None,
    }
}

fn strong_row_miss(
    row: &RelatedTest,
    change: &RowMissChange<'_>,
    prep: &AlignmentPrep<'_>,
) -> Option<RelatedTestMiss> {
    let alignment = classify_sink_alignment_prepared(
        change.owner,
        change.line_text,
        change.old_line_text,
        std::slice::from_ref(row),
        prep,
    );
    (!alignment.observes()).then_some(RelatedTestMiss::ObservationUnconfirmed)
}

/// "No call to the changed code" is a claim about a callable. A module
/// constant or module-level statement is read, not called, so a row for it
/// cannot carry `no_call_path`.
fn callable_owner(owner: &PythonOwner) -> bool {
    matches!(
        owner.owner_kind,
        Some(OwnerKind::Function | OwnerKind::Method | OwnerKind::ClassMethod)
    )
}

/// Fail-closed: true unless every occurrence of the owner name (or an import
/// alias local of it) in `body` is one of three trivial, non-invoking forms:
/// exactly `callable(<name>)`, `<name>.__name__` (or `__qualname__`,
/// `__module__`, `__doc__`) not followed by `(`, or a whole statement
/// `ident = <name>` whose single plain identifier appears nowhere else. A body
/// with no occurrence at all (a fixture-name or stem relation) has nothing that
/// could invoke the owner.
fn body_may_invoke_owner(body: &str, names: &[&str]) -> bool {
    let bytes = body.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    for name in names {
        for (start, _) in body.match_indices(name) {
            let end = start + name.len();
            if (start > 0 && is_ident(bytes[start - 1]))
                || (end < bytes.len() && is_ident(bytes[end]))
            {
                continue;
            }
            let before = &body[..start];
            let after = &body[end..];
            if before.ends_with("callable(")
                && !before[..before.len() - "callable(".len()]
                    .ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                && after.starts_with(')')
            {
                continue;
            }
            if let Some(attr) = after.strip_prefix('.') {
                let word: String = attr
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let rest = attr[word.len()..].trim_start_matches([' ', '\t']);
                if matches!(
                    word.as_str(),
                    "__name__" | "__qualname__" | "__module__" | "__doc__"
                ) && !rest.starts_with('(')
                {
                    continue;
                }
            }
            if trivial_alias_statement(body, start, end, name) {
                continue;
            }
            return true;
        }
    }
    false
}

/// The line holding `body[start..end]` is exactly `ident = name`.
fn trivial_alias_statement(body: &str, start: usize, end: usize, name: &str) -> bool {
    let line_start = body[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = body[end..].find('\n').map_or(body.len(), |i| end + i);
    let raw_line = &body[line_start..line_end];
    if Some(indent_of(raw_line)) != own_statement_indent(body) {
        return false;
    }
    let line = raw_line.trim();
    if !starts_top_level_statement(&body[..line_start])
        || line.ends_with(['\\', '(', '[', '{', ','])
    {
        return false;
    }
    let Some((lhs, rhs)) = line.split_once('=') else {
        return false;
    };
    let (lhs, rhs) = (lhs.trim(), rhs.trim());
    rhs == name
        && lhs != name
        && !lhs.starts_with("__")
        && !lhs.is_empty()
        && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !lhs.starts_with(|c: char| c.is_ascii_digit())
        && word_count(body, lhs) == 1
}

fn word_count(body: &str, word: &str) -> usize {
    let bytes = body.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    body.match_indices(word)
        .filter(|(i, _)| {
            let end = i + word.len();
            (*i == 0 || !is_ident(bytes[i - 1])) && (end >= bytes.len() || !is_ident(bytes[end]))
        })
        .count()
}

/// True only when `prefix` (everything before a line) leaves bracket depth 0
/// and its last non-blank line does not continue onto the next one. Strings
/// and comments are skipped; triple quotes or an unterminated string are
/// unsure and read as "not top level".
fn starts_top_level_statement(prefix: &str) -> bool {
    let mut depth = 0i32;
    let mut chars = prefix.chars();
    while let Some(c) = chars.next() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '#' => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
            }
            '\'' | '"' => {
                let mut probe = chars.clone();
                if probe.next() == Some(c) && probe.next() == Some(c) {
                    return false;
                }
                let mut closed = false;
                while let Some(n) = chars.next() {
                    if n == '\\' {
                        chars.next();
                    } else if n == c {
                        closed = true;
                        break;
                    } else if n == '\n' {
                        break;
                    }
                }
                if !closed {
                    return false;
                }
            }
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    let last = prefix.lines().rev().find(|l| !l.trim().is_empty());
    depth == 0 && last.is_none_or(|l| !l.trim_end().ends_with(['\\', ',', '(', '[', '{']))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// Indent of the test function's own first body statement: the first
/// non-blank line after its `def` header. `None` when no header is found, so
/// nothing is exempt.
fn own_statement_indent(body: &str) -> Option<usize> {
    let mut lines = body.lines();
    lines.find(|l| {
        let t = l.trim_start();
        t.starts_with("def ") || t.starts_with("async def ")
    })?;
    lines.find(|l| !l.trim().is_empty()).map(indent_of)
}

/// Decorator arguments are outside `body_text` and are not retained as text
/// (`decorators` keeps names only), so this reads them fail closed: a
/// parametrized or generated test, or any decorator other than a plain
/// `pytest.mark.<marker>` (`patch`, `given`, `example`, user decorators), may
/// hand the owner to the test as a parameter or wrap it, and withholds the
/// claim. The bodies of the fixtures the test resolves are checked for the
/// owner name or an import alias.
fn header_mentions_owner(test: &PythonTest, names: &[&str]) -> bool {
    let opaque_decorator = test.decorators.iter().any(|decorator| {
        decorator == "pytest.mark.parametrize" || !decorator.starts_with("pytest.mark.")
    });
    opaque_decorator
        || test.parametrized
        || test.parametrize.is_some()
        || !test.direct_parametrized.is_empty()
        || !test.generated_inputs.is_empty()
        || test.fixture_defs.iter().any(|fixture| {
            names
                .iter()
                .any(|name| word_count(&fixture.body_text, name) > 0)
        })
}
