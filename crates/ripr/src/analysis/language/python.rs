//! Python preview adapter.
//!
//! See `docs/specs/RIPR-SPEC-0028-python-preview-static-facts.md` and
//! `docs/adr/0009-python-parser-substrate.md`.
//!
//! This slice extracts the first useful syntax-first Python facts:
//!
//! - owners for module functions, async functions, class methods, and
//!   `@staticmethod` / `@classmethod` methods;
//! - pytest `test_*` functions, parametrized pytest tests, and
//!   `unittest.TestCase.test_*` methods;
//! - pytest, unittest, and mock assertion/oracle facts;
//! - related-test references by direct calls, import-alias calls, and
//!   conservative same-stem proximity.
//!
//! Import-graph matching, static limits, editor routing, generated tests,
//! runtime execution, and provider calls remain out of scope.
//! Strong exact-value assertions can produce `exposed`; weaker or unknown
//! related-test oracles produce `weakly_exposed`; missing related tests produce
//! `no_static_path`.

use super::super::{AnalysisOptions, diff::ChangedFile, fingerprint_probe_id};
use super::read_limit_disclosure::bounded_read_limit_limitations;
use super::{
    LanguageAdapter, LanguageDiffResult, LanguageId, LanguageRepoResult, PreviewFactCacheStats,
    route,
};
mod admission;
#[cfg(test)]
mod assertion_helper_tests;
mod assertion_selection;
mod bounded_read;
mod fact_serde;
mod fact_store;
pub(crate) use fact_store::PythonFileFactStore;
mod teardown;
mod workspace_reach;
use crate::analysis::workspace::{
    changed_source_files_absent_from_worktree, limitations_for_absent_changed_files,
};
use crate::analysis_outcome::{
    AnalysisLimitation, AnalysisLimitationKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage,
};
use crate::config::{
    OraclePolicy, is_detectable_excluded_python_path, is_detectable_generated_python_path,
};
#[cfg(test)]
use crate::domain::{
    DeltaKind, FlowSinkKind, LanguageId as DomainLanguageId, LanguageStatus, RelatedTest,
    StageState,
};
use crate::domain::{
    ExposureClass, Finding, MissingDiscriminatorFact, OracleKind, OracleStrength, OwnerKind,
    ProbeFamily, StaticLimitKind, StopReason, SymbolId,
};
use bounded_read::{
    PYTHON_MAX_WORKSPACE_FILES_ENV, PythonDiffWalkLimits, WorkspaceFileSelection,
    read_workspace_sources_capped,
};
use rayon::prelude::*;
use rustpython_parser::ast::Expr;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

/// Mark unavailable lexical context. This exact text is not an established stable
/// equivalence; consumers still need to refuse identity-based reuse for v1u.
fn normalize_python_expression(expression: &str) -> String {
    format!("\0python-uncertain-v1\0{expression}")
}

/// Lex a whole file once so fragment boundaries are not mistaken for string
/// boundaries. None means the caller lacks trustworthy source context.
struct PythonIdentityContext<'a> {
    source: &'a str,
    lines: Vec<(usize, usize)>,
    literals: Vec<(usize, usize, bool)>,
    comment_ends: Vec<usize>,
}

impl<'a> PythonIdentityContext<'a> {
    fn new(source: &'a str) -> Option<Self> {
        use rustpython_parser::{Mode, Tok, lexer::lex};
        let mut literals = Vec::new();
        for token in lex(source, Mode::Module) {
            if let (Tok::String { .. }, range) = token.ok()? {
                let (start, end) = (usize::from(range.start()), usize::from(range.end()));
                literals.push((
                    start,
                    end,
                    source.get(start..end)?.chars().any(char::is_whitespace),
                ));
            }
        }
        // The installed lexer skips comments. Walk bytes outside its verified
        // string token spans, consuming each comment through its physical line
        // ending. Quotes or # characters inside comments cannot open a string.
        let bytes = source.as_bytes();
        let (mut pos, mut literal_index) = (0, 0);
        let mut comment_ends = Vec::new();
        while pos < bytes.len() {
            while literals
                .get(literal_index)
                .is_some_and(|(_, end, _)| *end <= pos)
            {
                literal_index += 1;
            }
            if let Some((start, end, _)) = literals.get(literal_index)
                && *start <= pos
                && pos < *end
            {
                pos = *end;
                continue;
            }
            if bytes[pos] == b'#' {
                while pos < bytes.len() && !matches!(bytes[pos], b'\r' | b'\n') {
                    pos += 1;
                }
                if pos < bytes.len() {
                    comment_ends.push(pos);
                }
            } else {
                pos += 1;
            }
        }
        let mut lines = Vec::new();
        let mut start = 0;
        for segment in source.split_inclusive('\n') {
            let line = segment.strip_suffix('\n').unwrap_or(segment);
            let line = line.strip_suffix('\r').unwrap_or(line);
            lines.push((start, start + line.len()));
            start += segment.len();
        }
        Some(Self {
            source,
            lines,
            literals,
            comment_ends,
        })
    }

    fn normalize_line(&self, line: usize, expected: &str) -> Option<String> {
        let &(start, end) = self.lines.get(line.checked_sub(1)?)?;
        if self.source.get(start..end)? != expected {
            return None;
        }
        self.normalize(line, expected)
    }

    fn normalize(&self, line: usize, expected: &str) -> Option<String> {
        let &(line_start, line_end) = self.lines.get(line.checked_sub(1)?)?;
        let source_line = self.source.get(line_start..line_end)?;
        // Diff lines include their indentation; repo facts contain the exact
        // (possibly multiline) AST source span without leading indentation.
        if source_line == expected {
            return self.normalize_span(line_start, expected);
        }
        let first = expected.split('\n').next()?;
        let first = first.strip_suffix('\r').unwrap_or(first);
        if first.is_empty() {
            return None;
        }
        // The same statement text may repeat on one physical line
        // (`return x; return x`). That is ambiguous only when the occurrences
        // normalize differently; identical readings are one identity.
        let mut result: Option<String> = None;
        for (offset, _) in source_line.match_indices(first) {
            let Some(candidate) = self.normalize_span(line_start + offset, expected) else {
                continue;
            };
            match &result {
                Some(previous) if *previous != candidate => return None,
                _ => result = Some(candidate),
            }
        }
        result
    }

    fn normalize_span(&self, start: usize, expected: &str) -> Option<String> {
        let end = start.checked_add(expected.len())?;
        let text = self.source.get(start..end)?;
        if text != expected {
            return None;
        }
        let mut result = String::with_capacity(text.len());
        let mut pending_space = false;
        let mut versioned = false;
        let mut after_comment = false;
        let mut skip_lf_at = None;
        let mut comment_index = self.comment_ends.partition_point(|end| *end < start);
        let mut index = self
            .literals
            .partition_point(|(_, close, _)| *close <= start);
        for (relative, ch) in text.char_indices() {
            let offset = start + relative;
            if skip_lf_at == Some(offset) && ch == '\n' {
                skip_lf_at = None;
                continue;
            }
            if self.comment_ends.get(comment_index) == Some(&offset) {
                pending_space = false;
                result.push('\n');
                versioned = true;
                after_comment = true;
                if ch == '\r' && self.source.as_bytes().get(offset + 1) == Some(&b'\n') {
                    skip_lf_at = Some(offset + 1);
                }
                comment_index += 1;
                continue;
            }
            while self
                .literals
                .get(index)
                .is_some_and(|(_, close, _)| *close <= offset)
            {
                index += 1;
            }
            let inside = self
                .literals
                .get(index)
                .is_some_and(|(open, close, _)| *open <= offset && offset < *close);
            if inside {
                versioned |= self.literals[index].2;
            }
            if ch.is_whitespace() && !inside {
                if !after_comment {
                    pending_space |= !result.is_empty();
                }
            } else {
                after_comment = false;
                if pending_space {
                    result.push(' ');
                    pending_space = false;
                }
                result.push(ch);
            }
        }
        if versioned {
            Some(format!("\0python-literal-v1\0{result}"))
        } else {
            Some(result)
        }
    }
}

/// Rebind a just-classified probe using source-verified lexical boundaries.
/// The caller rebinds its value-origin receipt after any subsequent ordinal.
fn reidentify_python_finding(finding: &mut Finding, normalized: &str) {
    // The Python path can contain colons on Unix. Split the fixed prefix
    // from the left and family/fingerprint from the right.
    let Some((prefix, remainder)) = finding.probe.id.0.split_once(':') else {
        return;
    };
    let Some((path_and_family, _fingerprint)) = remainder.rsplit_once(':') else {
        return;
    };
    let Some((path, family)) = path_and_family.rsplit_once(':') else {
        return;
    };
    let mut id = fingerprint_probe_id(
        prefix,
        path,
        family,
        finding
            .probe
            .owner
            .as_ref()
            .map_or("", |owner| owner.0.as_str()),
        normalized,
        1,
    );
    let marker = if normalized.starts_with("\0python-literal-v1\0") {
        Some("v1-")
    } else if normalized.starts_with("\0python-uncertain-v1\0") {
        Some("v1u-")
    } else {
        None
    };
    if let (Some(marker), Some(separator)) = (marker, id.0.rfind(':')) {
        id.0.insert_str(separator + 1, marker);
    }
    finding.id = id.0.clone();
    finding.probe.id = id;
}

#[cfg(test)]
mod python_identity_review_controls {
    use super::PythonIdentityContext;

    #[test]
    fn comment_boundaries_respect_literals_and_crlf() -> Result<(), String> {
        let expr = |source: &str| -> Result<String, String> {
            let context = PythonIdentityContext::new(source).ok_or("source did not lex")?;
            let expected = source
                .strip_prefix("def label(x):\n    ")
                .ok_or("fixture prefix missing")?
                .trim_end();
            context
                .normalize(2, expected)
                .ok_or_else(|| "source span mismatch".to_string())
        };
        let active = "def label(x):\n    return (\"a b\" # note\n      + x\n    )\n";
        let formatted = "def label(x):\n    return  (\"a b\" # note\r\n       + x\n    )\n";
        let commented = "def label(x):\n    return (\"a b\" # note + x\n    )\n";
        let quoted_hash = "def label(x):\n    return (\"a # b\"\n      + x\n    )\n";
        let quoted_hash_format = "def label(x):\n    return  (\"a # b\"\n       + x\n    )\n";
        assert_eq!(expr(active)?, expr(formatted)?);
        assert_ne!(expr(active)?, expr(commented)?);
        assert_eq!(expr(quoted_hash)?, expr(quoted_hash_format)?);
        Ok(())
    }

    #[test]
    fn a_statement_repeated_on_one_line_normalizes_when_readings_agree() -> Result<(), String> {
        let source = "def f(x):\n    return x; return x\n";
        let context = PythonIdentityContext::new(source).ok_or("source did not lex")?;
        assert_eq!(
            context.normalize(2, "return x"),
            Some("return x".to_string())
        );
        let quoted = "def f(x):\n    y = 'a  b'; z = 'a  b'; y = 'a  b'\n";
        let context = PythonIdentityContext::new(quoted).ok_or("source did not lex")?;
        assert!(context.normalize(2, "y = 'a  b'").is_some());
        // One reading is code and the other sits inside a literal: ambiguous.
        let mixed = "def f(x):\n    return x; s = 'return x'\n";
        let context = PythonIdentityContext::new(mixed).ok_or("source did not lex")?;
        assert!(context.normalize(2, "return x").is_none());
        Ok(())
    }

    #[test]
    fn multiline_literal_space_is_data_and_outer_formatting_is_not() -> Result<(), String> {
        let a = "def label():\n    return  \"\"\"foo \nbar\"\"\"\n";
        let b = "def label():\n    return    \"\"\"foo \nbar\"\"\"\n";
        let c = "def label():\n    return  \"\"\"foo  \nbar\"\"\"\n";
        let normalize = |source: &str| -> Result<String, String> {
            let line = source
                .lines()
                .nth(1)
                .ok_or("fixture opening line missing")?;
            PythonIdentityContext::new(source)
                .ok_or("fixture source did not lex")?
                .normalize(2, line)
                .ok_or_else(|| "fixture line did not match source context".to_string())
        };
        assert_eq!(normalize(a)?, normalize(b)?);
        assert_ne!(normalize(a)?, normalize(c)?);
        Ok(())
    }
}

/// Workspace files whose facts are extracted per parallel batch before the
/// batch is folded, bounding how many unfolded fact sets are alive at once.
const FACT_EXTRACTION_BATCH_FILES: usize = 256;

mod approx_band;
mod arm_activation;
mod bool_oracle;
mod boundary;
mod call_chain;
mod caplog_text;
mod classify;
mod coarse_projection;
mod conditional_defs;
mod conditional_expression;
mod dataclass_default;
mod deleted_lines;
mod derived_expectation;
mod doctest_eval;
mod doctest_tests;
mod dunder_truthiness;
mod guard_activation;
mod guard_atoms;
mod guard_credit;
mod helper_inline;
mod imported_call_operator;
mod known_equality_helpers;
mod loop_walk;
mod message_pin;
mod module_value;
mod test_local_literal;
mod type_guard;
mod wrapped_call;
use classify::{PythonNoBehaviorContext, classify_change_with_value_origins};
#[cfg(test)]
use classify::{
    classify_change, classify_change_with_context, classify_change_with_old,
    classify_change_with_source,
};
mod discriminators;
mod effect_alignment;
#[cfg(test)]
mod effect_order_tests;
mod hypothesis;
mod module_constants;
mod no_behavior;
mod oracles;
mod owner_exact;
mod owners_tests;
mod param_fixtures;
mod parametrize;
mod parse_budget;
mod probe_shape;
mod reexports;
mod related_tests;
mod repo;
mod route_client;
mod row_miss;
mod row_reach;
mod setup_receivers;
mod sink_alignment;
mod source_facts;
mod table_reads;
mod table_rows;
#[cfg(test)]
use discriminators::{
    first_python_keyword_argument, is_literal_python_model_field_value,
    is_python_constructor_callee, is_simple_python_model_field_value,
    python_keyword_argument_parts, python_output_or_call_discriminator,
    python_return_constructor_field_parts, python_return_dict_field_parts,
    python_route_response_field_discriminator, split_python_constructor_call, top_level_equals,
};
use discriminators::{
    first_python_string_literal, is_python_control_predicate_line, parse_attribute_assignment,
    python_assignment_constructor_field_parts, python_dict_field_segment_parts,
    python_exit_code_discriminator, python_return_constructor_field_discriminator,
    python_return_dict_field_discriminator, python_string_literal_value, split_python_assignment,
    top_level_python_segments,
};
#[cfg(test)]
use no_behavior::{
    analyze_call_args, changed_default_value_params, free_function_call_arglists,
    is_annotation_only_def_change, is_annotation_only_var_change,
};
use no_behavior::{
    header_param_line_defaults, is_python_import_line, is_python_no_behavior_line,
    is_python_structural_line, is_structural_def_header_text, multi_line_def_header_span,
    python_quiet_lines_covered_by_run,
};
use oracles::collect_assertions_from_statements;
#[cfg(test)]
use probe_shape::{
    canonical_python_gap_for, classify_probe_shape, classify_probe_shape_with_old,
    contains_mock_initializer, looks_like_call_expression,
};
#[cfg(test)]
use related_tests::{
    PythonRelatedCandidate, PythonRelationKind, binding_target_for_construction, body_calls_owner,
    construct_result_is_called, contains_any_attribute_call, find_related_tests,
    has_unclosed_quote, imported_module_matches_owner, local_binding_calls_owner,
    normalize_similarity_key, normalize_test_stem, owner_similarity_keys,
    python_repair_placement_on, related_test_candidates, related_test_relation, same_stem_related,
    similarity_key_contains, verify_command_for_test_on,
};
use related_tests::{
    assertion_calls_method_only_on_foreign_receiver, first_parenthesized_string_argument,
    import_source_module_matches_owner, strong_test_calls_owner_method_on_bound_receiver,
    strong_test_imports_owner_from_module, strong_tests_import_only_rival_modules,
};
#[cfg(test)]
use sink_alignment::strong_oracle_observes_owner;
#[cfg(test)]
use sink_alignment::{
    classify_sink_alignment, dict_changed_keys_and_values, fstring_change_is_length_invariant,
    fstring_template, oracle_is_pure_len_aggregate, oracle_text_observes_token,
    parse_dict_literal_fields,
};
#[cfg(test)]
use source_facts::parse_module;
pub(crate) use source_facts::{detect_python_test_framework, detect_python_test_frameworks};
use source_facts::{
    extract_source_facts, extract_workspace_source_facts, source_fact_snapshot_observation,
};
mod source_utils;
use source_utils::{is_test_file, normalized_path};
mod collection_config;
mod fixture_substitution;
mod fixtures;
mod forwarding_decorator;
mod helper_assertions;
mod literal_patch;
mod module_callees;
mod same_class_callees;
mod static_limits;
mod term_activation;
mod test_activation;
mod truthiness_swap;
use static_limits::{
    PythonStaticLimit, has_identifier_boundary, line_prefix_before,
    python_callee_start_has_boundary, python_prefix_hides_code,
};
mod cli_runner;
mod cli_subcommand;
mod transitive_reach;
#[cfg(test)]
use static_limits::{
    contains_dynamic_dispatch, contains_dynamic_import, contains_metaprogramming,
    is_known_mock_constructor_import, is_transparent_owner_decorator,
    is_transparent_owner_decorator_for_owner, line_uses_imported_symbol, static_limit_for_change,
    test_has_mocked_module,
};

mod workspace;
use crate::analysis::diff::reconstruct_old_source;
#[cfg(test)]
use workspace::collect_workspace_python_files_with_handwritten;
use workspace::{line_is_in_ranges, owner_for_changed_line, visit_workspace_with_handwritten};

/// Python preview adapter.
///
/// Stateless: routing, parsing, and per-file extraction only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PythonAdapter;

/// Publish one production Python finding after the shared witness retain hook
/// (#4790 / #6426 parity with Rust `push_retained_finding`).
pub(in crate::analysis::language::python) fn publish_production_finding(
    findings: &mut Vec<Finding>,
    finding: Finding,
) {
    crate::analysis::witness::retain_finding_projection(&finding);
    findings.push(finding);
}

/// Whether `text` holds `name` as a whole Python identifier.
fn mentions_python_name(text: &str, name: &str) -> bool {
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    !name.is_empty()
        && text.match_indices(name).any(|(start, _)| {
            let end = start + name.len();
            (start == 0 || !is_ident(text.as_bytes()[start - 1]))
                && !text.as_bytes().get(end).is_some_and(|byte| is_ident(*byte))
        })
}

/// Whether `line` is a decorator in a run of added decorator lines that ends
/// at an added `def` or `class` line: the decorators of a newly added
/// definition. A decorator added above an existing definition, or beside
/// unrelated added lines, is an edit of that definition and keeps its probe.
fn opens_added_definition(
    added_lines: &[crate::analysis::diff::ChangedLine],
    line: &crate::analysis::diff::ChangedLine,
) -> bool {
    let is_decorator = |text: &str| {
        text.trim_start()
            .strip_prefix('@')
            .and_then(|rest| rest.chars().next())
            .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
    };
    if !is_decorator(&line.text) {
        return false;
    }
    let mut next = line.line;
    loop {
        next += 1;
        let Some(following) = added_lines.iter().find(|added| added.line == next) else {
            return false;
        };
        let text = following.text.trim_start();
        if is_decorator(text) {
            continue;
        }
        return text.starts_with("def ")
            || text.starts_with("async def ")
            || text.starts_with("class ");
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonOwner {
    name: String,
    qualified_name: String,
    file: PathBuf,
    start_line: usize,
    end_line: usize,
    owner_kind: Option<OwnerKind>,
    decorators: Vec<String>,
    imports: Vec<PythonImport>,
    cli_receiver_names: Vec<String>,
    route_paths: Vec<String>,
    /// Lowercase HTTP verbs each `route_paths` entry accepts, index-aligned;
    /// an empty entry means the verb is not statically known.
    route_methods: Vec<Vec<String>>,
    /// Receiver of each route decorator (`app` in `@app.get`), index-aligned
    /// with `route_paths`.
    route_receivers: Vec<String>,
    dynamic_route_decorators: Vec<String>,
    /// Decorator names whose binding is not the plain name they spell: a
    /// decorator written as a call (`@final(wrapper)`) or one whose first
    /// segment is redefined at module level. Transparent-decorator rules must
    /// not trust the import for these.
    unplain_decorators: Vec<String>,
    /// First and last source line of a function owner's decorator list. The
    /// `def` line starts the owner, so a changed decorator argument
    /// (`@retry(3)`) would otherwise fall to the module owner and never see
    /// the owner's decorator-indirection limit. `None` without decorators and
    /// for class and module owners.
    decorator_lines: Option<(usize, usize)>,
    /// Declared parameters of a function/method owner, in declaration order.
    /// Empty for class and module owners. Used only to bind literal test-call
    /// arguments to predicate boundary operands (`boundary.rs`).
    parameters: Vec<PythonParameter>,
    /// Dotted package paths whose `__init__.py` re-exports this owner under
    /// its own name (`reexports.rs`). Empty until the workspace pass fills it.
    reexport_modules: Vec<String>,
    /// Whether a `from <owner module> import *` binds this owner: the name is
    /// public and the module's `__all__` is absent or lists it. `false` until
    /// the workspace pass fills it, and when the module source is unavailable
    /// or its `__all__` is unresolved (`reexports.rs`).
    star_exported: bool,
    /// Src-layout short module names of this owner's file that another
    /// workspace source file also produces (`related_tests.rs`, #4566).
    /// Empty until the workspace pass fills it.
    ambiguous_src_modules: Vec<related_tests::AmbiguousSrcModule>,
    /// Dotted module paths of other workspace source files that define a
    /// free function with this owner's name (`related_tests.rs`, #8759). A
    /// test that imports the name from one of them (and from no module that
    /// may be this owner's) exercises that definition instead. Empty until
    /// the workspace pass fills it.
    rival_definition_modules: Vec<String>,
    /// Module-scope literal constants visible in a function/method owner
    /// (not shadowed locally). Empty for class and module owners. Used only
    /// to resolve named predicate boundary operands (`boundary.rs`, #4227).
    module_constants: Vec<module_constants::PythonModuleConstant>,
    /// Same-class methods this method may call through its receiver
    /// (`self.` / `cls.`) or a bound-method alias (`f = self.x`). Empty for
    /// non-methods and `@staticmethod`. Used only to name a Python
    /// transitive-reach limitation on silent `no_static_path` findings (#4765).
    same_class_callees: Vec<String>,
    /// Names this function/method body calls as bare `name(` calls, minus
    /// parameters and names it rebinds. Used only to relate a test that calls
    /// a same-module caller of a changed helper (`helper_owner_call`, #4568).
    module_callees: Vec<String>,
    /// Dotted path of the enclosing classes of a method owner, outermost
    /// first (`Outer.Inner` for `Outer.Inner.__init__`). Empty for other
    /// owners. `qualified_name` keeps only the innermost class.
    class_path: String,
    /// What a function owner's body lets RIPR-SPEC-0238 rule 20 read: its
    /// guarded-return shape as a callee, and whether its parameters hold
    /// their call-site values as a caller (`term_activation.rs`, #6733).
    term_facts: term_activation::PythonTermFacts,
    /// Facts about the owner's class that a dunder-protocol binding needs
    /// (`related_tests.rs`, #9421). Empty until the workspace pass fills it,
    /// which keeps every consumer fail-closed.
    class_facts: related_tests::PythonClassFacts,
    /// Writes to module attributes found in the workspace's other Python
    /// files; only filled for the module owner of a changed file
    /// (`module_value.rs`, #9423).
    foreign_writes: module_value::ForeignWrites,
    /// Owner admitted through a decided interpreter-version condition.
    version_conditional: bool,
    /// The click group this top-level function is registered on as a
    /// subcommand, and the name a test passes to the group to reach it
    /// (`cli_subcommand.rs`, #9226). `None` unless the file proves it.
    cli_subcommand: Option<cli_subcommand::CliSubcommand>,
    /// For a nested function owner (`outer.inner`): whether the enclosing
    /// function's own body, outside the nested def, calls it by name. A test
    /// that only calls the enclosing function reaches the nested body only
    /// when this holds (`related_tests.rs`, #12141). `false` for every other
    /// owner, which keeps parent-call reach fail-closed.
    nested_called_by_parent: bool,
}

/// One declared parameter of a Python function owner.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonParameter {
    name: String,
    /// Source text of the default value, when one is declared.
    default: Option<String>,
    /// Keyword-only parameters (after `*` / `*args`) never bind positionally.
    keyword_only: bool,
    /// Positional-only parameters (before `/`) never bind by keyword.
    positional_only: bool,
}

impl PythonOwner {
    fn symbol_id(&self) -> SymbolId {
        SymbolId(format!(
            "python:{}::{}",
            normalized_path(&self.file),
            self.qualified_name
        ))
    }

    fn is_module_owner(&self) -> bool {
        self.qualified_name == "<module>"
    }

    fn specificity_rank(&self) -> usize {
        if self.owner_kind.is_some() {
            0
        } else if self.is_module_owner() {
            2
        } else {
            1
        }
    }

    fn span_width(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }

    fn kind_label(&self) -> &'static str {
        match self.owner_kind {
            Some(kind) => kind.as_str(),
            None if self.is_module_owner() => "module_function",
            None => "class",
        }
    }

    fn missing_test_reference(&self) -> String {
        if self.is_module_owner() {
            format!("module-level behavior in `{}`", normalized_path(&self.file))
        } else {
            format!("`{}(`", self.name)
        }
    }
}

/// Closed set of Python test frameworks the adapter classifies tests under.
///
/// Every consumer matches this exhaustively, so adding a variant is a compile
/// error until each verify-command, placement and activation site decides it.
/// `capability_disposition_tests` additionally binds each variant to its
/// `python/<framework>/diff/repair_routing` capability cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PythonFramework {
    Pytest,
    Unittest,
}

impl PythonFramework {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pytest => "pytest",
            Self::Unittest => "unittest",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonTest {
    name: String,
    qualified_name: String,
    file: PathBuf,
    line: usize,
    body_text: String,
    imports: Vec<PythonImport>,
    decorators: Vec<String>,
    /// Recognized test controls resolved before test-body imports can shadow aliases.
    activation_controls: Vec<test_activation::PythonActivationControl>,
    fixtures: Vec<String>,
    /// Receivers the test's class builds once in a setup hook (`self.stock =
    /// Stock(5)`); empty outside an unambiguous class (#9417).
    setup_receivers: Vec<setup_receivers::SetupReceiver>,
    /// Definitions the `fixtures` parameters resolve to, in the test's module
    /// or a `conftest.py` up its directory chain. Filled by the workspace
    /// pass (`fixtures.rs`); empty until then.
    fixture_defs: Vec<PythonFixtureDef>,
    parametrized: bool,
    /// The original single parametrization is a plain two-argument literal call.
    simple_literal_parametrize: bool,
    /// Argnames a pytest parametrize decorator supplies directly (not routed
    /// to a fixture by `indirect=`); never resolved as fixtures (#4563).
    direct_parametrized: Vec<String>,
    /// The `direct_parametrized` names whose `indirect=` value the read
    /// cannot settle: not resolved as fixtures, but still counted toward the
    /// fixture limit (#10685).
    unsettled_indirect: Vec<String>,
    /// Every argname the test's and its classes' pytest parametrize marks
    /// name, repeats kept: a repeat is a collection error (#10867).
    parametrize_argnames: Vec<String>,
    /// Literal `@pytest.mark.parametrize` cases, when statically certain (#4559).
    parametrize: Option<parametrize::PythonParametrizeCases>,
    /// Parameters Hypothesis `@given(...)` supplies (#6601); never fixtures.
    generated_inputs: Vec<String>,
    framework: PythonFramework,
    assertions: Vec<PythonAssertion>,
    /// Whether RIPR can say this test has no assertion, or only that it
    /// recognized none (#5571).
    assertion_admission: admission::PythonAssertionAdmission,
    /// Modules this test imports by a bare name that resolve to a file or
    /// directory beside the test file (`from runner import run_case` with
    /// `tests/runner.py`): pytest's rootdir insertion makes them importable
    /// and they are test support the single-file scan cannot read. Filled by
    /// the workspace pass (`fixtures.rs`, #5491); empty until then.
    bare_sibling_modules: Vec<PathBuf>,
    /// How the test and its module can rebind names and attributes; guards
    /// module-constant boundary resolution (`boundary.rs`, #4227).
    constant_rebinding: module_constants::PythonTestRebinding,
}

impl PythonTest {
    /// Whether pytest certainly passes `name`'s parametrize value to the
    /// test itself: a direct argname whose `indirect=` the read settled.
    /// Such a parameter runs no fixture code (#10685).
    fn takes_parametrize_value(&self, name: &str) -> bool {
        self.direct_parametrized.iter().any(|direct| direct == name)
            && !self
                .unsettled_indirect
                .iter()
                .any(|unsettled| unsettled == name)
    }
}

/// A module-level `@pytest.fixture` function: its body is read with the same
/// relation rules as a test body when a test names it as a parameter (#4563).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonFixtureDef {
    /// The public name a test requests; differs from `binding` when the
    /// decorator renames the fixture (`name=`).
    name: String,
    /// The module attribute the def binds, which a later def of the same name
    /// replaces.
    binding: String,
    file: PathBuf,
    body_text: String,
    imports: Vec<PythonImport>,
    /// The class the fixture returns a fresh literal-built instance of
    /// (`return Account(100)`), when that is its whole shape (#9619).
    literal_construction: Option<String>,
    /// The inert test-support class (`CliRunner`) the fixture returns a fresh
    /// literal-built instance of, when that is its whole shape; the
    /// fixture may take a lone unused `request` (#11016).
    inert_tool: Option<String>,
    /// The literal `params=` rows a fixture returning `request.param` hands
    /// the test, one run per row (#9619, `param_fixtures.rs`).
    literal_params: Option<Vec<param_fixtures::FixtureParamRow>>,
    /// Set when attached: no other fixture or hook visible to the test names
    /// this fixture, so the test receives the instance exactly as built.
    isolated: bool,
    /// Set when attached: the `from` imports of the test's module and its
    /// conftest chain, by importing file; each must be the owner's module.
    scope_imports: Vec<(PathBuf, PythonImport)>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonImport {
    imported: String,
    alias: String,
    /// The dotted source module of a `from M import Y` statement (e.g. `src.handler`).
    /// Relative imports are resolved against the importing file when possible (for
    /// example, `from .handler import validate` in `tests/test_api.py` resolves to
    /// `tests.handler`). Empty for a plain `import X`.
    source_module: String,
}

/// A helper's comparison is retained even when its execution/bindings cannot
/// establish effect credit. Named states prevent fallback from laundering it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum PythonHelperOrigin {
    Inline,
    ValueHelper,
    FieldHelperSupported,
    FieldHelperRefused,
}

impl PythonHelperOrigin {
    fn is_helper(self) -> bool {
        self != Self::Inline
    }

    fn effect_only(self) -> bool {
        matches!(self, Self::FieldHelperSupported | Self::FieldHelperRefused)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonAssertion {
    text: String,
    /// The text every alignment gate reads (RIPR-SPEC-0238 rule 4): an
    /// `assert` without its failure message, a comparison call's compared
    /// operands without `msg`, and an exception assertion's `with` item
    /// together with its body. `text` stays the reported oracle.
    gate_text: String,
    /// The call a callable-form exception assertion makes
    /// (`assertRaisesRegex(E, 'm', check, 20, n=3)` calls `check(20, n=3)`;
    /// RIPR-SPEC-0238 rule 13), with every argument it passes on.
    callable_call: Option<String>,
    /// The existing helper-resolution pass supplied this assertion. Its
    /// comparison alone does not establish a deferred owner's execution.
    helper_origin: PythonHelperOrigin,
    /// Native assertion/call span, relative to the containing test body.
    #[serde(with = "fact_serde::option_range")]
    source_range: Option<rustpython_parser::text_size::TextRange>,
    /// Only the native operands retained by the oracle gate, never its message.
    #[serde(with = "fact_serde::named_ranges")]
    observed_operands: Vec<(String, rustpython_parser::text_size::TextRange)>,
    line: usize,
    oracle_kind: OracleKind,
    oracle_strength: OracleStrength,
    oracle_shape: PythonOracleShape,
    /// Set for a bare `assert f(lit, ..)` / `assert not f(lit, ..)`: an
    /// assertion on a call whose arguments are all literals. Extraction reads
    /// it as a smoke truthiness check; `bool_oracle` re-reads it as an exact
    /// boolean pin once the callee is known to return `bool`.
    bare_bool_call: Option<PythonBareBoolCall>,
    /// Set for a call shaped like a numpy.testing or assertpy equality
    /// helper (#10936). Extraction cannot tell whether the name is the
    /// library's; `known_equality_helpers` re-reads the assertion as
    /// `assert left == right` once the file's imports prove it.
    known_equality_helper: Option<PythonKnownEqualityHelper>,
    /// Set when rule 25 demoted `f(g(x)) == g(x)`: `f` is the bare callee
    /// whose result the independent `g(x)` pins. The demotion stays for every
    /// other owner; `owner_exact.rs` re-reads it as an exact pin per finding
    /// whose owner is `f` (#9814).
    owner_exact_callee: Option<String>,
}

/// The call under a candidate numpy.testing or assertpy equality helper
/// (`known_equality_helpers.rs`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonKnownEqualityHelper {
    library: KnownEqualityLibrary,
    /// The callable as written: `np.testing.assert_array_equal`, or for
    /// assertpy the `assert_that` it chains from.
    callee: String,
    /// The two compared operands, parenthesized where `==` would regroup.
    left: String,
    right: String,
    /// Whether the test file's imports prove the helper, answered by the
    /// workspace source-fact parse so the credit pass does not parse the file
    /// again (#10914); None until a pass answers it.
    established: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum KnownEqualityLibrary {
    NumpyTesting,
    Assertpy,
}

/// The call under a bare boolean assertion (`bool_oracle.rs`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PythonBareBoolCall {
    /// The bare callee name; dotted callees are never resolved.
    callee: String,
    /// The call's source text, without the `not`.
    call_text: String,
    /// `Some(b)` for a `True`/`False` literal argument, `None` for any other
    /// literal, by position.
    bool_arguments: Vec<Option<bool>>,
    negated: bool,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
enum PythonOracleShape {
    ExactAssertion,
    BoundaryAssertion,
    ExceptionAssertion,
    FieldAssertion,
    OutputAssertion,
    StatusCodeAssertion,
    BroadSmokeAssertion,
    MockExpectation,
    UnknownCustomHelper,
}

impl PythonOracleShape {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExactAssertion => "exact_assertion",
            Self::BoundaryAssertion => "boundary_assertion",
            Self::ExceptionAssertion => "exception_assertion",
            Self::FieldAssertion => "field_assertion",
            Self::OutputAssertion => "output_assertion",
            Self::StatusCodeAssertion => "status_code_assertion",
            Self::BroadSmokeAssertion => "broad_smoke_assertion",
            Self::MockExpectation => "mock_expectation",
            Self::UnknownCustomHelper => "unknown_custom_helper",
        }
    }
}

fn expr_full_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => expr_full_name(attribute.value.as_ref())
            .map(|prefix| format!("{prefix}.{}", attribute.attr)),
        Expr::Call(call) => expr_full_name(call.func.as_ref()),
        _ => None,
    }
}

fn stop_reason_for_python_static_limit(limit: &PythonStaticLimit) -> StopReason {
    match limit.kind {
        StaticLimitKind::DynamicDispatch => StopReason::DynamicDispatchUnresolved,
        StaticLimitKind::PythonTransitiveReachUnresolved => StopReason::TransitiveReachUnresolved,
        _ => StopReason::StaticProbeUnknown,
    }
}

fn python_weak_missing_summary(
    owner: &PythonOwner,
    probe_family: &ProbeFamily,
    strongest_kind: &OracleKind,
) -> String {
    format!(
        "Related Python test reaches `{}` but the strongest extracted oracle is `{}`; add or strengthen a focused assertion for {}.",
        owner.name,
        strongest_kind.as_str(),
        python_family_assertion_target(probe_family)
    )
}

/// The missing summary when related tests assert, but every assertion
/// observes another behavior family than the change (#5572).
fn python_no_family_relevant_missing_summary(
    owner: &PythonOwner,
    probe_family: &ProbeFamily,
    other_kind: &OracleKind,
) -> String {
    format!(
        "Related Python test reaches `{}`, but its assertions observe a different behavior (strongest: `{}`) and none observes the changed {}; add or strengthen a focused assertion for {}.",
        owner.name,
        other_kind.as_str(),
        probe_family.as_str().replace('_', " "),
        python_family_assertion_target(probe_family)
    )
}

fn python_family_assertion_target(probe_family: &ProbeFamily) -> &'static str {
    match probe_family {
        ProbeFamily::Predicate => "the changed boundary",
        ProbeFamily::ReturnValue => "the returned value",
        ProbeFamily::ErrorPath => "the exact exception type/message",
        ProbeFamily::FieldConstruction => "the changed field or object value",
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => "the changed output/log/call effect",
        ProbeFamily::MatchArm => "the changed match arm",
        ProbeFamily::StaticUnknown => "the changed behavior",
    }
}

fn python_unlinked_owner_next_step(
    owner: &PythonOwner,
    probe_family: &ProbeFamily,
) -> Option<String> {
    // #10708: a directly resolvable owner with no static link is a plain
    // function owner; class-shaped, module, decorated, routed, or otherwise
    // indirected owners stay silent instead of inventing a direct call
    // target. NoStaticPath establishes no statically linked test call, not
    // the universal absence of a test, so the wording names the honest
    // ordered route: add a relevant test/assertion, run the project's native
    // verifier, then refresh. No exact test file or verify command is named:
    // neither is evidenced for an owner with no static link. A text-only
    // mention (an unrelated test that merely names the owner) is not a
    // related test and keeps this generic guidance rather than a credited
    // target.
    if owner.is_module_owner() || owner.owner_kind != Some(OwnerKind::Function) {
        return None;
    }
    // Command-registered functions stay silent: a Typer `@app.command()` (or
    // any decorator) makes the owner a CLI entry, not a directly callable
    // helper under test. A plain helper that merely shares a module with a
    // Typer app keeps guidance — the file-wide receiver list is not evidence
    // the helper itself is registered.
    if !owner.decorators.is_empty()
        || !owner.route_paths.is_empty()
        || !owner.dynamic_route_decorators.is_empty()
    {
        return None;
    }
    let action = match probe_family {
        ProbeFamily::Predicate => "a focused boundary assertion",
        ProbeFamily::ReturnValue => "an exact return-value assertion",
        ProbeFamily::ErrorPath => "an exception assertion",
        ProbeFamily::FieldConstruction => "a field/object assertion",
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
            "an output/log/call-effect assertion"
        }
        ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => "a focused assertion",
    };
    Some(format!(
        "Python preview: no statically linked test calls `{}(`; add a test that calls the changed owner with {action}, run the project's native Python test command, then rerun `ripr check` to refresh the finding.",
        owner.name
    ))
}

fn python_recommended_next_step(
    class: &ExposureClass,
    owner: &PythonOwner,
    probe_family: &ProbeFamily,
    has_oracle_eligible_relation: bool,
    missing_discriminators: &[MissingDiscriminatorFact],
) -> Option<String> {
    match class {
        ExposureClass::StaticUnknown => None,
        ExposureClass::NoStaticPath => python_unlinked_owner_next_step(owner, probe_family),
        ExposureClass::Exposed => {
            Some("Python preview: changed behavior is observed under a strong oracle; verify the assertion targets the changed behavior.".to_string())
        }
        _ if !has_oracle_eligible_relation => None,
        _ => {
            let missing = &missing_discriminators.first()?.value;
            let action = match probe_family {
                ProbeFamily::Predicate => "strengthen the existing related test with a focused boundary assertion",
                ProbeFamily::ReturnValue => {
                    "strengthen the existing related test with an exact return-value assertion"
                }
                ProbeFamily::ErrorPath => {
                    "strengthen the existing related test with an exception assertion"
                }
                ProbeFamily::FieldConstruction => {
                    "strengthen the existing related test with a field/object assertion"
                }
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
                    "strengthen the existing related test with an output/log/call-effect assertion"
                }
                ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => {
                    "strengthen the existing related test with a focused assertion"
                }
            };
            Some(format!(
                "Python preview: {action} for the missing check `{missing}`."
            ))
        }
    }
}

/// Significant identifier and string-literal tokens from a changed source line.
/// These approximate the *changed sink* — the attribute/field/value the change
/// touches — so an oracle that asserts on them is observing the change.
fn significant_change_tokens(line_text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "self", "cls", "return", "if", "elif", "else", "while", "for", "def", "none", "true",
        "false", "and", "or", "not", "in", "is", "raise", "assert", "yield", "await", "async",
        "class", "import", "from", "as", "with", "try", "except", "lambda",
    ];
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in line_text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            push_identifier(&mut out, &mut current, STOP);
        }
    }
    push_identifier(&mut out, &mut current, STOP);
    for quote in ['"', '\''] {
        for (index, part) in line_text.split(quote).enumerate() {
            if index % 2 == 1 && part.len() >= 2 {
                out.push(part.to_string());
            }
        }
    }
    out
}

fn push_identifier(out: &mut Vec<String>, current: &mut String, stop: &[&str]) {
    if current.len() >= 3
        && !stop.contains(&current.to_ascii_lowercase().as_str())
        && !current.chars().all(|c| c.is_ascii_digit())
    {
        out.push(current.clone());
    }
    current.clear();
}

/// How one file's facts were obtained, for the `-v` fact-cache counters.
enum FactLookup {
    /// The store was not consulted (unusable base or a lossy path).
    Skipped,
    Hit,
    Miss {
        stored: Result<bool, String>,
    },
    Corrupt {
        stored: Result<bool, String>,
    },
}

impl FactLookup {
    fn count(&self, stats: &mut PreviewFactCacheStats) {
        let stored = match self {
            Self::Skipped => return,
            Self::Hit => {
                stats.hits += 1;
                return;
            }
            Self::Miss { stored } => stored,
            Self::Corrupt { stored } => {
                stats.corrupt_ignored += 1;
                stored
            }
        };
        stats.misses += 1;
        match stored {
            Ok(true) => stats.stores += 1,
            Ok(false) => {}
            Err(_) => stats.store_errors += 1,
        }
    }
}

/// `relative`'s facts from the store when a content-keyed envelope admits
/// them, else a fresh extraction that is then persisted (#10914).
fn stored_or_extracted_facts(
    store: Option<&PythonFileFactStore>,
    root: &Path,
    relative: &Path,
    source: &str,
) -> (source_facts::PythonSourceFacts, FactLookup) {
    // An envelope is named by the normalized path; a Unix filename holding a
    // literal backslash (or a non-UTF-8 name) would share one with its folded
    // twin, so such a file is neither read nor persisted (as in #10725).
    let lossless = relative
        .to_str()
        .is_some_and(|path| cfg!(windows) || !path.contains('\\'));
    let Some(store) = store.filter(|_| lossless) else {
        return (
            extract_workspace_source_facts(relative, source),
            FactLookup::Skipped,
        );
    };
    let key = fact_store::PythonFileFactKey::new(relative, source, root);
    let corrupt = match store.load(&key) {
        crate::analysis::seam_cache::CacheLoad::Hit(facts) => {
            debug_assert_eq!(
                facts,
                fact_store::served_view(extract_workspace_source_facts(relative, source)),
                "a stored envelope served facts a fresh extraction of {} does not produce",
                relative.display()
            );
            return (facts, FactLookup::Hit);
        }
        crate::analysis::seam_cache::CacheLoad::Miss => false,
        crate::analysis::seam_cache::CacheLoad::CorruptIgnored { .. } => true,
    };
    let facts = extract_workspace_source_facts(relative, source);
    let stored = store.store(&key, &facts);
    let lookup = if corrupt {
        FactLookup::Corrupt { stored }
    } else {
        FactLookup::Miss { stored }
    };
    (facts, lookup)
}

fn parse_budget_limitation(
    relative: &Path,
    facts: &source_facts::PythonSourceFacts,
) -> Result<Option<AnalysisLimitation>, String> {
    let Some(detail) = facts.limitations.iter().find_map(|limitation| {
        limitation
            .evidence
            .split_once("parse_budget:")
            .map(|(_, rest)| format!("parse_budget: {}", rest.trim()))
    }) else {
        return Ok(None);
    };
    Ok(Some(
        AnalysisLimitation::new(
            AnalysisLimitationKind::LanguageScopeUnsupported,
            AnalysisStage::LanguageAdapter,
            AnalysisRecovery::new(
                AnalysisRecoveryKind::Retry,
                "Split or simplify the deeply nested Python expression, operator chain, or elif chain, then re-run the analysis.",
            )?,
        )
        .with_path(normalized_path(relative))?
        .with_affected_items(1)?
        .with_detail(detail)?,
    ))
}

fn identity_context_limitation(relative: &Path) -> Result<AnalysisLimitation, String> {
    let portable = normalized_path(relative);
    let limitation = AnalysisLimitation::new(
        AnalysisLimitationKind::ProducerFailure,
        AnalysisStage::LanguageAdapter,
        AnalysisRecovery::new(AnalysisRecoveryKind::InspectFailure,
            "Restore matching, lexable Python source for the diff and re-run the analysis.")?,
    ).with_affected_items(1)?.with_detail(format!(
        "{portable}: Python source identity context was unavailable or did not match the diff; findings for this file were withheld."
    ))?;
    Ok(limitation
        .clone()
        .with_path(&portable)
        .unwrap_or(limitation))
}

/// One typed `producer_failure` per changed Python file the facts producer
/// could not parse (#6824). The repo-mode evidence producer already types
/// this failure (`source_facts_parse_error`, #3554 PR B), but the PR-local
/// diff/worktree pass used to drop it, so a syntax error in the edited file
/// collapsed the run into a complete, limitations-empty
/// `no_behavioral_candidates` zero. The shape mirrors the Rust lexical
/// fallback parity limitation (`producer_failure` / `language_adapter` /
/// `inspect_failure`, `crates/ripr/src/analysis/language/rust/mod.rs`).
fn source_fact_parse_error_limitation(
    relative: &Path,
    facts: &source_facts::PythonSourceFacts,
) -> Result<Option<AnalysisLimitation>, String> {
    let Some(parse_error) = source_facts::source_facts_syntax_parse_error(facts) else {
        return Ok(None);
    };
    let portable = normalized_path(relative);
    let reason = parse_error
        .evidence
        .strip_prefix("source_fact_parse_error: ")
        .unwrap_or(parse_error.evidence.as_str());
    let detail = format!(
        "{portable}: the Python parser reported a syntax error ({reason}), so no source facts \
         were extracted; this file's changed lines seed no probe shapes and can lose its \
         related tests, so its findings are incomplete."
    )
    .chars()
    .take(crate::analysis_outcome::MAX_ANALYSIS_LIMITATION_DETAIL_CHARS)
    .collect::<String>();
    let limitation = AnalysisLimitation::new(
        AnalysisLimitationKind::ProducerFailure,
        AnalysisStage::LanguageAdapter,
        AnalysisRecovery::new(
            AnalysisRecoveryKind::InspectFailure,
            "Fix the file so it parses as Python, then re-run the analysis.",
        )?,
    )
    .with_affected_items(1)?
    .with_detail(detail)?;
    // A path the portable-path rules reject (a legal Unix filename spelled
    // like a Windows drive path, for example) still gets the limitation;
    // the detail already names it (mirrors the Rust parity limitation).
    let limitation = match limitation.clone().with_path(&portable) {
        Ok(with_path) => with_path,
        Err(_) => limitation,
    };
    Ok(Some(limitation))
}

impl LanguageAdapter for PythonAdapter {
    fn accepts_path(&self, path: &Path) -> bool {
        matches!(route(path), Some(LanguageId::Python))
    }

    fn analyze_diff(
        &self,
        options: &AnalysisOptions,
        _oracle_policy: &OraclePolicy,
        changed_files: &[ChangedFile],
    ) -> Result<LanguageDiffResult, String> {
        PythonAdapter::analyze_diff_with_limits(
            options,
            changed_files,
            PythonDiffWalkLimits::from_env(),
        )
    }

    fn analyze_repo(
        &self,
        options: &AnalysisOptions,
        _oracle_policy: &OraclePolicy,
    ) -> Result<LanguageRepoResult, String> {
        // #2109: the shared repo working-set override bounds the run before
        // any read; an invalid override fails closed as a named error.
        let working_set_limit = repo::repo_working_set_limit()?;
        PythonAdapter::analyze_repo_with_limit(&options.root, working_set_limit)
    }
}

impl PythonAdapter {
    /// The deterministic diff-mode core of [`LanguageAdapter::analyze_diff`]
    /// with the workspace-walk bounds injected (mirrors
    /// [`PythonAdapter::analyze_repo_with_limit`]: `analyze_diff` resolves
    /// the environment, this entry point carries the bounds, so tests can
    /// inject tiny caps without `set_var`, which edition 2024 forbids).
    ///
    /// The bounded walk (`bounded_read.rs`, mirroring the TypeScript
    /// adapter's bounded pattern) caps the discovered-file count, then reads
    /// every retained source ONCE under a per-file cap and an aggregate
    /// byte budget. Files refused by any bound, and unreadable files the
    /// diff touches, become named typed limitations — never silent skips.
    pub(in crate::analysis::language::python) fn analyze_diff_with_limits(
        options: &AnalysisOptions,
        changed_files: &[ChangedFile],
        walk_limits: PythonDiffWalkLimits,
    ) -> Result<LanguageDiffResult, String> {
        Self::analyze_diff_with_handwritten(options, changed_files, walk_limits, &[])
    }

    /// [`analyze_diff_with_handwritten`] with the environment-resolved walk
    /// bounds, for callers outside the Python module (the pipeline) that
    /// cannot name the crate-private [`PythonDiffWalkLimits`] type.
    pub(crate) fn analyze_diff_with_handwritten_env(
        options: &AnalysisOptions,
        changed_files: &[ChangedFile],
        handwritten: &[String],
    ) -> Result<LanguageDiffResult, String> {
        Self::analyze_diff_with_handwritten(
            options,
            changed_files,
            PythonDiffWalkLimits::from_env(),
            handwritten,
        )
    }

    /// [`analyze_diff_with_limits`] plus the `[languages.python]
    /// handwritten_files` declarations (#7386): exact repository-relative
    /// paths admitted past the built-in excluded-directory conventions, so a
    /// hand-written source under a `build/`, `dist/` or `env/` directory is
    /// walked, read and counted instead of silently dropped.
    pub(in crate::analysis::language::python) fn analyze_diff_with_handwritten(
        options: &AnalysisOptions,
        changed_files: &[ChangedFile],
        walk_limits: PythonDiffWalkLimits,
        handwritten: &[String],
    ) -> Result<LanguageDiffResult, String> {
        // A previous run's facts are freed before this run builds its own
        // (sequential callers; see `teardown` for concurrent ones).
        teardown::await_previous();
        // Changed files are the subject of the diff, so the cap only ever
        // refuses context around them (#10461). The walk streams into a
        // bounded selection so discovery memory stays at the cap rather
        // than the workspace size (#10594).
        let changed_walk_paths: BTreeSet<String> = changed_files
            .iter()
            .map(|changed| normalized_path(&changed.path))
            .collect();
        let mut selection =
            WorkspaceFileSelection::new(walk_limits.max_workspace_files, |file: &Path| {
                changed_walk_paths.contains(&normalized_path(file))
            });
        visit_workspace_with_handwritten(&options.root, &options.root, handwritten, &mut |file| {
            selection.push(file)
        });
        let (mut workspace_files, mut refused_files) = selection.finish();
        // Past the cap, spend it on the files whose text can reach the diff
        // instead of the earliest paths (#10785). The second walk holds every
        // path, so it runs only below the scan's own file bound.
        let mut reach = None;
        if refused_files > 0
            && workspace_files.len() + refused_files <= workspace_reach::MAX_SCANNED_FILES
        {
            let mut discovered = Vec::new();
            visit_workspace_with_handwritten(
                &options.root,
                &options.root,
                handwritten,
                &mut |file| discovered.push(file),
            );
            reach = workspace_reach::admit_by_reach(
                &options.root,
                discovered,
                changed_files,
                &changed_walk_paths,
                walk_limits.max_workspace_files,
                walk_limits.max_file_read_bytes,
            );
            if let Some(admission) = &reach {
                workspace_files = admission.kept.clone();
                refused_files = admission.discovered.len() - admission.kept.len();
            }
        }
        let workspace_read = read_workspace_sources_capped(
            &options.root,
            &workspace_files,
            walk_limits.max_file_read_bytes,
            walk_limits.max_workspace_read_bytes,
        );
        let mut all_owners: Vec<PythonOwner> = Vec::new();
        let mut all_tests: Vec<PythonTest> = Vec::new();
        let mut fixture_defs_by_file: BTreeMap<PathBuf, Vec<PythonFixtureDef>> = BTreeMap::new();
        let mut fixture_side_by_file: BTreeMap<PathBuf, fixtures::FixtureSideEffects> =
            BTreeMap::new();
        let mut substitution_by_file: BTreeMap<PathBuf, fixture_substitution::SubstitutionScope> =
            BTreeMap::new();
        // Conftests that install implicit setup around tests (#6657).
        let mut implicit_conftests: BTreeSet<PathBuf> = BTreeSet::new();
        let mut reach_conftests: BTreeSet<PathBuf> = BTreeSet::new();
        let mut helper_modules_by_file: BTreeMap<PathBuf, helper_assertions::PythonHelperModule> =
            BTreeMap::new();
        let mut docstring_ranges_by_file: BTreeMap<PathBuf, Vec<RangeInclusive<usize>>> =
            BTreeMap::new();
        let mut star_facts_by_file: BTreeMap<PathBuf, reexports::PrimedStarFacts> = BTreeMap::new();
        let mut foreign_prior_by_file: BTreeMap<PathBuf, module_value::PriorFileFacts> =
            BTreeMap::new();
        let mut import_ranges_by_file: BTreeMap<PathBuf, Vec<RangeInclusive<usize>>> =
            BTreeMap::new();
        let mut module_attribute_writes: Vec<(
            PathBuf,
            Vec<module_constants::PythonModuleAttributeWrite>,
        )> = Vec::new();
        let mut limitations = Vec::new();
        let mut parse_failure_paths = BTreeSet::new();
        // The diff-scoped limitation set describes the changed scope (the
        // read-failure disclosure model below); parse-failure promotions
        // bind to the same admitted changed paths.
        let changed_paths: Vec<String> = changed_files
            .iter()
            .map(|changed| normalized_path(&changed.path))
            .collect();
        // Fact extraction is per-file and pure, so each bounded batch runs on
        // the rayon workers (#10441: most of a large workspace's run) and is
        // then folded sequentially before the next starts. Indexed collection
        // keeps workspace order, so the fold sees exactly the sequential
        // order, and at most one batch of unfolded facts is held at a time.
        let token = crate::analysis::cancellation::current_token();
        // #10914: persistent per-file fact store. A content-keyed hit serves
        // the file's facts without parsing it; a miss or a corrupt entry
        // extracts and persists. An unusable base is reported once and
        // nothing is read or stored.
        let fact_store = PythonFileFactStore::at(&options.root);
        let mut fact_cache_stats = PreviewFactCacheStats::zero_work();
        fact_cache_stats.unusable_base = fact_store.unusable_base_reason().map(str::to_owned);
        let fact_store_usable = fact_cache_stats.unusable_base.is_none();
        for batch in workspace_files.chunks(FACT_EXTRACTION_BATCH_FILES) {
            let extracted: Vec<(&PathBuf, _, FactLookup)> = batch
                .par_iter()
                .filter_map(|relative| {
                    let source = workspace_read.sources.get(relative)?;
                    let (facts, lookup) =
                        crate::analysis::cancellation::with_optional_token(token.as_ref(), || {
                            stored_or_extracted_facts(
                                fact_store_usable.then_some(&fact_store),
                                &options.root,
                                relative,
                                source,
                            )
                        });
                    Some((relative, facts, lookup))
                })
                .collect();
            for (relative, facts, lookup) in extracted {
                lookup.count(&mut fact_cache_stats);
                debug_assert!(source_fact_snapshot_observation(&facts) > 0);
                if let Some(limitation) = parse_budget_limitation(relative, &facts)? {
                    limitations.push(limitation);
                }
                // #6824: a syntax error in a changed file must surface as a
                // named producer failure, never as a silent complete zero. An
                // unchanged file's parse failure stays out of the diff-scoped
                // limitation set (its context loss matches the Rust twin's
                // unchanged-file behavior).
                if changed_paths
                    .iter()
                    .any(|changed| changed == &normalized_path(relative))
                    && let Some(limitation) = source_fact_parse_error_limitation(relative, &facts)?
                {
                    parse_failure_paths.insert(normalized_path(relative));
                    limitations.push(limitation);
                }
                if let Some(star) = &facts.star_facts {
                    star_facts_by_file.insert(relative.clone(), star.clone());
                }
                foreign_prior_by_file.insert(
                    relative.clone(),
                    module_value::PriorFileFacts {
                        assigned: facts.assigned_attributes.clone(),
                        lexes: facts.foreign_lexes.clone(),
                    },
                );
                docstring_ranges_by_file
                    .insert(relative.clone(), facts.docstring_line_ranges.clone());
                import_ranges_by_file.insert(relative.clone(), facts.import_line_ranges.clone());
                if !facts.fixtures.is_empty() {
                    fixture_defs_by_file.insert(relative.clone(), facts.fixtures.clone());
                }
                if facts.substitution_scope != fixture_substitution::SubstitutionScope::default() {
                    substitution_by_file.insert(relative.clone(), facts.substitution_scope.clone());
                }
                let side = fixtures::side_effects_of(&facts);
                if side != fixtures::FixtureSideEffects::default() {
                    fixture_side_by_file.insert(relative.clone(), side);
                }
                helper_modules_by_file.insert(relative.clone(), facts.helper_module.clone());
                // A conftest with implicit setup, or one that only partly parsed,
                // may assert around every test below it.
                if facts.conftest_implicit_setup
                    || (relative.file_name().and_then(|name| name.to_str()) == Some("conftest.py")
                        && !facts.limitations.is_empty())
                {
                    implicit_conftests.insert(relative.clone());
                }
                if facts.conftest_declares_fixture_or_hook
                    || (relative.file_name().and_then(|name| name.to_str()) == Some("conftest.py")
                        && !facts.limitations.is_empty())
                {
                    reach_conftests.insert(relative.clone());
                }
                module_attribute_writes.push((relative.clone(), facts.module_attribute_writes));
                if is_test_file(relative) {
                    all_tests.extend(facts.tests);
                } else {
                    all_owners.extend(facts.owners);
                }
            }
        }
        fixtures::attach_fixture_defs(&mut all_tests, &fixture_defs_by_file);
        helper_assertions::apply_helper_assertions(&mut all_tests, &helper_modules_by_file);
        // A chain conftest that exists but was never read (capped, excluded or
        // unreadable) cannot be ruled out.
        let unreadable_conftest = |conftest: &Path| {
            !workspace_read.sources.contains_key(conftest) && options.root.join(conftest).is_file()
        };
        fixture_substitution::apply_fixture_substitution(
            &mut all_tests,
            &substitution_by_file,
            unreadable_conftest,
        );
        fixtures::mark_isolated_fixture_defs(
            &mut all_tests,
            &fixture_defs_by_file,
            &fixture_side_by_file,
            unreadable_conftest,
        );
        param_fixtures::apply_literal_param_fixtures(&mut all_tests);
        fixtures::taint_admission_by_conftest(
            &mut all_tests,
            &implicit_conftests,
            unreadable_conftest,
        );
        fixtures::mark_implicit_reach_by_conftest(
            &mut all_tests,
            &reach_conftests,
            unreadable_conftest,
        );
        fixtures::taint_admission_by_conftest_declarations(
            &mut all_tests,
            &reach_conftests,
            unreadable_conftest,
        );
        fixtures::record_bare_sibling_imports(&mut all_tests, |path| {
            options.root.join(path).exists()
        });
        module_constants::apply_foreign_module_attribute_writes(
            &mut all_tests,
            &module_attribute_writes,
        );
        collection_config::apply_collection_config(&mut all_tests, &options.root);
        reexports::apply_package_reexports(
            &mut all_owners,
            |file| workspace_read.sources.get(file).map(String::as_str),
            &|file| star_facts_by_file.get(file),
        );
        bool_oracle::credit_bare_bool_asserts(&mut all_tests, &all_owners, |file| {
            workspace_read.sources.get(file).map(String::as_str)
        });
        known_equality_helpers::credit_known_equality_helpers(&mut all_tests, |file| {
            workspace_read.sources.get(file).map(String::as_str)
        });
        derived_expectation::demote_derived_expectations(&mut all_tests, &all_owners, |file| {
            workspace_read.sources.get(file).map(String::as_str)
        });
        // Rival src-layout modules are found by path, so an over-cap run still
        // sees every discovered module, read or not.
        related_tests::apply_src_module_ambiguity(
            &mut all_owners,
            reach
                .as_ref()
                .map_or(&workspace_files, |admission| &admission.discovered)
                .iter()
                .filter(|file| !is_test_file(file)),
        );
        related_tests::apply_rival_definition_modules(&mut all_owners);
        related_tests::apply_class_facts(&mut all_owners);

        // Walk-count cap disclosure: one named limitation carrying the
        // refused count, mirroring the TypeScript adapter's
        // `DiffScopeOversized` truncated-scan disclosure.
        // With a reach admission, only refused files that may reach the diff
        // leave it partial; the rest name nothing the diff changes.
        let refused_may_reach = reach
            .as_ref()
            .map_or(refused_files, |admission| admission.refused_reaching);
        if refused_may_reach > 0 {
            let detail = match &reach {
                None => format!(
                    "python workspace walk reached the {}-file cap ({PYTHON_MAX_WORKSPACE_FILES_ENV}); {refused_files} discovered .py file(s) were not analyzed; changed .py files are kept past this cap.",
                    walk_limits.max_workspace_files,
                ),
                Some(admission) => format!(
                    "python workspace walk reached the {}-file cap ({PYTHON_MAX_WORKSPACE_FILES_ENV}); {refused_may_reach} of the {} discovered .py file(s) that may reach the diff (naming what it changes, imported by a test or changed file that does, a conftest.py, or a package __init__.py above a changed file) were not analyzed, and {} that name nothing it changes were skipped; changed .py files are kept past this cap.",
                    walk_limits.max_workspace_files,
                    admission.reaching,
                    admission.refused_unreached,
                ),
            };
            limitations.push(
                AnalysisLimitation::new(
                    AnalysisLimitationKind::DiffScopeOversized,
                    AnalysisStage::LanguageAdapter,
                    AnalysisRecovery::new(
                        AnalysisRecoveryKind::IncreaseConfiguredLimit,
                        "Raise RIPR_PYTHON_MAX_WORKSPACE_FILES, then re-run the analysis.",
                    )?,
                )
                .with_affected_items(
                    u64::try_from(refused_may_reach)
                        .map_err(|err| format!("refused file count overflows u64: {err}"))?,
                )?
                .with_detail(detail)?,
            );
        }

        // Read-bound disclosures: named limitations for the files the read
        // caps refuse, bounded per run (#5022). The disclosure is a
        // stable-sorted sample of refused paths; refusals beyond the sample
        // fold into one summary entry carrying the true refused count, and
        // its detail states why the full per-file list is not materialized.
        // Fail-closed behavior is unchanged: every refused file is still
        // refused; only the disclosure is sampled. The recovery names the
        // env knobs so operators can raise the bounds.
        limitations.extend(bounded_read_limit_limitations(
            "python",
            workspace_read
                .limits
                .iter()
                .map(|(file, err)| (normalized_path(file), err.reason()))
                .collect(),
            "Raise RIPR_PYTHON_MAX_FILE_READ_BYTES and/or RIPR_PYTHON_MAX_WORKSPACE_READ_BYTES, then re-run the analysis.",
        )?);

        // Read-failure disclosure (the TypeScript adapter's #4099 model):
        // an unreadable CHANGED file is never classified and its tests
        // vanish from the index, so the diff-scoped result names the path
        // and the concrete read failure. Unreadable unchanged files are
        // counted in `skipped_files` below but stay out of the diff-scoped
        // limitation set.
        for (file, error) in &workspace_read.io_failures {
            if !changed_paths
                .iter()
                .any(|changed| changed == &normalized_path(file))
            {
                continue;
            }
            limitations.push(
                AnalysisLimitation::new(
                    AnalysisLimitationKind::LanguageScopeUnsupported,
                    AnalysisStage::LanguageAdapter,
                    AnalysisRecovery::new(
                        AnalysisRecoveryKind::Retry,
                        "Restore read access to the file (check permissions and UTF-8 encoding), then re-run the analysis.",
                    )?,
                )
                .with_path(normalized_path(file))?
                .with_affected_items(1)?
                .with_detail(format!("read failed: {error}"))?,
            );
        }
        let skipped_files = workspace_read.io_failures.len();

        // Excluded subtrees and generated files are skipped before admission
        // and counting (#3672); the pipeline owns their skipped-scope disclosure.
        let python_changed_files = changed_files
            .iter()
            .filter(|changed| {
                matches!(route(&changed.path), Some(LanguageId::Python))
                    && !is_detectable_generated_python_path(&changed.path)
                    && (!is_detectable_excluded_python_path(&changed.path)
                        || crate::config::is_declared_handwritten_python_path(
                            handwritten,
                            &changed.path,
                        ))
            })
            .collect::<Vec<_>>();
        // The workspace walk cannot discover a missing NEW-side path. Reuse
        // source admission before counting or classifying it (#5110); genuine
        // deletions are already omitted by the diff parser.
        let absent_changed_files = changed_source_files_absent_from_worktree(
            &options.root,
            python_changed_files
                .iter()
                .map(|changed| changed.path.as_path()),
        );
        limitations.extend(limitations_for_absent_changed_files(&absent_changed_files)?);
        let absent_changed_paths = absent_changed_files
            .iter()
            .map(|path| normalized_path(path))
            .collect::<BTreeSet<_>>();

        // Module-value reads (#9423) need what the rest of the workspace can
        // do to a changed module's attributes, and which tests the diff also
        // edits. Both are filled once per changed file, before classification.
        let workspace_incomplete = refused_files > 0
            || !workspace_read.limits.is_empty()
            || !workspace_read.io_failures.is_empty();
        // This physical-only capability cannot establish the complete HEAD
        // import environment for committed-history or materialized subjects.
        // Explicit diff input is the pipeline's live-source view.
        if options.diff_file.is_some()
            && all_owners.iter().any(|owner| {
                owner.name == "__bool__"
                    && python_changed_files
                        .iter()
                        .any(|changed| changed.path == owner.file)
            })
        {
            dunder_truthiness::bind_workspace_observer(
                &options.root,
                &mut all_owners,
                &workspace_read.sources,
                workspace_incomplete,
            );
        }
        let foreign_index = module_value::ForeignIndex::build_with_prior(
            workspace_read
                .sources
                .iter()
                .map(|(file, source)| (file.as_path(), source.as_str())),
            workspace_incomplete,
            &foreign_prior_by_file,
        );
        // Bytecode CPython never checks against its source, or an import path
        // a test configuration adds, can run code other than what is read
        // here (#9226). Read once, only when a file changed.
        let shadowed_import = !python_changed_files.is_empty()
            && (module_value::unchecked_bytecode(
                &options.root,
                workspace_read.sources.keys().map(|file| file.as_path()),
            ) || module_value::import_path_configured(
                &options.root,
                workspace_read.sources.keys().map(|file| file.as_path()),
            ));
        let extension_names = if python_changed_files.is_empty() {
            Some(BTreeSet::new())
        } else {
            module_value::extension_names(
                &options.root,
                workspace_read.sources.keys().map(|file| file.as_path()),
            )
        };
        // pytest's prepend mode can put any ancestor of a test file on
        // `sys.path`, including one with no source of its own.
        let import_roots: BTreeSet<&Path> = workspace_read
            .sources
            .keys()
            .flat_map(|file| file.ancestors().skip(1))
            .collect();
        let foreign_of = |file: &Path| {
            let mut foreign = foreign_index.excluding(file);
            if shadowed_import
                || module_value::extension_beside(&options.root, file)
                || module_value::compiled_package_shadow(&options.root, &import_roots, file)
                || extension_names
                    .as_ref()
                    .is_none_or(|names| module_value::shadowed_by_extension(names, file))
            {
                foreign.mark_compiled_shadow();
            }
            foreign
        };
        for changed in &python_changed_files {
            let mut foreign = foreign_of(&changed.path);
            // A sole top-level line replacement may be a module constant a
            // sibling module reads through a relative import of one of its
            // readers (#9619); each such module gets its own write view.
            if let ([added], [_]) = (
                changed.added_lines.as_slice(),
                changed.removed_lines.as_slice(),
            ) && added
                .text
                .chars()
                .next()
                .is_some_and(|first| first.is_alphabetic() || first == '_')
            {
                foreign.set_forwarders(
                    module_value::forwarder_candidates(
                        &changed.path,
                        workspace_read
                            .sources
                            .iter()
                            .map(|(file, source)| (file.as_path(), source.as_str())),
                        |file| workspace_read.sources.contains_key(file),
                    )
                    .into_iter()
                    .map(|(file, source)| module_value::ForwarderModule {
                        file: file.to_path_buf(),
                        source: source.to_string(),
                        foreign: foreign_of(file),
                    })
                    .collect(),
                );
            }
            conditional_defs::retain_workspace_certain(&mut all_owners, &changed.path, &foreign);
            for owner in all_owners
                .iter_mut()
                .filter(|owner| owner.is_module_owner() && owner.file == changed.path)
            {
                owner.foreign_writes = foreign.clone();
            }
        }
        // Doctest examples (RIPR-SPEC-0238 rule 26) are tests that live in a
        // changed owner file; they need the file's old source to prove the
        // example held before the edit.
        for changed in &python_changed_files {
            if is_test_file(&changed.path)
                || absent_changed_paths.contains(&normalized_path(&changed.path))
            {
                continue;
            }
            let Some(new_source) = workspace_read.sources.get(&changed.path) else {
                continue;
            };
            let Some(old_source) = reconstruct_old_source(new_source, changed) else {
                continue;
            };
            all_tests.extend(doctest_tests::doctest_tests(
                &changed.path,
                new_source,
                &old_source,
                &options.root,
            ));
        }
        for test in &mut all_tests {
            if python_changed_files
                .iter()
                .any(|changed| changed.path == test.file)
            {
                test.constant_rebinding.edited_in_diff = true;
            }
        }

        let mut findings: Vec<Finding> = Vec::new();
        let mut value_origins = crate::value_origins::ValueOrigins::default();
        let mut related_memo = related_tests::RelatedCandidateMemo::default();
        let mut changed_count: usize = 0;
        for changed in python_changed_files {
            if absent_changed_paths.contains(&normalized_path(&changed.path)) {
                continue;
            }
            changed_count += 1;
            let file_start = findings.len();
            let identity_context = workspace_read
                .sources
                .get(&changed.path)
                .and_then(|source| PythonIdentityContext::new(source));
            if is_test_file(&changed.path) {
                continue;
            }
            let new_docstring_ranges = docstring_ranges_by_file
                .get(&changed.path)
                .map(Vec::as_slice)
                .unwrap_or_default();
            // The old-source reconstruction reuses the once-read capped
            // workspace source. A fresh read here would defeat the walk
            // bounds above; a capped-out file degrades to empty ranges,
            // exactly the pre-existing unreadable-file path, and the file
            // itself is already named in the limitation set.
            let old_source = workspace_read
                .sources
                .get(&changed.path)
                .and_then(|source| reconstruct_old_source(source, changed));
            // Never mint an ID from a fragment without source-verified string
            // boundaries. Paired deletions do not become deleted findings.
            let paired_old: BTreeSet<usize> = changed
                .aligned_counterparts()
                .values()
                .map(|line| line.line)
                .collect();
            let old_identity = old_source.as_deref().and_then(PythonIdentityContext::new);
            let valid_new = identity_context.as_ref().is_some_and(|context| {
                changed
                    .added_lines
                    .iter()
                    .all(|line| context.normalize_line(line.line, &line.text).is_some())
            });
            let valid_old = changed
                .removed_lines
                .iter()
                .filter(|line| !paired_old.contains(&line.line))
                .all(|line| {
                    old_identity.as_ref().is_some_and(|context| {
                        context.normalize_line(line.line, &line.text).is_some()
                    })
                });
            if !valid_new || !valid_old {
                // Missing/read-failed files already have their own disclosure.
                if workspace_read.sources.contains_key(&changed.path)
                    && !parse_failure_paths.contains(&normalized_path(&changed.path))
                {
                    limitations.push(identity_context_limitation(&changed.path)?);
                }
                continue;
            }
            let (old_docstring_ranges, old_import_ranges) = old_source
                .as_deref()
                .map(|source| {
                    let old_facts = extract_source_facts(&changed.path, source);
                    (
                        old_facts.docstring_line_ranges,
                        old_facts.import_line_ranges,
                    )
                })
                .unwrap_or_default();
            // Lines inside a multi-line string are string content, not
            // statements; an unreadable or unlexable file is treated as all
            // string so the trailing-comment guard fails closed (#10706).
            let string_line_ranges = workspace_read
                .sources
                .get(&changed.path)
                .and_then(|source| no_behavior::multi_line_string_line_ranges(source));
            // The paired old line gets the same test against the
            // reconstructed old source, so a delimiter edit that turns old
            // string content into code is not read as a spacing edit.
            let old_string_line_ranges = old_source
                .as_deref()
                .and_then(no_behavior::multi_line_string_line_ranges);
            let import_ranges = import_ranges_by_file
                .get(&changed.path)
                .map(Vec::as_slice)
                .unwrap_or_default();
            // An import line that replaces an import line re-points a name
            // (`from a import x` -> `from b import x`, or `old as x,` inside a
            // parenthesized import), so it is behavior of its own; any other
            // added import line is not.
            // Every removed line of a replacement block shares the new-side
            // line where the added run starts, so matching on that coordinate
            // alone finds an old line only for the run's first added line
            // (#7556, #7524 for TypeScript). Every old-line reader below takes
            // it from one pairing: by position in an even block, by content in
            // an uneven one, by coordinate where a removed line interrupts the
            // run.
            let counterparts = changed.aligned_counterparts();
            let is_added_import = |added: &crate::analysis::diff::ChangedLine| {
                line_is_in_ranges(added.line, import_ranges)
                    && counterparts.get(&added.line).is_none_or(|removed| {
                        !line_is_in_ranges(removed.line, &old_import_ranges)
                            && !is_python_import_line(&removed.text)
                    })
            };
            let covered = python_quiet_lines_covered_by_run(&changed.added_lines, |added| {
                line_is_in_ranges(added.line, new_docstring_ranges)
                    || is_added_import(added)
                    || is_python_no_behavior_line(&added.text)
                    || is_python_structural_line(&added.text)
                    // A decorator that opens a newly added function is part of
                    // its definition; the function's own lines carry the change.
                    // A decorator that replaces an old line is an edit and keeps
                    // its probe (#9756).
                    || (!counterparts.contains_key(&added.line)
                        && opens_added_definition(&changed.added_lines, added))
            });
            // Header span per owner, computed once per changed file.
            let mut header_spans: BTreeMap<usize, Option<(usize, usize)>> = BTreeMap::new();
            for (added_index, added) in changed.added_lines.iter().enumerate() {
                // Pair the replaced old line so the classifier can credit the
                // changed-sink token on the DELTA only.
                let old_line = counterparts.get(&added.line).copied();
                // Git pairs a rewritten block line by line, so a comment, a
                // lone bracket or an import that lands where old code stood is
                // not the carrier of that code's change: the behavioral lines of
                // the same added run are (#4216 for Rust). An added import is
                // not a behavior probe of its own either (the Rust adapter
                // ignores `use` lines).
                if covered[added_index] || is_added_import(added) {
                    continue;
                }
                let old_line_text = old_line.map(|removed| removed.text.as_str());
                // Only a structural or default-carrying line can matter inside a
                // multi-line header, so other lines skip the span lookup.
                let in_multi_line_def_header = (is_structural_def_header_text(&added.text)
                    || header_param_line_defaults(&added.text).is_some())
                    && workspace_read
                        .sources
                        .get(&changed.path)
                        .zip(owner_for_changed_line(
                            &changed.path,
                            added.line,
                            &all_owners,
                        ))
                        .and_then(|(source, owner)| {
                            *header_spans.entry(owner.start_line).or_insert_with(|| {
                                multi_line_def_header_span(source, owner.start_line)
                            })
                        })
                        .is_some_and(|(def_line, header_end)| {
                            (def_line..=header_end).contains(&added.line)
                        });
                let no_behavior = PythonNoBehaviorContext {
                    new_line_in_docstring: line_is_in_ranges(added.line, new_docstring_ranges),
                    new_line_in_multiline_string: string_line_ranges
                        .as_deref()
                        .is_none_or(|ranges| line_is_in_ranges(added.line, ranges))
                        || old_line.is_some_and(|removed| {
                            old_string_line_ranges
                                .as_deref()
                                .is_none_or(|ranges| line_is_in_ranges(removed.line, ranges))
                        }),
                    old_line_in_docstring: old_line.is_some_and(|removed| {
                        line_is_in_ranges(removed.line, &old_docstring_ranges)
                    }),
                    opens_owner_with_added_body: owner_for_changed_line(
                        &changed.path,
                        added.line,
                        &all_owners,
                    )
                    .is_some_and(|owner| {
                        owner.start_line == added.line
                            // An old line naming the owner means its `def`
                            // existed before, even when git pairs the old
                            // header with an unrelated inserted line.
                            && !changed
                                .removed_lines
                                .iter()
                                .any(|removed| mentions_python_name(&removed.text, &owner.name))
                            && changed.added_lines.iter().any(|other| {
                                other.line > owner.start_line
                                    && other.line <= owner.end_line
                                    && !line_is_in_ranges(other.line, new_docstring_ranges)
                                    && !is_python_no_behavior_line(&other.text)
                            })
                    }),
                    structural_def_header_line: in_multi_line_def_header
                        && is_structural_def_header_text(&added.text),
                    multi_line_def_header_line: in_multi_line_def_header,
                    // Removed lines carry the new-side line where the removal
                    // sits, so a replacement in place has equal coordinates.
                    sole_line_replacement: matches!(
                        (changed.added_lines.as_slice(), changed.removed_lines.as_slice()),
                        ([added_line], [removed_line]) if added_line.line == removed_line.line
                    ),
                };
                if let Some(mut finding) = classify_change_with_value_origins(
                    &changed.path,
                    added.line,
                    &added.text,
                    old_line_text,
                    &all_owners,
                    &all_tests,
                    no_behavior,
                    workspace_read
                        .sources
                        .get(&changed.path)
                        .map(String::as_str),
                    &mut value_origins,
                    Some(&mut related_memo),
                ) {
                    let original = finding.probe.clone();
                    if let Some(normalized) = identity_context
                        .as_ref()
                        .and_then(|context| context.normalize_line(added.line, &added.text))
                    {
                        reidentify_python_finding(&mut finding, &normalized);
                        value_origins.rebind_probe(&original, &finding.probe);
                    }
                    publish_production_finding(&mut findings, finding);
                }
            }
            classify::number_colliding_candidates(
                &mut findings[file_start..],
                &mut value_origins,
                identity_context.as_ref(),
            );
            // A deleted behavior line no added line pairs with is kept as
            // base-side evidence on its owner, as the Rust adapter keeps it
            // (#7587, RIPR-SPEC-0156).
            let deleted = deleted_lines::deleted_line_findings_with_value_origins(
                changed,
                &counterparts,
                &deleted_lines::OldSideRanges {
                    docstrings: &old_docstring_ranges,
                    imports: &old_import_ranges,
                    source: old_source.as_deref(),
                },
                workspace_read
                    .sources
                    .get(&changed.path)
                    .map(String::as_str),
                &all_owners,
                &all_tests,
                &findings,
                &mut value_origins,
                &mut related_memo,
            );
            findings.extend(deleted);
        }
        // Freeing the run's facts takes ~0.3 s on one thread for a large
        // workspace (#10914: pandas, ~23k tests). Nothing in the result
        // borrows them, so they are freed off the result path (`teardown`).
        drop(related_memo);
        drop(foreign_index);
        drop(import_roots);
        let facts = (
            all_tests,
            all_owners,
            workspace_read,
            foreign_prior_by_file,
            fixture_defs_by_file,
            fixture_side_by_file,
            helper_modules_by_file,
            docstring_ranges_by_file,
            import_ranges_by_file,
            star_facts_by_file,
        );
        teardown::free_in_background(facts);
        Ok(LanguageDiffResult {
            value_origins,
            findings,
            harness_projections: Vec::new(),
            changed_files: changed_count,
            candidate_line_count: 0,
            changed_files_by_language: Vec::new(),
            partial_scope: None,
            skipped_files,
            limitations,
            rust_diagnostic_origins: Default::default(),
            rust_consumed_sources: Default::default(),
            preview_fact_cache: fact_cache_stats,
        })
    }

    /// The deterministic repo-mode core of
    /// [`LanguageAdapter::analyze_repo`] with the working-set limit
    /// injected (#2109, #3554 PR C).
    ///
    /// Calls the producer-owned input authority
    /// (`select_repo_input_with_limit`) and the native evidence producer
    /// (`build_repo_evidence`), then projects onto the shared
    /// `LanguageRepoResult`: findings keep their native Python diff-mode
    /// shape, counts come from the input authority, harness projections
    /// stay empty (no registrations in repo mode), and a capped/partial run
    /// discloses itself through `partial_reason` so the pipeline records a
    /// partial `LanguageRun` that gates fail closed on.
    pub(in crate::analysis::language::python) fn analyze_repo_with_limit(
        root: &Path,
        working_set_limit: repo::RepoWorkingSetLimit,
    ) -> Result<LanguageRepoResult, String> {
        let input = repo::select_repo_input_with_limit(root, working_set_limit);
        let evidence = repo::build_repo_evidence(&input, root);
        let partial_reason = repo::partial_disclosure(&evidence);
        let production_files = input.production_files.len();
        let skipped_files = evidence.counts.skipped;
        Ok(LanguageRepoResult {
            value_origins: evidence.value_origins,
            findings: evidence.findings,
            // No test-harness registry registrations exist in repo mode
            // (#3532): the projection is empty.
            harness_projections: Vec::new(),
            production_files,
            skipped_files,
            partial_reason,
            file_accounting: repo::file_accounting(&input),
            rust_diagnostic_origins: Default::default(),
            rust_consumed_sources: Default::default(),
        })
    }
}

#[cfg(test)]
mod property_assertion_tests;

#[cfg(test)]
mod unreachable_assertion_tests;

#[cfg(test)]
mod arm_activation_tests;

#[cfg(test)]
mod guard_atoms_tests;

#[cfg(test)]
mod conditional_expression_tests;

#[cfg(test)]
mod assertion_selection_tests;

#[cfg(test)]
mod bool_oracle_tests;
#[cfg(test)]
mod derived_expectation_tests;
#[cfg(test)]
mod known_equality_helpers_tests;

#[cfg(test)]
mod collection_config_tests;

#[cfg(test)]
mod conftest_admission_tests;

#[cfg(test)]
mod source_facts_tests;

#[cfg(test)]
mod deleted_lines_tests;

#[cfg(test)]
mod fixtures_tests;

#[cfg(test)]
mod fixture_substitution_tests;

#[cfg(test)]
mod import_writer_tests;
#[cfg(test)]
mod inert_tool_fixture_tests;
#[cfg(test)]
mod literal_fixture_tests;

#[cfg(test)]
mod param_fixture_tests;

#[cfg(test)]
mod dunder_entry_tests;
#[cfg(test)]
mod dunder_truthiness_tests;

#[cfg(test)]
mod module_field_tests;

#[cfg(test)]
mod helper_assertions_tests;

#[cfg(test)]
mod capability_disposition_tests;

#[cfg(test)]
mod helper_inline_tests;
#[cfg(test)]
mod literal_patch_tests;

#[cfg(test)]
mod cli_runner_tests;
#[cfg(test)]
mod cli_subcommand_tests;
#[cfg(test)]
mod helper_owner_call_tests;
#[cfg(test)]
mod imported_literal_tests;
#[cfg(test)]
mod test_body_writer_tests;
#[cfg(test)]
mod test_local_literal_tests;
#[cfg(test)]
mod test_module_constant_tests;
#[cfg(test)]
mod trailing_comment_tests;

#[cfg(test)]
mod call_chain_tests;
#[cfg(test)]
mod forwarder_tests;
#[cfg(test)]
mod imported_call_operator_tests;
#[cfg(test)]
mod module_value_tests;
#[cfg(test)]
mod new_declaration_tests;

#[cfg(test)]
mod python_tests;

#[cfg(test)]
mod reexport_tests;

#[cfg(test)]
mod replaced_block_tests;

#[cfg(test)]
mod route_identity_tests;

#[cfg(test)]
mod row_miss_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod loop_walk_tests;
#[cfg(test)]
mod verdict_corpus_tests;

#[cfg(test)]
mod doctest_gate_tests;
