use super::super::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::domain::{
    CALLEE_ONLY_REACH_PREFIX, Confidence, RelationReason, StageEvidence, StageState,
};
use ra_ap_syntax::{AstNode, ast};
use std::collections::HashSet;

pub(in crate::analysis) fn reach_evidence(
    related_tests: &[(&TestSummary, RelationReason)],
    owner_fn: Option<&FunctionSummary>,
    owner_may_be_reached_unseen: impl FnOnce() -> bool,
) -> StageEvidence {
    if related_tests.is_empty() {
        return StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            "No static test path found for the changed owner",
        );
    }
    // #3714 round-2 review (devin hDRL2): a `SeamCalleeCall` relation means
    // the test exercises the seam's converted callee — it never invokes the
    // changed owner or its conversion, so the reach summary must not claim
    // owner reach for it. Owner-anchored relations keep the established
    // phrasing; callee-only relations carry their own honest summary (the
    // exposure class stays `weakly_exposed` — the conversion's variant
    // binding remains the typed `wrapper_error_binding_unresolved`
    // limitation per #3700).
    let target = owner_fn.map(|f| f.name.as_str()).unwrap_or("changed owner");
    let owner_anchored: Vec<&TestSummary> = related_tests
        .iter()
        .filter(|(_, reason)| *reason != RelationReason::SeamCalleeCall)
        .map(|(test, _)| *test)
        .collect();
    let callee_only: Vec<&TestSummary> = related_tests
        .iter()
        .filter(|(_, reason)| *reason == RelationReason::SeamCalleeCall)
        .map(|(test, _)| *test)
        .collect();
    // A test that only shares the changed file or a name token with the
    // owner is a suggested location, not evidence that it runs the owner.
    // Treating it as reach let an uncalled function inherit a neighbour's
    // strong assertion and report `exposed`, and treating it as weak reach
    // reported `weakly_exposed` for a function no test calls. Reach is `No`
    // only when nothing could be calling the owner unseen: no source text
    // outside its own definition names it (see
    // `owner_may_be_reached_unseen`) and no proximity test invokes a
    // non-assertion macro (whose expansion may call it).
    let owner_is_function = owner_fn.is_some();
    let proximity_only = !owner_anchored.is_empty()
        && related_tests
            .iter()
            .filter(|(_, reason)| *reason != RelationReason::SeamCalleeCall)
            .all(|(_, reason)| !relation_establishes_reach(*reason, owner_is_function));
    if proximity_only {
        let names = owner_anchored
            .iter()
            .take(3)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if owner_anchored
            .iter()
            .any(|test| invokes_opaque_macro(&test.body))
            || owner_may_be_reached_unseen()
        {
            return StageEvidence::new(
                StageState::Weak,
                Confidence::Low,
                format!(
                    "No test is seen calling {target}; tests share only its file or a name token: {names}"
                ),
            );
        }
        return StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            format!(
                "No test is seen calling {target}; tests share only its file or a name token: {names}"
            ),
        );
    }
    // #7003: callee-only relations never invoke the changed owner — the
    // callee summary says so itself — so reporting `Yes` contradicted the
    // evidence in one struct and every `reach.state` consumer inherited the
    // lie (the all-no-path honesty note stayed suppressed, hints claimed a
    // reaching test). `Weak`/`Low` matches the proximity-only branch, which
    // faces the same "no test is seen calling the owner" situation. `No`
    // would be stronger than the evidence: it would fire `reach_ruled_out`
    // (rewriting downstream `Yes` stages to unreached) and demote the class
    // to `no_static_path`, while the tests do exercise the seam's callee.
    if owner_anchored.is_empty() {
        let names = callee_only
            .iter()
            .take(3)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return StageEvidence::new(
            StageState::Weak,
            Confidence::Low,
            format!("{CALLEE_ONLY_REACH_PREFIX}: {names}"),
        );
    }
    let names = owner_anchored
        .iter()
        .take(3)
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    StageEvidence::new(
        StageState::Yes,
        Confidence::Medium,
        format!("Related tests appear to reach {target}: {names}"),
    )
}

/// Whether a test could run `owner` without any indexed test body calling it
/// by name.
///
/// A trait-impl method runs through operators, formatting macros, `Deref`
/// and generic dispatch that never spell its name (`Money(5).to_string()`
/// calls `Display::fmt`). Any other function is reachable only through
/// something that names it: a caller, a function pointer (`.map(owner)`), a
/// `use .. as` alias, a doctest (including a README pulled in with
/// `#![doc = include_str!(..)]`), or a macro block such as `proptest!`. So
/// when no workspace source names the owner outside its own `fn` line,
/// nothing can reach it; when anything does, reach stays undecided.
pub(in crate::analysis) fn owner_may_be_reached_unseen(
    owner: &FunctionSummary,
    index: &RustIndex,
) -> bool {
    is_trait_impl_method(owner)
        || index.files().values().any(|file| {
            names_identifier_outside_fn_definition(&file.source, &owner.name)
                || includes_external_docs(&file.source)
        })
}

/// [`owner_may_be_reached_unseen`] answered from one scan of the workspace,
/// for callers that ask about many owners (repo seam evidence asks once per
/// owner whose related tests are all proximity).
#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct UnseenReachNames {
    /// Every identifier some source names other than directly after `fn`.
    named: HashSet<String>,
    /// Some file pulls external docs in as doctests.
    external_docs: bool,
}

impl UnseenReachNames {
    pub(in crate::analysis) fn build(index: &RustIndex) -> Self {
        use rayon::prelude::*;
        let sources: Vec<&str> = index
            .files()
            .values()
            .map(|file| file.data().source.as_ref())
            .collect();
        // The external-docs parse is most of this scan and every file is
        // independent, so the parses run on the rayon pool (`build` only runs
        // on a caller's own thread, never from inside a classification
        // worker, where `memoize_many` computes in line).
        let external_docs = includes_external_docs_many(&sources)
            .into_iter()
            .any(|includes| includes);
        fn collect_names<'a>(mut names: HashSet<&'a str>, source: &'a str) -> HashSet<&'a str> {
            names.extend(identifiers_outside_fn_definition(source));
            names
        }
        fn merge_names<'a>(
            mut left: HashSet<&'a str>,
            mut right: HashSet<&'a str>,
        ) -> HashSet<&'a str> {
            if left.len() < right.len() {
                std::mem::swap(&mut left, &mut right);
            }
            left.extend(right);
            left
        }
        let named: HashSet<&str> = if rayon::current_thread_index().is_none() {
            sources
                .par_iter()
                .fold(HashSet::new, |names, source| collect_names(names, source))
                .reduce(HashSet::new, merge_names)
        } else {
            sources
                .iter()
                .fold(HashSet::new(), |names, source| collect_names(names, source))
        };
        Self {
            named: named.into_iter().map(str::to_string).collect(),
            external_docs,
        }
    }

    pub(in crate::analysis) fn may_reach_unseen(&self, owner: &FunctionSummary) -> bool {
        is_trait_impl_method(owner)
            || self.external_docs
            || (!owner.name.is_empty() && self.named.contains(&owner.name))
    }
}

#[cfg(test)]
thread_local! {
    static EXTERNAL_DOCS_PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(in crate::analysis) fn take_external_docs_parse_count() -> usize {
    EXTERNAL_DOCS_PARSE_COUNT.with(|count| count.replace(0))
}

/// Macro-derived documentation can produce unindexed doctest calls, including
/// external files and names assembled by `concat!` or supplied by `env!`.
pub(in crate::analysis) fn includes_external_docs(source: &str) -> bool {
    if !may_include_external_docs(source) {
        return false;
    }
    // Pure in the source text, and rescanned for every overlapping index a
    // diff run builds (#5363), so a run scope computes each text once.
    crate::analysis::syntax::run_memo::memoize(
        crate::analysis::syntax::run_memo::Product::ExternalDocs,
        0,
        source,
        || {
            #[cfg(test)]
            EXTERNAL_DOCS_PARSE_COUNT.with(|count| count.set(count.get() + 1));
            external_docs_uncached(source)
        },
    )
}

/// The cheap text gate in front of the parse: a source that never spells
/// `doc` and `!` cannot include external docs.
fn may_include_external_docs(source: &str) -> bool {
    source.contains("doc") && source.contains('!')
}

/// [`includes_external_docs`] for every source of a scan, in order. The
/// parses of the texts the run scope has not seen run on the rayon pool
/// (see [`crate::analysis::syntax::run_memo::memoize_many`]); the answers and
/// the memo's contents are those of calling the single form per source.
pub(in crate::analysis) fn includes_external_docs_many(sources: &[&str]) -> Vec<bool> {
    let candidates: Vec<&str> = sources
        .iter()
        .copied()
        .filter(|source| may_include_external_docs(source))
        .collect();
    let parsed = std::sync::atomic::AtomicUsize::new(0);
    let mut answers = crate::analysis::syntax::run_memo::memoize_many(
        crate::analysis::syntax::run_memo::Product::ExternalDocs,
        &candidates,
        |source| {
            parsed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            external_docs_uncached(source)
        },
    )
    .into_iter();
    // Parses ran on workers, so the owning thread's test counter is credited
    // here, where the single form would have counted them.
    #[cfg(test)]
    EXTERNAL_DOCS_PARSE_COUNT
        .with(|count| count.set(count.get() + parsed.load(std::sync::atomic::Ordering::Relaxed)));
    sources
        .iter()
        .map(|source| may_include_external_docs(source) && answers.next().unwrap_or(true))
        .collect()
}

fn external_docs_uncached(source: &str) -> bool {
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
        return true;
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Attr::cast)
        .any(|attribute| {
            attribute.meta().is_some_and(|meta| {
                meta.skip_cfg_attrs().into_iter().any(|meta| {
                    meta.simple_name().is_some_and(|name| name == "doc")
                        && matches!(meta, ast::Meta::KeyValueMeta(value)
                        if !matches!(value.expr(), Some(ast::Expr::Literal(_))))
                })
            })
        })
}

/// Parser-backed owner ids carry their impl segment
/// (`src/lib.rs::impl Display for Money::fmt`).
pub(in crate::analysis) fn is_trait_impl_method(owner: &FunctionSummary) -> bool {
    owner
        .id
        .0
        .split("::impl ")
        .skip(1)
        .any(|segment| segment.contains(" for "))
}

/// Whether `source` contains `name` as a whole identifier anywhere other than
/// directly after the `fn` keyword. Comments and strings count: a doctest or
/// a stringly dispatched call may be what reaches the owner.
pub(in crate::analysis) fn names_identifier_outside_fn_definition(
    source: &str,
    name: &str,
) -> bool {
    !name.is_empty() && identifiers_outside_fn_definition(source).any(|ident| ident == name)
}

/// Each whole identifier in `source` (a Unicode alphanumeric or `_` run, so
/// a non-ASCII owner name such as `café` is one identifier, not an ASCII
/// fragment) that does not sit directly after the `fn` keyword.
fn identifiers_outside_fn_definition(source: &str) -> impl Iterator<Item = &str> {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = 0;
    std::iter::from_fn(move || {
        while start < source.len() {
            let Some(first) = source[start..].chars().next() else {
                break;
            };
            if !is_ident(first) {
                start += first.len_utf8();
                continue;
            }
            let begin = start;
            while let Some(next) = source[start..].chars().next() {
                if !is_ident(next) {
                    break;
                }
                start += next.len_utf8();
            }
            let before = source[..begin]
                .strip_suffix("r#")
                .unwrap_or(&source[..begin]);
            let is_fn_definition = before
                .trim_end()
                .strip_suffix("fn")
                .is_some_and(|prefix| !prefix.chars().last().is_some_and(is_ident));
            if !is_fn_definition {
                return Some(&source[begin..start]);
            }
        }
        None
    })
}

/// Macros whose expansion never calls a user function by itself; any other
/// macro invocation in a test body may hide a call to the changed owner.
const TRANSPARENT_TEST_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "assert_matches",
    "matches",
    "vec",
    "format",
    "print",
    "println",
    "eprint",
    "eprintln",
    "write",
    "writeln",
    "panic",
    "todo",
    "unimplemented",
    "unreachable",
    "dbg",
    "concat",
    "stringify",
    "include_str",
    "env",
];

/// True when `body` invokes a macro (`name!(`, `name![`, `name! {`) outside
/// [`TRANSPARENT_TEST_MACROS`]. Text inside string literals counts too, which
/// only keeps reach uncertain.
pub(in crate::analysis) fn invokes_opaque_macro(body: &str) -> bool {
    let bytes = body.as_bytes();
    bytes.iter().enumerate().any(|(bang, byte)| {
        if *byte != b'!' {
            return false;
        }
        let opener = bytes[bang + 1..]
            .iter()
            .find(|next| !next.is_ascii_whitespace());
        if !matches!(opener, Some(b'(' | b'[' | b'{')) {
            return false;
        }
        // Step past the whole separator: `→assert!(` must not slice inside
        // the arrow.
        let start = body[..bang]
            .char_indices()
            .rev()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
            .map_or(0, |(index, c)| index + c.len_utf8());
        let name = &body[start..bang];
        !name.is_empty() && !TRANSPARENT_TEST_MACROS.contains(&name)
    })
}

/// Whether a relation by itself is evidence that the test runs a seam's
/// owner, shared by diff-mode reach and repo-mode seam reach (#5335). The
/// rule lives on [`RelationReason::establishes_reach`] so the classifier's
/// miss reasons and the verdict corpus read the same answer (#7531).
pub(in crate::analysis) fn relation_establishes_reach(
    reason: RelationReason,
    owner_is_function: bool,
) -> bool {
    reason.establishes_reach(owner_is_function)
}

/// Relations that come from file or name proximity alone
/// ([`RelationReason::is_proximity_only`]).
pub(in crate::analysis) fn is_proximity_only(reason: RelationReason) -> bool {
    reason.is_proximity_only()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::{FileFacts, FunctionSourceRole, OwnedRustIndex};
    use crate::domain::SymbolId;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn given_no_related_tests_when_building_reach_evidence_then_stage_is_no() {
        let evidence = reach_evidence(&[], None, || false);

        assert_eq!(evidence.state, StageState::No);
        assert_eq!(evidence.confidence, Confidence::Medium);
        assert_eq!(
            evidence.summary,
            "No static test path found for the changed owner"
        );
    }

    #[test]
    fn given_related_tests_when_building_reach_evidence_then_names_owner_and_tests() {
        let owner = function("discounted_total");
        let first = test("below_threshold");
        let second = test("at_threshold");
        let third = test("above_threshold");
        let fourth = test("large_amount");
        let related = vec![
            (&first, RelationReason::DirectOwnerCall),
            (&second, RelationReason::DirectOwnerCall),
            (&third, RelationReason::DirectOwnerCall),
            (&fourth, RelationReason::DirectOwnerCall),
        ];

        let evidence = reach_evidence(&related, Some(&owner), || false);

        assert_eq!(evidence.state, StageState::Yes);
        assert_eq!(evidence.confidence, Confidence::Medium);
        assert_eq!(
            evidence.summary,
            "Related tests appear to reach discounted_total: below_threshold, at_threshold, above_threshold"
        );
    }

    #[test]
    fn given_only_proximity_relations_when_building_reach_evidence_then_reach_is_no() {
        let owner = function("untested_rounding");
        let neighbour = test("discount_applies_at_100");
        let token_match = test("rounding_table_loads");
        let module_peer = test("module_smoke");
        let related = vec![
            (&neighbour, RelationReason::SameTestFile),
            (&token_match, RelationReason::WeakTokenSubstring),
            (&module_peer, RelationReason::SameModule),
        ];

        let evidence = reach_evidence(&related, Some(&owner), || false);

        assert_eq!(evidence.state, StageState::No);
        assert_eq!(evidence.confidence, Confidence::Medium);
        assert_eq!(
            evidence.summary,
            "No test is seen calling untested_rounding; tests share only its file or a name token: discount_applies_at_100, rounding_table_loads, module_smoke"
        );
    }

    #[test]
    fn given_proximity_test_invoking_a_custom_macro_when_building_reach_evidence_then_reach_stays_weak()
     {
        let owner = function("tax_total");
        let mut macro_caller = test("vat_boundary_is_checked_by_macro");
        macro_caller.body = "assert_eq!(macro_tax_case!(100), 120);".into();
        let related = vec![(&macro_caller, RelationReason::WeakTokenSubstring)];

        let evidence = reach_evidence(&related, Some(&owner), || false);

        assert_eq!(evidence.state, StageState::Weak);
        assert_eq!(evidence.confidence, Confidence::Low);
    }

    #[test]
    fn given_only_proximity_relations_for_an_owner_named_elsewhere_then_reach_stays_weak() {
        let owner = function("check_score_invariants");
        let neighbour = test("validate_score_accepts_in_range_values");
        let related = vec![(&neighbour, RelationReason::WeakTokenSubstring)];

        let evidence = reach_evidence(&related, Some(&owner), || true);

        assert_eq!(evidence.state, StageState::Weak);
        assert_eq!(evidence.confidence, Confidence::Low);
    }

    #[test]
    fn owner_name_counts_as_referenced_anywhere_but_its_fn_line() {
        let own = "pub fn fragile_fee(w: u32) -> u32 {\n    w\n}\n";
        assert!(!names_identifier_outside_fn_definition(own, "fragile_fee"));
        assert!(!names_identifier_outside_fn_definition(
            "fn r#fragile_fee() {}\nfn fragile_fees() {}\nlet x = my_fragile_fee;",
            "fragile_fee"
        ));
        for referencing in [
            "quote(fragile_fee(1))",
            "v.map(fragile_fee)",
            "pub use crate::fragile_fee as fee;",
            "/// assert_eq!(fragile_fee(1), 0);",
            "proptest! { fn p(w in 0u32..) { fragile_fee(w); } }",
            "let f = crate::fragile_fee;",
        ] {
            assert!(
                names_identifier_outside_fn_definition(
                    &format!("{own}{referencing}"),
                    "fragile_fee"
                ),
                "{referencing}"
            );
        }
        // `fn` must be the keyword, not the tail of another identifier.
        assert!(names_identifier_outside_fn_definition(
            "let cfn fragile_fee",
            "fragile_fee"
        ));
    }

    #[test]
    fn unseen_reach_names_parses_each_file_once_not_per_owner() {
        let mut files = BTreeMap::new();
        let mut functions = Vec::new();
        for file_i in 0..4 {
            let path = PathBuf::from(format!("src/mod{file_i}.rs"));
            // Three files parse `doc`+`!` but are not external docs, so the
            // uncached `.any()` walk cannot short-circuit before the last file.
            let mut source = if file_i == 3 {
                String::from("#![doc = include_str!(\"README.md\")]\n")
            } else {
                String::from("#[doc = \"include_str! is described here\"]\n")
            };
            for fn_i in 0..10 {
                let name = format!("owner_{file_i}_{fn_i}");
                source.push_str(&format!("pub fn {name}() {{}}\n"));
                let mut owner = function(&name);
                owner.file = path.clone();
                functions.push(owner);
            }
            files.insert(
                path.clone(),
                FileFacts {
                    path,
                    source: source.into(),
                    ..FileFacts::default()
                },
            );
        }
        let index = RustIndex::from_owned(OwnedRustIndex {
            files,
            functions: functions.clone(),
            ..Default::default()
        });
        let _ = take_external_docs_parse_count();
        let names = UnseenReachNames::build(&index);
        assert_eq!(
            take_external_docs_parse_count(),
            4,
            "workspace unseen-reach must parse each external-doc file once"
        );
        for _ in 0..5 {
            for owner in &functions {
                assert!(
                    names.may_reach_unseen(owner),
                    "external docs keep every owner possibly unseen"
                );
            }
        }
        assert_eq!(
            take_external_docs_parse_count(),
            0,
            "cached may_reach_unseen must not reparse"
        );
        let _ = take_external_docs_parse_count();
        for owner in &functions {
            assert!(owner_may_be_reached_unseen(owner, &index));
        }
        assert_eq!(
            take_external_docs_parse_count(),
            4 * functions.len(),
            "the uncached per-owner path still parses every file for every owner"
        );
    }

    #[test]
    fn unseen_reach_names_agrees_with_per_owner_scan() {
        let named = "pub fn fragile_fee(w: u32) -> u32 { w }\nfn helper() { fragile_fee(1); }\n";
        let silent = "pub fn lonely() {}\n";
        let docs = "#![doc = include_str!(\"README.md\")]\npub fn unused() {}\n";
        let file = |path: &str, source: &str| {
            (
                PathBuf::from(path),
                FileFacts {
                    path: PathBuf::from(path),
                    source: source.into(),
                    ..FileFacts::default()
                },
            )
        };
        let index_for = |files, functions| {
            RustIndex::from_owned(OwnedRustIndex {
                files,
                functions,
                ..Default::default()
            })
        };
        let named_index = index_for(
            BTreeMap::from([file("src/named.rs", named), file("src/silent.rs", silent)]),
            vec![function("fragile_fee"), function("lonely")],
        );
        let docs_index = index_for(
            BTreeMap::from([file("src/docs.rs", docs)]),
            vec![function("unused"), function("lonely")],
        );
        let mut trait_owner = function("fmt");
        trait_owner.id = SymbolId("src/lib.rs::impl Display for Money::fmt".to_string());
        let trait_index = index_for(
            BTreeMap::from([file("src/silent.rs", silent)]),
            vec![trait_owner],
        );
        for index in [&named_index, &docs_index, &trait_index] {
            let names = UnseenReachNames::build(index);
            for owner in index.functions().iter() {
                assert_eq!(
                    names.may_reach_unseen(owner),
                    owner_may_be_reached_unseen(owner, index),
                    "{}",
                    owner.name
                );
            }
        }
        let names = UnseenReachNames::build(&named_index);
        assert!(names.may_reach_unseen(&function("fragile_fee")));
        assert!(!names.may_reach_unseen(&function("lonely")));
        assert!(UnseenReachNames::build(&docs_index).may_reach_unseen(&function("lonely")));
    }

    #[test]
    fn includes_external_docs_many_answers_like_the_single_form() {
        let sources = [
            "pub fn plain() {}",
            "#![doc = include_str!(\"README.md\")]\npub fn a() {}",
            "/// docs! mention the words without an attribute\nfn b() {}",
            "#![doc = \"literal\"] fn c() { assert!(true) }",
            "fn broken( { doc !",
            "#![doc = include_str!(\"README.md\")]\npub fn a() {}",
        ];
        let expected: Vec<bool> = sources
            .iter()
            .map(|source| includes_external_docs(source))
            .collect();
        assert_eq!(expected, vec![false, true, false, false, true, true]);
        let _ = take_external_docs_parse_count();
        assert_eq!(includes_external_docs_many(&sources), expected);
        assert_eq!(
            take_external_docs_parse_count(),
            5,
            "without a scope every source that passes the text gate parses"
        );
        let _scope = crate::analysis::syntax::run_memo::enter();
        assert_eq!(includes_external_docs_many(&sources), expected);
        assert_eq!(
            take_external_docs_parse_count(),
            4,
            "inside a scope a repeated text parses once"
        );
        assert_eq!(includes_external_docs_many(&sources), expected);
        assert_eq!(
            take_external_docs_parse_count(),
            0,
            "a second batch is served from the scope"
        );
        assert!(includes_external_docs(sources[1]));
        assert_eq!(
            take_external_docs_parse_count(),
            0,
            "the single form hits too"
        );
    }

    #[test]
    fn included_doc_files_count_as_unseen_callers() {
        for (source, expected) in [
            (
                "#![doc = include_str!(\"../README.md\")]\npub fn a() {}",
                true,
            ),
            ("#[doc = include_str!(\"x.md\")] fn a() {}", true),
            ("#![doc =\n include_str ! (\"x.md\")\n]\nfn a() {}", true),
            (
                "#![doc = concat!(\"mean_\", \"latency(&[])\")] fn a() {}",
                true,
            ),
            ("#![doc = env!(\"DOCS\")] fn a() {}", true),
            (
                "const T: &str = include_str!(\"t.txt\");\n#[doc = \"x\"] fn a() {}",
                false,
            ),
            (
                "#[doc = \"x\"] const T: &str = include_str!(\"t.txt\");",
                false,
            ),
            (
                "#[doc = \"include_str! is described here\"] fn a() {}",
                false,
            ),
        ] {
            assert!(
                crate::analysis::syntax::parse_clean_source_file(source).is_some(),
                "{source}"
            );
            assert_eq!(includes_external_docs(source), expected, "{source}");
        }
        let malformed = "const T: &str = include_str!(\"t.txt\");\n#[doc = \"x\"]";
        assert!(crate::analysis::syntax::parse_clean_source_file(malformed).is_none());
        assert!(includes_external_docs(malformed));
    }

    #[test]
    fn trait_impl_methods_are_recognized_from_the_owner_id() {
        let mut owner = function("fmt");
        owner.id = SymbolId("src/lib.rs::impl Display for Money::fmt".to_string());
        assert!(is_trait_impl_method(&owner));
        owner.id = SymbolId("src/lib.rs::impl Money::fmt".to_string());
        assert!(!is_trait_impl_method(&owner));
        owner.id = SymbolId("src/lib.rs::fmt".to_string());
        assert!(!is_trait_impl_method(&owner));
    }

    #[test]
    fn opaque_macro_detection_ignores_assertion_macros_and_operators() {
        assert!(!invokes_opaque_macro(
            "assert_eq!(tax_bps(\"EU\"), 2000); assert!(a != b); let v = vec![1];"
        ));
        assert!(!invokes_opaque_macro("assert!(!flag);"));
        assert!(invokes_opaque_macro("assert_eq!(case!(1), 2);"));
        assert!(invokes_opaque_macro("proptest! { }"));
        assert!(invokes_opaque_macro("check_all![1, 2];"));
        // A multi-byte character before the name must not split a slice.
        assert!(!invokes_opaque_macro("x→assert!(ok);"));
        assert!(invokes_opaque_macro("x→case!(1);"));
    }

    #[test]
    fn given_one_calling_test_among_proximity_relations_when_building_reach_evidence_then_reach_is_yes()
     {
        let owner = function("discount");
        let neighbour = test("tags_nonempty");
        let caller = test("discount_applies_at_100");
        let related = vec![
            (&neighbour, RelationReason::SameTestFile),
            (&caller, RelationReason::DirectOwnerCall),
        ];

        let evidence = reach_evidence(&related, Some(&owner), || false);

        assert_eq!(evidence.state, StageState::Yes);
    }

    // #3714 round-2 review (devin hDRL2): callee-only relations must not
    // claim owner reach in the summary. #7003: the state must agree with
    // that summary — `Weak`/`Low`, never `Yes`.
    #[test]
    fn given_callee_only_relations_when_building_reach_evidence_then_summary_names_callee_affinity()
    {
        let owner = function("parse_summary");
        let first = test("observes_callee_outcome");
        let second = test("other_callee_probe");
        let related = vec![
            (&first, RelationReason::SeamCalleeCall),
            (&second, RelationReason::SeamCalleeCall),
        ];

        let evidence = reach_evidence(&related, Some(&owner), || false);

        assert_eq!(evidence.state, StageState::Weak);
        assert_eq!(evidence.confidence, Confidence::Low);
        assert_eq!(
            evidence.summary,
            "Related tests exercise the wrapper seam's converted callee (the changed owner is not invoked by them): observes_callee_outcome, other_callee_probe"
        );
    }

    // #3714 round-2 review (devin hDRL2): mixed relations keep the
    // established owner-reach phrasing for the owner-anchored tests.
    #[test]
    fn given_mixed_relations_when_building_reach_evidence_then_owner_reach_is_named() {
        let owner = function("parse_summary");
        let callee_only = test("observes_callee_outcome");
        let anchored = test("parse_summary_fails_closed");
        let related = vec![
            (&callee_only, RelationReason::SeamCalleeCall),
            (&anchored, RelationReason::DirectOwnerCall),
        ];

        let evidence = reach_evidence(&related, Some(&owner), || false);

        // #7003 negative: one owner-anchored relation still reports `Yes`.
        assert_eq!(evidence.state, StageState::Yes);
        assert_eq!(
            evidence.summary,
            "Related tests appear to reach parse_summary: parse_summary_fails_closed"
        );
    }

    // #5335 review (coderabbitai, devin): a relation that only names the
    // owner — in the test's name, an import path, or a fixture-shaped
    // helper call — is a suggested location, not owner reach. The reach
    // question falls to the unseen-path check, so an uncalled owner stays
    // `No` instead of inheriting a sibling's oracle.
    #[test]
    fn given_only_name_import_or_fixture_affinity_when_building_reach_evidence_then_reach_is_not_yes()
     {
        let owner = function("discounted_total");
        let named = test("discounted_total_smoke");
        let imported = test("imports_pricing_api");
        let fixtured = test("pricing_fixture_roundtrip");
        for (name, reason) in [
            ("owner_named_test", RelationReason::OwnerNamedTest),
            ("import_path_affinity", RelationReason::ImportPathAffinity),
            (
                "fixture_owner_affinity",
                RelationReason::FixtureOwnerAffinity,
            ),
        ] {
            assert!(
                !relation_establishes_reach(reason, true),
                "{name} must not establish reach for a function owner"
            );
            let evidence = reach_evidence(&[(&named, reason)], Some(&owner), || false);
            assert_eq!(evidence.state, StageState::No, "{name}");
            // A caller that names the owner somewhere unseen keeps it weak.
            let evidence = reach_evidence(&[(&imported, reason)], Some(&owner), || true);
            assert_eq!(evidence.state, StageState::Weak, "{name}");
            assert_eq!(
                reach_evidence(&[(&fixtured, reason)], Some(&owner), || false).state,
                StageState::No,
                "{name}"
            );
        }
    }

    #[test]
    fn unicode_owner_names_are_whole_identifiers_in_the_unseen_scan() {
        // `café` called from a helper: the scan must see the whole name, so
        // a proximity-only seam over `café` stays weak instead of claiming
        // a confirmed gap (#5335 review, devin).
        let source = "fn café() -> u32 { 2 } fn helper() { café(); }";
        assert!(names_identifier_outside_fn_definition(source, "café"));
        assert!(!names_identifier_outside_fn_definition(
            "fn café() -> u32 { 2 }",
            "café"
        ));
        let identifiers: Vec<&str> =
            identifiers_outside_fn_definition("let café_count = café(1);").collect();
        assert_eq!(identifiers, vec!["let", "café_count", "café", "1"]);
    }

    #[test]
    fn relation_establishes_reach_admits_only_owner_execution_paths() {
        for reaching in [
            RelationReason::DirectOwnerCall,
            RelationReason::HelperOwnerCall,
            RelationReason::ReExportChainFollowed,
            RelationReason::AssertionTargetAffinity,
        ] {
            assert!(
                relation_establishes_reach(reaching, false),
                "{reaching:?} reaches a module-level owner"
            );
        }
        for not_reaching in [
            RelationReason::AssertionTargetAffinity,
            RelationReason::SameTestFile,
            RelationReason::SameModule,
            RelationReason::OwnerNamedTest,
            RelationReason::ImportPathAffinity,
            RelationReason::FixtureOwnerAffinity,
            RelationReason::WeakTokenSubstring,
            RelationReason::SeamCalleeCall,
        ] {
            assert!(
                !relation_establishes_reach(not_reaching, true),
                "{not_reaching:?} must not reach a function owner"
            );
        }
        // Proximity and callee-only relations never reach a module-level
        // owner either: no function body runs.
        for not_reaching in [
            RelationReason::SameTestFile,
            RelationReason::SameModule,
            RelationReason::OwnerNamedTest,
            RelationReason::ImportPathAffinity,
            RelationReason::FixtureOwnerAffinity,
            RelationReason::WeakTokenSubstring,
            RelationReason::SeamCalleeCall,
        ] {
            assert!(
                !relation_establishes_reach(not_reaching, false),
                "{not_reaching:?} must not reach a module-level owner"
            );
        }
    }

    fn function(name: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: String::new().into(),
            calls: Vec::new(),
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

    fn test(name: &str) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/pricing.rs"),
            start_line: 1,
            end_line: 3,
            body: String::new().into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }
}
