use super::super::rust_index::{
    FunctionSummary, RustIndex, TestSummary, extract_identifier_tokens,
};
use crate::domain::{Probe, RelationReason};
use std::path::Path;

/// Minimum token length for the `assertions_reference_owner` signal.
/// Tokens shorter than this threshold are too common to safely assert ownership.
const ASSERTION_TOKEN_MIN_LEN: usize = 5;

pub(in crate::analysis) fn find_related_tests<'a>(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    index: &'a RustIndex,
    workspace_complete: bool,
    helper_chain: Option<&super::helper_transfer::HelperChain>,
) -> Vec<(&'a TestSummary, RelationReason)> {
    let mut related: Vec<(&TestSummary, RelationReason)> = Vec::new();
    let owner_name = owner_fn.map(|f| f.name.as_str()).unwrap_or("");
    let probe_tokens = extract_identifier_tokens(&probe.expression);
    let file_name = normalized_file_stem(&probe.location.file);
    let owner_package_prefix = owner_fn.and_then(|owner| package_prefix(&owner.file));

    // #2971 ambiguity rule: a call to `owner_name` can bypass the
    // package-prefix guard only when (a) the owner name is unique in the
    // workspace AND (b) the index spans the whole workspace. When multiple
    // crates define functions with the same name, a bare call in crate_b
    // calling `score()` cannot be attributed to crate_a::score — that is the
    // token-coincidence false-exposed family.
    //
    // (b) is not a property of the analysis mode. The diff path indexes only
    // the changed files whenever `include_unchanged_tests` is false — Deep and
    // Ready included — and only the changed packages under Draft/Fast, so a
    // same-named function in an unindexed file is absent from the count and
    // the name would falsely appear unique. The caller therefore derives
    // `workspace_complete` from the file selection that actually built the
    // index, and anything narrower fails closed. This follows the
    // owner_shape.rs precedent of scanning index.functions for same-name
    // collision.
    let owner_name_is_unique = workspace_complete
        && !owner_name.is_empty()
        && index
            .functions
            .iter()
            .filter(|f| f.name == owner_name)
            .count()
            == 1;

    // For module-level struct/field probes (owner_fn is None) derive the package
    // prefix from the probe's source file so cross-crate spurious matches are
    // still filtered out.
    let struct_package_prefix = if owner_fn.is_none() {
        package_prefix(&probe.location.file)
    } else {
        None
    };

    // #3235: an absolute owner/probe path without a `crates/` segment carries
    // no derivable package scope, and the old behavior treated that as "no
    // scoping needed", silently disabling the package guard. When the
    // candidate test's path is *also* absolute, a weak match cannot be scoped
    // at all, so fail closed: only the strong uniquely-owned call path (#2971)
    // may admit. A *relative* test file keeps current behavior — the
    // mixed-form companion flow (absolute probe, relative test files) is
    // production-pinned. A future producer that supplies absolute test paths
    // must carry an explicit relativization authority instead of relying on
    // ambient prefix stripping.
    let owner_scope_unscopable = owner_fn
        .is_some_and(|owner| owner_package_prefix.is_none() && path_is_absolute_form(&owner.file));
    let struct_scope_unscopable = owner_fn.is_none()
        && struct_package_prefix.is_none()
        && path_is_absolute_form(&probe.location.file);

    // Probe tokens that are long enough to assert ownership via assertion text.
    let long_probe_tokens: Vec<&str> = probe_tokens
        .iter()
        .filter(|t| t.len() >= ASSERTION_TOKEN_MIN_LEN)
        .map(String::as_str)
        .collect();

    for test in &index.tests {
        // Compute calls_owner BEFORE the package-prefix guard so a cross-crate
        // test that genuinely calls a uniquely-named owner is not filtered out
        // before the strong signal can save it.
        let calls_owner = !owner_name.is_empty()
            && (test.calls.iter().any(|call| call.name == owner_name)
                || body_contains_owner_call(&test.body, owner_name));
        // #3296 review: the test-to-entry edge must also be a direct
        // free-function call site — a method or qualified call sharing
        // the entry's terminal name is not callee identity.
        let calls_helper_entry = helper_chain.is_some_and(|chain| {
            test.calls.iter().any(|call| {
                chain.hops.iter().any(|hop| {
                    hop.caller.name == call.name
                        && super::helper_transfer::is_direct_call_site(&call.text, &hop.caller.name)
                })
            })
        });

        // #2971: Only apply the package-prefix guard to weak signals — tests
        // that do not directly call the owner, OR tests that call a bare name
        // that is ambiguous across crates. A cross-crate test that calls a
        // uniquely-named owner is genuinely related and must not be suppressed.
        if let Some(prefix) = &owner_package_prefix
            && !normalize_path(&test.file).starts_with(prefix)
            && !(calls_owner && owner_name_is_unique)
            && !calls_helper_entry
        {
            continue;
        }
        // Apply the same package-prefix guard for the owner_fn=None path.
        if let Some(prefix) = &struct_package_prefix
            && !normalize_path(&test.file).starts_with(prefix)
        {
            continue;
        }
        // #3235: unscopable-absolute owner against an absolute test file —
        // no package comparison is possible, so weak matches fail closed.
        if owner_scope_unscopable
            && path_is_absolute_form(&test.file)
            && !(calls_owner && owner_name_is_unique)
        {
            continue;
        }
        if struct_scope_unscopable && path_is_absolute_form(&test.file) {
            continue;
        }

        // Signal: a test's assertion observed_tokens intersect the probe's
        // long-enough identifier tokens. Only fires when the probe has no named
        // function owner (i.e. it is a module-level struct/field), so it cannot
        // override the `calls_owner` path for function-bodied probes.
        let assertions_reference_owner = owner_fn.is_none()
            && !long_probe_tokens.is_empty()
            && test.assertions.iter().any(|oracle| {
                oracle.observed_tokens.iter().any(|obs_tok| {
                    obs_tok.len() >= ASSERTION_TOKEN_MIN_LEN
                        && long_probe_tokens.contains(&obs_tok.as_str())
                })
            });

        let test_name = test.name.to_ascii_lowercase();
        let owner_name_lc = owner_name.to_ascii_lowercase();
        let same_test_file = same_test_file(&probe.location.file, &test.file);
        // Keep the broad path-token association available to callers, but do
        // not publish it as file identity.  A short source stem such as
        // `config` can occur in unrelated test paths (for example
        // `reconfigure.rs`).
        let file_path_token_matches =
            !file_name.is_empty() && normalize_path(&test.file).contains(&file_name);
        let owner_name_in_test = !owner_name_lc.is_empty() && test_name.contains(&owner_name_lc);
        let token_in_test_name = probe_tokens
            .iter()
            .any(|token| token.len() > 2 && test_name.contains(&token.to_ascii_lowercase()));
        let same_file_or_named =
            same_test_file || file_path_token_matches || owner_name_in_test || token_in_test_name;

        // #3296 helper transfer: a test that calls a resolved hop's
        // caller directly reaches the owner through the bounded chain
        // (same authority the activation rows consume). The chain is
        // resolved once per probe above the test loop (#3296 review
        // M1 — the per-test re-resolution was O(tests x functions)).
        let helper_chain_reaches = !calls_owner
            && !assertions_reference_owner
            && !same_file_or_named
            && calls_helper_entry;

        if !calls_owner
            && !assertions_reference_owner
            && !same_file_or_named
            && !helper_chain_reaches
        {
            continue;
        }

        // Determine the single highest-priority reason for the match so the
        // emitted `RelatedTest` can carry `relation_reason` /
        // `relation_confidence` tags for consumer filtering.
        let reason = if calls_owner {
            // The test directly calls or mentions the changed owner function.
            RelationReason::DirectOwnerCall
        } else if helper_chain_reaches {
            // The test calls a function that reaches the owner through
            // the bounded helper-transfer chain (#3296).
            RelationReason::HelperOwnerCall
        } else if assertions_reference_owner {
            // Struct/field probe whose tokens appear in assertion observed_tokens.
            RelationReason::AssertionTargetAffinity
        } else if owner_name_in_test {
            // Test name contains the owner function name.
            RelationReason::OwnerNamedTest
        } else if same_test_file {
            // Test file uses the probe's source stem or one of the canonical
            // `_test`/`_tests` companion conventions.
            RelationReason::SameTestFile
        } else {
            // A path or test-name token substring is the broadest, least
            // precise match branch. `same_file_or_named` guarantees that one
            // of those weak signals is present here.
            RelationReason::WeakTokenSubstring
        };

        related.push((test, reason));
    }

    related.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name).then_with(|| a.file.cmp(&b.file)));
    related.dedup_by(|(a, _), (b, _)| a.name == b.name && a.file == b.file);
    related
}

/// Return whether `test_file` is the canonical companion test file for
/// `probe_file`.  This intentionally mirrors the stronger test-grip
/// authority: only the source stem itself and its `_test`/`_tests` variants
/// count as file identity.  Paths are compared by stem, so host-specific
/// separators do not affect the result.
fn same_test_file(probe_file: &Path, test_file: &Path) -> bool {
    // Merge resolution (#3545 x PR-3547): keep the PR's `cross_host_stem`
    // authority — it normalizes both separator flavors (main's #3545 intent)
    // AND fails closed on non-UTF-8 paths, where lossy conversion could
    // collapse distinct names into one stem (#3545 review).  Keep main's
    // empty-stem guard so degenerate stems never count as file identity.
    let Some(probe_stem) = cross_host_stem(probe_file) else {
        return false;
    };
    if probe_stem.is_empty() {
        return false;
    }
    let Some(test_stem) = cross_host_stem(test_file) else {
        return false;
    };
    if test_stem.is_empty() {
        return false;
    }
    test_stem == probe_stem
        || test_stem == format!("{probe_stem}_test")
        || test_stem == format!("{probe_stem}_tests")
}

/// Extract the file stem on any host: split on both separator flavors
/// (a probe path recorded with `\` must resolve on Unix and vice versa),
/// then strip the extension the way [`std::path::Path::file_stem`] does.
/// Non-UTF-8 paths fail closed — lossy conversion could collapse distinct
/// names into one stem and fake a canonical-companion relation (#3545
/// review).
fn cross_host_stem(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let unified = text.replace('\\', "/");
    let name = unified.rsplit('/').next()?;
    let stem = match name.rfind('.') {
        Some(0) | None => name,
        Some(extension) => &name[..extension],
    };
    Some(stem.to_string())
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

/// Extract a source-file stem after normalizing separators from either host.
/// Analysis inputs can contain paths produced on a different platform than the
/// host running the association pass. Non-UTF-8 paths fail closed: lossy
/// replacement characters can collapse distinct leaf names onto one stem and
/// fabricate file-name affinity (#3599; same guard as the grip-side copy
/// added in #3598).
fn normalized_file_stem(path: &Path) -> String {
    let Some(text) = path.to_str() else {
        return String::new();
    };
    let unified = text.replace('\\', "/").trim_start_matches("./").to_string();
    let file = unified.rsplit('/').next().unwrap_or_default();
    Path::new(file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string()
}

/// #3235: one authority for recognizing an absolute path form on any host —
/// Unix-rooted, Windows drive-prefixed, or host-absolute. Shared by
/// `package_prefix` (which cannot derive a package scope from such a path
/// without a `crates/` segment) and the package guards in
/// `find_related_tests`, which fail closed when both sides of a candidate
/// relation are unscopable absolute paths.
fn path_is_absolute_form(path: &Path) -> bool {
    let normalized = normalize_path(path);
    let has_drive_prefix = normalized.as_bytes().get(1).copied() == Some(b':');
    path.is_absolute() || normalized.starts_with('/') || has_drive_prefix
}

fn package_prefix(path: &Path) -> Option<String> {
    let normalized = normalize_path(path);
    let crate_relative = normalized
        .strip_prefix("crates/")
        .or_else(|| normalized.rsplit_once("/crates/").map(|(_, rest)| rest));
    if let Some(crate_relative) = crate_relative
        && let Some((crate_name, package_relative)) = crate_relative.split_once('/')
        && (package_relative.starts_with("src/") || package_relative.starts_with("tests/"))
    {
        return Some(format!("crates/{crate_name}/"));
    }
    if path_is_absolute_form(path) {
        return None;
    }
    for marker in ["/src/", "/tests/"] {
        if let Some(idx) = normalized.rfind(marker) {
            let prefix = &normalized[..idx];
            if prefix.is_empty() {
                return None;
            }
            return Some(format!("{prefix}/"));
        }
    }
    None
}

/// True when `body` mentions `owner_name` immediately followed by `(`.
///
/// A fallback for tests whose `calls` facts did not capture the call, e.g. a
/// path-qualified `crate_under_test::internal::inner(10, 3)`.
///
/// The boundary check rejects a longer identifier that merely ends in
/// `owner_name` (`compute_score(`), but deliberately does **not** reject a
/// preceding `.`. A receiver is not evidence of a different function: the
/// owner is frequently an impl method, and `ledger.apply(5)` is the only way a
/// test can call `Ledger::apply` at all. #3047 tried rejecting `.` to suppress
/// cross-crate `other.compute_hash(...)` matches and downgraded every
/// method-owner fixture from `direct_owner_call` to `owner_named_test`.
///
/// That suppression is also unnecessary. `CallFact` carries no receiver or
/// path information (`analysis/extract/calls.rs` keeps only the bare trailing
/// identifier), so receiver identity cannot be recovered here — but the
/// cross-crate bypass is already gated on the owner name being unique across
/// `index.functions`, and `syntax/ra.rs` indexes impl methods alongside free
/// functions. A same-named method on another type is therefore itself in the
/// index, the name is not unique, and the bypass never fires. The uniqueness
/// gate subsumes the receiver concern.
pub(in crate::analysis) fn body_contains_owner_call(body: &str, owner_name: &str) -> bool {
    if owner_name.is_empty() {
        return false;
    }
    body.match_indices(owner_name).any(|(start, _)| {
        let end = start.saturating_add(owner_name.len());
        let before_ok = start == 0
            || !body
                .as_bytes()
                .get(start - 1)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        let after_call = body
            .get(end..)
            .map(|tail| tail.trim_start().starts_with('('))
            .unwrap_or(false);
        before_ok && after_call
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::rust_index::{CallFact, OracleFact, extract_identifier_tokens};
    use crate::domain::{
        DeltaKind, OracleKind, OracleStrength, ProbeFamily, ProbeId, SourceLocation, SymbolId,
    };
    use std::path::PathBuf;

    #[test]
    fn given_owner_function_when_tests_share_name_across_packages_then_filters_to_package() {
        let owner = function("crates/crate_a/src/lib.rs", "score");
        let index = RustIndex {
            functions: vec![
                function("crates/crate_a/src/lib.rs", "score"),
                function("crates/crate_b/src/lib.rs", "score"),
            ],
            tests: vec![
                test(
                    "crates/crate_b/tests/score.rs",
                    "crate_b_score_test",
                    "score(2)",
                ),
                test(
                    "crates/crate_a/tests/score.rs",
                    "crate_a_score_test",
                    "score(1)",
                ),
            ],
            ..RustIndex::default()
        };
        let probe = probe("crates/crate_a/src/lib.rs", "score + 1");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].0.name, "crate_a_score_test");
    }

    /// #2971 / #3033 positive control: a cross-crate test that calls a
    /// **uniquely-named** owner is retained via `DirectOwnerCall`. The owner
    /// name `compute_hash` appears once in the workspace, so the bare call in
    /// a separate crate's integration test is unambiguously calling the owner.
    #[test]
    fn given_unique_owner_when_cross_crate_test_calls_owner_then_retained() {
        let owner = function("crates/digest/src/lib.rs", "compute_hash");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![test_with_call(
                "crates/digest-tests/tests/integration.rs",
                "hash_integration_test",
                "let result = compute_hash(b\"input\");",
                "compute_hash",
            )],
            ..RustIndex::default()
        };
        let probe = probe("crates/digest/src/lib.rs", "compute_hash(input)");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(
            related.len(),
            1,
            "cross-crate test calling unique owner should be retained"
        );
        assert_eq!(related[0].0.name, "hash_integration_test");
        assert_eq!(related[0].1, RelationReason::DirectOwnerCall);
    }

    /// An impl-method owner is reachable only through a receiver, so a `.`
    /// before the name must not disqualify the call. The `propagate_*` goldens
    /// are exactly this shape — owner `Ledger::apply`, test body
    /// `ledger.apply(5);` — and a `.`-rejecting matcher silently downgraded
    /// them from `direct_owner_call` to `owner_named_test`, weakening the
    /// relation on every method owner in the corpus.
    #[test]
    fn given_method_owner_when_test_calls_through_receiver_then_direct_owner_call() {
        let owner = function("src/lib.rs", "apply");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![TestSummary {
                name: "changes_balance".to_string(),
                file: PathBuf::from("tests/ledger_tests.rs"),
                start_line: 1,
                end_line: 4,
                body: "let mut ledger = Ledger::new(100);\nledger.apply(5);".to_string(),
                // Body-only: the receiver form is what must be credited.
                calls: Vec::new(),
                assertions: Vec::new(),
                literals: Vec::new(),
                attrs: Vec::new(),
            }],
            ..RustIndex::default()
        };
        let probe = probe("src/lib.rs", "self.persist(amount * 9)");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 1, "method owner must keep its calling test");
        assert_eq!(related[0].1, RelationReason::DirectOwnerCall);
    }

    /// #2971 scope control: the same workspace as the positive control above,
    /// reached through a partial index. The diff path indexes only the changed
    /// files whenever `include_unchanged_tests` is false, and only the changed
    /// packages under Draft/Fast, so a name seen once in that index is not
    /// evidence that it is unique in the workspace. The bypass must not fire:
    /// a sibling crate's unindexed same-named function would otherwise be
    /// credited as the owner — the token-coincidence false-`exposed` family.
    #[test]
    fn given_incomplete_index_when_cross_crate_test_calls_owner_then_filtered() {
        let owner = function("crates/digest/src/lib.rs", "compute_hash");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![test_with_call(
                "crates/digest-tests/tests/integration.rs",
                "hash_integration_test",
                "let result = compute_hash(b\"input\");",
                "compute_hash",
            )],
            ..RustIndex::default()
        };
        let probe = probe("crates/digest/src/lib.rs", "compute_hash(input)");

        // The only difference from the positive control.
        let related = find_related_tests(&probe, Some(&owner), &index, false, None);

        assert!(
            related.is_empty(),
            "a partial index cannot establish name uniqueness, so the \
             package-prefix guard must stay unconditional"
        );
    }

    /// #2971 / #3033 negative control: a cross-crate test calling an
    /// **ambiguously-named** function is filtered by the package-prefix guard.
    /// Both crate_a and crate_b define `score`, so a bare `score()` call in
    /// crate_b cannot be attributed to crate_a::score.
    #[test]
    fn given_ambiguous_owner_when_cross_crate_test_calls_same_name_then_filtered() {
        let owner = function("crates/crate_a/src/lib.rs", "score");
        let crate_b_fn = function("crates/crate_b/src/lib.rs", "score");
        let index = RustIndex {
            functions: vec![owner.clone(), crate_b_fn],
            tests: vec![
                test_with_call(
                    "crates/crate_b/tests/score.rs",
                    "crate_b_calls_score",
                    "score(42)",
                    "score",
                ),
                test(
                    "crates/crate_a/tests/score.rs",
                    "crate_a_score_test",
                    "score(1)",
                ),
            ],
            ..RustIndex::default()
        };
        let probe = probe("crates/crate_a/src/lib.rs", "score + 1");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        // The crate_a test is in-package and passes; the crate_b test is
        // cross-crate and filtered despite calling `score()` because the name
        // is ambiguous across crates.
        assert_eq!(
            related.len(),
            1,
            "ambiguous-name cross-crate test must be filtered"
        );
        assert_eq!(related[0].0.name, "crate_a_score_test");
    }

    #[test]
    fn given_same_named_tests_when_finding_related_then_orders_by_file_path() {
        let owner = function("src/lib.rs", "score");
        let index = RustIndex {
            tests: vec![
                test("tests/z_case.rs", "score_shared", "score(3)"),
                test("tests/a_case.rs", "score_shared", "score(1)"),
            ],
            ..RustIndex::default()
        };
        let probe = probe("src/lib.rs", "score + 1");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 2);
        assert_eq!(related[0].0.file, PathBuf::from("tests/a_case.rs"));
        assert_eq!(related[1].0.file, PathBuf::from("tests/z_case.rs"));
    }

    #[test]
    fn path_substring_match_is_weak_not_same_test_file() {
        let owner = function("crates/core/src/config.rs", "load_config");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![test(
                "crates/core/tests/reconfigure.rs",
                "checks_value",
                "assert_eq!(value, 3);",
            )],
            ..RustIndex::default()
        };
        let probe = probe("crates/core/src/config.rs", "marker");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].1, RelationReason::WeakTokenSubstring);
    }

    #[test]
    fn canonical_companion_stems_remain_same_test_file() {
        let owner = function("crates/core/src/config.rs", "load_config");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![
                test(
                    "crates/core/tests/config.rs",
                    "config_file",
                    "assert!(true);",
                ),
                test(
                    "crates/core/tests/config_test.rs",
                    "config_test_file",
                    "assert!(true);",
                ),
                test(
                    "crates/core/tests/config_tests.rs",
                    "config_tests_file",
                    "assert!(true);",
                ),
            ],
            ..RustIndex::default()
        };
        let probe = probe("crates/core/src/config.rs", "marker");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 3);
        assert!(
            related
                .iter()
                .all(|(_, reason)| *reason == RelationReason::SameTestFile)
        );
    }

    #[test]
    fn foreign_separator_probe_still_resolves_the_canonical_companion() {
        // The probe path was recorded with Windows separators; the stem
        // extraction must split on both flavors so `config_tests` stays a
        // canonical companion (SameTestFile, medium confidence) instead of
        // degrading to WeakTokenSubstring on a Unix host.
        let owner = function("crates/core/src/config.rs", "load_config");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![test(
                "crates/core/tests/config_tests.rs",
                "config_tests_file",
                "assert!(true);",
            )],
            ..RustIndex::default()
        };
        let probe = probe("crates\\core\\src\\config.rs", "marker");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].1, RelationReason::SameTestFile);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_stems_fail_closed_instead_of_lossily_collapsing() {
        use std::os::unix::ffi::OsStringExt;
        // Two DISTINCT names whose lossy renderings would both collapse to
        // the same `foo<FFFD>` stem: the companion check must not fire.
        let probe_file = std::path::PathBuf::from(std::ffi::OsString::from_vec(
            b"crates/core/src/foo\x80.rs".to_vec(),
        ));
        let test_file = std::path::PathBuf::from(std::ffi::OsString::from_vec(
            b"crates/core/tests/foo\x81_tests.rs".to_vec(),
        ));
        assert!(!super::same_test_file(&probe_file, &test_file));
    }

    #[test]
    fn given_probe_token_in_test_name_when_owner_is_not_called_then_test_is_related() {
        let owner = function("src/lib.rs", "tax_total");
        let index = RustIndex {
            tests: vec![test(
                "tests/tax.rs",
                "vat_boundary_is_checked_by_macro",
                "assert_eq!(macro_tax_case!(100), 120);",
            )],
            ..RustIndex::default()
        };
        let probe = probe("src/lib.rs", "vat >= threshold");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].0.name, "vat_boundary_is_checked_by_macro");
    }

    #[test]
    fn given_macro_name_contains_owner_when_no_owner_call_then_test_is_not_directly_related() {
        let owner = function("src/internal.rs", "inner");
        let macro_test = TestSummary {
            name: "macro_wrapper_boundary".to_string(),
            file: PathBuf::from("tests/macro_boundary.rs"),
            start_line: 1,
            end_line: 5,
            body: "let result = call_inner!(10, 3); assert_eq!(result, 7);".to_string(),
            calls: vec![CallFact {
                line: 1,
                name: "call_inner".to_string(),
                text: "call_inner!(10, 3)".to_string(),
            }],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
        };
        let index = RustIndex {
            tests: vec![macro_test],
            ..RustIndex::default()
        };
        let probe = probe("src/internal.rs", "if a >= b");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert!(
            related.is_empty(),
            "macro wrapper name containing the owner must not become a direct owner call"
        );
    }

    #[test]
    fn given_body_contains_qualified_owner_call_then_fallback_is_directly_related() {
        let owner = function("src/internal.rs", "inner");
        let call_test = TestSummary {
            name: "public_api_calls_inner".to_string(),
            file: PathBuf::from("tests/public_api.rs"),
            start_line: 1,
            end_line: 5,
            body: "let result = crate_under_test::internal::inner(10, 3);".to_string(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
        };
        let index = RustIndex {
            tests: vec![call_test],
            ..RustIndex::default()
        };
        let probe = probe("src/internal.rs", "if a >= b");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].1, RelationReason::DirectOwnerCall);
    }

    #[test]
    fn given_workspace_paths_when_extracting_package_prefix_then_handles_nested_markers() {
        assert_eq!(
            package_prefix(Path::new("crates/foo/src/support/src/lib.rs")).as_deref(),
            Some("crates/foo/")
        );
        assert_eq!(
            package_prefix(Path::new("crates/foo/tests/support/tests/cases.rs")).as_deref(),
            Some("crates/foo/")
        );
        assert_eq!(
            package_prefix(Path::new("vendor/foo/src/support/src/lib.rs")).as_deref(),
            Some("vendor/foo/src/support/")
        );
        assert_eq!(
            package_prefix(Path::new("crates/ripr/examples/sample/src/lib.rs")).as_deref(),
            Some("crates/ripr/examples/sample/")
        );
    }

    #[test]
    fn given_non_workspace_paths_when_extracting_package_prefix_then_returns_none() {
        assert_eq!(package_prefix(Path::new("src/lib.rs")), None);
        assert_eq!(package_prefix(Path::new("tests/basic.rs")), None);
        assert_eq!(package_prefix(Path::new("README.md")), None);
    }

    #[test]
    fn given_mixed_separator_path_when_normalizing_then_uses_workspace_relative_form() {
        let normalized = normalize_path(Path::new("./crates\\ripr\\src\\lib.rs"));
        assert_eq!(normalized, "crates/ripr/src/lib.rs");
    }

    fn function(file: &str, name: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("{file}::{name}")),
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 3,
            body: String::new(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
        }
    }

    fn test(file: &str, name: &str, body: &str) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 4,
            body: body.to_string(),
            calls: vec![CallFact {
                line: 1,
                name: "score".to_string(),
                text: body.to_string(),
            }],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
        }
    }

    /// Like `test` but with a configurable call name — needed for cross-crate
    /// tests where the owner name is not hardcoded to `"score"`.
    fn test_with_call(file: &str, name: &str, body: &str, call_name: &str) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 4,
            body: body.to_string(),
            calls: vec![CallFact {
                line: 1,
                name: call_name.to_string(),
                text: body.to_string(),
            }],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
        }
    }

    fn probe(file: &str, expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:test".to_string()),
            location: SourceLocation::new(file, 2, 1),
            owner: Some(SymbolId(format!("{file}::owner"))),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: None,
            after: Some(expression.to_string()),
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    /// A probe with no function owner — models a module-level struct field.
    fn struct_field_probe(file: &str, expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:struct-field".to_string()),
            location: SourceLocation::new(file, 46, 1),
            owner: None,
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Value,
            before: None,
            after: Some(expression.to_string()),
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn oracle_fact(assertion: &str, kind: OracleKind, strength: OracleStrength) -> OracleFact {
        OracleFact {
            line: 2,
            text: assertion.to_string(),
            kind,
            strength,
            observed_tokens: extract_identifier_tokens(assertion),
        }
    }

    fn test_with_assertions(
        file: &str,
        name: &str,
        body: &str,
        assertions: Vec<OracleFact>,
    ) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 10,
            body: body.to_string(),
            calls: Vec::new(),
            assertions,
            literals: Vec::new(),
            attrs: Vec::new(),
        }
    }

    // --- #1052 repro ---
    // A module-level struct-field probe (owner_fn=None) whose changed expression
    // token (`open_in`) appears in the assertion observed_tokens of an exact-value
    // oracle test.  The oracle test MUST be selected as related.
    #[test]
    fn given_struct_field_probe_when_assertion_references_field_token_then_oracle_test_is_related()
    {
        let assertion_text = "assert_eq!(lane.open_in, OpenIn::Browser);";
        let oracle_test = test_with_assertions(
            "crates/ripr/tests/repo_lane.rs",
            "repo_lane_deserializes_fields_correctly",
            &format!("let lane = RepoLane {{ open_in: OpenIn::Browser, .. }}; {assertion_text}"),
            vec![oracle_fact(
                assertion_text,
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let index = RustIndex {
            tests: vec![oracle_test],
            ..RustIndex::default()
        };
        // Struct field probe: expression contains `open_in` (7 chars, >= 5)
        let probe = struct_field_probe("crates/ripr/src/config.rs", "open_in");

        let related = find_related_tests(&probe, None, &index, true, None);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].0.name, "repo_lane_deserializes_fields_correctly");
    }

    #[test]
    fn given_foreign_separator_file_stem_when_associated_then_assertion_text_is_preserved() {
        let assertion_text = "assert_eq!(value, 3);";
        let oracle_test = test_with_assertions(
            "crates/ripr/tests/config.rs",
            "checks_value",
            assertion_text,
            vec![oracle_fact(
                assertion_text,
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let index = RustIndex {
            tests: vec![oracle_test],
            ..RustIndex::default()
        };
        let forward_probe = struct_field_probe("crates/ripr/src/config.rs", "marker");
        let windows_probe = struct_field_probe("crates\\ripr\\src\\config.rs", "marker");

        let forward_related = find_related_tests(&forward_probe, None, &index, true, None);
        let windows_related = find_related_tests(&windows_probe, None, &index, true, None);

        assert_eq!(forward_related.len(), 1);
        assert_eq!(windows_related.len(), 1);
        assert_eq!(forward_related[0].0.name, windows_related[0].0.name);
        assert_eq!(forward_related[0].1, RelationReason::SameTestFile);
        assert_eq!(windows_related[0].1, RelationReason::SameTestFile);
        assert_eq!(
            forward_related[0].0.assertions[0].text, assertion_text,
            "path normalization must not rewrite assertion text"
        );
        assert_eq!(
            windows_related[0].0.assertions[0].text, assertion_text,
            "path normalization must not rewrite assertion text"
        );
    }

    #[test]
    fn given_dotfile_probe_when_associated_then_only_matching_companion_is_related() {
        let matching_test = test_with_assertions(
            "crates/ripr/tests/.config.rs",
            "checks_config_value",
            "assert_eq!(value, 3);",
            Vec::new(),
        );
        let unrelated_test = test_with_assertions(
            "crates/ripr/tests/config.rs",
            "checks_value",
            "assert_eq!(value, 3);",
            Vec::new(),
        );
        let index = RustIndex {
            tests: vec![matching_test, unrelated_test],
            ..RustIndex::default()
        };
        let forward_probe = struct_field_probe("crates/ripr/src/.config", "marker");
        let windows_probe = struct_field_probe("crates\\ripr\\src\\.config", "marker");

        for probe in [forward_probe, windows_probe] {
            let related = find_related_tests(&probe, None, &index, true, None);

            assert_eq!(related.len(), 1);
            assert_eq!(related[0].0.file, Path::new("crates/ripr/tests/.config.rs"));
            assert_eq!(related[0].1, RelationReason::SameTestFile);
        }
    }

    #[test]
    #[cfg(unix)]
    fn given_non_utf8_stems_when_classify_extracts_then_lossy_collision_fails_closed() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        // Two distinct non-UTF-8 leaf names share one lossy rendering;
        // deriving stems through lossy conversion would fabricate
        // file-name affinity between unrelated entities, so extraction
        // must fail closed instead (#3599).
        let first = Path::new(OsStr::from_bytes(b"crates/app/src/pricing_\xff.rs"));
        let second = Path::new(OsStr::from_bytes(b"crates/app/src/pricing_\xfe.rs"));
        assert_eq!(normalized_file_stem(first), "");
        assert_eq!(normalized_file_stem(second), "");
        // Valid-UTF-8 stems are unchanged, including dotfiles and
        // `./`-prefixed forms that `normalize_path` trims.
        assert_eq!(normalized_file_stem(Path::new("src/config.rs")), "config");
        assert_eq!(normalized_file_stem(Path::new("./src/config.rs")), "config");
    }

    // --- #1054 repro ---
    // Two sibling struct-field probes (`open_in` and `open_cap`) are both
    // asserted by the SAME exact-value oracle test.  Both MUST select that test
    // consistently (no asymmetry).
    #[test]
    fn given_sibling_struct_field_probes_when_both_asserted_then_both_select_same_oracle_test() {
        let assertion_open_in = "assert_eq!(lane.open_in, OpenIn::Browser);";
        let assertion_open_cap = "assert_eq!(lane.open_cap, 8);";
        let body =
            format!("let lane = RepoLane::default(); {assertion_open_in} {assertion_open_cap}");
        let oracle_test = test_with_assertions(
            "crates/ripr/tests/repo_lane.rs",
            "repo_lane_fields_have_expected_defaults",
            &body,
            vec![
                oracle_fact(
                    assertion_open_in,
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                ),
                oracle_fact(
                    assertion_open_cap,
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                ),
            ],
        );
        let index = RustIndex {
            tests: vec![oracle_test],
            ..RustIndex::default()
        };

        let probe_open_in = struct_field_probe("crates/ripr/src/config.rs", "open_in");
        let probe_open_cap = struct_field_probe("crates/ripr/src/config.rs", "open_cap");

        let related_in = find_related_tests(&probe_open_in, None, &index, true, None);
        let related_cap = find_related_tests(&probe_open_cap, None, &index, true, None);

        assert_eq!(
            related_in.len(),
            1,
            "open_in probe must find the oracle test"
        );
        assert_eq!(
            related_cap.len(),
            1,
            "open_cap probe must find the oracle test"
        );
        assert_eq!(
            related_in[0].0.name, related_cap[0].0.name,
            "sibling fields must select the same oracle test"
        );
    }

    // --- Anti-over-association: short 4-char token rejected ---
    // A struct-field probe whose only shared token with a test assertion is a
    // common 4-char name (`port`) must NOT select that test via the
    // assertions_reference_owner signal — the >= 5 threshold rejects it.
    // The test and probe live in different-stem files and the test name does
    // not contain the probe token, so same_file_or_named also does not fire.
    #[test]
    fn given_struct_field_probe_when_shared_token_is_4_chars_then_assertions_signal_does_not_fire()
    {
        // Test file stem "rendering" != probe file stem "scheduler".
        // Test name "rendering_is_initialized" does not contain "port".
        let assertion_text = "assert_eq!(srv.port, 8080);";
        let unrelated_test = test_with_assertions(
            "crates/ripr/tests/rendering.rs",
            "rendering_is_initialized",
            assertion_text,
            vec![oracle_fact(
                assertion_text,
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let index = RustIndex {
            tests: vec![unrelated_test],
            ..RustIndex::default()
        };
        // Probe expression is `port` (4 chars) — below the >= 5 threshold.
        let probe = struct_field_probe("crates/ripr/src/scheduler.rs", "port");

        let related = find_related_tests(&probe, None, &index, true, None);

        assert!(
            related.is_empty(),
            "a 4-char shared token must not trigger assertions_reference_owner"
        );
    }

    // --- Anti-over-association: owner_fn=Some blocks the signal ---
    // When owner_fn is Some, assertions_reference_owner must NOT fire even if
    // probe tokens appear in assertion observed_tokens.  The calls_owner / name
    // proximity gates govern function-bodied probes.
    #[test]
    fn given_function_owner_probe_when_assertion_references_probe_token_then_signal_does_not_fire()
    {
        let owner = function("src/lib.rs", "discounted_total");
        let assertion_text =
            "assert_eq!(token_label(\"discount_threshold\"), \"token:discount_threshold\");";
        let unrelated_test = test_with_assertions(
            "tests/tokens.rs",
            "token_label_includes_token_text",
            assertion_text,
            vec![oracle_fact(
                assertion_text,
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let index = RustIndex {
            tests: vec![unrelated_test],
            ..RustIndex::default()
        };
        // Probe expression contains `discount_threshold` (>= 5 chars) which is
        // also in the assertion observed_tokens — but owner_fn is Some, so the
        // assertions_reference_owner signal must NOT fire.
        let probe = probe("src/lib.rs", "amount >= discount_threshold");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert!(
            related.is_empty(),
            "assertions_reference_owner must not fire when owner_fn is Some"
        );
    }

    #[test]
    fn absolute_root_probe_matches_relative_companion_test_file() -> Result<(), String> {
        let index = RustIndex {
            tests: vec![test(
                "src/tests/gate_watchdog_tests.rs",
                "terminal_states_are_exact",
                "classify_gate_watchdog(&input);",
            )],
            ..RustIndex::default()
        };
        let probe = struct_field_probe(
            "/repo/ub-review/src/gate_watchdog.rs",
            "pub(crate) state: GateWatchdogState",
        );

        let related = find_related_tests(&probe, None, &index, true, None);
        let Some((test, reason)) = related.first() else {
            return Err(
                "absolute root probe did not match its relative companion test".to_string(),
            );
        };
        if test.file != Path::new("src/tests/gate_watchdog_tests.rs")
            || *reason != RelationReason::SameTestFile
        {
            return Err(format!("unexpected companion-test relation: {related:?}"));
        }
        Ok(())
    }

    #[test]
    fn absolute_workspace_probe_keeps_relative_cross_crate_guard() -> Result<(), String> {
        let index = RustIndex {
            tests: vec![test(
                "crates/other/tests/state_tests.rs",
                "state_is_exact",
                "state();",
            )],
            ..RustIndex::default()
        };
        let probe = struct_field_probe(
            "/repo/ripr/crates/core/src/state.rs",
            "pub(crate) state: State",
        );

        let related = find_related_tests(&probe, None, &index, true, None);
        if !related.is_empty() {
            return Err(format!("cross-crate test must stay unrelated: {related:?}"));
        }
        Ok(())
    }

    /// #3235: two non-`crates/` packages addressed by absolute paths carry no
    /// derivable package scope. Before the fail-closed guard the prefix came
    /// back `None` and the package scoping was silently disabled, admitting
    /// wrong-package name/token coincidences. The fixture is a pure weak
    /// match — the test never calls the owner, only its name contains it.
    #[test]
    fn given_absolute_owner_and_absolute_wrong_package_test_when_weak_match_then_filtered() {
        let owner = function("/ws/pkg-a/src/lib.rs", "score");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![test_with_call(
                "/ws/pkg-b/tests/coincidence.rs",
                "pkg_b_score_named_test",
                "observe(&input);",
                "observe",
            )],
            ..RustIndex::default()
        };
        let probe = probe("/ws/pkg-a/src/lib.rs", "score + 1");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert!(
            related.is_empty(),
            "wrong-package weak match across unscopable absolute paths must be filtered: {related:?}"
        );
    }

    /// #3235 positive control: the #2971 strong path still admits a
    /// cross-package test that calls a uniquely-named owner even when both
    /// paths are absolute — failing closed must not break the deliberate
    /// unique-owner bypass.
    #[test]
    fn given_absolute_paths_when_unique_owner_is_called_cross_package_then_retained() {
        let owner = function("/ws/pkg-a/src/lib.rs", "compute_hash");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![test_with_call(
                "/ws/pkg-b/tests/integration.rs",
                "hash_integration_test",
                "let result = compute_hash(b\"input\");",
                "compute_hash",
            )],
            ..RustIndex::default()
        };
        let probe = probe("/ws/pkg-a/src/lib.rs", "compute_hash(input)");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert_eq!(
            related.len(),
            1,
            "uniquely-owned call must stay admitted across absolute paths"
        );
        assert_eq!(related[0].1, RelationReason::DirectOwnerCall);
    }

    /// #3235: the Windows drive-prefixed form must fail closed through the
    /// same authority as the Unix-rooted form on every host. Pure weak match:
    /// the test never calls the owner. The tokens are built at runtime so no
    /// literal drive-letter path sits in the source tree.
    #[test]
    fn given_drive_prefixed_owner_and_absolute_wrong_package_test_when_weak_match_then_filtered() {
        let owner_file = format!("F:{}ws/pkg-a/src/lib.rs", '/');
        let wrong_package_test = format!("F:{}ws/pkg-b/tests/coincidence.rs", '/');
        let owner = function(&owner_file, "score");
        let index = RustIndex {
            functions: vec![owner.clone()],
            tests: vec![test_with_call(
                &wrong_package_test,
                "pkg_b_score_named_test",
                "observe(&input);",
                "observe",
            )],
            ..RustIndex::default()
        };
        let probe = probe(&owner_file, "score + 1");

        let related = find_related_tests(&probe, Some(&owner), &index, true, None);

        assert!(
            related.is_empty(),
            "drive-prefixed wrong-package weak match must be filtered: {related:?}"
        );
    }

    /// #3235 struct-probe symmetry: an unscopable-absolute module-level probe
    /// against an absolute wrong-package test fails closed too.
    #[test]
    fn given_absolute_struct_probe_and_absolute_wrong_package_test_when_weak_match_then_filtered()
    -> Result<(), String> {
        let index = RustIndex {
            tests: vec![test(
                "/ws/pkg-b/tests/gate_watchdog_tests.rs",
                "gate_watchdog_is_exact",
                "observe(&input);",
            )],
            ..RustIndex::default()
        };
        let probe = struct_field_probe("/ws/pkg-a/src/gate_watchdog.rs", "pub(crate) state: State");

        let related = find_related_tests(&probe, None, &index, true, None);
        if !related.is_empty() {
            return Err(format!(
                "struct-probe wrong-package weak match must be filtered: {related:?}"
            ));
        }
        Ok(())
    }
}
