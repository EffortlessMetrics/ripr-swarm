use super::*;

pub(super) mod context;

pub(crate) use context::CompactGripContext;
pub(super) use context::{CompactTest, ScopedImportedFunctionAlias, call_text_contains_named_call};

type ReexportAliasesByModule = BTreeMap<(String, String), (String, String)>;

pub(super) fn direct_import_modules_from_source(source: &str) -> BTreeMap<String, String> {
    let mut imports = BTreeMap::new();
    for raw in source.lines() {
        let cleaned = strip_comments_and_strings(raw);
        let Some(import) = cleaned
            .trim()
            .strip_prefix("use ")
            .or_else(|| cleaned.trim().strip_prefix("pub use "))
        else {
            continue;
        };
        let import = import.trim_end_matches(';').trim();
        if let Some((base, rest)) = import.split_once("::{") {
            let Some(body) = rest.strip_suffix('}') else {
                continue;
            };
            let module = base.trim().strip_prefix("crate::").unwrap_or(base.trim());
            for item in body.split(',').map(str::trim) {
                if item.is_empty() || item == "self" || item == "*" {
                    continue;
                }
                let (name, alias) = item.split_once(" as ").map_or(
                    (item, item.rsplit("::").next().unwrap_or(item)),
                    |(name, alias)| (name.trim(), alias.trim()),
                );
                if !name.is_empty() && !alias.is_empty() {
                    imports.insert(alias.to_string(), module.to_string());
                }
            }
            continue;
        }
        let (path, alias) = import.split_once(" as ").map_or(
            (import, import.rsplit("::").next().unwrap_or(import)),
            |(path, alias)| (path.trim(), alias.trim()),
        );
        let Some((module, _name)) = path.rsplit_once("::") else {
            continue;
        };
        let module = module.strip_prefix("crate::").unwrap_or(module);
        if !alias.is_empty() {
            imports.insert(alias.to_string(), module.to_string());
        }
    }
    imports
}

/// Walk `index.tests` and return tests that plausibly relate to `seam`,
/// each tagged with the single highest-priority `RelationReason` it
/// satisfies. The two-step "match then rank" replaces the old binary
/// `calls_owner || same_file_or_named` check from earlier campaigns.
///
/// Detection per reason — strict ordering: the first reason that fires
/// wins, so e.g. a test that both `calls owner` and `is in same file`
/// carries `direct_owner_call`, never `same_test_file`.
pub(super) fn find_related_tests_with_context<'context, 'index>(
    seam: &RepoSeam,
    context: &'context CompactGripContext<'index>,
) -> Vec<(&'context CompactTest<'index>, RelationReason)> {
    let owner = OwnerContext::resolve(seam, context);
    let target_tokens = assertion_target_tokens(seam);
    let prefix = owner.prefix.as_deref();

    let mut candidates: BTreeMap<usize, RelationReason> = BTreeMap::new();
    match_direct_owner_call(&mut candidates, context, prefix, &owner);
    if owner.function.as_ref().is_some_and(|function| {
        function.file.file_name().and_then(|name| name.to_str()) == Some("lib.rs")
    }) {
        let owner_crate_name = owner
            .function
            .as_ref()
            .and_then(|function| context.crate_name_for_path(&function.file));
        for (test_index, test) in context.tests.iter().enumerate() {
            if test._root_import_names.contains(&owner.name)
                && test
                    .direct_import_modules
                    .get(&owner.name)
                    .is_some_and(|module| {
                        module.split("::").count() == 1
                            && owner_crate_name.as_deref() == Some(module.as_str())
                    })
                && test.test.calls.iter().any(|call| call.name == owner.name)
            {
                candidates.insert(test_index, RelationReason::DirectOwnerCall);
            }
        }
    }
    match_helper_owner_call(&mut candidates, context, prefix, &owner);
    match_target_affinity_owner_call(
        &mut candidates,
        seam,
        context,
        prefix,
        &owner,
        &target_tokens,
    );
    match_assertion_target_affinity(&mut candidates, context, prefix, &target_tokens);
    match_same_test_file(&mut candidates, context, prefix, &owner);
    match_same_module(&mut candidates, context, prefix, &owner);
    match_owner_named_test(&mut candidates, context, prefix, &owner);
    match_import_path_affinity(&mut candidates, context, prefix, &owner);
    match_fixture_owner_affinity(&mut candidates, context, prefix, &owner);

    dedupe_related_candidates(candidates, context)
}

/// Cached facts about a seam's owning function that every relationship
/// strategy reads from. Resolved once per seam to avoid re-walking the
/// index and re-deriving file stems / module paths per strategy.
pub(super) struct OwnerContext {
    function: Option<FunctionSummary>,
    name: String,
    name_lower: String,
    file_stem: String,
    module_path: Option<String>,
    prefix: Option<String>,
    fixture_names: BTreeSet<String>,
}

impl OwnerContext {
    fn resolve(seam: &RepoSeam, context: &CompactGripContext<'_>) -> Self {
        let owner_fn = find_owner_function(seam, context.index);
        let function = owner_fn.cloned();
        let name = owner_fn.map(|f| f.name.as_str()).unwrap_or("").to_string();
        let name_lower = name.to_ascii_lowercase();
        let owner_file = owner_fn.map(|f| f.file.as_path());
        let file_stem = owner_file
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let module_path = owner_fn.and_then(function_module_path).or_else(|| {
            owner_file
                .and_then(module_path_for)
                .map(|path| path.replace('/', "::"))
        });
        let prefix = owner_fn.and_then(|f| package_prefix(&f.file));
        let fixture_names = owner_file
            .and_then(|file| context.index.files.get(file))
            .map(fixture_names_for_owner_file)
            .unwrap_or_default();
        Self {
            function,
            name,
            name_lower,
            file_stem,
            module_path,
            prefix,
            fixture_names,
        }
    }
}

/// Tokens from `RequiredDiscriminator` and `ExpectedSink` that an
/// `assertion_target_affinity` match must mention. Already filtered
/// through `extract_identifier_tokens`, so stop-words and short tokens
/// are excluded.
pub(super) fn assertion_target_tokens(seam: &RepoSeam) -> BTreeSet<String> {
    let discriminator_tokens = match seam.required_discriminator() {
        crate::analysis::seams::RequiredDiscriminator::MatchArmTaken { arm } => {
            match_arm_assertion_target_tokens(arm)
        }
        _ if seam.kind() == SeamKind::CallPresence => {
            call_presence_callee_target_tokens(required_discriminator_text(seam))
        }
        _ => required_discriminator_tokens(seam),
    };
    let sink_tokens = if seam.kind() == SeamKind::MatchArm {
        Vec::new()
    } else {
        extract_identifier_tokens(seam.expected_sink().as_str())
    };
    let filters_generic_call_tokens = seam.kind() == SeamKind::CallPresence;
    discriminator_tokens
        .into_iter()
        .chain(sink_tokens)
        .filter(|token| {
            !filters_generic_call_tokens
                || call_presence_assertion_affinity_token_is_specific_enough(token)
        })
        .collect()
}

pub(super) fn match_arm_assertion_target_tokens(arm: &str) -> Vec<String> {
    extract_identifier_tokens(arm)
        .into_iter()
        .filter(|token| match_arm_assertion_target_token_is_specific_enough(token))
        .collect()
}

pub(super) fn match_arm_assertion_target_token_is_specific_enough(token: &str) -> bool {
    token.chars().next().is_some_and(|ch| ch.is_uppercase())
}

pub(super) fn call_presence_callee_target_tokens(expression: &str) -> Vec<String> {
    let mut tokens = extract_call_facts(expression, 0)
        .into_iter()
        .flat_map(|call| extract_identifier_tokens(&call.name))
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens
}

pub(super) fn call_presence_assertion_affinity_token_is_specific_enough(token: &str) -> bool {
    if matches!(
        token,
        "arg"
            | "args"
            | "arm"
            | "class"
            | "clone"
            | "context"
            | "count"
            | "counts"
            | "data"
            | "dedup"
            | "description"
            | "entry"
            | "evidence"
            | "field"
            | "file"
            | "files"
            | "from"
            | "input"
            | "is_empty"
            | "iter"
            | "item"
            | "items"
            | "kind"
            | "line"
            | "lines"
            | "missing"
            | "model"
            | "name"
            | "owner"
            | "output"
            | "path"
            | "paths"
            | "probe"
            | "result"
            | "results"
            | "side_effect"
            | "sink"
            | "sort"
            | "summary"
            | "target"
            | "test"
            | "tests"
            | "text"
            | "u64"
            | "value"
            | "values"
            | "variant"
            | "byte"
            | "bytes"
    ) {
        return false;
    }
    true
}

pub(super) fn match_direct_owner_call(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
) {
    if owner.name.is_empty() {
        return;
    }
    let mut indices = context.module_call_indices(
        owner.module_path.as_deref().unwrap_or_default(),
        &owner.name,
    );
    let package_unique = owner
        .function
        .as_ref()
        .is_some_and(|function| context.owner_name_is_unique_for_package(function));
    let owner_is_crate_root = owner.function.as_ref().is_some_and(|function| {
        function.file.file_name().and_then(|name| name.to_str()) == Some("lib.rs")
    });
    let owner_crate_name = owner
        .function
        .as_ref()
        .and_then(|function| context.crate_name_for_path(&function.file));
    if package_unique || owner_is_crate_root {
        if let Some(name_indices) = context.tests_by_call_name.get(&owner.name) {
            indices.extend(name_indices.iter().copied().filter(|test_index| {
                context.tests.get(*test_index).is_some_and(|indexed| {
                    bare_owner_call_is_admissible(indexed, owner)
                        || (owner_is_crate_root
                            && indexed.direct_import_modules.get(&owner.name).is_some_and(
                                |module| {
                                    module.split("::").count() == 1
                                        && owner_crate_name.as_deref() == Some(module.as_str())
                                },
                            ))
                })
            }));
        }
        if owner_is_crate_root {
            indices.extend(
                context
                    .tests
                    .iter()
                    .enumerate()
                    .filter(|(_, indexed)| {
                        indexed
                            .direct_import_modules
                            .get(&owner.name)
                            .is_some_and(|module| {
                                module.split("::").count() == 1
                                    && owner_crate_name.as_deref() == Some(module.as_str())
                            })
                            && indexed
                                .test
                                .calls
                                .iter()
                                .any(|call| call.name == owner.name)
                    })
                    .map(|(index, _)| index),
            );
        }
    }
    indices.sort_unstable();
    indices.dedup();
    for test_index in indices {
        insert_related_candidate(
            candidates,
            context,
            prefix,
            test_index,
            RelationReason::DirectOwnerCall,
        );
    }
}

/// A bare leaf name is only a compatibility fallback for older integration
/// fixtures that omit imports. Reject explicit imports from another module,
/// local shadows, and method syntax; those shapes need a resolved path or
/// remain unknown rather than being credited by coincidence (#3214).
fn bare_owner_call_is_admissible(test: &CompactTest<'_>, owner: &OwnerContext) -> bool {
    let Some(function) = owner.function.as_ref() else {
        return false;
    };
    if let Some(imported) = test.direct_import_modules.get(&owner.name)
        && owner.module_path.as_deref() != Some(imported.as_str())
    {
        // Integration tests spell a root item through the package crate name
        // (`use package_name::item`). The compact index has no Cargo manifest
        // identity, so retain the legacy root-item fallback for that bounded
        // shape; non-root module imports remain fail-closed on mismatch.
        let owner_is_crate_root = owner.function.as_ref().is_some_and(|function| {
            function.file.file_name().and_then(|name| name.to_str()) == Some("lib.rs")
        });
        if !owner_is_crate_root {
            return false;
        }
        if imported.split("::").count() == 1
            && crate_name_for_path(&function.file).is_none_or(|crate_name| crate_name != *imported)
        {
            return false;
        }
    }
    if test
        .code_lines
        .iter()
        .any(|line| line_declares_name(line, &owner.name))
        || test.local_function_names.contains(&owner.name)
    {
        return false;
    }
    if test
        .test
        .calls
        .iter()
        .any(|call| call.name == owner.name && call_is_method_or_macro(&call.text, &call.name))
    {
        return false;
    }
    // Root imports are intentionally handled here because the legacy import
    // parser has no module segment to retain for `use crate::name;`.
    let owner_module = owner.module_path.as_deref().unwrap_or_default();
    !test.code_lines.iter().any(|line| {
        let Some(import) = line
            .trim_start()
            .strip_prefix("use ")
            .or_else(|| line.trim_start().strip_prefix("pub use "))
        else {
            return false;
        };
        let import = import.trim_end_matches(';').trim();
        let (path, alias) = import.split_once(" as ").map_or(
            (import, import.rsplit("::").next().unwrap_or(import)),
            |(path, alias)| (path.trim(), alias.trim()),
        );
        if alias != owner.name {
            return false;
        }
        let Some((module, _name)) = path.rsplit_once("::") else {
            return false;
        };
        let module = module.strip_prefix("crate::").unwrap_or(module);
        module != owner_module
    }) && function.name == owner.name
}

fn line_declares_name(line: &str, name: &str) -> bool {
    ["fn ", "let ", "const ", "static ", "struct ", "enum "]
        .iter()
        .any(|prefix| {
            line.trim_start().strip_prefix(prefix).is_some_and(|rest| {
                rest.trim_start().starts_with(&format!("{name}("))
                    || rest.trim_start().starts_with(&format!("{name} "))
                    || rest.trim_start().starts_with(&format!("{name}:"))
            })
        })
}

fn call_is_method_or_macro(text: &str, name: &str) -> bool {
    let Some(start) = text.find(name) else {
        return false;
    };
    let before = text[..start].trim_end();
    before.ends_with('.') || before.ends_with('!')
}

pub(super) fn match_helper_owner_call(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
) {
    if owner.name.is_empty() {
        return;
    }
    let Some(indices) = context.tests_by_helper_owner_call_name.get(&owner.name) else {
        return;
    };
    for test_index in indices {
        let test = &context.tests[*test_index];
        let package_root_direct_import = test._root_import_names.contains(&owner.name)
            && test
                .direct_import_modules
                .get(&owner.name)
                .is_some_and(|module| module.split("::").count() == 1)
            && test.test.calls.iter().any(|call| call.name == owner.name);
        if package_root_direct_import {
            continue;
        }
        insert_related_candidate(
            candidates,
            context,
            prefix,
            *test_index,
            RelationReason::HelperOwnerCall,
        );
    }
}

pub(super) fn match_target_affinity_owner_call(
    candidates: &mut BTreeMap<usize, RelationReason>,
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
    target_tokens: &BTreeSet<String>,
) {
    if requires_concrete_activation_values(seam)
        || owner.name.is_empty()
        || target_tokens.is_empty()
    {
        return;
    }
    for (test_index, test) in context.tests.iter().enumerate() {
        if !test
            .target_affinity_owner_call_names
            .contains(owner.name.as_str())
            || !test_assertion_mentions_any_target_token(test, target_tokens)
        {
            continue;
        }
        if owner.function.as_ref().is_some_and(|function| {
            function.file.file_name().and_then(|name| name.to_str()) == Some("lib.rs")
        }) && test.direct_import_modules.contains_key(&owner.name)
        {
            continue;
        }
        insert_related_candidate(
            candidates,
            context,
            prefix,
            test_index,
            RelationReason::HelperOwnerCall,
        );
    }
}

pub(super) fn test_assertion_mentions_any_target_token(
    test: &CompactTest<'_>,
    target_tokens: &BTreeSet<String>,
) -> bool {
    target_tokens
        .iter()
        .any(|token| test.assertion_tokens.contains(token))
}

pub(super) fn match_assertion_target_affinity(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    target_tokens: &BTreeSet<String>,
) {
    for token in target_tokens {
        if let Some(indices) = context.tests_by_assertion_token.get(token) {
            for test_index in indices {
                insert_related_candidate(
                    candidates,
                    context,
                    prefix,
                    *test_index,
                    RelationReason::AssertionTargetAffinity,
                );
            }
        }
    }
}

pub(super) fn match_same_test_file(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
) {
    if owner.file_stem.is_empty() {
        return;
    }
    let stems = [
        owner.file_stem.clone(),
        format!("{}_test", owner.file_stem),
        format!("{}_tests", owner.file_stem),
    ];
    for stem in stems {
        if let Some(indices) = context.tests_by_file_stem.get(&stem) {
            for test_index in indices {
                insert_related_candidate(
                    candidates,
                    context,
                    prefix,
                    *test_index,
                    RelationReason::SameTestFile,
                );
            }
        }
    }
}

pub(super) fn match_same_module(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
) {
    let Some(module_path) = owner.module_path.as_deref() else {
        return;
    };
    for test_index in context.same_module_indices(module_path) {
        insert_related_candidate(
            candidates,
            context,
            prefix,
            test_index,
            RelationReason::SameModule,
        );
    }
}

pub(super) fn match_owner_named_test(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
) {
    for test_index in context.owner_named_indices(&owner.name_lower) {
        insert_related_candidate(
            candidates,
            context,
            prefix,
            test_index,
            RelationReason::OwnerNamedTest,
        );
    }
}

pub(super) fn match_import_path_affinity(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
) {
    if owner.name.is_empty() {
        return;
    }
    let Some(indices) = context.tests_by_import_token.get(&owner.name) else {
        return;
    };
    for test_index in indices {
        let Some(indexed) = context.tests.get(*test_index) else {
            continue;
        };
        let exact_module_call = owner.module_path.as_deref().is_some_and(|module| {
            context
                .module_call_indices(module, &owner.name)
                .contains(test_index)
        });
        let exact_module_import = owner
            .module_path
            .as_deref()
            .is_some_and(|module| test_imports_owner_module_compact(indexed, module, &owner.name));
        let package_unique = owner
            .function
            .as_ref()
            .is_some_and(|function| context.owner_name_is_unique_for_package(function));
        let name_only_import = package_unique && test_imports_owner_compact(indexed, &owner.name);
        if !exact_module_call && !exact_module_import && !name_only_import {
            continue;
        }
        insert_related_candidate(
            candidates,
            context,
            prefix,
            *test_index,
            RelationReason::ImportPathAffinity,
        );
    }
}

pub(super) fn match_fixture_owner_affinity(
    candidates: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext,
) {
    for fixture_name in &owner.fixture_names {
        if let Some(indices) = context.tests_by_call_name.get(fixture_name) {
            for test_index in indices {
                insert_related_candidate(
                    candidates,
                    context,
                    prefix,
                    *test_index,
                    RelationReason::FixtureOwnerAffinity,
                );
            }
        }
    }
}

pub(super) fn dedupe_related_candidates<'context, 'index>(
    candidates: BTreeMap<usize, RelationReason>,
    context: &'context CompactGripContext<'index>,
) -> Vec<(&'context CompactTest<'index>, RelationReason)> {
    let mut related: Vec<(&'context CompactTest<'index>, RelationReason)> = Vec::new();
    let mut seen: std::collections::HashSet<(String, PathBuf, usize)> =
        std::collections::HashSet::new();

    for (test_index, reason) in candidates {
        let Some(indexed) = context.tests.get(test_index) else {
            continue;
        };
        let key = (
            indexed.test.name.clone(),
            indexed.test.file.clone(),
            indexed.test.start_line,
        );
        if seen.insert(key) {
            related.push((indexed, reason));
        }
    }
    related
}

pub(super) fn insert_related_candidate(
    candidate_reasons: &mut BTreeMap<usize, RelationReason>,
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    test_index: usize,
    reason: RelationReason,
) {
    if candidate_reasons.contains_key(&test_index) {
        return;
    }
    let Some(indexed) = context.tests.get(test_index) else {
        return;
    };
    if let Some(prefix) = prefix
        && !indexed.path_normalized.starts_with(prefix)
    {
        return;
    }
    candidate_reasons.insert(test_index, reason);
}

pub(super) fn find_related_tests_compact<'a>(
    seam: &RepoSeam,
    context: &'a CompactGripContext<'_>,
) -> Vec<&'a CompactTest<'a>> {
    let mut related = find_related_tests_with_context(seam, context);
    sort_related_tests_for_seam(seam, context, &mut related);
    related
        .into_iter()
        .take(COMPACT_RELATED_TEST_LIMIT)
        .map(|(indexed, _reason)| indexed)
        .collect()
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct RelatedTestRankKey {
    relation_confidence: u8,
    relation_reason: u8,
    oracle_strength: Reverse<u8>,
    activation_overlap: Reverse<usize>,
    file: PathBuf,
    test_name: String,
    line: usize,
}

pub(super) fn sort_related_tests_for_seam(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
    related: &mut [(&CompactTest<'_>, RelationReason)],
) {
    related.sort_by_cached_key(|entry| {
        let (indexed, reason) = *entry;
        related_test_rank_key(seam, context, indexed, reason)
    });
}

pub(super) fn related_test_rank_key(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
    indexed: &CompactTest<'_>,
    reason: RelationReason,
) -> RelatedTestRankKey {
    let (_oracle_kind, oracle_strength) = best_oracle(indexed.test, seam);
    RelatedTestRankKey {
        relation_confidence: reason.confidence().rank(),
        relation_reason: reason.priority(),
        oracle_strength: Reverse(oracle_strength.rank()),
        activation_overlap: Reverse(activation_overlap_score(seam, context, indexed)),
        file: indexed.test.file.clone(),
        test_name: indexed.test.name.clone(),
        line: indexed.test.start_line,
    }
}

pub(super) fn fixture_names_for_owner_file(facts: &rust_index::FileFacts) -> BTreeSet<String> {
    facts
        .functions
        .iter()
        .filter(|f| !f.is_test && (is_fixture_named(&f.name) || f.body.contains("#[fixture]")))
        .map(|f| f.name.clone())
        .collect()
}

/// Tokens drawn from a `RepoSeam`'s `RequiredDiscriminator`. Filtered
/// through `extract_identifier_tokens` so common short words and
/// stop-tokens are already excluded.
pub(super) fn required_discriminator_tokens(seam: &RepoSeam) -> Vec<String> {
    extract_identifier_tokens(required_discriminator_text(seam))
}

pub(super) fn required_discriminator_text(seam: &RepoSeam) -> &str {
    use crate::analysis::seams::RequiredDiscriminator;
    match seam.required_discriminator() {
        RequiredDiscriminator::BoundaryValue { description }
        | RequiredDiscriminator::ReturnValue { description } => description.as_str(),
        RequiredDiscriminator::ErrorVariant { variant } => variant.as_str(),
        RequiredDiscriminator::FieldValue { field } => field.as_str(),
        RequiredDiscriminator::Effect { sink } => sink.as_str(),
        RequiredDiscriminator::MatchArmTaken { arm } => arm.as_str(),
        RequiredDiscriminator::CallSite { target } => target.as_str(),
    }
}

/// Token-aware: does any assertion text in `test` contain at least one
/// of `tokens` as a whole identifier? Substring match would let
/// `discount` accidentally match `discount_threshold`; we want exact
/// identifier hits.
#[cfg(test)]
pub(super) fn assertion_targets_seam(test: &TestSummary, tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    for assertion in &test.assertions {
        let assertion_tokens = extract_identifier_tokens(&assertion.text);
        if assertion_tokens
            .iter()
            .any(|at| tokens.iter().any(|t| at == t))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
pub(super) fn same_test_file(test_file: &Path, owner_stem: &str) -> bool {
    let stem = match test_file.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return false,
    };
    if stem == owner_stem {
        return true;
    }
    // Suffix check avoids the allocation that `stem == format!("{owner_stem}_test")`
    // would do per call. Two suffix variants cover the common naming
    // conventions: `*_test.rs` and `*_tests.rs`.
    if let Some(prefix) = stem.strip_suffix("_test")
        && prefix == owner_stem
    {
        return true;
    }
    if let Some(prefix) = stem.strip_suffix("_tests")
        && prefix == owner_stem
    {
        return true;
    }
    false
}

/// Module path slug for a Rust source file: the path components below
/// `src/` or `tests/`, joined by `/`, dropping the file extension.
/// Returns `None` for files that do not sit under one of those roots.
/// Examples (Unix-style after normalize):
/// - `crates/ripr/src/auth/login.rs` → `auth/login`
/// - `tests/cli_smoke.rs`            → `cli_smoke`
pub(super) fn module_path_for(file: &Path) -> Option<String> {
    let normalized = normalize_path(file);
    let body = normalized
        .rfind("/src/")
        .map(|idx| &normalized[idx + "/src/".len()..])
        .or_else(|| {
            normalized
                .rfind("/tests/")
                .map(|idx| &normalized[idx + "/tests/".len()..])
        })
        .or_else(|| normalized.strip_prefix("src/"))
        .or_else(|| normalized.strip_prefix("tests/"))?;
    let trimmed = body.strip_suffix(".rs").unwrap_or(body);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Canonical crate-relative module path for a parsed function. The
/// file-derived module path alone cannot distinguish inline modules, so
/// recover those segments from the parser symbol id as well (#3214).
pub(super) fn function_module_path(function: &FunctionSummary) -> Option<String> {
    let file = normalize_path(&function.file);
    let mut module = crate_module_path_for_file(&function.file)?;
    let prefix = format!("{file}::");
    let inline = function.id.0.strip_prefix(&prefix)?;
    let mut segments = inline.split("::").collect::<Vec<_>>();
    segments.pop();
    segments.retain(|segment| !segment.starts_with("impl "));
    if !segments.is_empty() {
        if !module.is_empty() {
            module.push_str("::");
        }
        module.push_str(&segments.join("::"));
    }
    Some(module)
}

fn crate_module_path_for_file(file: &Path) -> Option<String> {
    let normalized = normalize_path(file);
    let body = normalized
        .rfind("/src/")
        .map(|idx| &normalized[idx + "/src/".len()..])
        .or_else(|| normalized.strip_prefix("src/"))
        .or_else(|| {
            normalized
                .rfind("/tests/")
                .map(|idx| &normalized[idx + "/tests/".len()..])
        })
        .or_else(|| normalized.strip_prefix("tests/"))?;
    let trimmed = body.strip_suffix(".rs").unwrap_or(body);
    let module = if matches!(trimmed, "lib" | "main") {
        String::new()
    } else if let Some(parent) = trimmed.strip_suffix("/mod") {
        parent.to_string()
    } else {
        trimmed.to_string()
    };
    Some(module.replace('/', "::"))
}

/// Resolve direct call spellings to a small set of crate-relative module
/// paths. Qualified paths are the discriminating control for duplicate names;
/// bare calls retain the package-unique fallback in `match_direct_owner_call`.
pub(super) fn resolved_call_module_paths(
    call: &CallFact,
    test_module: &str,
    direct_imports: Option<&BTreeMap<String, ScopedImportedFunctionAlias>>,
    root_import_names: Option<&BTreeSet<String>>,
    package_name: Option<&str>,
    reexports: &ReexportAliasesByModule,
    code_lines: &[String],
) -> BTreeSet<(String, String)> {
    let mut paths = BTreeSet::new();
    let Some(start) = call.text.find(&call.name) else {
        return paths;
    };
    let before = call.text[..start].trim_end();
    let mut qualifier_start = before.len();
    let bytes = before.as_bytes();
    while qualifier_start > 0
        && (bytes[qualifier_start - 1].is_ascii_alphanumeric()
            || bytes[qualifier_start - 1] == b'_'
            || bytes[qualifier_start - 1] == b':')
    {
        qualifier_start -= 1;
    }
    let qualifier = before[qualifier_start..].trim_end_matches("::");
    if qualifier.is_empty() {
        if let Some(imported) = direct_imports
            .and_then(|imports| imports.get(&call.name))
            .and_then(|imported| imported.binding_at(call.line))
        {
            paths.insert((
                resolve_call_module_path(test_module, &imported.module_path),
                imported.name.clone(),
            ));
            // Integration tests import root items through the package crate
            // name (`use package_name::item`). The grouped form is kept out
            // of this compatibility bridge so grouped external imports
            // remain fail-closed.
            if imported.module_path.split("::").count() == 1
                && root_import_names.is_some_and(|names| names.contains(&call.name))
                && package_name == Some(imported.module_path.as_str())
            {
                paths.insert((String::new(), imported.name.clone()));
            }
        }
        if code_lines.iter().any(|line| {
            let line = line.trim_start();
            line.starts_with(&format!("use crate::{}", call.name))
                || line.starts_with(&format!("pub use crate::{}", call.name))
        }) {
            paths.insert((String::new(), call.name.clone()));
        }
        paths.insert((test_module.to_string(), call.name.clone()));
        let aliases = paths.clone();
        for (module_path, name) in aliases {
            if let Some(target) = reexports.get(&(module_path, name)) {
                paths.insert(target.clone());
            }
        }
        return paths;
    }
    let qualifier = qualifier.trim_start_matches("::");
    let mut segments = qualifier.split("::").filter(|segment| !segment.is_empty());
    let Some(first) = segments.next() else {
        return paths;
    };
    let mut base = match first {
        "crate" => Vec::new(),
        "self" => test_module
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>(),
        "super" => {
            let mut current = test_module
                .split("::")
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            current.pop();
            current
        }
        _ => test_module
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>(),
    };
    if !matches!(first, "crate" | "self" | "super") {
        base.push(first);
    }
    base.extend(segments);
    let module_path = base.join("::");
    paths.insert((module_path.clone(), call.name.clone()));
    if let Some(target) = reexports.get(&(module_path, call.name.clone())) {
        paths.insert(target.clone());
    }
    paths
}

fn reexport_aliases_by_module(index: &RustIndex) -> ReexportAliasesByModule {
    let mut aliases = BTreeMap::new();
    for (file, facts) in &index.files {
        let Some(file_module_path) = crate_module_path_for_file(file) else {
            continue;
        };
        let mut depth = 0usize;
        let mut inline_modules: Vec<(String, usize)> = Vec::new();
        for raw in facts.source.lines() {
            let cleaned = strip_comments_and_strings(raw);
            let line = cleaned.trim();
            let module_path = if inline_modules.is_empty() {
                file_module_path.clone()
            } else {
                let mut path = file_module_path.clone();
                for (name, _) in &inline_modules {
                    if !path.is_empty() {
                        path.push_str("::");
                    }
                    path.push_str(name);
                }
                path
            };
            if let Some((module_header, inline_body)) = line.split_once('{')
                && let Some(module_name) = module_header
                    .trim()
                    .strip_prefix("mod ")
                    .and_then(|rest| rest.split_whitespace().next())
                && let Some(use_start) = inline_body.find("pub use ")
            {
                let mut inline_module_path = module_path.clone();
                if !inline_module_path.is_empty() {
                    inline_module_path.push_str("::");
                }
                inline_module_path.push_str(module_name);
                if let Some(inline_import) = inline_body[use_start..].split(';').next() {
                    let inline_import = inline_import
                        .trim()
                        .strip_prefix("pub use ")
                        .or_else(|| inline_import.trim().strip_prefix("pub(crate) use "))
                        .unwrap_or(inline_import.trim());
                    register_reexport_import(&mut aliases, &inline_module_path, inline_import);
                }
            }
            let Some(import) = line
                .strip_prefix("pub use ")
                .or_else(|| line.strip_prefix("pub(crate) use "))
            else {
                update_inline_module_scope(line, &mut depth, &mut inline_modules);
                continue;
            };
            let import = import.trim_end_matches(';').trim();
            register_reexport_import(&mut aliases, &module_path, import);
            update_inline_module_scope(line, &mut depth, &mut inline_modules);
        }
    }
    aliases
}

fn register_reexport_import(
    aliases: &mut ReexportAliasesByModule,
    current_module: &str,
    import: &str,
) {
    if let Some((base, body)) = import.split_once("::{") {
        let Some(body) = body.strip_suffix('}') else {
            return;
        };
        let target_module = resolve_reexport_module_path(current_module, base.trim());
        for item in body.split(',').map(str::trim) {
            register_reexport_item(aliases, current_module, &target_module, item);
        }
    } else {
        register_reexport_item(aliases, current_module, "", import);
    }
}

fn register_reexport_item(
    aliases: &mut ReexportAliasesByModule,
    current_module: &str,
    grouped_target_module: &str,
    item: &str,
) {
    let item = item.trim();
    if item.is_empty() || item == "self" || item == "*" {
        return;
    }
    let (path, alias) = match item.split_once(" as ") {
        Some((path, alias)) => (path.trim(), alias.trim()),
        None => (item, item.rsplit("::").next().unwrap_or(item).trim()),
    };
    let (target_module, target_name) = if grouped_target_module.is_empty() {
        let Some((target_module, target_name)) = path.rsplit_once("::") else {
            return;
        };
        (
            resolve_reexport_module_path(current_module, target_module),
            target_name.trim(),
        )
    } else {
        (grouped_target_module.to_string(), path.trim())
    };
    if target_module.is_empty() || target_name.is_empty() || alias.is_empty() {
        return;
    }
    aliases.insert(
        (current_module.to_string(), alias.to_string()),
        (target_module, target_name.to_string()),
    );
}

fn update_inline_module_scope(
    line: &str,
    depth: &mut usize,
    inline_modules: &mut Vec<(String, usize)>,
) {
    let opens = line.chars().filter(|ch| *ch == '{').count();
    let closes = line.chars().filter(|ch| *ch == '}').count();
    if let Some(rest) = line.split_once("mod ").map(|(_, rest)| rest)
        && opens > 0
        && !rest.trim_start().starts_with('(')
        && let Some(name) = rest
            .trim_start()
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .next()
            .filter(|name| !name.is_empty())
    {
        inline_modules.push((name.to_string(), *depth + opens));
    }
    *depth = depth.saturating_add(opens).saturating_sub(closes);
    while inline_modules
        .last()
        .is_some_and(|(_, module_depth)| *module_depth > *depth)
    {
        inline_modules.pop();
    }
}

fn resolve_reexport_module_path(current: &str, target: &str) -> String {
    if let Some(target) = target.strip_prefix("crate::") {
        return target.to_string();
    }
    let target = target.trim();
    if target.starts_with("self::") {
        return format!(
            "{}{}",
            current,
            target.strip_prefix("self").unwrap_or_default()
        )
        .trim_matches(':')
        .to_string();
    }
    if target == "self" {
        return current.to_string();
    }
    if target.starts_with("super::") {
        let mut segments = current
            .split("::")
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let mut rest = target;
        while let Some(next) = rest.strip_prefix("super::") {
            segments.pop();
            rest = next;
        }
        if !rest.is_empty() {
            segments.extend(rest.split("::"));
        }
        return segments.join("::");
    }
    let mut segments = current
        .split("::")
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    segments.extend(target.split("::"));
    segments.join("::")
}

fn resolve_call_module_path(current: &str, imported: &str) -> String {
    if let Some(path) = imported.strip_prefix("crate::") {
        return path.to_string();
    }
    if imported == "self" {
        return current.to_string();
    }
    if let Some(path) = imported.strip_prefix("self::") {
        return format!("{current}::{path}");
    }
    if imported.starts_with("super") {
        let mut segments = current
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        let mut rest = imported;
        while let Some(next) = rest.strip_prefix("super::") {
            segments.pop();
            rest = next;
        }
        if rest == "super" {
            segments.pop();
            rest = "";
        }
        if !rest.is_empty() {
            segments.extend(rest.split("::"));
        }
        return segments.join("::");
    }
    imported.to_string()
}

/// Two files share a module if any non-leaf segment of the owner's
/// module path appears as a prefix of the test's module path. The leaf
/// stem is excluded so this does not duplicate `same_test_file`.
pub(super) fn same_module(owner_module: &str, test_module: &str) -> bool {
    let (parent, separator) = match owner_module.rsplit_once("::") {
        Some((parent, _leaf)) => (parent, "::"),
        None => match owner_module.rsplit_once('/') {
            Some((parent, _leaf)) => (parent, "/"),
            None => return false,
        },
    };
    if parent.is_empty() {
        return false;
    }
    test_module == parent
        || test_module.starts_with(&format!("{parent}{separator}"))
        || (separator == "/" && test_module.starts_with(&format!("{}/", parent.replace('/', "_"))))
}

/// Body mentions the owner via an explicit qualified-path or `use`
/// shape — without calling it. The direct-call check has already
/// excluded callers, so this fires for tests that import the symbol
/// (or qualify it via a path) but route through some wrapper (common
/// in integration tests).
///
/// Tightened per #310 review: pure token co-occurrence
/// (owner_name appearing as a bare identifier somewhere in the body)
/// was too easy to satisfy with local bindings, comments, or
/// unrelated identifiers. The detector now requires either:
///
/// 1. a `module::owner_name` qualified path anywhere in the body
///    (catches `crate::pricing::discounted_total`,
///    `super::pricing::discounted_total`, `pricing::discounted_total`
///    — they all contain `::owner_name`); or
/// 2. an inline `use ... owner_name` line in the test body. File-
///    scope `use` lines are not in `test.body` so this only covers
///    in-function imports.
pub(super) fn test_imports_owner_compact(test: &CompactTest<'_>, owner_name: &str) -> bool {
    if owner_name.is_empty() {
        return false;
    }
    let qualified = format!("::{owner_name}");
    for code in &test.code_lines {
        if code.contains(&qualified) {
            return true;
        }
        if code.trim_start().starts_with("use ")
            && extract_identifier_tokens(code)
                .iter()
                .any(|token| token == owner_name)
        {
            return true;
        }
    }
    false
}

fn test_imports_owner_module_compact(
    test: &CompactTest<'_>,
    owner_module: &str,
    owner_name: &str,
) -> bool {
    let normalized = owner_module.replace('/', "::");
    let target = if normalized.is_empty() {
        format!("::{owner_name}")
    } else {
        format!("::{normalized}::{owner_name}")
    };
    test.code_lines.iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("use ")
            && (trimmed.contains(&target)
                || (normalized.is_empty() && trimmed.contains(&format!("crate::{owner_name}"))))
    })
}

pub(super) fn import_affinity_tokens(code_lines: &[String]) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    for code in code_lines {
        let trimmed = code.trim_start();
        if code.contains("::") || trimmed.starts_with("use ") {
            tokens.extend(extract_identifier_tokens(code));
        }
    }
    tokens
}

/// Drop everything after a `//` line comment and replace string-literal
/// contents with empty strings. v1 best-effort: handles `"..."` with
/// `\\` and `\"` escapes; raw strings (`r#"..."#`), char literals
/// (`'a'`), and block comments (`/* ... */`) are out of scope — those
/// shapes are rare inside test bodies and treating them as code is a
/// safe over-match (the previous helper accepted them all).
pub(super) fn strip_comments_and_strings(line: &str) -> String {
    // Strip `//` line comments first; everything after is non-code.
    let without_comment = match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    };
    let mut out = String::with_capacity(without_comment.len());
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;
    let mut chars = without_comment.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string || in_char {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' if in_string => in_string = false,
                '\'' if in_char => in_char = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '\'' => {
                let mut lookahead = chars.clone();
                let is_char_literal = match lookahead.next() {
                    Some('\\') => {
                        if lookahead.next() == Some('u') {
                            if lookahead.next() == Some('{') {
                                for next in lookahead.by_ref() {
                                    if next == '}' {
                                        break;
                                    }
                                }
                            }
                        } else {
                            lookahead.next();
                        }
                        lookahead.next() == Some('\'')
                    }
                    Some(_) => lookahead.next() == Some('\''),
                    None => false,
                };
                if is_char_literal {
                    in_char = true;
                } else {
                    out.push(ch);
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn is_fixture_named(name: &str) -> bool {
    let prefixes = ["fixture_", "setup_", "make_", "build_", "new_", "mock_"];
    let suffixes = ["_fixture", "_factory"];
    prefixes.iter().any(|p| name.starts_with(p)) || suffixes.iter().any(|s| name.ends_with(s))
}

pub(super) fn find_owner_function<'a>(
    seam: &RepoSeam,
    index: &'a RustIndex,
) -> Option<&'a FunctionSummary> {
    rust_index::find_owner_function(index, seam.file(), seam.display_line())
}

pub(super) fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

pub(super) fn package_prefix(path: &Path) -> Option<String> {
    let normalized = normalize_path(path);
    if let Some(rest) = normalized.strip_prefix("crates/")
        && let Some((crate_name, crate_relative)) = rest.split_once('/')
        && (crate_relative.starts_with("src/") || crate_relative.starts_with("tests/"))
    {
        return Some(format!("crates/{crate_name}/"));
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

pub(super) fn package_scope(path: &Path) -> Option<String> {
    let normalized = normalize_path(path);
    if let Some(prefix) = package_prefix(path) {
        return Some(prefix);
    }
    if normalized.starts_with("src/") || normalized.starts_with("tests/") {
        return Some(String::new());
    }
    None
}

/// Return the Rust crate name implied by a workspace package path.
///
/// A one-segment `use package::item` path is only a crate-root identity when
/// `package` matches the owning package. Synthetic `src/` paths intentionally
/// return `None` because they do not carry package identity and must fail
/// closed.
pub(super) fn crate_name_for_path(path: &Path) -> Option<String> {
    let normalized = normalize_path(path);
    for marker in ["/src/", "/tests/"] {
        let Some(index) = normalized.rfind(marker) else {
            continue;
        };
        // Real fixture/workspace paths can carry a package name that differs
        // from the directory containing `src/` (for example a fixture whose
        // Cargo package is `boundary_gap_fixture` under an `input/` folder).
        // Use the manifest as the authoritative crate-root identity whenever
        // the path is materialized. Relative in-memory fixtures deliberately
        // retain the fail-closed path-only behavior below.
        if path.is_absolute()
            && let Some(name) = cargo_package_name(path)
        {
            return Some(name);
        }
        let package_root = &normalized[..index];
        let name = package_root.rsplit('/').next()?;
        if !name.is_empty() {
            return Some(name.replace('-', "_"));
        }
    }
    None
}

fn cargo_package_name(path: &Path) -> Option<String> {
    let mut directory = path.parent();
    while let Some(current) = directory {
        let manifest = current.join("Cargo.toml");
        if let Ok(source) = std::fs::read_to_string(manifest) {
            let mut in_package = false;
            for raw in source.lines() {
                let line = raw.trim();
                if line == "[package]" {
                    in_package = true;
                    continue;
                }
                if line.starts_with('[') {
                    in_package = false;
                }
                if !in_package {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if key.trim() != "name" {
                    continue;
                }
                let name = value.trim().trim_matches('"');
                if !name.is_empty() {
                    return Some(name.replace('-', "_"));
                }
            }
            return None;
        }
        directory = current.parent();
    }
    None
}
