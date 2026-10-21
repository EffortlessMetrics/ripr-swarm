use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::source_utils::normalized_path;
use super::{
    PythonImport, PythonOwner, PythonTest, first_python_string_literal, line_prefix_before,
    python_callee_start_has_boundary, python_prefix_hides_code, python_string_literal_value,
};
use crate::domain::{ExposureClass, OracleKind, OracleStrength, OwnerKind, RelatedTest};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod verify_command_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonRelationKind {
    SyntacticCall,
    ImportAliasCall,
    ApiClientRouteCall,
    ConstructCall,
    ConstructorCall,
    LocalBinding,
    DunderProtocol,
    ProtocolEntryCall,
    SameStem,
    TestNameSimilarity,
    FixtureName,
    /// The test calls a same-module caller of the owner, and no test calls the
    /// owner itself (#4568). Reach evidence only; never an oracle.
    HelperOwnerCall,
    /// The test names a pytest fixture whose body calls the owner (#4563).
    /// Reach evidence only; the fixture value is not an oracle.
    FixtureOwnerCall,
    ActivationLimited,
}

impl PythonRelationKind {
    fn rank(self) -> u8 {
        match self {
            Self::SyntacticCall => 5,
            Self::ImportAliasCall => 4,
            Self::ApiClientRouteCall => 4,
            Self::ConstructCall => 4,
            Self::ConstructorCall => 4,
            Self::LocalBinding => 4,
            Self::DunderProtocol => 3,
            Self::ProtocolEntryCall => 4,
            Self::SameStem => 3,
            Self::TestNameSimilarity => 2,
            Self::FixtureName => 1,
            Self::FixtureOwnerCall => 2,
            Self::HelperOwnerCall => 2,
            Self::ActivationLimited => 0,
        }
    }

    pub(super) fn uses_oracle(self) -> bool {
        matches!(
            self,
            Self::SyntacticCall
                | Self::ImportAliasCall
                | Self::ApiClientRouteCall
                | Self::ConstructCall
                | Self::ConstructorCall
                | Self::LocalBinding
                | Self::ProtocolEntryCall
        )
    }

    pub(super) fn is_uncertain(self) -> bool {
        !self.uses_oracle()
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::SyntacticCall => "syntactic_call",
            Self::ImportAliasCall => "import_alias_call",
            Self::ApiClientRouteCall => "api_client_route_call",
            Self::ConstructCall => "construct_call",
            Self::ConstructorCall => "constructor_call",
            Self::LocalBinding => "local_binding",
            Self::DunderProtocol => "dunder_protocol",
            Self::ProtocolEntryCall => "protocol_entry_call",
            Self::SameStem => "same_stem",
            Self::TestNameSimilarity => "test_name_similarity",
            Self::FixtureName => "fixture_name",
            Self::FixtureOwnerCall => "fixture_owner_call",
            Self::HelperOwnerCall => "helper_owner_call",
            Self::ActivationLimited => "test_activation_unestablished",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PythonRelatedCandidate<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) relation: PythonRelationKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonRepairPlacement {
    pub(super) repair_action: &'static str,
    pub(super) suggested_test_file: String,
    pub(super) suggested_test_name: String,
    pub(super) suggested_test_line: Option<usize>,
    pub(super) suggested_test_node_id: Option<String>,
    pub(super) verify_command: String,
    pub(super) verify_command_confidence: &'static str,
    pub(super) location_reason: &'static str,
}

pub(super) fn related_test_candidates<'a>(
    owner: &PythonOwner,
    all_tests: &'a [PythonTest],
) -> Vec<PythonRelatedCandidate<'a>> {
    let mut candidates: Vec<PythonRelatedCandidate<'a>> = all_tests
        .iter()
        .filter_map(|test| {
            related_test_relation(test, owner).map(|relation| PythonRelatedCandidate {
                test,
                relation: if super::test_activation::activation_control(test).is_some() {
                    PythonRelationKind::ActivationLimited
                } else {
                    relation
                },
            })
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .relation
            .rank()
            .cmp(&left.relation.rank())
            .then_with(|| {
                // Candidate order is owner-level and shared by every changed
                // line of the owner, so it has no changed family: it ranks by
                // the strongest assertion overall (focus `None`). Which
                // assertion a row displays and the classifier judges is the
                // per-change selection in `related_tests_for_candidates`.
                let left_rank = strongest_rank(left.test);
                let right_rank = strongest_rank(right.test);
                right_rank.cmp(&left_rank)
            })
            .then_with(|| left.test.file.cmp(&right.test.file))
            .then_with(|| left.test.name.cmp(&right.test.name))
    });
    candidates
}

/// Most same-module caller hops followed from a changed helper (#4568).
const MAX_HELPER_CALLER_DEPTH: usize = 3;

/// A weak `helper_owner_call` candidate and the same-module caller the test
/// calls (`Version.__init__` for `_validate_epoch`).
pub(super) struct HelperOwnerCallCandidate<'a> {
    pub(super) candidate: PythonRelatedCandidate<'a>,
    pub(super) caller: String,
}

/// Tests that call a same-module caller of a changed top-level helper, for an
/// owner no test calls itself (#4568). The caller list is bounded to
/// [`MAX_HELPER_CALLER_DEPTH`] hops of bare-name calls in the owner's file;
/// the helper name must be unique in that file, and a caller that rebinds the
/// name or takes it as a parameter makes no edge (`module_callees`). Every
/// candidate is reach-only: the relation never carries an oracle.
pub(super) fn helper_owner_call_candidates<'a>(
    owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Vec<HelperOwnerCallCandidate<'a>> {
    let top_level_function = |candidate: &PythonOwner| {
        candidate.owner_kind == Some(OwnerKind::Function)
            && candidate.class_path.is_empty()
            && candidate.qualified_name == candidate.name
    };
    if !top_level_function(owner)
        || owners
            .iter()
            .filter(|other| {
                other.file == owner.file && other.name == owner.name && top_level_function(other)
            })
            .count()
            != 1
    {
        return Vec::new();
    }
    let mut frontier = vec![owner.name.clone()];
    let mut callers: Vec<&PythonOwner> = Vec::new();
    for _ in 0..MAX_HELPER_CALLER_DEPTH {
        let mut next = Vec::new();
        let fresh: Vec<&PythonOwner> = owners
            .iter()
            .filter(|caller| {
                caller.file == owner.file
                    && !caller.is_module_owner()
                    && caller.owner_kind.is_some()
                    && caller.name != owner.name
                    && !callers.iter().any(|seen| std::ptr::eq(*seen, *caller))
                    && caller
                        .module_callees
                        .iter()
                        .any(|callee| frontier.contains(callee))
            })
            .collect();
        for caller in fresh {
            callers.push(caller);
            // Only a top-level function is callable by bare name again.
            if top_level_function(caller) {
                next.push(caller.name.clone());
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    let mut found: Vec<HelperOwnerCallCandidate<'a>> = Vec::new();
    for caller in callers {
        for test in all_tests {
            if found.iter().any(|existing| {
                existing.candidate.test.file == test.file
                    && existing.candidate.test.qualified_name == test.qualified_name
            }) {
                continue;
            }
            if related_test_relation(test, caller).is_some_and(PythonRelationKind::uses_oracle) {
                found.push(HelperOwnerCallCandidate {
                    candidate: PythonRelatedCandidate {
                        test,
                        relation: if super::test_activation::activation_control(test).is_some() {
                            PythonRelationKind::ActivationLimited
                        } else {
                            PythonRelationKind::HelperOwnerCall
                        },
                    },
                    caller: caller.qualified_name.clone(),
                });
            }
        }
    }
    found.sort_by(|left, right| {
        left.candidate
            .test
            .file
            .cmp(&right.candidate.test.file)
            .then_with(|| left.candidate.test.name.cmp(&right.candidate.test.name))
    });
    found
}

fn strongest_rank(test: &PythonTest) -> u8 {
    select_relevant_assertion(&test.assertions, None)
        .assertion()
        .map_or(0, |assertion| assertion.oracle_strength.rank())
}

/// The public related-test rows for one owner, each projecting the assertion
/// relevant to `focus` (#5572). `focus` is the changed line's family; `None`
/// only for owner-level surfaces that have no changed line.
pub(super) fn find_related_tests(
    owner: &PythonOwner,
    all_tests: &[PythonTest],
    focus: Option<&PythonAssertionFocus>,
) -> Vec<RelatedTest> {
    related_tests_for_candidates(&related_test_candidates(owner, all_tests), focus)
}

/// Project already-matched candidates to public rows, in candidate order.
/// The row's oracle is the assertion [`select_relevant_assertion`] picks for
/// `focus`, the same identity every other consumer of that test judges.
pub(super) fn related_tests_for_candidates(
    candidates: &[PythonRelatedCandidate<'_>],
    focus: Option<&PythonAssertionFocus>,
) -> Vec<RelatedTest> {
    candidates
        .iter()
        .map(|candidate| {
            let selected = candidate
                .relation
                .uses_oracle()
                .then(|| select_relevant_assertion(&candidate.test.assertions, focus).assertion())
                .flatten();
            let (oracle_kind, oracle_strength, oracle) = match selected {
                Some(assertion) => (
                    assertion.oracle_kind.clone(),
                    assertion.oracle_strength.clone(),
                    Some(assertion.text.clone()),
                ),
                // Parameterization is input evidence, never an oracle (#5571);
                // a test whose assertions all observe another family has no
                // oracle for this change either (#5572).
                None => (OracleKind::Unknown, OracleStrength::Unknown, None),
            };
            RelatedTest {
                name: candidate.test.name.clone(),
                file: candidate.test.file.clone(),
                line: candidate.test.line,
                oracle,
                oracle_kind,
                oracle_strength,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
            }
        })
        .collect()
}

pub(super) fn verify_command_for_test(test: &PythonTest) -> Option<String> {
    verify_command_for_test_on(test, cfg!(windows))
}

/// [`verify_command_for_test`] with the host made explicit so both shell
/// grammars are testable anywhere (#4378 pattern).
pub(super) fn verify_command_for_test_on(test: &PythonTest, windows: bool) -> Option<String> {
    let path = normalized_path(&test.file);
    match test.framework {
        "pytest" => {
            let node = test.qualified_name.replace('.', "::");
            // #8752: the quoting grammar must match the host's default shell
            // or the suggested command fails for exactly the paths that need
            // quoting. On Windows the node id sits inside the quotes: cmd
            // only strips surrounding quotes when an argument both starts and
            // ends with one, so `pytest "path"::test` would hand pytest the
            // quote bytes as part of the file name. A token with bytes cmd
            // or PowerShell expand inside double quotes gets no command.
            let selector = if windows {
                shell_quote_file_arg_for_host(&format!("{path}::{node}"), true)?
            } else {
                format!("{}::{node}", shell_quote_file_arg_for_host(&path, false)?)
            };
            Some(format!(
                "{} {selector}",
                crate::domain::PYTEST_VERIFY_PROGRAM
            ))
        }
        "unittest" => {
            let module = unittest_module_for_path(&path)?;
            let selector = if windows {
                shell_quote_file_arg_for_host(&format!("{module}.{}", test.qualified_name), true)?
            } else {
                format!(
                    "{}.{}",
                    shell_quote_file_arg_for_host(&module, false)?,
                    test.qualified_name
                )
            };
            Some(format!("python -m unittest {selector}"))
        }
        _ => None,
    }
}

fn unittest_module_for_path(path: &str) -> Option<String> {
    let module_path = path.strip_suffix(".py").unwrap_or(path);
    // unittest splits every dot as a module separator. A literal path dot
    // can therefore select a different file, even when shell quoting is
    // correct. Keep that unresolved identity out of verification routes.
    if module_path.contains('.') {
        return None;
    }
    Some(module_path.replace(['/', '\\'], "."))
}

/// Quote one path or module token for the host's default shell grammar.
///
/// Suggested verify commands are text an agent may paste into a shell, so the
/// quoting must match the host's grammar or the command fails for exactly the
/// paths that need quoting (#8752). A relative path made of ASCII letters,
/// digits, `.`, `_`, `/`, `-` stays readable (`:` too on Windows, where it is
/// not a metachar and the pytest `::` separator must not force quotes); any
/// other byte is quoted so spaces, unicode, and shell metacharacters reach
/// the test runner intact. On POSIX hosts the quote is single-quote escaping
/// (`'\''`), so `$()`, backticks, spaces, and quotes are not expanded. On
/// Windows hosts the quote is double quotes — the common denominator of cmd
/// and PowerShell for literal text — but both shells still expand some bytes
/// inside double quotes (PowerShell: `$()`, `$var`, and backtick escapes;
/// cmd: `%VAR%`). A token carrying any of those bytes gets no paste-ready
/// command at all: the verify route stays silent rather than emitting a
/// command whose paste executes the file name. An embedded `"` in a
/// quotable token is doubled per the PowerShell grammar; cmd has no
/// representation for an embedded quote, which stays a documented limit.
/// Stored node ids and test-file paths stay the raw spelling; only the
/// command text is quoted. This does not run the command.
fn shell_quote_file_arg_for_host(file_str: &str, windows: bool) -> Option<String> {
    let in_safe_class = |c: char| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-') || (windows && c == ':')
    };
    if !file_str.is_empty() && file_str.chars().all(in_safe_class) {
        return Some(file_str.to_string());
    }
    if windows {
        if file_str.contains(['$', '`', '%']) {
            return None;
        }
        return Some(format!("\"{}\"", file_str.replace('"', "\"\"")));
    }
    Some(format!("'{}'", file_str.replace('\'', "'\\''")))
}

pub(super) fn python_repair_placement(
    class: &ExposureClass,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<PythonRepairPlacement> {
    python_repair_placement_on(class, related_candidates, cfg!(windows))
}

/// [`python_repair_placement`] with the host made explicit so both shell
/// grammars are testable anywhere (#4378 pattern).
pub(super) fn python_repair_placement_on(
    class: &ExposureClass,
    related_candidates: &[PythonRelatedCandidate<'_>],
    windows: bool,
) -> Option<PythonRepairPlacement> {
    if !matches!(class, ExposureClass::WeaklyExposed) {
        return None;
    }
    let (candidate, verify_command) = related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .find_map(|candidate| {
            verify_command_for_test_on(candidate.test, windows).map(|command| (candidate, command))
        })?;
    let path = normalized_path(&candidate.test.file);
    // Public rows use bare names, so two classes/frameworks in one file can
    // share the same file/name identity. Carry the selected source line only
    // where consumers need it to distinguish those rows.
    let suggested_test_line = related_candidates
        .iter()
        .filter(|other| {
            other.test.name == candidate.test.name && normalized_path(&other.test.file) == path
        })
        .nth(1)
        .map(|_| candidate.test.line);
    match candidate.test.framework {
        "pytest" => {
            let node = candidate.test.qualified_name.replace('.', "::");
            let node_id = format!("{path}::{node}");
            Some(PythonRepairPlacement {
                repair_action: "strengthen_existing_test",
                suggested_test_file: path,
                suggested_test_name: candidate.test.name.clone(),
                suggested_test_line,
                suggested_test_node_id: Some(node_id),
                verify_command,
                verify_command_confidence: "high",
                location_reason: "strengthen existing weak pytest relation",
            })
        }
        "unittest" => Some(PythonRepairPlacement {
            repair_action: "strengthen_existing_test",
            suggested_test_file: path,
            suggested_test_name: candidate.test.name.clone(),
            suggested_test_line,
            suggested_test_node_id: None,
            verify_command,
            verify_command_confidence: "high",
            location_reason: "strengthen existing weak unittest relation",
        }),
        _ => None,
    }
}

pub(super) fn related_test_relation(
    test: &PythonTest,
    owner: &PythonOwner,
) -> Option<PythonRelationKind> {
    if let Some(class) = dunder_method_class(owner) {
        return dunder_method_relation(test, owner, class);
    }
    if body_calls_owner(&test.body_text, owner) || exception_assertion_calls_owner(test, owner) {
        return Some(PythonRelationKind::SyntacticCall);
    }
    // The bounded list oracle already owns a parsed direct-call operand.
    // Preserve its identity for valid spacing/grouping that text calls miss.
    if test.assertions.iter().any(|assertion| {
        assertion.oracle_strength == OracleStrength::Strong
            && super::oracles::unittest_list::calls_owner(&assertion.text, Some(test), owner)
                == Some(true)
    }) {
        return Some(PythonRelationKind::SyntacticCall);
    }
    if import_alias_calls_owner(test, owner) {
        return Some(PythonRelationKind::ImportAliasCall);
    }
    if api_client_route_calls_owner(test, owner) {
        return Some(PythonRelationKind::ApiClientRouteCall);
    }
    if construct_call_invokes_owner(test, owner) {
        return Some(PythonRelationKind::ConstructCall);
    }
    if local_binding_calls_owner(test, owner) {
        return Some(PythonRelationKind::LocalBinding);
    }
    if fixture_owner_call(test, owner) {
        return Some(PythonRelationKind::FixtureOwnerCall);
    }
    // A test is related to an owner only when it references the owner
    // (RIPR-SPEC-0028). File-stem, test-name and fixture-name proximity are
    // ranking labels for a test that already references the owner without a
    // recognized call shape; they never relate a test that only exercises a
    // sibling owner in the same module (flat layout: `loyalty_price` in
    // `pricing.py` was linked to `tests/test_pricing.py` tests that only call
    // `discounted_total`).
    if !test_references_owner(test, owner) {
        return None;
    }
    if same_stem_related(test, owner) {
        return Some(PythonRelationKind::SameStem);
    }
    if test_name_similar_to_owner(test, owner) {
        return Some(PythonRelationKind::TestNameSimilarity);
    }
    if fixture_name_related_to_owner(test, owner) {
        return Some(PythonRelationKind::FixtureName);
    }
    None
}

/// Whether a fixture the test names calls the owner by the rules a test body
/// uses (#4563). A test that rebinds the parameter name in its body no longer
/// holds the fixture value. One hop only: a fixture's own fixtures are not
/// followed.
fn fixture_owner_call(test: &PythonTest, owner: &PythonOwner) -> bool {
    test.fixture_defs.iter().any(|def| {
        if test_body_binds_local(test, &def.name) {
            return false;
        }
        let mut view = test.clone();
        view.file = def.file.clone();
        view.body_text = def.body_text.clone();
        view.imports = def.imports.clone();
        view.assertions.clear();
        view.fixtures.clear();
        view.fixture_defs.clear();
        related_test_relation(&view, owner).is_some_and(PythonRelationKind::uses_oracle)
    })
}

/// The class of a dunder method owner (`LowerBound` for
/// `LowerBound.__init__`, `Outer.Inner` for a nested class), or `None` for
/// any other owner.
pub(super) fn dunder_method_class(owner: &PythonOwner) -> Option<&str> {
    if !matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    ) || !is_dunder_name(&owner.name)
    {
        return None;
    }
    if !owner.class_path.is_empty() {
        return Some(owner.class_path.as_str());
    }
    owner
        .qualified_name
        .rsplit_once('.')
        .map(|(class, _)| class)
        .filter(|class| !class.is_empty())
}

pub(super) fn is_dunder_name(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

/// Builtins through which Python invokes a dunder method on their sole
/// argument: `reversed(x)` runs `type(x).__reversed__`, `len(x)` runs
/// `__len__`. Operator syntax (`x[i]`, `a == b`) is deliberately absent: it
/// cannot be bound to a receiver by call shape.
pub(super) const PROTOCOL_ENTRIES: [(&str, &str); 8] = [
    ("reversed", "__reversed__"),
    ("len", "__len__"),
    ("bool", "__bool__"),
    ("iter", "__iter__"),
    ("next", "__next__"),
    ("hash", "__hash__"),
    ("str", "__str__"),
    ("repr", "__repr__"),
];

/// `(builtin start, builtin's open paren)` of each `builtin(<receiver>)` call
/// in `text` whose sole argument is a receiver statically bound to the owner
/// class known as `local`: an inline construction `Local(...)`, or the one
/// local that `body` binds once to `Local(...)`. Mirrors the bound-receiver
/// shapes of [`body_calls_method_on_owner_bound_receiver`].
fn protocol_entry_calls(text: &str, body: &str, local: &str, builtin: &str) -> Vec<(usize, usize)> {
    let construct = format!("{local}(");
    let constructions: Vec<usize> = body
        .match_indices(&construct)
        .filter(|(idx, _)| {
            python_callee_start_has_boundary(body, *idx) && !python_text_hides_code(body, *idx)
        })
        .map(|(idx, _)| idx)
        .collect();
    let bound = match constructions.as_slice() {
        [only] => binding_target_for_construction(body, *only)
            .filter(|var| assignment_count(body, var) == 1)
            .filter(|var| !keyword_or_operator_binds_text(body, var, true))
            .filter(|var| !binds_in_target_list(body, var))
            .filter(|_| construction_is_whole_value(body, constructions[0], &construct)),
        _ => None,
    };
    let needle = format!("{builtin}(");
    text.match_indices(&needle)
        .filter(|(idx, _)| {
            python_callee_start_has_boundary(text, *idx) && !python_text_hides_code(text, *idx)
        })
        .filter(|(idx, _)| {
            let argument = text[idx + needle.len()..].trim_start();
            let inline = argument.starts_with(&construct) && {
                let open = text.len() - argument.len() + construct.len() - 1;
                super::no_behavior::matching_call_paren(text, open)
                    .is_some_and(|close| text[close + 1..].trim_start().starts_with(')'))
            };
            inline
                || bound.as_ref().is_some_and(|var| {
                    argument
                        .strip_prefix(var.as_str())
                        .is_some_and(|rest| rest.trim_start().starts_with(')'))
                })
        })
        .map(|(idx, _)| (idx, idx + needle.len() - 1))
        .collect()
}

/// Whether `var` appears in a position that can bind it through a target list
/// or an annotation (`x, r = ...`, `for i, r in ...`, `with a as (b, r)`,
/// `r: list = ...`). Any `var,` / `, var` / `var:` occurrence counts, which
/// also demotes some reads (`f(r, 1)`); that only fails closed.
fn binds_in_target_list(body: &str, var: &str) -> bool {
    body.match_indices(var).any(|(idx, _)| {
        let end = idx + var.len();
        python_callee_start_has_boundary(body, idx)
            && !next_char_is_identifier(body, end)
            && !python_text_hides_code(body, idx)
            && {
                let after = body[end..].trim_start_matches([' ', '\t']);
                let before = body[..idx].trim_end_matches([' ', '\t']);
                after.starts_with(',')
                    || (after.starts_with(':') && !after.starts_with(":="))
                    || before.ends_with(',')
                    || before.ends_with('*')
            }
    })
}

/// Whether the construction at `idx` is the whole right-hand side of its
/// statement: its matching paren ends the line (or is followed only by a
/// comment), so `Class(0).as_list()`, `Class(0) or [1]` and
/// `Class(0) if c else [1]` do not bind the receiver.
fn construction_is_whole_value(body: &str, idx: usize, construct: &str) -> bool {
    let open = idx + construct.len() - 1;
    super::no_behavior::matching_call_paren(body, open).is_some_and(|close| {
        let rest = body[close + 1..].split('\n').next().unwrap_or("").trim();
        rest.is_empty() || rest.starts_with('#')
    })
}

/// Whether the test rebinds the builtin `name` (an import, an assignment,
/// a `def`/`class`/`for`/`as`/walrus target, or a parameter), so a call spelled
/// `name(...)` is no longer the builtin.
fn builtin_is_shadowed(test: &PythonTest, name: &str) -> bool {
    test.imports.iter().any(|import| import.alias == name)
        || test_rebinds_import_alias(test, name)
        || test.constant_rebinding.module_opaque
        || test
            .constant_rebinding
            .module_bound
            .iter()
            .any(|bound| bound == name)
}

/// Protocol-entry calls of the owner dunder in `text` on a receiver bound to
/// the owner class, each as `(builtin start, open paren)`; `body` is the
/// test body that fixes the receiver binding.
fn owner_protocol_entry_calls(
    text: &str,
    test: &PythonTest,
    owner: &PythonOwner,
) -> Vec<(usize, usize)> {
    let builtins: Vec<&str> = PROTOCOL_ENTRIES
        .iter()
        .filter(|(_, dunder)| *dunder == owner.name)
        .map(|(builtin, _)| *builtin)
        .filter(|builtin| !builtin_is_shadowed(test, builtin))
        .collect();
    if builtins.is_empty() {
        return Vec::new();
    }
    let Some(class) = dunder_method_class(owner) else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    for local in owner_class_locals(test, owner, class) {
        for builtin in &builtins {
            calls.extend(protocol_entry_calls(text, &test.body_text, &local, builtin));
        }
    }
    calls
}

/// Whether the test body applies a builtin protocol entry of the owner dunder
/// to a receiver built from the owner class (`reversed(Range(0))`).
fn protocol_entry_in_body(test: &PythonTest, owner: &PythonOwner) -> bool {
    super::test_activation::activation_control(test).is_none()
        && !owner_protocol_entry_calls(&test.body_text, test, owner).is_empty()
}

/// Whether an assertion's text takes a protocol entry of the owner dunder on a
/// bound owner-class receiver as an asserted operand (`list(reversed(Range(0)))
/// == []`), not nested in a wrapper that may ignore it.
pub(super) fn oracle_operand_protocol_entry(
    text: &str,
    test: &PythonTest,
    owner: &PythonOwner,
) -> bool {
    let Some(depth) = oracle_operand_depth(text) else {
        return false;
    };
    let wrappers: Vec<&str> = ORDER_PRESERVING_WRAPPERS
        .into_iter()
        .filter(|wrapper| !builtin_is_shadowed(test, wrapper))
        .collect();
    super::test_activation::activation_control(test).is_none()
        && owner_protocol_entry_calls(text, test, owner)
            .into_iter()
            .any(|(start, open)| reaches_operand_depth(text, start, open, depth, &wrappers))
}

/// Relation for a dunder method owner (`__init__`, `__setitem__`, ...).
///
/// Every class defines the same dunder names, so the bare name says nothing
/// about which class a test exercises: `super().__init__(...)` or a local
/// `def __init__(self)` in a test-local helper class is not a call of
/// `LowerBound.__init__`. A test relates only when it references the owner
/// class. Python invokes these methods through syntax rather than by name:
///
/// - an explicit `Class.__x__(` or `obj.__x__(` call is a syntactic call;
/// - constructing the class (`Class(...)`, an alias, or `module.Class(...)`)
///   calls its `__init__` / `__new__` / `__post_init__`, so a constructor
///   owner relates like a direct call;
/// - any other dunder (`obj[key] = value` for `__setitem__`, `a == b` for
///   `__eq__`) runs on an instance the test built, but syntax alone cannot
///   bind the protocol use to that instance, so the relation stays uncertain.
fn dunder_method_relation(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
) -> Option<PythonRelationKind> {
    if !test_references_owner_class(test, owner, class) {
        return None;
    }
    let body = &test.body_text;
    if contains_call_name(body, &owner.qualified_name)
        || contains_any_attribute_call(body, &owner.name)
    {
        return Some(PythonRelationKind::SyntacticCall);
    }
    if construct_call_invokes_owner(test, owner) {
        return Some(PythonRelationKind::ConstructCall);
    }
    if local_binding_calls_owner(test, owner) {
        return Some(PythonRelationKind::LocalBinding);
    }
    if protocol_entry_in_body(test, owner) {
        return Some(PythonRelationKind::ProtocolEntryCall);
    }
    if test_constructs_class(test, owner, class) {
        return Some(
            if matches!(
                owner.name.as_str(),
                "__init__" | "__new__" | "__post_init__"
            ) {
                PythonRelationKind::ConstructorCall
            } else {
                PythonRelationKind::DunderProtocol
            },
        );
    }
    if same_stem_related(test, owner) {
        return Some(PythonRelationKind::SameStem);
    }
    if test_name_similar_to_owner(test, owner) {
        return Some(PythonRelationKind::TestNameSimilarity);
    }
    if fixture_name_related_to_owner(test, owner) {
        return Some(PythonRelationKind::FixtureName);
    }
    None
}

/// Whether the test references the owner's `class` through an import that
/// reaches the owner's module: the bare name imported from it, a renamed
/// import (`from pkg.cache import Cache as C`), or a member of an imported
/// owner module (`cache.Cache`). A bare `Cache` with no such import, or one
/// imported from another module, is a different class.
fn test_references_owner_class(test: &PythonTest, owner: &PythonOwner, class: &str) -> bool {
    test_uses_owner_class(
        test,
        owner,
        class,
        contains_name_reference,
        contains_member_reference,
    )
}

/// Whether the test calls the owner's `class` (see
/// [`test_references_owner_class`] for which spellings reach it).
fn test_constructs_class(test: &PythonTest, owner: &PythonOwner, class: &str) -> bool {
    test_uses_owner_class(
        test,
        owner,
        class,
        contains_call_name,
        contains_attribute_call,
    )
}

fn test_uses_owner_class(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
    uses_name: fn(&str, &str) -> bool,
    uses_member: fn(&str, &str, &str) -> bool,
) -> bool {
    let body = &test.body_text;
    // A nested class is reached through its outermost class:
    // `from pkg.shapes import Outer` then `Outer.Inner(...)`.
    let (top, nested) = class
        .split_once('.')
        .map_or((class, ""), |(top, _)| (top, &class[top.len()..]));
    test.imports.iter().any(|import| {
        if imports_owner_class(import, owner, top) {
            // `from pkg.cache import *` binds the class under its own name.
            let local = if import.imported == "*" {
                top
            } else {
                import.alias.as_str()
            };
            return !test_binds_local(test, local) && uses_name(body, &format!("{local}{nested}"));
        }
        imports_owner_module(import, owner)
            && !test_binds_local(test, &import.alias)
            && uses_member(body, &import.alias, class)
    })
}

/// `from M import <class>` (or `from M import *`) where `M` is the owner's
/// module or a package that contains it (`from attr import Attribute` for
/// `src/attr/_make.py`: packages re-export their submodules' classes). Method
/// owners carry no resolved re-export set, so the package prefix stands in
/// for it.
fn imports_owner_class(import: &PythonImport, owner: &PythonOwner, class: &str) -> bool {
    (import.imported == class || import.imported == "*")
        && module_contains_owner(&import.source_module, owner)
}

/// An import that binds the owner's module or a package containing it:
/// `import pkg.cache`, `from pkg import cache`, `import cachetools` for
/// `src/cachetools/__init__.py`, `import attr` for `src/attr/_make.py`. The
/// full dotted path must match: `from other import cache` binds a different
/// `cache` module.
fn imports_owner_module(import: &PythonImport, owner: &PythonOwner) -> bool {
    if import.imported == "*" {
        return false;
    }
    let module = if import.source_module.is_empty() {
        import.imported.clone()
    } else {
        format!("{}.{}", import.source_module, import.imported)
    };
    module_contains_owner(&module, owner)
}

/// Whether dotted `module` names the owner's module or a package above it.
/// Only the owner's own module paths count (repository root, below `src`,
/// or below a root `lib`): a trailing part of one (`collections` for
/// `src/mylib/collections.py`, `util.cache` for `src/pkg/util/cache.py`)
/// can name an unrelated module, such as the standard library's. A bare `src`
/// layout root is not a package. Empty never matches.
fn module_contains_owner(module: &str, owner: &PythonOwner) -> bool {
    !module.is_empty()
        && module != "src"
        && owner_module_paths(&owner.file).iter().any(|path| {
            path == module
                || path
                    .strip_prefix(module)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
}

/// Whether a test may reach the dunder owner's class in a shape this adapter
/// cannot bind: its module imports the class or the owner module
/// (`self.Cache(...)` through a unittest mixin attribute, or a test-local
/// subclass), imports anything from the owner module or a package above it
/// (`from cachetools import cachedmethod` builds a subclass of the private
/// `_DescriptorBase`, whose `__get__` runs on attribute access), or its body
/// names the class through an import this adapter does not read
/// (`try: from pkg.cache import Cache`). Such a test makes the owner a
/// dynamic-dispatch limit rather than `no_static_path`.
pub(super) fn test_may_reach_owner_class(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
) -> bool {
    let top = class.split_once('.').map_or(class, |(top, _)| top);
    contains_name_reference(&test.body_text, class)
        || test.imports.iter().any(|import| {
            imports_owner_module(import, owner)
                || imports_owner_class(import, owner, top)
                || module_contains_owner(&import.source_module, owner)
        })
}

/// RIPR-SPEC-0238 rule 13: an exception assertion passed the owner as its
/// callable (`assertRaisesRegex(KeyError, 'empty', parse, '')`) calls it.
fn exception_assertion_calls_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    test.assertions.iter().any(|assertion| {
        assertion
            .callable_call
            .as_deref()
            .is_some_and(|call| body_calls_owner(call, owner))
    })
}

pub(super) fn body_calls_owner(body_text: &str, owner: &PythonOwner) -> bool {
    contains_call_name(body_text, &owner.name)
        || (owner.qualified_name != owner.name
            && contains_call_name(body_text, &owner.qualified_name))
        || (matches!(
            owner.owner_kind,
            Some(OwnerKind::Method | OwnerKind::ClassMethod)
        ) && contains_any_attribute_call(body_text, &owner.name))
}

/// Detects an inline construct-call `OwnerClass(...)(...)` that invokes a changed
/// `__call__` owner directly (e.g. `LogfmtRenderer()(None, None, event_dict)`), a
/// cross-file shape the name/attribute and import-alias heuristics miss — the
/// changed sink is the class's `__call__`, but the test never names `__call__`.
/// Strictly gated so it never over-links: the changed owner must be a `__call__`
/// method (Guard A); the test must import the owner's class by name or alias
/// (Guard B), which blocks a same-named class from an unrelated module; and the
/// constructed instance must be *immediately* called (the balanced-paren check),
/// which distinguishes the inline `C()(...)` from a bound local `x = C(); x(...)`
/// (the latter stays uncertain, consistent with the local-callable limitation).
fn construct_call_invokes_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    // Guard A: only callable-class `__call__` owners.
    if owner.name != "__call__"
        || !matches!(
            owner.owner_kind,
            Some(OwnerKind::Method | OwnerKind::ClassMethod)
        )
    {
        return false;
    }
    let Some((class_name, _)) = owner.qualified_name.rsplit_once('.') else {
        return false;
    };
    if class_name.is_empty() || !class_name.chars().all(is_python_identifier_char) {
        return false;
    }
    // Guard B: the test must import the owner's class — blocks same-named classes
    // in unrelated modules from cross-linking.
    let imports_class = test
        .imports
        .iter()
        .any(|import| import.imported == class_name || import.alias == class_name);
    if !imports_class {
        return false;
    }
    let needle = format!("{class_name}(");
    test.body_text.match_indices(&needle).any(|(idx, _)| {
        python_callee_start_has_boundary(&test.body_text, idx)
            && !line_prefix_looks_like_comment_or_string(&test.body_text, idx)
            && construct_result_is_called(&test.body_text, idx + needle.len() - 1)
    })
}

/// Given the byte index of the `(` that opens a constructor call, returns whether
/// its matching `)` is immediately followed (skipping spaces/tabs) by another `(`
/// — i.e. the constructed instance is called inline, `C(...)(...)`.
pub(super) fn construct_result_is_called(text: &str, open_paren_idx: usize) -> bool {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut index = open_paren_idx;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let mut next = index + 1;
                    while matches!(bytes.get(next), Some(b' ' | b'\t')) {
                        next += 1;
                    }
                    return bytes.get(next) == Some(&b'(');
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}

/// Detects a single unambiguous local binding `local = OwnerClass(...)` whose
/// bound local is then called `local(...)`, invoking a changed `__call__` owner
/// indirectly — the tenacity `stop = stop_after_attempt(3); assertTrue(stop(3))`
/// shape that the Tier B judging measured as a false-actionable (#1160). The test
/// genuinely reaches the owner, so the relation is *direct*: surfacing it lets the
/// existing oracle on the bound call (here a broad-boolean smoke `assertTrue`) be
/// reported instead of dropped, correcting the misleading "no direct test exists"
/// diagnosis. This NEVER credits `exposed` — a smoke oracle stays below `Strong`,
/// so the classification remains `weakly_exposed` (matching the sibling
/// `python_broad_boolean_assertion` golden), only the relation/oracle/card change.
///
/// Strictly gated so it never over-links and never collides with `ConstructCall`
/// (the inline `C()(...)` shape, which is checked first):
///   A. the owner is a `__call__` method;
///   B. the test imports the owner's class by name or alias (blocks a same-named
///      class in an unrelated module);
///   C. exactly one real `Class(` construction appears (call boundary, not a
///      comment/string) and it is *not* called inline (inline is `ConstructCall`);
///   D. that construction is a direct assignment `local = Class(` on its line —
///      keyword-arg / wrapper shapes like `Retrying(stop=stop_after_attempt(3))`
///      fail here and stay uncertain;
///   E. the bound `local` is itself called `local(`, and it is assigned exactly
///      once (a reassigned / rebound local is ambiguous and is rejected).
pub(super) fn local_binding_calls_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    // Guard A: only callable-class `__call__` owners.
    if owner.name != "__call__"
        || !matches!(
            owner.owner_kind,
            Some(OwnerKind::Method | OwnerKind::ClassMethod)
        )
    {
        return false;
    }
    let Some((class_name, _)) = owner.qualified_name.rsplit_once('.') else {
        return false;
    };
    if class_name.is_empty() || !class_name.chars().all(is_python_identifier_char) {
        return false;
    }
    // Guard B: the test must import the owner's class.
    let imports_class = test
        .imports
        .iter()
        .any(|import| import.imported == class_name || import.alias == class_name);
    if !imports_class {
        return false;
    }
    let body = &test.body_text;
    let needle = format!("{class_name}(");
    // Guard C: exactly one real, non-inline `Class(` construction.
    let mut constructions = body.match_indices(&needle).filter(|(idx, _)| {
        python_callee_start_has_boundary(body, *idx)
            && !line_prefix_looks_like_comment_or_string(body, *idx)
    });
    let Some((idx, _)) = constructions.next() else {
        return false;
    };
    if constructions.next().is_some() {
        // More than one construction of the class — ambiguous; stay conservative.
        return false;
    }
    // An inline `Class()(...)` is `ConstructCall` territory, not a bound local.
    if construct_result_is_called(body, idx + needle.len() - 1) {
        return false;
    }
    // Guard D: the construction is a direct assignment `local = Class(` on its line.
    let Some(local_var) = binding_target_for_construction(body, idx) else {
        return false;
    };
    // Guard E: the bound local is called, and assigned exactly once.
    contains_call_name(body, &local_var) && assignment_count(body, &local_var) == 1
}

/// Given the byte index of a `Class(` construction, returns the single local
/// variable it is directly assigned to on the same line — `local = Class(` yields
/// `Some("local")`. Returns `None` for keyword-argument (`stop=Class(`), chained
/// (`a = b = Class(`), augmented, attribute-target (`self.x = Class(`), or any
/// non-bare-identifier assignment, so wrapper/dispatch shapes stay uncertain.
pub(super) fn binding_target_for_construction(
    body_text: &str,
    construction_idx: usize,
) -> Option<String> {
    let line_start = body_text[..construction_idx]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let prefix = body_text[line_start..construction_idx].trim();
    // Require the line to be exactly `<identifier> =` immediately before `Class(`.
    let assign = prefix.strip_suffix('=')?;
    // Reject compound/comparison/augmented operators (`==`, `!=`, `<=`, `+=`, ...).
    if assign.ends_with([
        '=', '!', '<', '>', '+', '-', '*', '/', '%', '&', '|', '^', '~', ':',
    ]) {
        return None;
    }
    let name = assign.trim();
    if name.is_empty()
        || name.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        || !name.chars().all(is_python_identifier_char)
    {
        return None;
    }
    Some(name.to_string())
}

/// Counts direct assignments `name = ...` (whole-token target, not `==`, not an
/// augmented assignment, not a substring of a longer identifier or an attribute
/// like `self.name`) across the test body, so a reassigned binding is rejected.
fn assignment_count(body_text: &str, name: &str) -> usize {
    body_text
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix(name) else {
                return false;
            };
            // Whole-token boundary: the char after `name` ends the identifier.
            if rest.chars().next().is_some_and(is_python_identifier_char) {
                return false;
            }
            let rest = rest.trim_start();
            rest.starts_with('=') && !rest.starts_with("==")
        })
        .count()
}

fn import_alias_calls_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    // A method/classmethod cannot be imported directly by its bare name, so the
    // `import.imported == owner.name` branch would only ever match a same-named
    // free function in an unrelated module — a false relation that then feeds a
    // false-`exposed`. Restrict that branch to non-method owners.
    let is_method_owner = matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    );
    test.imports.iter().any(|import| {
        (!is_method_owner
            && import.imported == owner.name
            && import.alias != owner.name
            && import_module_may_be_owners(import, owner, &test.file)
            && contains_call_name(&test.body_text, &import.alias))
            || (imported_module_matches_owner(import, owner, &test.file)
                // A parameter, fixture or assignment named like the module
                // alias (`def test_one(pkg): pkg.one(...)`) calls a local
                // value, not the imported module.
                && !test_rebinds_import_alias(test, &import.alias)
                && contains_attribute_call(&test.body_text, &import.alias, &owner.name))
            || (!is_method_owner
                && !test_rebinds_import_alias(test, &import.alias)
                && submodule_receivers(import, owner, &test.file)
                    .iter()
                    .any(|receiver| {
                        contains_attribute_call(&test.body_text, receiver, &owner.name)
                    }))
    })
}

/// Callee spellings with which `test` calls a free-function owner with module
/// identity (#4567): the local bound by `from <owner module> import owner [as
/// alias]` (including a package re-export), and `receiver.owner` for every
/// receiver that reaches the owner's module by its dotted path
/// ([`submodule_receivers`]), or a re-exporting package alias. A local that
/// shadows the import is not a spelling. Methods have none.
pub(super) fn owner_module_callees(test: &PythonTest, owner: &PythonOwner) -> Vec<String> {
    if matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    ) || owner.is_module_owner()
    {
        return Vec::new();
    }
    let mut callees = Vec::new();
    for import in &test.imports {
        if test_rebinds_import_alias(test, &import.alias) {
            continue;
        }
        if import.imported == owner.name
            && import_source_module_matches_owner(import, owner, &test.file)
        {
            callees.push(import.alias.clone());
        }
        let reexporting_package =
            import.source_module.is_empty() && owner.reexport_modules.contains(&import.imported);
        let receivers = submodule_receivers(import, owner, &test.file)
            .into_iter()
            .chain(reexporting_package.then(|| import.alias.clone()));
        for receiver in receivers {
            callees.push(format!("{receiver}.{}", owner.name));
        }
    }
    callees.sort();
    callees.dedup();
    callees
}

/// Locals the test body binds exactly once to the result of an owner call
/// through one of `callees` (`result = utils.sign(0)`), so an assertion on the
/// local observes the owner's output (#4567).
pub(super) fn owner_result_locals(test: &PythonTest, callees: &[String]) -> Vec<String> {
    let body = test.body_text.as_str();
    let mut locals = Vec::new();
    let mut line_start = 0usize;
    for line in body.split_inclusive('\n') {
        let start = line_start;
        line_start += line.len();
        let Some(eq) = line.find('=') else {
            continue;
        };
        let (target, value) = (line[..eq].trim(), &line[eq + 1..]);
        let value_start = start + eq + 1 + (value.len() - value.trim_start().len());
        let value = value.trim_start();
        // The whole assigned value must be the owner call, so the local holds
        // its result on every path (`r = sign(0)`, not `r = sign(0) or 1`).
        // A sole-argument materializer (`r = list(gen(0))`) keeps that result.
        let is_whole_call = whole_owner_call_end(body, value_start, callees).is_some_and(|end| {
            let after = body[end..].split('\n').next().unwrap_or_default();
            let after = after.trim();
            after.is_empty() || after.starts_with('#')
        });
        if super::static_limits::is_simple_python_identifier(target)
            && !value.starts_with('=')
            && is_whole_call
            && !python_text_hides_code(body, start + (line.len() - line.trim_start().len()))
            && bracket_depth_at(body, start) == 0
            && assignment_count(body, target) == 1
            && !binds_other_than_assignment(test, target)
            && !test.fixtures.iter().any(|fixture| fixture == target)
        {
            locals.push(target.to_string());
        }
    }
    locals.sort();
    locals.dedup();
    locals
}

/// End (exclusive) of the expression at `start` when it is exactly one owner
/// call, optionally wrapped in sole-argument [`MATERIALIZING_WRAPPERS`].
fn whole_owner_call_end(body: &str, start: usize, callees: &[String]) -> Option<usize> {
    let value = &body[start..];
    for callee in callees {
        if let Some(rest) = value.strip_prefix(callee.as_str()) {
            let open = start + callee.len() + (rest.len() - rest.trim_start().len());
            if rest.trim_start().starts_with('(') {
                return super::no_behavior::matching_call_paren(body, open).map(|close| close + 1);
            }
        }
    }
    for wrapper in MATERIALIZING_WRAPPERS {
        let Some(rest) = value.strip_prefix(wrapper) else {
            continue;
        };
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('(') {
            continue;
        }
        let open = start + wrapper.len() + (rest.len() - trimmed.len());
        let close = super::no_behavior::matching_call_paren(body, open)?;
        let inner = open + 1 + (body[open + 1..].len() - body[open + 1..].trim_start().len());
        let inner_end = whole_owner_call_end(body, inner, callees)?;
        return body[inner_end..close]
            .trim()
            .is_empty()
            .then_some(close + 1);
    }
    None
}

/// Open-bracket depth at byte `idx` of `text`, skipping quoted strings, so a
/// `name=value` line inside a multi-line call reads as a keyword argument,
/// not an assignment.
fn bracket_depth_at(text: &str, idx: usize) -> usize {
    let bytes = &text.as_bytes()[..idx];
    let mut depth = 0usize;
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'#' => {
                cursor += bytes[cursor..]
                    .iter()
                    .position(|&byte| byte == b'\n')
                    .unwrap_or(bytes.len() - cursor);
            }
            quote @ (b'\'' | b'"') => {
                let triple = bytes[cursor..].starts_with(&[quote; 3]);
                let delimiter: &[u8] = if triple { &[quote; 3] } else { &[quote] };
                cursor += delimiter.len();
                while cursor < bytes.len() && !bytes[cursor..].starts_with(delimiter) {
                    cursor += if bytes[cursor] == b'\\' { 2 } else { 1 };
                }
                cursor += delimiter.len();
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                cursor += 1;
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }
    depth
}

/// Bracket depth of an asserted operand in an assertion's text: `0` in an
/// `assert` statement, `1` inside an assertion call's arguments
/// (`self.assertEqual(...)`). `None` when the text is neither shape.
fn oracle_operand_depth(text: &str) -> Option<usize> {
    if text
        .strip_prefix("assert")
        .and_then(|rest| rest.chars().next())
        .is_some_and(char::is_whitespace)
    {
        return Some(0);
    }
    let (callee, _) = text.split_once('(')?;
    (!callee.is_empty()
        && callee
            .chars()
            .all(|ch| ch == '.' || is_python_identifier_char(ch))
        && text.trim_end().ends_with(')'))
    .then_some(1)
}

/// Whether an assertion's text calls `callee` (with an identifier boundary:
/// `utils.sign(`, not `myutils.sign(`) as an asserted operand, not nested
/// inside another call (`always_true(utils.sign(0))` asserts the
/// wrapper's result, which may ignore the owner's output).
pub(super) fn oracle_operand_calls(text: &str, callee: &str) -> bool {
    let Some(depth) = oracle_operand_depth(text) else {
        return false;
    };
    let needle = format!("{callee}(");
    text.match_indices(&needle).any(|(idx, _)| {
        python_callee_start_has_boundary(text, idx)
            && !line_prefix_looks_like_comment_or_string(text, idx)
            && reaches_operand_depth(
                text,
                idx,
                idx + needle.len() - 1,
                depth,
                &MATERIALIZING_WRAPPERS,
            )
    })
}

/// Builtins that only re-materialize their one argument, so a call wrapped in
/// them (`list(utils.sign(0))`) still asserts the owner's output. Any other
/// wrapper may ignore it (`always_true(utils.sign(0))`).
const MATERIALIZING_WRAPPERS: [&str; 5] = ["list", "tuple", "set", "frozenset", "sorted"];

/// Wrappers that keep the entry's element order. `sorted`/`set`/`frozenset`
/// erase the ordering `__reversed__` determines, so a protocol entry under them
/// asserts nothing about the owner.
const ORDER_PRESERVING_WRAPPERS: [&str; 2] = ["list", "tuple"];

/// Whether the call starting at `start` (its `(` at `open`) sits at the
/// asserted operand `depth`, directly or only inside the given `wrappers`
/// that take it as their sole argument.
fn reaches_operand_depth(
    text: &str,
    start: usize,
    open: usize,
    depth: usize,
    wrappers: &[&str],
) -> bool {
    let (mut start, mut open) = (start, open);
    loop {
        let here = bracket_depth_at(text, start);
        if here == depth {
            return true;
        }
        if here < depth {
            return false;
        }
        let Some(close) = super::no_behavior::matching_call_paren(text, open) else {
            return false;
        };
        if !text[close + 1..].trim_start().starts_with(')') {
            return false;
        }
        let Some(before) = text[..start].trim_end().strip_suffix('(') else {
            return false;
        };
        let name_start = before
            .char_indices()
            .rev()
            .find(|(_, ch)| !is_python_identifier_char(*ch))
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        if !wrappers.contains(&&before[name_start..]) || before[..name_start].ends_with('.') {
            return false;
        }
        (start, open) = (name_start, before.len());
    }
}

/// Whether an assertion's text names the local `name` as an asserted operand
/// (`assert rate == 0.15`), not as an argument of another call or an
/// attribute of something else.
pub(super) fn oracle_operand_names(text: &str, name: &str) -> bool {
    let Some(depth) = oracle_operand_depth(text) else {
        return false;
    };
    !name.is_empty()
        && text.match_indices(name).any(|(idx, _)| {
            let end = idx + name.len();
            !text[..idx]
                .chars()
                .next_back()
                .is_some_and(|prev| prev == '.' || is_python_identifier_char(prev))
                && !next_char_is_identifier(text, end)
                && !python_text_hides_code(text, idx)
                && bracket_depth_at(text, idx) == depth
        })
}

/// Receivers that reach the owner's module through an import, by its full
/// dotted module path (#4560). The import binds module path `P`
/// (`import P [as A]`, or `from S import m [as A]` with `P = S.m`). When `P`
/// is the owner's module the receiver is `A`; this covers a package
/// `__init__.py` owner (`from dateutil import zoneinfo` for
/// `src/dateutil/zoneinfo/__init__.py`), whose file stem `__init__` never
/// matches. When the owner's module is `P.<rest>` the receiver is `A.<rest>`:
/// `import click` reaches `click.utils._expand_args(`. Only exact dotted
/// paths match, never a file stem. A module name another workspace project
/// also produces reaches nothing from outside the owner's project (#4566).
pub(super) fn submodule_receivers(
    import: &PythonImport,
    owner: &PythonOwner,
    test_file: &Path,
) -> Vec<String> {
    if !import_module_may_be_owners(import, owner, test_file) {
        return Vec::new();
    }
    let bound = if import.source_module.is_empty() {
        import.imported.clone()
    } else {
        format!("{}.{}", import.source_module, import.imported)
    };
    let prefix = format!("{bound}.");
    owner_module_paths(&owner.file)
        .iter()
        .filter_map(|path| {
            if *path == bound {
                Some(import.alias.clone())
            } else {
                path.strip_prefix(&prefix)
                    .filter(|rest| !rest.is_empty())
                    .map(|rest| format!("{}.{rest}", import.alias))
            }
        })
        .collect()
}

pub(super) fn imported_module_matches_owner(
    import: &PythonImport,
    owner: &PythonOwner,
    test_file: &Path,
) -> bool {
    let matches = owner
        .file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| import.imported.rsplit('.').next() == Some(stem))
        // `import humanize` / `import more_itertools as mi` binds a package
        // whose `__init__.py` re-exports the owner (`reexports.rs`). The full
        // dotted package path must match; the caller still requires the
        // owner's name through the alias (`mi.one(`).
        || (import.source_module.is_empty() && owner.reexport_modules.contains(&import.imported));
    matches && import_module_may_be_owners(import, owner, test_file)
}

/// Whether every module an import names could be the owner's: false when
/// one of them is a name another workspace project also produces (#4566) and
/// the test is not inside the owner's project. A plain `import X` names `X`;
/// `from M import Y` names `M` and, when `Y` is a submodule, `M.Y` (`from
/// shared import calc`).
pub(super) fn import_module_may_be_owners(
    import: &PythonImport,
    owner: &PythonOwner,
    test_file: &Path,
) -> bool {
    if import.source_module.is_empty() {
        return module_name_identifies_owner_for(owner, &import.imported, test_file);
    }
    module_name_identifies_owner_for(owner, &import.source_module, test_file)
        && module_name_identifies_owner_for(
            owner,
            &format!("{}.{}", import.source_module, import.imported),
            test_file,
        )
}

/// The dotted module paths under which the owner file can be imported.
///
/// The first entry is the owner file's full repository-relative module path:
/// `src/handler.py` → `src.handler`, `src/pkg/__init__.py` → `src.pkg`.
/// Identity comparisons must use a full dotted path — a bare file stem is the
/// token-coincidence family (`src/tests/test_handler.py` importing `.handler`
/// resolves to `src.tests.handler`, a different module with the same stem).
///
/// A directory named `src` is the PyPA *src layout* import root: with
/// `src/pricing/discounts.py`, tests import `pricing.discounts` (pytest
/// `pythonpath = ["src"]` or an installed package), never `src.pricing...`. So
/// for every `src` directory segment the dotted path *below* it is also an
/// importable name of the same file (`pricing.discounts`, and in a monorepo
/// `packages/foo/src/foo/bar.py` → `foo.bar`). The full path is kept too, so
/// projects that really write `from src.pricing.discounts import ...` still
/// match. Each form is a complete module path compared by exact equality; no
/// stem or suffix matching is introduced.
pub(super) fn owner_module_paths(file: &Path) -> Vec<String> {
    let normalized = normalized_path(file);
    let mut parts = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if let Some(last) = parts.last_mut() {
        if let Some(stem) = last.strip_suffix(".py") {
            *last = stem;
        }
        if *last == "__init__" {
            parts.pop();
        }
    }
    let mut paths = vec![parts.join(".")];
    // Only directory segments are import roots: the final segment is the module
    // itself (`src.py` / `src/__init__.py` is a module named `src`).
    let directory_count = parts.len().saturating_sub(1);
    for (idx, part) in parts.iter().enumerate().take(directory_count) {
        // `src/` is a layout root at any depth (`packages/x/src/pkg`); `lib/`
        // only at the repository root, since a nested `lib` is usually a
        // package of its own (`pkg/lib/util.py` is `pkg.lib.util`).
        if *part == "src" || (idx == 0 && *part == "lib") {
            let below = parts.get(idx + 1..).unwrap_or_default().join(".");
            if !below.is_empty() && !paths.contains(&below) {
                paths.push(below);
            }
        }
    }
    paths
}

/// A src-layout short module name of an owner file that another workspace
/// source file also produces (#4566): `a/src/shared/calc.py` and
/// `b/src/shared/calc.py` are both importable as `shared.calc`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AmbiguousSrcModule {
    /// The shared dotted name.
    module: String,
    /// The directory holding the owner's `src/` that yields `module`.
    owner_root: PathBuf,
    /// The same directory for every other file yielding `module`.
    rival_roots: Vec<PathBuf>,
}

/// The src-layout short names of `file`, each with its project root (the
/// directory holding that `src` segment). Mirrors the short forms of
/// [`owner_module_paths`]; the full dotted path is never ambiguous.
fn src_layout_module_names(file: &Path) -> Vec<(String, PathBuf)> {
    let paths = owner_module_paths(file);
    let normalized = normalized_path(file);
    let segments = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    // The dotted name below a `src` segment is the full dotted path with the
    // segments up to and including that `src` removed.
    let Some(full) = paths.first() else {
        return Vec::new();
    };
    let directory_count = segments.len().saturating_sub(1);
    segments
        .iter()
        .enumerate()
        .take(directory_count)
        .filter(|(_, part)| **part == "src")
        .filter_map(|(idx, _)| {
            let module = full.split('.').skip(idx + 1).collect::<Vec<_>>().join(".");
            (!module.is_empty() && paths.iter().skip(1).any(|path| *path == module))
                .then(|| (module, segments.iter().take(idx).collect::<PathBuf>()))
        })
        .collect()
}

/// Records on each owner the src-layout short names that another workspace
/// source file also produces, so module identity through such a name can be
/// decided by where the importing test lives. Covers the owner file's own
/// short names and the re-exporting package names `apply_package_reexports`
/// already recorded (`a/src/shared/__init__.py` and `b/src/shared/__init__.py`
/// both import as `shared`), so it must run after that pass.
pub(super) fn apply_src_module_ambiguity<'a>(
    owners: &mut [PythonOwner],
    source_files: impl Iterator<Item = &'a PathBuf>,
) {
    let mut roots_by_module: BTreeMap<String, Vec<(PathBuf, PathBuf)>> = BTreeMap::new();
    for file in source_files {
        for (module, root) in src_layout_module_names(file) {
            roots_by_module
                .entry(module)
                .or_default()
                .push((file.clone(), root));
        }
    }
    for owner in owners.iter_mut() {
        let owner_file = normalized_path(&owner.file);
        let own = src_layout_module_names(&owner.file);
        let mut ambiguous = Vec::new();
        for (module, owner_root) in &own {
            let rival_roots = rival_roots_for(&roots_by_module, module, |file, _| {
                normalized_path(file) == owner_file
            });
            if !rival_roots.is_empty() {
                ambiguous.push(AmbiguousSrcModule {
                    module: module.clone(),
                    owner_root: owner_root.clone(),
                    rival_roots,
                });
            }
        }
        for module in &owner.reexport_modules {
            if own.iter().any(|(own_module, _)| own_module == module) {
                continue;
            }
            // The re-exporting package is the one whose project root holds the
            // owner file; a deeper such root wins over an enclosing one.
            let Some(owner_root) = roots_by_module
                .get(module)
                .into_iter()
                .flatten()
                .map(|(_, root)| root)
                .filter(|root| path_is_under(&owner.file, root))
                .max_by_key(|root| root.components().count())
                .cloned()
            else {
                continue;
            };
            let rival_roots =
                rival_roots_for(&roots_by_module, module, |_, root| *root == owner_root);
            if !rival_roots.is_empty() {
                ambiguous.push(AmbiguousSrcModule {
                    module: module.clone(),
                    owner_root,
                    rival_roots,
                });
            }
        }
        owner.ambiguous_src_modules = ambiguous;
    }
}

/// The project roots of every file producing `module` except the owner's own.
fn rival_roots_for(
    roots_by_module: &BTreeMap<String, Vec<(PathBuf, PathBuf)>>,
    module: &str,
    is_owners: impl Fn(&Path, &PathBuf) -> bool,
) -> Vec<PathBuf> {
    roots_by_module
        .get(module)
        .into_iter()
        .flatten()
        .filter(|(file, root)| !is_owners(file, root))
        .map(|(_, root)| root.clone())
        .collect()
}

/// Whether `file` lies inside directory `root` (an empty root is the
/// repository root and holds everything).
fn path_is_under(file: &Path, root: &Path) -> bool {
    let root = normalized_path(root);
    root.is_empty() || normalized_path(file).starts_with(&format!("{root}/"))
}

/// Whether `test_file` may take `module` as the owner's module. An
/// unambiguous name always may. A name another workspace file also produces
/// identifies the owner only for a test under the owner's project root that
/// is not inside a rival's (deeper) project root; anywhere else the import is
/// as likely to be the rival's module, so it fails closed.
fn module_name_identifies_owner_for(owner: &PythonOwner, module: &str, test_file: &Path) -> bool {
    let Some(ambiguous) = owner
        .ambiguous_src_modules
        .iter()
        .find(|ambiguous| ambiguous.module == module)
    else {
        return true;
    };
    let owner_depth = ambiguous.owner_root.components().count();
    path_is_under(test_file, &ambiguous.owner_root)
        && !ambiguous.rival_roots.iter().any(|rival| {
            rival.components().count() > owner_depth && path_is_under(test_file, rival)
        })
}

/// Whether a `from M import Y` statement's source module `M` is the owner's
/// module. `M` must equal one of the owner's full dotted module paths (see
/// [`owner_module_paths`]): `from src.handler import validate`, a resolved
/// `from .handler import validate` in a sibling file, and — for the src layout
/// — `from handler import validate` all match an owner in `src/handler.py`;
/// `from src.checker import validate` or `from other.handler import validate`
/// do not. A plain `import X` has an empty `source_module` and so never
/// matches — fail closed.
pub(super) fn import_source_module_matches_owner(
    import: &PythonImport,
    owner: &PythonOwner,
    test_file: &Path,
) -> bool {
    if import.source_module.is_empty() {
        return false;
    }
    let names_owner_module = owner_module_paths(&owner.file).contains(&import.source_module)
        // `from humanize import naturaldelta`: the package re-exports the
        // owner under its own name, so the package path identifies it too.
        || (import.imported == owner.name && owner.reexport_modules.contains(&import.source_module));
    names_owner_module && module_name_identifies_owner_for(owner, &import.source_module, test_file)
}

/// Identity index over the parsed tests: a related test is joined back to its
/// source facts by `(file, name)`. Built once per finding so the join is a hash
/// lookup rather than a scan of every test per related test (which made a
/// finding with N related tests cost O(N^2)). Insertion keeps source order, so
/// `matching` yields the same tests in the same order as a linear filter.
pub(super) struct TestLookup<'a> {
    by_name: HashMap<&'a str, Vec<&'a PythonTest>>,
}

impl<'a> TestLookup<'a> {
    pub(super) fn new(all_tests: &'a [PythonTest]) -> Self {
        let mut by_name: HashMap<&'a str, Vec<&'a PythonTest>> = HashMap::new();
        for test in all_tests {
            by_name.entry(test.name.as_str()).or_default().push(test);
        }
        Self { by_name }
    }

    /// Every test whose file and name equal the related test's.
    pub(super) fn matching<'b>(
        &'b self,
        related: &'b RelatedTest,
    ) -> impl Iterator<Item = &'a PythonTest> + 'b {
        self.by_name
            .get(related.name.as_str())
            .into_iter()
            .flatten()
            .copied()
            .filter(move |test| test.file == related.file)
    }

    /// Number of distinct test names indexed.
    #[cfg(test)]
    pub(super) fn identity_count(&self) -> usize {
        self.by_name.len()
    }
}

/// Free-function module-identity evidence: a strong observing test imports the
/// owner's function *from the owner's module*. This is what distinguishes a
/// genuine `from src.handler import validate` from a same-named function pulled in
/// via `from src.checker import validate` — the bare function-name token alone is
/// not identity-bearing for a free-function owner.
pub(super) fn strong_test_imports_owner_from_module(
    strong_tests: &[&RelatedTest],
    all_tests: &TestLookup<'_>,
    owner: &PythonOwner,
) -> bool {
    strong_tests.iter().any(|related_test| {
        all_tests.matching(related_test).any(|test| {
            super::test_activation::activation_control(test).is_none()
                && test.imports.iter().any(|import| {
                    import.imported == owner.name
                        && import_source_module_matches_owner(import, owner, &test.file)
                })
        })
    })
}

fn api_client_route_calls_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    owner
        .route_paths
        .iter()
        .any(|route| body_calls_api_client_route(&test.body_text, route))
}

fn body_calls_api_client_route(body_text: &str, route: &str) -> bool {
    [
        "client.get",
        "client.post",
        "client.put",
        "client.patch",
        "client.delete",
        "client.options",
        "client.head",
    ]
    .into_iter()
    .any(|callee| contains_python_call_with_first_string_argument(body_text, callee, route))
}

fn contains_python_call_with_first_string_argument(
    text: &str,
    callee: &str,
    expected: &str,
) -> bool {
    text.match_indices(callee).any(|(idx, _)| {
        if !python_callee_start_has_boundary(text, idx)
            || python_prefix_hides_code(line_prefix_before(text, idx))
        {
            return false;
        }
        let Some(argument) = first_parenthesized_string_argument(
            text.get(idx + callee.len()..)
                .unwrap_or_default()
                .trim_start(),
        ) else {
            return false;
        };
        argument == expected
    })
}

pub(super) fn first_parenthesized_string_argument(text: &str) -> Option<String> {
    let body = text.strip_prefix('(')?.trim_start();
    let literal = first_python_string_literal(body)?;
    body.starts_with(&literal)
        .then(|| python_string_literal_value(&literal))
        .flatten()
}

pub(super) fn contains_call_name(body_text: &str, call_name: &str) -> bool {
    let needle = format!("{call_name}(");
    body_text
        .match_indices(&needle)
        .any(|(idx, _)| is_bare_call_site(body_text, idx))
}

/// Whether a `name(` at byte `idx` is a bare call: an identifier boundary (no
/// attribute receiver), outside comments, strings and docstrings, and not a
/// `def` / `class` header. The one definition both `contains_call_name`
/// (test-to-owner calls) and `module_callees::bare_callees` (owner-to-helper
/// edges) use, so the two cannot drift (#4568).
pub(super) fn is_bare_call_site(text: &str, idx: usize) -> bool {
    python_callee_start_has_boundary(text, idx)
        && !line_prefix_looks_like_comment_or_string(text, idx)
        && !python_text_hides_code(text, idx)
        && !is_definition_name(text, idx)
}

/// `def name(` / `async def name(` / `class name(` defines `name`; it does not
/// call it.
pub(super) fn is_definition_name(body_text: &str, idx: usize) -> bool {
    matches!(
        line_prefix_before(body_text, idx)
            .split_whitespace()
            .next_back(),
        Some("def" | "class")
    )
}

fn contains_attribute_call(body_text: &str, receiver: &str, attr: &str) -> bool {
    let needle = format!("{receiver}.{attr}(");
    body_text.match_indices(&needle).any(|(idx, _)| {
        python_callee_start_has_boundary(body_text, idx)
            && !line_prefix_looks_like_comment_or_string(body_text, idx)
    })
}

pub(super) fn contains_any_attribute_call(body_text: &str, attr: &str) -> bool {
    let needle = format!(".{attr}(");
    body_text
        .match_indices(&needle)
        .any(|(idx, _)| !line_prefix_looks_like_comment_or_string(body_text, idx))
}

/// Given the byte index of the `(` that opens a `Class(` construction, returns
/// whether its matching `)` is immediately followed (skipping spaces/tabs) by
/// `.method(` — i.e. the constructed instance's method is called inline,
/// `Class(...).method(...)`. Companion to [`construct_result_is_called`] (which
/// detects the `Class()()` callable-instance shape).
fn construct_result_calls_method(text: &str, open_paren_idx: usize, method: &str) -> bool {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut index = open_paren_idx;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let mut next = index + 1;
                    while matches!(bytes.get(next), Some(b' ' | b'\t')) {
                        next += 1;
                    }
                    return text[next..].starts_with(&format!(".{method}("));
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}

/// Local names the owner class is known by in `test`: its imported name plus any
/// `as` alias. Empty when the class is not imported — conservative by design, so a
/// class defined elsewhere and never imported cannot lend its identity. Only the
/// `imported == class` form is identity-bearing: a different class aliased *to* the
/// owner's name (`from m import Other as OwnerClass`) refers to `Other`, not the
/// owner, so it must not contribute a local.
pub(super) fn owner_class_locals(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
) -> Vec<String> {
    let mut locals = Vec::new();
    for import in &test.imports {
        if import.imported == class
            && !import.alias.is_empty()
            && !locals.contains(&import.alias)
            && !test_rebinds_import_alias(test, &import.alias)
            // A second import of the same name rebinds it at module level.
            && test
                .imports
                .iter()
                .filter(|other| other.alias == import.alias)
                .count()
                == 1
            && import_module_may_be_owners(import, owner, &test.file)
        {
            locals.push(import.alias.clone());
        }
        // The class through a module receiver that reaches the owner's module
        // (`from shared import calc`, then `calc.Calculator`), like a
        // module-qualified free-function call (#4567).
        for receiver in submodule_receivers(import, owner, &test.file) {
            let qualified = format!("{receiver}.{class}");
            if !test_rebinds_import_alias(test, &import.alias) && !locals.contains(&qualified) {
                locals.push(qualified);
            }
        }
    }
    locals
}

/// Whether `body` calls `method` on a receiver statically bound to the owner class
/// (known locally as `local`). Three bound-receiver shapes, all excluding
/// comment/string occurrences:
///   * `Local.method(...)`         — classmethod / direct call on the class;
///   * `Local(...).method(...)`    — inline construct then method call;
///   * `v = Local(...); v.method(...)` — single local binding then method call.
///
/// A bare `.method(` on an unrelated or unresolved receiver is NOT matched: that
/// is the false-`exposed` guard — importing or merely mentioning the owner class
/// is not evidence the asserted method ran on an instance of it.
pub(super) fn body_calls_method_on_owner_bound_receiver(
    body: &str,
    local: &str,
    method: &str,
) -> bool {
    // Pattern 1: `Local.method(` — classmethod / direct call on the class itself.
    if contains_attribute_call(body, local, method) {
        return true;
    }
    let construct = format!("{local}(");
    let constructions: Vec<usize> = body
        .match_indices(&construct)
        .filter(|(idx, _)| {
            python_callee_start_has_boundary(body, *idx)
                && !line_prefix_looks_like_comment_or_string(body, *idx)
        })
        .map(|(idx, _)| idx)
        .collect();
    // Pattern 2: `Local(...).method(` — inline construct then method call.
    if constructions
        .iter()
        .any(|&idx| construct_result_calls_method(body, idx + construct.len() - 1, method))
    {
        return true;
    }
    // Pattern 3: `v = Local(...); v.method(` — a single unambiguous local binding
    // (reuses the LocalBinding guards: one construction, direct assignment, one
    // assignment of the bound local) then a method call on that local.
    if constructions.len() == 1
        && let Some(var) = binding_target_for_construction(body, constructions[0])
        && assignment_count(body, &var) == 1
        && contains_attribute_call(body, &var, method)
    {
        return true;
    }
    false
}

/// Locals `v` the test binds once to a construction of the owner's class
/// (`v = Cls(...)`, `Cls` imported from the owner's module) and then calls the
/// owner `method` on (`v.method(...)`): the receiver whose state the owner
/// method writes. Pattern 3 of [`body_calls_method_on_owner_bound_receiver`].
pub(super) fn owner_method_bound_locals(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
    method: &str,
) -> Vec<String> {
    let body = test.body_text.as_str();
    let mut locals = Vec::new();
    for class_local in owner_class_locals(test, owner, class) {
        let construct = format!("{class_local}(");
        let constructions: Vec<usize> = body
            .match_indices(&construct)
            .filter(|(idx, _)| {
                python_callee_start_has_boundary(body, *idx)
                    && !line_prefix_looks_like_comment_or_string(body, *idx)
            })
            .map(|(idx, _)| idx)
            .collect();
        if constructions.len() == 1
            && let Some(var) = binding_target_for_construction(body, constructions[0])
            && assignment_count(body, &var) == 1
            && !binds_other_than_assignment(test, &var)
            && contains_attribute_call(body, &var, method)
            && !locals.contains(&var)
        {
            locals.push(var);
        }
    }
    locals
}

/// Whether the assertion text calls `.method(` only on receivers that are not
/// statically bound to the owner class (#8247). The relation-level evidence
/// ([`strong_test_calls_owner_method_on_bound_receiver`]) reads the whole test
/// body, so a test that calls the owner on `c = Cart()` and asserts a different
/// object's `.total(...)` borrows the owner's identity. An assertion with no
/// `.method(` call (it reads a result local, say) is not judged here.
pub(super) fn assertion_calls_method_only_on_foreign_receiver(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
    method: &str,
    assertion_text: &str,
) -> bool {
    let needle = format!(".{method}(");
    let class_locals = owner_class_locals(test, owner, class);
    let bound_locals = owner_method_bound_locals(test, owner, class, method);
    let mut saw_call = false;
    for (dot, _) in assertion_text.match_indices(&needle) {
        if line_prefix_looks_like_comment_or_string(assertion_text, dot) {
            continue;
        }
        saw_call = true;
        let before = assertion_text[..dot].trim_end();
        let receiver = if before.ends_with(')') {
            inline_construct_callee(before)
        } else {
            let start = before
                .rfind(|ch: char| !(is_python_identifier_char(ch) || ch == '.'))
                .map_or(0, |idx| idx + 1);
            Some(&before[start..])
        };
        let bound = receiver.is_some_and(|receiver| {
            class_locals.iter().any(|local| local == receiver)
                || bound_locals.iter().any(|local| local == receiver)
        });
        if bound {
            return false;
        }
    }
    saw_call
}

/// The callee name of the trailing call in `before` (`Cart(...)` -> `Cart`,
/// `pkg.Cart(...)` -> `pkg.Cart`), or None when the parentheses do not balance.
fn inline_construct_callee(before: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (idx, ch) in before.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    let head = &before[..idx];
                    let start = head
                        .rfind(|ch: char| !(is_python_identifier_char(ch) || ch == '.'))
                        .map_or(0, |found| found + 1);
                    return Some(&head[start..]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Relation-layer receiver-identity evidence for a method/classmethod owner: a
/// strong observing test calls the owner's method on a receiver statically bound
/// to the owner class (see [`body_calls_method_on_owner_bound_receiver`]). This
/// supersedes the weaker "imports + mentions the owner class" gate, which credited
/// `exposed` whenever the class name merely appeared in the test — even as a dead
/// reference or while the asserted `.method(` ran on an unrelated receiver.
pub(super) fn strong_test_calls_owner_method_on_bound_receiver(
    owner: &PythonOwner,
    owner_class_token: Option<&String>,
    method_name: Option<&String>,
    strong_tests: &[&RelatedTest],
    all_tests: &TestLookup<'_>,
) -> bool {
    let (Some(class), Some(method)) = (owner_class_token, method_name) else {
        return false;
    };
    strong_tests.iter().any(|related_test| {
        all_tests.matching(related_test).any(|test| {
            super::test_activation::activation_control(test).is_none()
                && owner_class_locals(test, owner, class).iter().any(|local| {
                    body_calls_method_on_owner_bound_receiver(&test.body_text, local, method)
                })
        })
    })
}

/// Whether every strong test imports a same-named module of another
/// workspace project (#4566): `from shared.calc import Calculator` or `from
/// shared import calc` in `b/tests` names `b`'s code when `a` and `b` both
/// ship `src/shared/calc.py`. Method-owner identity is otherwise class and
/// method name, so this is the module check for that path.
pub(super) fn strong_tests_import_only_rival_modules(
    owner: &PythonOwner,
    strong_tests: &[&RelatedTest],
    all_tests: &TestLookup<'_>,
) -> bool {
    if owner.ambiguous_src_modules.is_empty() || strong_tests.is_empty() {
        return false;
    }
    strong_tests.iter().all(|related_test| {
        all_tests.matching(related_test).any(|test| {
            super::test_activation::activation_control(test).is_none()
                && test
                    .imports
                    .iter()
                    .any(|import| !import_module_may_be_owners(import, owner, &test.file))
        })
    })
}

pub(super) fn line_prefix_looks_like_comment_or_string(body_text: &str, idx: usize) -> bool {
    let line_start = body_text[..idx].rfind('\n').map_or(0, |offset| offset + 1);
    let prefix = &body_text[line_start..idx];
    prefix.trim_start().starts_with('#') || has_unclosed_quote(prefix)
}

pub(super) fn has_unclosed_quote(prefix: &str) -> bool {
    let mut escaped = false;
    let mut in_single = false;
    let mut in_double = false;
    for ch in prefix.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
        } else if ch == '"' && !in_single {
            in_double = !in_double;
        }
    }
    in_single || in_double
}

pub(super) fn is_python_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

/// Whether the test body references the changed owner, even when the reference
/// is not a recognized call shape. Gates the heuristic relations (same stem,
/// test name, fixture name); the direct relations already imply a reference.
///
/// Counted references, each outside comments and string literals:
///
/// - function or class owner: the bare owner name (`handler = loyalty_price`,
///   `assert callable(loyalty_price)`), unless the test binds a local of the
///   same name (parameter/fixture, assignment, nested `def`/`class`) or the name
///   is a keyword-argument or assignment target (`f(loyalty_price=1)`); a
///   renamed import local (`from pricing import loyalty_price as lp` then `lp`)
///   whose source module is the owner module; or a module-qualified member
///   (`pricing.loyalty_price`, `p.loyalty_price` after `import pricing as p`)
///   through an import of the owner module;
/// - method or class-method owner: an attribute reference `.name` (the direct
///   rule already relates any `.name(` call for these owners), and for a dunder
///   method (`__init__`, `__eq__`, ...) a reference to the owner class, since
///   the class invokes it implicitly;
/// - module owner: a local bound by an import of, or from, the owner module.
///
/// A test that names the owner only in its title, a fixture name, its file
/// stem, a comment or a string does not reference it, and neither does an
/// arbitrary object-member use such as `order.loyalty_price` for a free
/// function owner.
pub(super) fn test_references_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    if owner.is_module_owner() {
        return test_references_owner_module(test, owner);
    }
    if matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    ) {
        if contains_attribute_reference(&test.body_text, &owner.name) {
            return true;
        }
        return is_dunder_name(&owner.name)
            && owner
                .qualified_name
                .rsplit_once('.')
                .is_some_and(|(class, _)| test_references_module_symbol(test, owner, class));
    }
    test_references_module_symbol(test, owner, &owner.name)
}

/// A reference to `symbol`, a top-level name defined in the owner module: the
/// bare name, a renamed import local from the owner module, or a member access
/// through an import of the owner module.
fn test_references_module_symbol(test: &PythonTest, owner: &PythonOwner, symbol: &str) -> bool {
    let body = &test.body_text;
    if !test_binds_local(test, symbol) && contains_name_reference(body, symbol) {
        return true;
    }
    test.imports.iter().any(|import| {
        if import.imported == symbol
            && import.alias != symbol
            && import_source_module_matches_owner(import, owner, &test.file)
        {
            return !test_binds_local(test, &import.alias)
                && contains_name_reference(body, &import.alias);
        }
        !test_binds_local(test, &import.alias)
            && ((imported_module_matches_owner(import, owner, &test.file)
                && contains_member_reference(body, &import.alias, symbol))
                || submodule_receivers(import, owner, &test.file)
                    .iter()
                    .any(|receiver| contains_member_reference(body, receiver, symbol)))
    })
}

/// Module-owner reference: the test uses a local bound by `from <owner module>
/// import X` or by an import of the owner module itself.
fn test_references_owner_module(test: &PythonTest, owner: &PythonOwner) -> bool {
    test.imports.iter().any(|import| {
        (import_source_module_matches_owner(import, owner, &test.file)
            || imported_module_matches_owner(import, owner, &test.file))
            && !test_binds_local(test, &import.alias)
            && contains_name_reference(&test.body_text, &import.alias)
    })
}

/// Whether the test binds its own local named `name`: a parameter (pytest
/// fixture), a direct assignment, a walrus (`name :=`), a `for name in` loop
/// target, an `as name` target (`with`, `except`, in-body `import`), or a
/// nested `def`/`class`. Such a local shadows the imported owner, so bare uses
/// of `name` are not owner references.
fn test_binds_local(test: &PythonTest, name: &str) -> bool {
    test.fixtures.iter().any(|fixture| fixture == name) || test_body_binds_local(test, name)
}

/// Whether the test BODY binds `name` (every [`test_binds_local`] form except
/// a parameter), so a parameter of that name no longer holds its argument.
pub(super) fn test_body_binds_local(test: &PythonTest, name: &str) -> bool {
    text_binds_name(&test.body_text, name)
}

/// Every binding form of [`test_body_binds_local`] except a plain `name =`
/// assignment, plus an augmented assignment (`name += 1`).
fn binds_other_than_assignment(test: &PythonTest, name: &str) -> bool {
    keyword_or_operator_binds_text(&test.body_text, name, true)
}

/// Whether `text` binds `name` by assignment, `def`/`class`/`for`/`as`/import,
/// augmented assignment or walrus. Shared by test-body and owner-body checks.
pub(super) fn text_binds_name(text: &str, name: &str) -> bool {
    assignment_count(text, name) > 0 || keyword_or_operator_binds_text(text, name, true)
}

/// [`test_binds_local`] for an imported alias: the test body's own
/// `import pkg.mod as alias` statement is the import being checked (#4567
/// records it among the test's imports), not a rebinding of it.
fn test_rebinds_import_alias(test: &PythonTest, alias: &str) -> bool {
    test.fixtures.iter().any(|fixture| fixture == alias)
        || assignment_count(&test.body_text, alias) > 0
        || keyword_or_operator_binds_text(&test.body_text, alias, false)
}

fn keyword_or_operator_binds_text(body: &str, name: &str, import_as_binds: bool) -> bool {
    augmented_assignment(body, name)
        || walrus_binds(body, name)
        || ["def ", "class ", "for ", "as "]
            .into_iter()
            .any(|keyword| {
                let needle = format!("{keyword}{name}");
                body.match_indices(&needle).any(|(idx, _)| {
                    let end = idx + needle.len();
                    let line_start = body[..idx].rfind('\n').map_or(0, |offset| offset + 1);
                    let line = body[line_start..].trim_start();
                    let import_line = line.starts_with("import ") || line.starts_with("from ");
                    python_callee_start_has_boundary(body, idx)
                        && !next_char_is_identifier(body, end)
                        && !python_text_hides_code(body, idx)
                        && (import_as_binds || keyword != "as " || !import_line)
                })
            })
}

/// A line that starts with `name <op>=` (`total += 1`, `x //= 2`).
fn augmented_assignment(body_text: &str, name: &str) -> bool {
    body_text.lines().any(|line| {
        let Some(rest) = line.trim_start().strip_prefix(name) else {
            return false;
        };
        if rest.chars().next().is_some_and(is_python_identifier_char) {
            return false;
        }
        let op_len = rest
            .trim_start()
            .find('=')
            .filter(|len| (1..=3).contains(len));
        op_len.is_some_and(|len| {
            rest.trim_start()[..len]
                .chars()
                .all(|ch| "+-*/%@&|^<>".contains(ch))
        })
    })
}

/// `name :=` with identifier boundaries, outside comments and strings.
fn walrus_binds(body_text: &str, name: &str) -> bool {
    body_text.match_indices(name).any(|(idx, _)| {
        let end = idx + name.len();
        python_callee_start_has_boundary(body_text, idx)
            && !next_char_is_identifier(body_text, end)
            && body_text[end..]
                .trim_start_matches([' ', '\t'])
                .starts_with(":=")
            && !python_text_hides_code(body_text, idx)
    })
}

/// Bare (possibly dotted, for a module alias) name reference: identifier
/// boundaries on both sides, not an attribute of another receiver, not a
/// keyword-argument or assignment target (`name=`, `name = `), and not inside a
/// comment or string literal.
fn contains_name_reference(body_text: &str, name: &str) -> bool {
    if !is_dotted_python_identifier(name) {
        return false;
    }
    body_text.match_indices(name).any(|(idx, _)| {
        let end = idx + name.len();
        python_callee_start_has_boundary(body_text, idx)
            && !next_char_is_identifier(body_text, end)
            && !is_binding_target(body_text, end)
            && !python_text_hides_code(body_text, idx)
    })
}

/// `receiver.member` with identifier boundaries, outside comments and strings.
fn contains_member_reference(body_text: &str, receiver: &str, member: &str) -> bool {
    if !is_dotted_python_identifier(receiver) || !is_dotted_python_identifier(member) {
        return false;
    }
    let needle = format!("{receiver}.{member}");
    body_text.match_indices(&needle).any(|(idx, _)| {
        python_callee_start_has_boundary(body_text, idx)
            && !next_char_is_identifier(body_text, idx + needle.len())
            && !python_text_hides_code(body_text, idx)
    })
}

/// `.attr` on any receiver, outside comments and strings.
fn contains_attribute_reference(body_text: &str, attr: &str) -> bool {
    if !is_dotted_python_identifier(attr) {
        return false;
    }
    let needle = format!(".{attr}");
    body_text.match_indices(&needle).any(|(idx, _)| {
        !next_char_is_identifier(body_text, idx + needle.len())
            && !python_text_hides_code(body_text, idx)
    })
}

fn next_char_is_identifier(text: &str, idx: usize) -> bool {
    text[idx..]
        .chars()
        .next()
        .is_some_and(is_python_identifier_char)
}

/// `name=` / `name = ` (not `==`): a keyword argument or an assignment target.
fn is_binding_target(text: &str, end: usize) -> bool {
    let rest = text[end..].trim_start_matches([' ', '\t']);
    rest.starts_with('=') && !rest.starts_with("==")
}

fn is_dotted_python_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|segment| {
            !segment.is_empty()
                && !segment.starts_with(|ch: char| ch.is_ascii_digit())
                && segment.chars().all(is_python_identifier_char)
        })
}

/// Whether `idx` sits in a comment or string: a `#` or an open quote earlier on
/// the same line, or an open triple-quoted string (docstring) from an earlier
/// line.
pub(super) fn python_text_hides_code(text: &str, idx: usize) -> bool {
    python_prefix_hides_code(line_prefix_before(text, idx))
        || inside_triple_quoted_string(text, idx)
}

/// Whether `idx` falls inside an open triple-quoted string. A small lexer:
/// triple delimiters inside ordinary single-line strings (`'"""'`) and `#`
/// comments do not open a docstring, and a backslash escapes the next char.
fn inside_triple_quoted_string(text: &str, idx: usize) -> bool {
    let mut triple: Option<&str> = None;
    let mut cursor = 0;
    while cursor < idx {
        let rest = &text[cursor..];
        if let Some(delimiter) = triple {
            if let Some(escaped) = rest.strip_prefix('\\') {
                cursor += 1 + escaped.chars().next().map_or(0, char::len_utf8);
            } else if rest.starts_with(delimiter) {
                triple = None;
                cursor += delimiter.len();
            } else {
                cursor += rest.chars().next().map_or(1, char::len_utf8);
            }
            continue;
        }
        if rest.starts_with("\"\"\"") {
            triple = Some("\"\"\"");
            cursor += 3;
        } else if rest.starts_with("'''") {
            triple = Some("'''");
            cursor += 3;
        } else if rest.starts_with('#') || rest.starts_with('"') || rest.starts_with('\'') {
            let len = if rest.starts_with('#') {
                rest.find('\n').unwrap_or(rest.len())
            } else {
                single_line_string_len(rest)
            };
            // `idx` inside a comment or a (possibly backslash-continued)
            // string literal is hidden code, not a call.
            if idx < cursor + len {
                return true;
            }
            cursor += len;
        } else {
            cursor += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    triple.is_some()
}

/// Byte length of the ordinary single-line string literal at the start of
/// `rest` (which begins with its quote): through the closing quote, or to the
/// end of the line when unterminated.
fn single_line_string_len(rest: &str) -> usize {
    let quote = rest.as_bytes()[0];
    let bytes = rest.as_bytes();
    let mut index = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'\n' => return index,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    rest.len()
}

pub(super) fn same_stem_related(test: &PythonTest, owner: &PythonOwner) -> bool {
    let Some(owner_stem) = owner.file.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    let Some(test_stem) = test.file.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    normalize_test_stem(test_stem) == owner_stem
}

pub(super) fn normalize_test_stem(stem: &str) -> &str {
    stem.strip_prefix("test_")
        .or_else(|| stem.strip_suffix("_test"))
        .unwrap_or(stem)
}

fn test_name_similar_to_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    let test_key = normalize_similarity_key(&test.name);
    owner_similarity_keys(owner)
        .into_iter()
        .any(|key| similarity_key_contains(&test_key, &key))
}

fn fixture_name_related_to_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    test.fixtures.iter().any(|fixture| {
        let fixture_key = normalize_similarity_key(fixture);
        owner_similarity_keys(owner)
            .into_iter()
            .any(|key| similarity_key_contains(&fixture_key, &key))
    })
}

pub(super) fn owner_similarity_keys(owner: &PythonOwner) -> Vec<String> {
    let mut keys = Vec::new();
    if !owner.is_module_owner() {
        keys.push(normalize_similarity_key(&owner.name));
        if owner.qualified_name != owner.name {
            keys.push(normalize_similarity_key(
                &owner.qualified_name.replace('.', "_"),
            ));
        }
    }
    if let Some(stem) = owner.file.file_stem().and_then(|stem| stem.to_str()) {
        keys.push(normalize_similarity_key(stem));
    }
    keys.sort();
    keys.dedup();
    keys.into_iter().filter(|key| key.len() >= 4).collect()
}

pub(super) fn normalize_similarity_key(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_separator = true;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            out.push('_');
            last_was_separator = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

pub(super) fn similarity_key_contains(haystack: &str, needle: &str) -> bool {
    if haystack.is_empty() || needle.is_empty() {
        return false;
    }
    haystack == needle
        || haystack
            .strip_prefix(needle)
            .is_some_and(|tail| tail.starts_with('_'))
        || haystack
            .strip_suffix(needle)
            .is_some_and(|head| head.ends_with('_'))
        || haystack.contains(&format!("_{needle}_"))
}

#[cfg(test)]
mod oracle_operand_tests {
    use super::{oracle_operand_calls, oracle_operand_names};

    #[test]
    fn owner_call_or_local_must_be_an_asserted_operand() {
        for (text, expected) in [
            ("assert utils.sign(0) == 0", true),
            ("assert utils.sign(0)[\"k\"] == 0", true),
            ("self.assertEqual(utils.sign(0), 0)", true),
            ("assert always_true(utils.sign(0)) == True", false),
            ("self.assertTrue(always_true(utils.sign(0)))", false),
            ("assert myutils.sign(0) == 0", false),
            ("assert list(utils.sign(0)) == []", true),
            ("self.assertEqual(list(utils.sign(0)), [])", true),
            ("self.assertEqual(sorted(list(utils.sign(0))), [])", true),
            ("assert always_true(list(utils.sign(0))) == True", false),
            ("assert list(always_true(utils.sign(0))) == []", false),
            ("assert list(utils.sign(0), 1) == []", false),
            ("assert x.list(utils.sign(0)) == []", false),
            ("assert list(utils.sign(0),) == []", false),
            ("assert list(*utils.sign(0)) == []", false),
            ("assert sorted(utils.sign(0), reverse=True) == []", false),
            ("assert tuple(utils.sign(0), 1) == ()", false),
            ("assert set(utils.sign(0))", true),
            ("assert frozenset(utils.sign(0)) == frozenset()", true),
            ("assert list(utils.sign(0))[0] == 1", true),
        ] {
            assert_eq!(oracle_operand_calls(text, "utils.sign"), expected, "{text}");
        }
        for (text, expected) in [
            ("assert rate == 0.15", true),
            ("self.assertEqual(rate, 0.15)", true),
            ("assert always_true(rate)", false),
            ("assert self.rate == 0.15", false),
            ("assert rated == 0.15", false),
        ] {
            assert_eq!(oracle_operand_names(text, "rate"), expected, "{text}");
        }
    }
}
