//! Rust-adapter-local oracle and assertion limitation extraction.
//!
//! Shared assertion classification lives in `analysis::extract::oracles`.
//! This module owns only the adapter-local post-classification limitations:
//! unresolved assertion-like macros, wrapper-error binding, and FFI/cross-
//! language oracle visibility. It reuses shared lexical masks rather than
//! duplicating their scanners.

use super::probes::mask_rust_comments_and_strings;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{FunctionSummary, RustIndex};
use crate::domain::{ExposureClass, Finding, Probe, StaticLimitKind, StopReason};

/// Returns `true` when the owner function carries an FFI or language-binding
/// attribute that indicates its surface may be exercised by an external-language
/// test oracle rather than a Rust test.
///
/// The markers checked are the attribute path segments used by the major
/// Rust FFI and binding crates. `extern "C"` is intentionally excluded: it is
/// an ABI qualifier on the `fn` keyword and is not captured in
/// `FunctionFact.attrs`.
pub(super) fn owner_has_ffi_attr(owner_fn: &FunctionSummary) -> bool {
    owner_fn
        .attrs
        .iter()
        .chain(&owner_fn.impl_attrs)
        .any(|attr| attr_is_ffi_binding(attr))
}

/// PyO3's own attributes (`#[pyfunction]`, `#[pymethods]`, `#[pyclass]`,
/// `#[pymodule]`) do not contain the crate name `pyo3` unless written
/// path-qualified, so each is listed. A method is exposed through the
/// attribute on its `impl` block (`#[pymethods] impl Ledger`,
/// `#[wasm_bindgen] impl Counter`, `#[napi] impl Store`).
const FFI_ATTR_MARKERS: &[&str] = &[
    "no_mangle",
    "export_name",
    "wasm_bindgen",
    "napi",
    "pyo3",
    "pyfunction",
    "pymethods",
    "pyclass",
    "pymodule",
    "uniffi",
    "cxx",
];

/// Match the attribute's parsed path, not its text: `#[doc = "pyfunction
/// helper"]` or `#[my_pyfunction_like]` must not credit a binding. Only the
/// wrappers that carry another attribute as an argument are opened:
/// `#[unsafe(no_mangle)]` (Rust 2024) and `#[cfg_attr(pred, attr, ...)]`,
/// whose predicate is skipped.
pub(super) fn attr_is_ffi_binding(attr: &str) -> bool {
    // Mask before reading paths as well as argument boundaries: comment text
    // can contain a marker or separate real path and wrapper tokens.
    let masked = mask_comments_and_strings(attr);
    let body = masked.trim();
    let body = if let Some(rest) = body.strip_prefix('#') {
        let rest = rest.trim_start();
        let rest = rest.strip_prefix('!').unwrap_or(rest).trim_start();
        rest.strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(body)
    } else {
        body
    };
    attr_body_is_ffi_binding(body)
}

fn attr_body_is_ffi_binding(body: &str) -> bool {
    let body = body.trim();
    let path_end = body.find(['(', '=', '[', '{']).unwrap_or(body.len());
    let path = body[..path_end].trim();
    if path.split("::").any(|segment| {
        let segment = segment.trim();
        let name = segment.strip_prefix("r#").unwrap_or(segment);
        FFI_ATTR_MARKERS.contains(&name)
    }) {
        return true;
    }
    let args = body[path_end..]
        .trim()
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'));
    let Some(args) = args else {
        return false;
    };
    match path {
        // `unsafe` is special attribute syntax. `r#unsafe` instead names an
        // ordinary attribute and cannot stand in for this keyword wrapper.
        "unsafe" => attr_body_is_ffi_binding(args),
        "cfg_attr" | "r#cfg_attr" => split_top_level_args(args)
            .into_iter()
            .skip(1)
            .any(attr_body_is_ffi_binding),
        _ => false,
    }
}

/// Split already-masked attribute arguments on commas outside nested
/// delimiters. Comments and literals were removed at the attribute entry point.
fn split_top_level_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, ch) in args.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&args[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&args[start..]);
    parts
}

/// Resolve the probe's owner function from the index and check for FFI attrs.
/// Returns `Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)` when
/// the probe owner is FFI/binding-exposed and the finding class is an
/// unrevealed gap; `None` otherwise. Pure-Rust owners (no FFI attrs) return
/// `None` unconditionally.
pub(super) fn cross_language_limit_kind(
    probe: &crate::domain::Probe,
    index: &RustIndex,
    class: &ExposureClass,
) -> Option<StaticLimitKind> {
    // `NoStaticPath` is included: an FFI owner tested only from the other
    // language has no Rust test path by construction, and RIPR-SPEC-0062
    // forbids flattening that to a bare `no_static_path`.
    let is_gap_class = matches!(
        class,
        ExposureClass::WeaklyExposed
            | ExposureClass::ReachableUnrevealed
            | ExposureClass::InfectionUnknown
            | ExposureClass::NoStaticPath
    );
    if !is_gap_class {
        return None;
    }
    let owner_id = probe.owner.as_ref()?;
    let owner_fn = index
        .functions()
        .iter()
        .find(|function| &function.id == owner_id)?;
    if owner_has_ffi_attr(owner_fn) {
        Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
    } else {
        None
    }
}

/// Attach the cross-language limitation to an FFI-exposed owner's gap finding.
/// A `no_static_path` finding's generic next step tells the reader to add a
/// co-located Rust test, which is the wrong repair when the tests live in
/// the other language, so it takes the limitation's own guidance instead.
/// A `no_static_path` finding that already names a limitation keeps it: that
/// limitation (a transitive or macro reach witness, for example) points at a
/// Rust test the finding's evidence lines already describe; so does any
/// finding that carries a reach witness.
pub(super) fn apply_cross_language_limit(finding: &mut Finding, probe: &Probe, index: &RustIndex) {
    let Some(limit) = cross_language_limit_kind(probe, index, &finding.class) else {
        return;
    };
    if finding.class == ExposureClass::NoStaticPath {
        if finding.static_limit_kind.is_some() {
            return;
        }
        finding.recommended_next_step = Some(limit.describe().to_string());
    }
    // A proximity-only finding's reach witness (#7071) names a Rust test the
    // evidence lines and next step already describe; keep that limit whole.
    if finding.stop_reasons.iter().any(|reason| {
        matches!(
            reason,
            StopReason::TransitiveReachUnresolved | StopReason::MacroReachUnresolved
        )
    }) {
        return;
    }
    finding.static_limit_kind = Some(limit);
}

/// Borrowed, pass-local lookup. Each source identifier is indexed once, never
/// by copying a function/body. Empty inputs make every later lookup constant
/// time; no-path findings do not rescan all files or macro token trees.
pub(super) struct PropertyMacroMentionIndex<'a> {
    captured_root: Option<&'a std::path::Path>,
    selected_root: &'a std::path::Path,
    by_identifier: std::collections::BTreeMap<&'a str, Vec<PropertyMacroLocation<'a>>>,
    package_authority: Option<
        &'a std::collections::BTreeMap<
            std::path::PathBuf,
            crate::analysis::facts::WorkspaceFileAuthority,
        >,
    >,
}

struct PropertyMacroLocation<'a> {
    file: &'a std::path::Path,
    witness: &'a crate::analysis::facts::UnresolvedPropertyMacroFact,
}

impl<'a> PropertyMacroMentionIndex<'a> {
    pub(super) fn new(index: &'a RustIndex, selected_root: &'a std::path::Path) -> Self {
        let mut by_identifier = std::collections::BTreeMap::new();
        for (file, facts) in index.files().iter() {
            for witness in &facts.data().unresolved_property_macros {
                for identifier in &witness.mentioned_identifiers {
                    by_identifier
                        .entry(identifier.as_str())
                        .or_insert_with(Vec::new)
                        .push(PropertyMacroLocation {
                            file: file.as_path(),
                            witness,
                        });
                }
            }
        }
        Self {
            by_identifier,
            selected_root,
            captured_root: index
                .workspace_authority
                .as_ref()
                .map(|authority| authority.root.as_path()),
            package_authority: index
                .workspace_authority
                .as_ref()
                .map(|authority| &authority.files),
        }
    }

    fn known_package(&self, indexed_file: &std::path::Path) -> Option<&str> {
        self.package_authority?
            .get(indexed_file)
            .filter(|authority| authority.valid)
            .map(|authority| authority.package_identity.as_str())
    }

    fn owner_package(&self, probe_file: &std::path::Path) -> Option<&str> {
        // Probe paths are constructed from the selected analysis root, unlike
        // the root-relative witness keys. Strip that exact prefix first so a
        // decoy indexed path beginning with the same root text cannot win.
        let relative = probe_file
            .strip_prefix(self.selected_root)
            .ok()
            .or_else(|| probe_file.strip_prefix(self.captured_root?).ok())?;
        self.known_package(relative)
    }
}

/// A suffix/name match is a possible lexical mention, never resolved reach.
/// A property block may generate tests, or may discard all its tokens.
pub(super) fn apply_unresolved_property_macro_limit(
    finding: &mut Finding,
    owner_name: &str,
    owner_file: &std::path::Path,
    mentions: &PropertyMacroMentionIndex<'_>,
) -> bool {
    let Some(locations) = mentions.by_identifier.get(owner_name) else {
        return false;
    };
    // This is retained manifest/source authority, not a path-layout guess.
    // No per-finding filesystem reads or manifest resolution are performed.
    let owner_package = mentions.owner_package(owner_file);
    let Some(PropertyMacroLocation { file, witness }) = locations.iter().find(|location| {
        match (owner_package, mentions.known_package(location.file)) {
            (Some(owner), Some(mentioned)) => owner == mentioned,
            // An unresolved package identity cannot prove unrelatedness.
            _ => true,
        }
    }) else {
        return false;
    };
    // A lexical mention supplies no runtime edge. Keep all affected stages
    // explicitly unresolved instead of inheriting discarded argument values.
    for stage in [
        &mut finding.ripr.reach,
        &mut finding.ripr.infect,
        &mut finding.ripr.propagate,
    ] {
        let old_summary = stage.summary.clone();
        stage.state = crate::domain::StageState::Unknown;
        stage.confidence = crate::domain::Confidence::Low;
        stage.summary = "Property macro expansion and owner execution are unresolved".to_string();
        finding.evidence.retain(|evidence| evidence != &old_summary);
    }
    finding.static_limit_kind = Some(StaticLimitKind::RustMacroReachUnresolved);
    finding
        .stop_reasons
        .push(crate::domain::StopReason::MacroReachUnresolved);
    finding.evidence.push(format!(
        "Opaque property macro `{}!` at {}:{} lexically mentions `{owner_name}`; macro provenance, test collection and execution are unresolved.",
        witness.name, file.display().to_string().replace('\\', "/"), witness.line,
    ));
    finding.evidence.extend([
        format!("limitation_last_established_edge: source invocation `{}!` at {}:{}", witness.name, file.display().to_string().replace('\\', "/"), witness.line),
        "limitation_first_unresolved_edge: property macro expansion, executable-test collection and owner reach".to_string(),
        "limitation_analyzer_route: analysis/rust-property-macro-provenance".to_string(),
        "limitation_non_claim: lexical mention only; no test existence, absence, execution, reach or assertion discrimination is established".to_string(),
    ]);
    finding.recommended_next_step = Some(
        "Inspect the property macro's definition and collected tests, then run its existing test suite; this limitation does not establish a missing test.".to_string(),
    );
    true
}

pub(super) fn apply_rust_macro_wrapped_assertion_limit(
    finding: &mut Finding,
    index: &RustIndex,
    owner_name: Option<&str>,
) {
    if finding.static_limit_kind.is_some() {
        return;
    }
    let witness = if finding.class == ExposureClass::ReachableUnrevealed
        && finding.oracle_related_tests().next().is_some()
        && finding.ripr.reveal.observe.state == crate::domain::StageState::No
        && finding
            .oracle_related_tests()
            .all(|related| related.oracle.is_none())
    {
        find_unresolved_assertion_macro_witness(finding, index, None)
    } else if let Some(owner_name) = owner_name
        && only_proximity_tests_supply_an_oracle(finding)
    {
        // #7089: a weak oracle from a test that only shares the owner's file
        // made the finding `weakly_exposed`, while every test that calls the
        // owner has no recognized oracle. When one of those tests calls the
        // owner inside an assertion-like macro ripr cannot read, that macro
        // is the likely discriminator, so name the limit instead of a gap.
        find_unresolved_assertion_macro_witness(finding, index, Some(owner_name))
    } else {
        return;
    };
    let Some(witness) = witness else {
        return;
    };

    finding.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
    if matches!(
        witness.macro_name.rsplit("::").next(),
        Some("prop_assert" | "prop_assert_eq" | "prop_assert_ne")
    ) {
        finding.recommended_next_step = Some(
            "Inspect the property assertion macro's definition and run the existing test; its spelling does not establish assertion semantics.".to_string(),
        );
    }
    finding.evidence.push(
        "A related Rust test uses an assertion-like macro that ripr does not classify as an oracle."
            .to_string(),
    );
    finding
        .evidence
        .push(rust_macro_assertion_witness_pointer(&witness));
    finding
        .evidence
        .extend(rust_macro_assertion_limitation_detail_lines(&witness));
}

/// #3700 (final consolidation): a wrapper error seam — a `map_err`
/// conversion whose changed expression carries no parseable error variant —
/// carries the typed `wrapper_error_binding_unresolved` limitation. Whether
/// the boxed conversion faithfully carries the converted callee's error
/// variant (`Into`/`From` through `Box<dyn Error>`) is not statically
/// establishable, so the seam stays below `exposed` and the limitation names
/// what ripr could not resolve instead of prescribing an assertion the suite
/// may already contain. Classification (already `weakly_exposed` via the
/// unconfirmed-observation rule in reveal) is unchanged.
pub(super) fn apply_wrapper_error_binding_limit(finding: &mut Finding, probe: &Probe) {
    if finding.class != ExposureClass::WeaklyExposed || finding.static_limit_kind.is_some() {
        return;
    }
    if !matches!(
        probe.family,
        crate::domain::ProbeFamily::ErrorPath | crate::domain::ProbeFamily::ReturnValue
    ) {
        return;
    }
    if !crate::analysis::classify::wrapper_error_seam_expression(&[probe.expression.as_str()]) {
        return;
    }

    finding.static_limit_kind = Some(StaticLimitKind::WrapperErrorBindingUnresolved);
    finding.evidence.push(
        "limitation_last_established_edge: changed wrapper conversion maps the converted callee's error into the boxed error channel".to_string(),
    );
    finding.evidence.push(
        "limitation_first_unresolved_edge: whether `Into`/`From` through `Box<dyn Error>` carries the callee's error variant to the wrapper's callers".to_string(),
    );
    finding
        .evidence
        .push("limitation_analyzer_route: analysis/wrapper-error-binding".to_string());
    finding.evidence.push(
        "limitation_non_claim: named analyzer limitation only; ripr does not confirm coverage or prescribe a repair test"
            .to_string(),
    );
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RustMacroAssertionWitness {
    test_name: String,
    test_file: std::path::PathBuf,
    test_line: usize,
    macro_name: String,
    macro_line: usize,
}

/// A relation that rests on the test calling or reaching the owner, not on
/// file proximity or a name token (`owner_named_test` is name-only too).
fn is_direct_relation(reason: Option<crate::domain::RelationReason>) -> bool {
    reason.is_some_and(|reason| {
        !crate::analysis::classify::is_proximity_only(reason)
            && reason != crate::domain::RelationReason::OwnerNamedTest
    })
}

/// A `weakly_exposed` finding whose recognized oracles all come from tests
/// related only by proximity or name, while at least one directly related
/// test exists and none has an oracle. A truncated related-test window may
/// hide a direct test's oracle, so it never qualifies.
fn only_proximity_tests_supply_an_oracle(finding: &Finding) -> bool {
    if finding.class != ExposureClass::WeaklyExposed
        || finding
            .related_tests_matched_total
            .is_some_and(|total| total > finding.related_tests.len())
    {
        return false;
    }
    let mut direct_seen = false;
    let mut proximity_oracle_seen = false;
    for related in finding.oracle_related_tests() {
        let direct = is_direct_relation(related.relation_reason);
        if related.relation_reason.is_none() || direct {
            // A refused assertion is still an assertion ripr judged.
            if related.oracle.is_some()
                || related.miss == Some(crate::domain::RelatedTestMiss::AssertionNotCredited)
            {
                return false;
            }
            direct_seen |= direct;
        } else if related.oracle.is_some() {
            proximity_oracle_seen = true;
        }
    }
    direct_seen && proximity_oracle_seen
}

/// With `owner_in_arguments`, only a test related by a direct relation counts,
/// and the macro's own arguments must call that owner.
fn find_unresolved_assertion_macro_witness(
    finding: &Finding,
    index: &RustIndex,
    owner_in_arguments: Option<&str>,
) -> Option<RustMacroAssertionWitness> {
    let mut candidates = Vec::new();
    for test in index
        .tests()
        .iter()
        .chain(index.files().values().flat_map(|file| file.tests.iter()))
    {
        if !finding.oracle_related_tests().any(|related| {
            related.name == test.name
                && related.file == test.file
                && (owner_in_arguments.is_none() || is_direct_relation(related.relation_reason))
        }) {
            continue;
        }
        for (macro_name, macro_line) in
            unresolved_assertion_macro_invocations(&test.body, test.start_line, owner_in_arguments)
        {
            candidates.push(RustMacroAssertionWitness {
                test_name: test.name.clone(),
                test_file: test.file.clone(),
                test_line: test.start_line,
                macro_name,
                macro_line,
            });
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates.into_iter().next()
}

fn unresolved_assertion_macro_invocations(
    body: &str,
    start_line: usize,
    owner_in_arguments: Option<&str>,
) -> Vec<(String, usize)> {
    let mut invocations = Vec::new();
    let masked_body = mask_rust_comments_and_strings(body);
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            masked_body
                .match_indices('\n')
                .map(|(offset, _)| offset + 1),
        )
        .collect();
    for (offset, line) in masked_body.lines().enumerate() {
        let mut search_start = 0usize;
        while let Some(relative_bang) = line[search_start..].find('!') {
            let bang = search_start + relative_bang;
            search_start = bang.saturating_add(1);
            if line[bang + 1..].starts_with('=') {
                continue;
            }
            if !line[bang + 1..]
                .trim_start()
                .chars()
                .next()
                .is_some_and(|ch| matches!(ch, '(' | '[' | '{'))
            {
                continue;
            }
            let Some(macro_name) = macro_name_before_bang(line, bang) else {
                continue;
            };
            if !is_unresolved_assertion_like_macro(&macro_name) {
                continue;
            }
            if let Some(owner_name) = owner_in_arguments {
                let Some(&line_start) = line_starts.get(offset) else {
                    continue;
                };
                if !macro_arguments_call(&masked_body, line_start + bang + 1, owner_name) {
                    continue;
                }
            }
            invocations.push((macro_name, start_line + offset));
        }
    }
    invocations.sort();
    invocations.dedup();
    invocations
}

/// Whether the delimited arguments that open after `after_bang` contain a bare
/// call `owner_name(`, read from masked text. A path- or method-qualified call
/// (`Bar::new(`, `x.len(`) may name a different item with the same last
/// segment, so it does not count; neither does an argument list that runs
/// past a `;` or holds a `'`: char literals are not masked, so `'('` can
/// unbalance the delimiters. The `'` refusal deliberately covers lifetimes
/// and labels too (`bulk::<'static>(1)`); the delimiter walk cannot tell
/// them apart, so narrowing it to char literals would need a real lexer. Arguments holding a nested macro invocation
/// (`stringify!(owner(1))`) do not count either, since that macro may never
/// evaluate the call.
fn macro_arguments_call(masked: &str, after_bang: usize, owner_name: &str) -> bool {
    let Some(rest) = masked.get(after_bang..) else {
        return false;
    };
    let Some(open) = rest.find(|ch: char| !ch.is_whitespace()) else {
        return false;
    };
    let bytes = rest.as_bytes();
    if !matches!(bytes.get(open), Some(b'(' | b'[' | b'{')) {
        return false;
    }
    let mut depth = 0usize;
    let mut close = None;
    for (index, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b';' if depth == 1 && bytes[open] != b'{' => return false,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(arguments) = close.and_then(|close| rest.get(open + 1..close)) else {
        return false;
    };
    if arguments.contains('\'') || contains_macro_invocation(arguments) {
        return false;
    }
    arguments.match_indices(owner_name).any(|(start, _)| {
        let qualified_or_ident = arguments[..start]
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ':' | '.'));
        let after = &arguments[start + owner_name.len()..];
        !qualified_or_ident && after.trim_start().starts_with('(')
    })
}

fn contains_macro_invocation(text: &str) -> bool {
    text.match_indices('!').any(|(bang, _)| {
        let before = text[..bang].trim_end();
        let after = text[bang + 1..].trim_start();
        before
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            && after.starts_with(['(', '[', '{'])
    })
}

fn macro_name_before_bang(line: &str, bang: usize) -> Option<String> {
    let prefix = line[..bang].trim_end();
    let end = prefix.len();
    if end == 0 {
        return None;
    }
    let mut start = end;
    for (idx, ch) in prefix.char_indices().rev() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' {
            start = idx;
        } else {
            break;
        }
    }
    let name = prefix[start..end].trim_matches(':');
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn is_unresolved_assertion_like_macro(macro_name: &str) -> bool {
    if is_known_rust_assertion_macro(macro_name) {
        return false;
    }
    let base = macro_name.rsplit("::").next().unwrap_or(macro_name);
    base == "assert"
        || base.starts_with("assert_")
        || matches!(base, "prop_assert" | "prop_assert_eq" | "prop_assert_ne")
}

fn is_known_rust_assertion_macro(macro_name: &str) -> bool {
    let compact = macro_name.replace(' ', "");
    let base = compact.rsplit("::").next().unwrap_or(compact.as_str());
    matches!(
        base,
        "assert" | "assert_eq" | "assert_ne" | "assert_matches" | "matches"
    ) || compact.starts_with("insta::assert")
        || compact.contains("snapshot")
}

fn rust_source_pointer(path: &std::path::Path, line: usize) -> String {
    format!("{}:{}", path.display().to_string().replace('\\', "/"), line)
}

fn rust_macro_assertion_witness_pointer(witness: &RustMacroAssertionWitness) -> String {
    let test_location = rust_source_pointer(&witness.test_file, witness.test_line);
    let macro_location = rust_source_pointer(&witness.test_file, witness.macro_line);
    format!(
        "{}`{}` ({}) reaches the changed owner, then invokes assertion-like macro `{}!` at {}. ripr does not classify that macro as an oracle.",
        crate::domain::TRANSITIVE_REACH_WITNESS_PREFIX,
        witness.test_name,
        test_location,
        witness.macro_name,
        macro_location
    )
}

fn rust_macro_assertion_limitation_detail_lines(
    witness: &RustMacroAssertionWitness,
) -> [String; 4] {
    let test_location = rust_source_pointer(&witness.test_file, witness.test_line);
    let macro_location = rust_source_pointer(&witness.test_file, witness.macro_line);
    [
        format!(
            "{}test `{}` ({}) -> assertion macro `{}!` at {}",
            crate::domain::LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX,
            witness.test_name,
            test_location,
            witness.macro_name,
            macro_location
        ),
        format!(
            "{}assertion macro `{}!` semantics toward the changed owner",
            crate::domain::LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX,
            witness.macro_name
        ),
        format!(
            "{}analysis/rust-macro-assertion-oracle",
            crate::domain::LIMITATION_ANALYZER_ROUTE_PREFIX
        ),
        format!(
            "{}named limitation only; ripr cannot confirm or deny that the macro assertion discriminates the change",
            crate::domain::LIMITATION_NON_CLAIM_PREFIX
        ),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn property_scope_uses_only_valid_retained_package_authority() {
        use crate::analysis::facts::{UnresolvedPropertyMacroFact, WorkspaceFileAuthority};
        use std::collections::BTreeMap;
        use std::path::{Path, PathBuf};
        let owner = Path::new("crates/owner/src/lib.rs");
        let witness_file = Path::new("custom/nested/src/property.rs");
        let absolute_owner = Path::new("/captured").join(owner);
        let selected_owner = Path::new("selected/root").join(owner);
        let witness = UnresolvedPropertyMacroFact {
            name: "proptest".into(),
            line: 1,
            mentioned_identifiers: vec!["gate".into()],
        };
        for (package, valid, limited) in [
            ("owner-manifest", true, true),
            ("other-manifest", true, false),
            ("other-manifest", false, true),
        ] {
            let authorities = BTreeMap::from([
                (
                    owner.to_path_buf(),
                    WorkspaceFileAuthority {
                        source_digest: String::new(),
                        package_identity: "owner-manifest".into(),
                        valid: true,
                    },
                ),
                (
                    selected_owner.clone(),
                    WorkspaceFileAuthority {
                        source_digest: String::new(),
                        package_identity: "decoy".into(),
                        valid: true,
                    },
                ),
                (
                    witness_file.to_path_buf(),
                    WorkspaceFileAuthority {
                        source_digest: String::new(),
                        package_identity: package.into(),
                        valid,
                    },
                ),
            ]);
            let mentions = super::PropertyMacroMentionIndex {
                captured_root: Some(Path::new("/captured")),
                selected_root: Path::new("selected/root"),
                by_identifier: BTreeMap::from([(
                    "gate",
                    vec![super::PropertyMacroLocation {
                        file: witness_file,
                        witness: &witness,
                    }],
                )]),
                package_authority: Some(&authorities),
            };
            for owner_path in [&absolute_owner, &selected_owner] {
                let mut finding = no_static_path_finding();
                assert_eq!(
                    super::apply_unresolved_property_macro_limit(
                        &mut finding,
                        "gate",
                        owner_path,
                        &mentions
                    ),
                    limited
                );
            }
        }
        let absent = BTreeMap::<PathBuf, WorkspaceFileAuthority>::new();
        for package_authority in [None, Some(&absent)] {
            let mentions = super::PropertyMacroMentionIndex {
                captured_root: Some(Path::new("/captured")),
                selected_root: Path::new("selected/root"),
                by_identifier: BTreeMap::from([(
                    "gate",
                    vec![super::PropertyMacroLocation {
                        file: witness_file,
                        witness: &witness,
                    }],
                )]),
                package_authority,
            };
            assert!(super::apply_unresolved_property_macro_limit(
                &mut no_static_path_finding(),
                "gate",
                &absolute_owner,
                &mentions
            ));
        }
    }

    use super::{
        apply_cross_language_limit, apply_rust_macro_wrapped_assertion_limit,
        apply_wrapper_error_binding_limit, attr_is_ffi_binding, cross_language_limit_kind,
        is_known_rust_assertion_macro, is_unresolved_assertion_like_macro, owner_has_ffi_attr,
        unresolved_assertion_macro_invocations,
    };
    use crate::analysis::facts::{
        CallFact, FunctionSourceRole, FunctionSummary, LiteralFact, RustIndex, TestSummary,
    };
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, ExposureClass, Finding, OracleKind,
        OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest, RevealEvidence, RiprEvidence,
        SourceLocation, StageEvidence, StageState, StaticLimitKind, SymbolId,
    };
    use std::path::PathBuf;

    fn ffi_function(file: &str, name: &str, attrs: Vec<&str>) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("{file}::{name}")),
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 5,
            body: format!("pub fn {name}(x: i32) -> i32 {{ x }}").into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: attrs.into_iter().map(|s| s.to_string()).collect(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        }
    }

    fn probe_for_owner(file: &str, name: &str, family: ProbeFamily) -> Probe {
        Probe {
            id: ProbeId(format!("probe:{file}::{name}")),
            location: SourceLocation::new(file, 2, 1),
            owner: Some(SymbolId(format!("{file}::{name}"))),
            family,
            delta: DeltaKind::Control,
            before: None,
            after: Some("x > 0".to_string()),
            expression: "x > 0".to_string(),
            expected_sinks: vec![],
            required_oracles: vec![],
        }
    }

    fn stage(state: StageState) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, "stage")
    }

    fn reachable_unrevealed_finding_with_related_test(
        test_name: &str,
        test_file: &str,
        test_line: usize,
    ) -> Finding {
        Finding {
            id: "probe:src_lib.rs:predicate:test".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:src_lib.rs:predicate:test".to_string()),
                location: SourceLocation::new("src/lib.rs", 2, 1),
                owner: Some(SymbolId("src/lib.rs::inner".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: Some("if a >= b {".to_string()),
                expression: "if a >= b {".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::ReachableUnrevealed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes),
                infect: stage(StageState::Yes),
                propagate: stage(StageState::Yes),
                reveal: RevealEvidence {
                    observe: stage(StageState::No),
                    discriminate: stage(StageState::No),
                },
            },
            confidence: 0.48,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: vec![RelatedTest {
                name: test_name.to_string(),
                file: PathBuf::from(test_file),
                line: test_line,
                oracle: None,
                oracle_kind: OracleKind::Unknown,
                oracle_strength: OracleStrength::None,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
            }],
            recommended_next_step: None,
            language: None,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    fn test_summary(name: &str, file: &str, start_line: usize, body: &str) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line,
            end_line: start_line + body.lines().count(),
            body: body.into(),
            calls: vec![CallFact {
                line: start_line,
                name: "inner".to_string(),
                text: "inner(10, 3)".to_string().into(),
            }],
            assertions: Vec::new(),
            literals: vec![
                LiteralFact {
                    line: start_line,
                    value: "10".to_string(),
                },
                LiteralFact {
                    line: start_line + 1,
                    value: "7".to_string(),
                },
            ],
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    #[test]
    fn known_assertion_macros_are_not_unresolved_even_when_path_qualified() {
        for name in [
            "assert",
            "assert_eq",
            "assert_ne",
            "assert_matches",
            "matches",
            "pretty_assertions::assert_eq",
            "insta::assert_snapshot",
            "insta::assert_debug_snapshot",
            "crate::snapshot_assert",
        ] {
            assert!(
                is_known_rust_assertion_macro(name),
                "{name} must remain a known oracle macro"
            );
            assert!(
                !is_unresolved_assertion_like_macro(name),
                "{name} must not be named as an unresolved assertion-like macro"
            );
        }
        // Documented over-credit: any compact name containing "snapshot" is
        // treated as known, so a custom `assert_*snapshot*` macro is not
        // named as unresolved. Pin that rather than silently tightening the
        // matcher in this extraction slice.
        assert!(is_known_rust_assertion_macro("assert_custom_snapshot"));
        assert!(!is_unresolved_assertion_like_macro(
            "assert_custom_snapshot"
        ));
    }

    #[test]
    fn custom_assert_star_macros_are_unresolved_and_std_assert_is_not() {
        assert!(is_unresolved_assertion_like_macro("assert_result"));
        assert!(is_unresolved_assertion_like_macro("assert_ok"));
        assert!(is_unresolved_assertion_like_macro("crate::assert_ok"));
        assert!(!is_unresolved_assertion_like_macro("assert_eq"));
        assert!(!is_unresolved_assertion_like_macro("ensure"));
        assert!(!is_unresolved_assertion_like_macro("println"));
    }

    #[test]
    fn unresolved_assertion_scan_skips_not_equal_and_non_invocation_bangs() {
        let invocations = unresolved_assertion_macro_invocations(
            "let ok = flag != false;\nlet _ = assert_ok!(value);\nmacro_rules! assert_ok { () => {}; }\n",
            4,
            None,
        );
        assert_eq!(invocations, vec![("assert_ok".to_string(), 5)]);
    }

    #[test]
    fn owner_scoped_assertion_scan_requires_an_owner_call_in_the_arguments() {
        let body = "{\n    assert_flag!(bulk(100), true);\n    assert_flag!(bulky(1), true);\n    assert_delta!(\n        0.5,\n        score(\"bulk(\")\n    );\n    assert_flag!(bulk, true); // bulk(1)\n    assert_close!(\n        0.0,\n        other.bulk (2)\n    );\n}\n";
        // Only the first invocation calls the owner bare; a string, a
        // comment, a longer identifier, a bare path and a method call on
        // another value do not.
        assert_eq!(
            unresolved_assertion_macro_invocations(body, 10, Some("bulk")),
            vec![("assert_flag".to_string(), 11)]
        );
        // Without an owner every assertion-like invocation is listed.
        assert_eq!(
            unresolved_assertion_macro_invocations(body, 10, None).len(),
            5
        );
        // A same-named item on another path, and arguments that an unmasked
        // char literal lets run past the statement, do not count.
        let tricky = "{\n    assert_custom!(Bar::bulk(2).is_ok());\n    assert_custom!(c == '('); let _ = bulk(1);\n    assert_twice!(c == '(' || c == '(');\n    let _ = bulk(1);\n    let close = (')', ')');\n    assert_quoted!(stringify!(bulk(1)), \"bulk(1)\");\n    assert_multi!(\n        0.5,\n        bulk (3)\n    );\n}\n";
        assert_eq!(
            unresolved_assertion_macro_invocations(tricky, 1, Some("bulk")),
            vec![("assert_multi".to_string(), 8)]
        );
        // Empty and whitespace-only argument spans name no call.
        assert!(!super::macro_arguments_call("assert_empty!();", 13, "bulk"));
        assert!(!super::macro_arguments_call(
            "assert_space!(   );",
            13,
            "bulk"
        ));
        assert!(!super::macro_arguments_call("assert_open!(", 12, "bulk"));
        assert!(super::macro_arguments_call(
            "assert_flag!(bulk(1));",
            12,
            "bulk"
        ));
        // Non-ASCII identifier characters qualify a name or a nested macro.
        assert!(!super::macro_arguments_call(
            "assert_flag!(quoté!(bulk(1)));",
            12,
            "bulk"
        ));
        assert!(!super::macro_arguments_call(
            "assert_flag!(ébulk(1));",
            12,
            "bulk"
        ));
    }

    #[test]
    fn macro_wrapped_assertion_limit_names_reachable_unobserved_assertion_macro() {
        let mut finding = reachable_unrevealed_finding_with_related_test(
            "test_inner_with_custom_assertion_macro",
            "tests/it.rs",
            4,
        );
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            tests: vec![test_summary(
                "test_inner_with_custom_assertion_macro",
                "tests/it.rs",
                4,
                "let result = inner(10, 3);\nassert_result!(result, 7);",
            )],
            ..Default::default()
        });

        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, None);

        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved)
        );
        assert!(finding.evidence.iter().any(|line| {
            line.contains("assertion-like macro `assert_result!` at tests/it.rs:5")
        }));
        assert!(finding.evidence.iter().any(|line| {
            line == "limitation_last_established_edge: test `test_inner_with_custom_assertion_macro` (tests/it.rs:4) -> assertion macro `assert_result!` at tests/it.rs:5"
        }));
        assert!(finding.evidence.iter().any(|line| {
            line == "limitation_first_unresolved_edge: assertion macro `assert_result!` semantics toward the changed owner"
        }));
        assert!(finding.evidence.iter().any(|line| {
            line == "limitation_analyzer_route: analysis/rust-macro-assertion-oracle"
        }));
        assert!(finding.evidence.iter().any(|line| {
            line == "limitation_non_claim: named limitation only; ripr cannot confirm or deny that the macro assertion discriminates the change"
        }));
    }

    /// #7089: a `weakly_exposed` finding whose direct test calls `inner`
    /// inside `assert_result!` and whose only oracle sits in a test that
    /// shares the file.
    fn weakly_exposed_beside_a_proximity_oracle() -> (Finding, RustIndex) {
        use crate::domain::RelationReason;
        let mut finding =
            reachable_unrevealed_finding_with_related_test("direct_inner", "src/lib.rs", 20);
        finding.class = ExposureClass::WeaklyExposed;
        finding.related_tests[0].relation_reason = Some(RelationReason::DirectOwnerCall);
        let mut proximity = finding.related_tests[0].clone();
        proximity.name = "other_is_positive".to_string();
        proximity.line = 30;
        proximity.relation_reason = Some(RelationReason::SameTestFile);
        proximity.oracle = Some("assert!(other(1) > 0);".to_string());
        proximity.oracle_strength = OracleStrength::Weak;
        finding.related_tests.push(proximity);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            tests: vec![test_summary(
                "direct_inner",
                "src/lib.rs",
                20,
                "assert_result!(inner(10, 3), 7);",
            )],
            ..Default::default()
        });
        (finding, index)
    }

    #[test]
    fn weakly_exposed_limit_needs_a_complete_direct_window_and_a_proximity_oracle() {
        use crate::domain::RelationReason;
        let (mut finding, index) = weakly_exposed_beside_a_proximity_oracle();
        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, Some("inner"));
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved)
        );

        // Without the owner name the weakly_exposed path does not run.
        let (mut finding, index) = weakly_exposed_beside_a_proximity_oracle();
        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, None);
        assert_eq!(finding.static_limit_kind, None);

        // A truncated window may hide a direct test's own oracle.
        let (mut finding, index) = weakly_exposed_beside_a_proximity_oracle();
        finding.related_tests_matched_total = Some(finding.related_tests.len() + 1);
        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, Some("inner"));
        assert_eq!(finding.static_limit_kind, None);

        // No proximity oracle: the weakness is not explained by proximity.
        let (mut finding, index) = weakly_exposed_beside_a_proximity_oracle();
        finding.related_tests[1].oracle = None;
        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, Some("inner"));
        assert_eq!(finding.static_limit_kind, None);

        // A test with no relation reason that holds an oracle may be direct.
        let (mut finding, index) = weakly_exposed_beside_a_proximity_oracle();
        let mut unknown = finding.related_tests[1].clone();
        unknown.name = "unknown_relation".to_string();
        unknown.relation_reason = None;
        finding.related_tests.push(unknown);
        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, Some("inner"));
        assert_eq!(finding.static_limit_kind, None);

        // A direct test whose assertion ripr refused still had one judged.
        let (mut finding, index) = weakly_exposed_beside_a_proximity_oracle();
        finding.related_tests[0].miss = Some(crate::domain::RelatedTestMiss::AssertionNotCredited);
        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, Some("inner"));
        assert_eq!(finding.static_limit_kind, None);

        // A name-only relation is not a direct test.
        let (mut finding, index) = weakly_exposed_beside_a_proximity_oracle();
        finding.related_tests[0].relation_reason = Some(RelationReason::OwnerNamedTest);
        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, Some("inner"));
        assert_eq!(finding.static_limit_kind, None);
    }

    #[test]
    fn macro_wrapped_assertion_limit_ignores_known_assertion_macros() {
        let mut finding = reachable_unrevealed_finding_with_related_test(
            "test_inner_with_known_assertion_macro",
            "tests/it.rs",
            4,
        );
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            tests: vec![test_summary(
                "test_inner_with_known_assertion_macro",
                "tests/it.rs",
                4,
                "let result = inner(10, 3);\nassert_eq!(result, 7);",
            )],
            ..Default::default()
        });

        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, None);

        assert_eq!(finding.static_limit_kind, None);
        assert!(finding.evidence.is_empty());
    }

    #[test]
    fn macro_wrapped_assertion_limit_ignores_comments_and_string_literals() {
        let mut finding = reachable_unrevealed_finding_with_related_test(
            "test_inner_with_commented_assertion_macro",
            "tests/it.rs",
            4,
        );
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            tests: vec![test_summary(
                "test_inner_with_commented_assertion_macro",
                "tests/it.rs",
                4,
                r##"let result = inner(10, 3);
// assert_result!(result, 7);
/* assert_block_result!(result, 7); */
let note = "assert_string_result!(result, 7)";
let raw = r#"assert_raw_result!(result, 7)"#;
let _ = (result, note, raw);"##,
            )],
            ..Default::default()
        });

        apply_rust_macro_wrapped_assertion_limit(&mut finding, &index, None);

        assert_eq!(finding.static_limit_kind, None);
        assert!(finding.evidence.is_empty());
    }

    #[test]
    fn wrapper_error_binding_limit_names_map_err_error_and_return_families_only() {
        let mut finding = reachable_unrevealed_finding_with_related_test("wraps", "tests/it.rs", 4);
        finding.class = ExposureClass::WeaklyExposed;
        finding.probe.family = ProbeFamily::ErrorPath;
        finding.probe.expression = "try_parse(raw).map_err(Into::into)".to_string();
        let probe = finding.probe.clone();
        apply_wrapper_error_binding_limit(&mut finding, &probe);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::WrapperErrorBindingUnresolved)
        );

        let mut returned =
            reachable_unrevealed_finding_with_related_test("wraps", "tests/it.rs", 4);
        returned.class = ExposureClass::WeaklyExposed;
        returned.probe.family = ProbeFamily::ReturnValue;
        returned.probe.expression = "try_parse(raw).map_err(Into::into)".to_string();
        let probe = returned.probe.clone();
        apply_wrapper_error_binding_limit(&mut returned, &probe);
        assert_eq!(
            returned.static_limit_kind,
            Some(StaticLimitKind::WrapperErrorBindingUnresolved)
        );

        let mut predicate =
            reachable_unrevealed_finding_with_related_test("wraps", "tests/it.rs", 4);
        predicate.class = ExposureClass::WeaklyExposed;
        predicate.probe.family = ProbeFamily::Predicate;
        predicate.probe.expression = "try_parse(raw).map_err(Into::into)".to_string();
        let probe = predicate.probe.clone();
        apply_wrapper_error_binding_limit(&mut predicate, &probe);
        assert_eq!(predicate.static_limit_kind, None);

        let mut already_named =
            reachable_unrevealed_finding_with_related_test("wraps", "tests/it.rs", 4);
        already_named.class = ExposureClass::WeaklyExposed;
        already_named.probe.family = ProbeFamily::ErrorPath;
        already_named.probe.expression = "try_parse(raw).map_err(Into::into)".to_string();
        already_named.static_limit_kind =
            Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
        let probe = already_named.probe.clone();
        apply_wrapper_error_binding_limit(&mut already_named, &probe);
        assert_eq!(
            already_named.static_limit_kind,
            Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved)
        );
    }

    #[test]
    fn owner_with_no_mangle_attr_is_ffi() {
        let owner = ffi_function("src/lib.rs", "ffi_fn", vec!["#[no_mangle]"]);
        assert!(owner_has_ffi_attr(&owner));
    }

    #[test]
    fn owner_with_wasm_bindgen_attr_is_ffi() {
        let owner = ffi_function("src/lib.rs", "wasm_fn", vec!["#[wasm_bindgen]"]);
        assert!(owner_has_ffi_attr(&owner));
    }

    #[test]
    fn remaining_ffi_markers_are_recognized_and_plain_attrs_are_not() {
        for marker in [
            "#[export_name = \"x\"]",
            "#[napi]",
            "#[pyo3]",
            "#[uniffi]",
            "#[cxx::bridge]",
        ] {
            let owner = ffi_function("src/lib.rs", "exported", vec![marker]);
            assert!(owner_has_ffi_attr(&owner), "{marker} should be FFI");
        }
        let owner = ffi_function("src/lib.rs", "plain_fn", vec!["#[test]"]);
        assert!(!owner_has_ffi_attr(&owner));
        let empty = ffi_function("src/lib.rs", "pure_fn", vec![]);
        assert!(!owner_has_ffi_attr(&empty));
    }

    #[test]
    fn cross_language_guard_fires_for_weakly_exposed_with_ffi_attr() {
        let owner = ffi_function("src/lib.rs", "exported_fn", vec!["#[no_mangle]"]);
        let probe = probe_for_owner("src/lib.rs", "exported_fn", ProbeFamily::Predicate);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![owner],
            ..Default::default()
        });
        let result = cross_language_limit_kind(&probe, &index, &ExposureClass::WeaklyExposed);
        assert_eq!(
            result,
            Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
        );
    }

    #[test]
    fn cross_language_guard_fires_for_reachable_unrevealed_with_wasm_bindgen() {
        let owner = ffi_function("src/lib.rs", "wasm_fn", vec!["#[wasm_bindgen]"]);
        let probe = probe_for_owner("src/lib.rs", "wasm_fn", ProbeFamily::ReturnValue);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![owner],
            ..Default::default()
        });
        let result = cross_language_limit_kind(&probe, &index, &ExposureClass::ReachableUnrevealed);
        assert_eq!(
            result,
            Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
        );
    }

    #[test]
    fn cross_language_guard_fires_for_infection_unknown_with_ffi_attr() {
        let owner = ffi_function("src/lib.rs", "exported_fn", vec!["#[no_mangle]"]);
        let probe = probe_for_owner("src/lib.rs", "exported_fn", ProbeFamily::Predicate);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![owner],
            ..Default::default()
        });
        let result = cross_language_limit_kind(&probe, &index, &ExposureClass::InfectionUnknown);
        assert_eq!(
            result,
            Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
        );
    }

    #[test]
    fn cross_language_guard_does_not_fire_for_pure_rust_owner_weakly_exposed() {
        let owner = ffi_function("src/lib.rs", "pure_fn", vec![]);
        let probe = probe_for_owner("src/lib.rs", "pure_fn", ProbeFamily::Predicate);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![owner],
            ..Default::default()
        });
        let result = cross_language_limit_kind(&probe, &index, &ExposureClass::WeaklyExposed);
        assert_eq!(result, None);
    }

    #[test]
    fn cross_language_guard_skips_exposed_and_names_no_static_path_with_ffi() {
        let owner = ffi_function("src/lib.rs", "exported_fn", vec!["#[no_mangle]"]);
        let probe = probe_for_owner("src/lib.rs", "exported_fn", ProbeFamily::ReturnValue);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![owner],
            ..Default::default()
        });
        assert_eq!(
            cross_language_limit_kind(&probe, &index, &ExposureClass::Exposed),
            None
        );
        // RIPR-SPEC-0062: a binding owner tested only from the other language
        // has no Rust test path by construction, so no_static_path names the
        // cross-language limitation instead of flattening to a bare gap.
        assert_eq!(
            cross_language_limit_kind(&probe, &index, &ExposureClass::NoStaticPath),
            Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
        );
    }

    #[test]
    fn cross_language_guard_does_not_fire_without_an_owner_id() {
        let owner = ffi_function("src/lib.rs", "exported_fn", vec!["#[no_mangle]"]);
        let mut probe = probe_for_owner("src/lib.rs", "exported_fn", ProbeFamily::Predicate);
        probe.owner = None;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![owner],
            ..Default::default()
        });
        assert_eq!(
            cross_language_limit_kind(&probe, &index, &ExposureClass::WeaklyExposed),
            None
        );
    }

    fn no_static_path_finding() -> Finding {
        let mut finding = reachable_unrevealed_finding_with_related_test("t", "tests/it.rs", 1);
        finding.class = ExposureClass::NoStaticPath;
        finding.related_tests.clear();
        finding.probe = probe_for_owner("src/lib.rs", "inner", ProbeFamily::Predicate);
        finding
    }

    #[test]
    fn ffi_attr_matches_the_parsed_path_not_the_attribute_text() {
        for attr in [
            "#[unsafe(no_mangle)]",
            "#[export_name = \"fee\"]",
            "#[wasm_bindgen::prelude::wasm_bindgen(js_name = fee)]",
            "#[pyo3::pyfunction]",
            "#[cfg_attr(feature = \"python\", pyfunction)]",
            "#[cfg_attr(feature = \"python\", pyo3::pymethods)]",
            "#[uniffi::export]",
        ] {
            assert!(attr_is_ffi_binding(attr), "{attr} must mark an FFI owner");
        }
        for attr in [
            "#[doc = \"pyfunction helper\"]",
            "/// exported with no_mangle",
            "#[my_pyfunction_like]",
            "#[cfg_attr(feature = \"pyfunction\", derive(Debug))]",
            "#[cfg(feature = \"napi\")]",
            "#[allow(clippy::pyfunction)]",
            "#[unsafe(link_section = \".napi\")]",
        ] {
            assert!(
                !attr_is_ffi_binding(attr),
                "{attr} must not mark an FFI owner"
            );
        }
    }

    #[test]
    fn pyo3_binding_attrs_are_ffi_on_the_function_or_its_impl() {
        // PyO3's attributes do not contain the string `pyo3`.
        for attr in ["#[pyfunction]", "#[pyclass]", "#[pymodule]"] {
            let owner = ffi_function("src/lib.rs", "fee", vec![attr]);
            assert!(owner_has_ffi_attr(&owner), "{attr} must mark an FFI owner");
        }
        let mut method = ffi_function("src/lib.rs", "charge", vec![]);
        assert!(!owner_has_ffi_attr(&method));
        method.impl_attrs = vec!["#[pymethods]".to_string()];
        assert!(
            owner_has_ffi_attr(&method),
            "a method is exposed through its impl block's binding attribute"
        );
        let mut plain_method = ffi_function("src/lib.rs", "charge", vec![]);
        plain_method.impl_attrs = vec!["#[derive(Debug)]".to_string()];
        assert!(!owner_has_ffi_attr(&plain_method));
    }

    #[test]
    fn cross_language_limit_replaces_the_co_located_test_step_on_no_static_path() {
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![ffi_function("src/lib.rs", "inner", vec!["#[pyfunction]"])],
            ..Default::default()
        });
        let mut finding = no_static_path_finding();
        finding.recommended_next_step = Some("add a co-located test".to_string());
        let probe = finding.probe.clone();
        apply_cross_language_limit(&mut finding, &probe, &index);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
        );
        assert_eq!(
            finding.recommended_next_step.as_deref(),
            Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved.describe())
        );

        // A gap class that already has reach keeps its own next step.
        let mut weak = no_static_path_finding();
        weak.class = ExposureClass::WeaklyExposed;
        weak.recommended_next_step = Some("strengthen the assertion".to_string());
        apply_cross_language_limit(&mut weak, &probe, &index);
        assert_eq!(
            weak.static_limit_kind,
            Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
        );
        assert_eq!(
            weak.recommended_next_step.as_deref(),
            Some("strengthen the assertion")
        );

        // A no_static_path finding that already names a Rust reach
        // limitation keeps it and its next step.
        let mut witnessed = no_static_path_finding();
        witnessed.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
        witnessed.recommended_next_step = Some("open the witnessing test".to_string());
        apply_cross_language_limit(&mut witnessed, &probe, &index);
        assert_eq!(
            witnessed.static_limit_kind,
            Some(StaticLimitKind::RustTransitiveReachUnresolved)
        );
        assert_eq!(
            witnessed.recommended_next_step.as_deref(),
            Some("open the witnessing test")
        );

        // A pure-Rust owner is untouched.
        let pure = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![ffi_function("src/lib.rs", "inner", vec![])],
            ..Default::default()
        });
        let mut plain = no_static_path_finding();
        plain.recommended_next_step = Some("add a co-located test".to_string());
        apply_cross_language_limit(&mut plain, &probe, &pure);
        assert_eq!(plain.static_limit_kind, None);
        assert_eq!(
            plain.recommended_next_step.as_deref(),
            Some("add a co-located test")
        );
    }
}
