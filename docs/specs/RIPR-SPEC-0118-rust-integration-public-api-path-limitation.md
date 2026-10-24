# RIPR-SPEC-0118: Rust Integration Public-API Path Limitation

Status: accepted

Owner: product / swarm

Created: 2026-06-20

Linked issues:

- First-run-trust / P3 Rust reachability lane: distinguish integration-test
  public-API reach limits from generic transitive helper limits.
- #9982: a workspace facade crate that re-exports the owner's crate and carries
  the tests reads as a definite `no_static_path`; name the limit instead, and
  stop an unrelated doctest from relating through a shared name token.

Linked PRs:

- #9995 (facade re-export trigger, #9982)

Support-tier impact:

- No tier change. This spec adds one additive `static_limit_kind` value,
  `rust_integration_public_api_path_unresolved`.
- Classification stays `no_static_path`, or `weakly_exposed` when every related
  test is proximity-only (#7071, RIPR-SPEC-0117). The value names a limitation, not a
  coverage claim, related-test relation, repair packet, or public-API
  reachability proof.
- No `schema_version` bump is required because `static_limit_kind` is already
  additive optional finding metadata.
- Claim boundaries remain governed by the canonical ledger in
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- Update `docs/OUTPUT_SCHEMA.md`, `docs/STATIC_LIMITS.md`, and
  `.ripr/traceability.toml`.
- No new crates, dependencies, workflow permissions, runtime executors, macro
  expansion, rustc driver integration, or LSP-specific behavior.

## Problem

RIPR-SPEC-0114 and RIPR-SPEC-0115 made Rust transitive reach limitations visible:
when a test appears to call an entry point that may lead to the changed owner,
RIPR emits a named `no_static_path` limitation and a concrete witness. But the
same generic `rust_transitive_reach_unresolved` label covers two different
operator questions:

- a local helper-chain path inside ordinary Rust source;
- an integration test under `tests/` that reaches the crate through public API
  or a test helper that calls public API.

For first-run trust, the second case should name the adoption blocker directly:
RIPR saw an integration test and candidate public-API path, but cannot yet cross
that path with enough identity/reach/oracle proof to promote the finding.

## Behavior

### Trigger condition

After direct related-test classification returns `no_static_path` with no
related tests, and after the RIPR-SPEC-0114 bounded transitive witness check
returns a witness, the Rust adapter chooses the limitation kind from the
witnessing test path:

1. If `TransitiveWitness.test_file` is an integration test path
   (`tests/...` or contains `/tests/` after path normalization), set
   `Finding.static_limit_kind =
   Some(StaticLimitKind::RustIntegrationPublicApiPathUnresolved)`.
2. Otherwise, keep the existing generic
   `StaticLimitKind::RustTransitiveReachUnresolved`.

#### Second trigger: workspace facade re-export (#9982)

After every witness above is absent, the same limitation kind is chosen for a
workspace facade crate that re-exports the owner's crate and carries the tests,
so a test calls the owner through `facade::Item` and never spells the owner or
its crate. All of these must hold, read from manifests and source, never from
names alone:

1. The path-dependency graph is `Complete`.
2. The facade package has a forward **normal** `[dependencies]` edge to the
   owner's package (a dev-dependency edge is not visible to its library).
3. The facade's `src/lib.rs` has a crate-root, unconditional, non-macro
   `pub use` of the owner's crate (dependency name, `-` folded to `_`) that
   covers the owner: the whole crate (`inner::*`, `inner`, `inner as x`,
   `inner::{self}`), or a path that is a prefix of the owner's public path
   (its module chain, then the free function, or for a method its enclosing
   type), or a glob or `self` under such a prefix. A path that leaves the
   owner's (`inner::sub::Other`, `inner::bump` for a method of `Counter`) does
   not cover it. A `pub use` inside a module,
   function, impl or macro body, behind `#[cfg]`/`#[cfg_attr]` or any
   attribute other than `doc`, `allow`, `expect`, `deprecated`, `must_use`,
   or restricted (`pub(crate)`), does not count.
4. The owner is an unrestricted `pub` free function at module depth zero, or
   an unrestricted `pub` method of a `pub struct`/`enum`/`union` declared at
   the root of a library file, and every module on the way is declared
   `pub mod` at the root of its parent file. A private, `pub(crate)`,
   trait-method, nested or unlocatable owner, a private module or type, or an
   owner in `src/main.rs` or `src/bin/*` never fires.
5. The facade crate's name is read from its manifest (`[lib] name`, else
   `[package] name`, `-` folded to `_`), never from the directory name; an
   unreadable manifest, a non-default `[lib] path`, or a std-crate name does
   not fire.
6. An integration test file of the facade package names the facade crate as a
   path root (`facade::...`) outside comments and strings and ties to the
   owner: it globs the facade (`use facade::*;`, `use facade::{.., *}`) or
   mentions the owner's name or its enclosing type as a whole word. Naming
   only an unrelated facade item (`facade::VERSION`) does not tie. (A test
   that spells the owner's name is also a related test of it, which is a
   different finding class.)

The finding keeps `no_static_path`, pushes `transitive_reach_unresolved`, and
names the test as an inspection pointer. The re-export is not resolved and no
item is matched beyond the names in (3): the pointer says only that the test
names the facade crate and the facade's root `pub use` names the owner's crate.

#### Doctest relation guard (#9982)

A doctest is named for the item it documents and runs only that item's
example. Outside the changed owner's own file, a doctest is related to a change
only through a call to the owner, a helper-chain or seam-callee route, an
equality dispatch, or an assertion reference. Sharing only a name token with the
changed expression (`ready` from `Poll::Ready`) no longer relates it. Every
other relation is unchanged.

### When found

The existing RIPR-SPEC-0114/0115 evidence remains unchanged:

- push `StopReason::TransitiveReachUnresolved`;
- keep `classification = no_static_path`;
- keep the witness pointer in `evidence`;
- keep structured `static_limitation` detail when the edge fields are present.

Only the static limitation kind becomes more specific for integration-test
origin witnesses.

### Fail-closed boundaries

- Never promote the finding.
- Never add the witness to `related_tests`.
- Never claim the integration test reaches, covers, tests, exercises, or
  discriminates the changed owner.
- Do not infer visibility, crate exports, trait dispatch, generics, macro
  expansion, or type identity from the path kind.
- If the witness path is not recognized as an integration test path, keep the
  existing generic `rust_transitive_reach_unresolved` kind.
- The facade trigger fails closed on anything it cannot prove: a missing,
  dev-only or limited graph edge, a nested, cfg-gated, macro or non-root
  `pub use`, a list that does not name the owner, a non-`pub` owner, a manifest
  that cannot name the crate, or a facade test that does not name the facade.
  It only ever withholds a repair gap; it never adds a related test.

## Wire Format

| Field | Integration-test witness | Non-integration transitive witness |
|---|---|---|
| `classification` | `no_static_path` | `no_static_path` |
| `static_limit_kind` | `rust_integration_public_api_path_unresolved` | `rust_transitive_reach_unresolved` |
| `stop_reasons` | includes `transitive_reach_unresolved` | includes `transitive_reach_unresolved` |
| `related_tests` | unchanged; witness is not added | unchanged; witness is not added |
| `static_limitation.analyzer_route` | `analysis/rust-public-api-transitive-reach` | unchanged |

## Non-Goals

- Public-API to internal reach promotion.
- Full crate visibility or type analysis.
- Macro-aware reach.
- Repair packet emission.
- Changing verify or receipt command availability.
- Changing the pinned external checkout model.

## Acceptance Examples

1. **Integration test calls public API**: `tests/it.rs` calls `outer()`, and
   `outer()` may lead to changed owner `inner`. Result: `no_static_path` plus
   `static_limit_kind: rust_integration_public_api_path_unresolved`.
   Fixture: `fixtures/rust_transitive_reach_positive/`.
2. **Integration test helper chain**: `tests/version_req.rs` calls
   `assert_public_api_accepts`, which calls crate public API that may lead to
   changed owner `matches_greater`. Result: `no_static_path` plus
   `static_limit_kind: rust_integration_public_api_path_unresolved`.
   Fixture: `fixtures/rust_transitive_reach_test_helper_chain/`.
3. **Pinned external semver launch point**: the semver boundary patch emits
   `rust_integration_public_api_path_unresolved`, discloses the exact witness
   and limitation detail, and stays non-actionable.
4. **Facade re-export**: `facade` depends on `inner`, has `pub use inner::*;`
   at its crate root, and `facade/tests/t.rs` has `use facade::*;` (a test that
   only has `use facade::Ext;` does not tie to the owner and does not fire). A changed
   `pub fn` of `inner` reads `no_static_path` plus
   `rust_integration_public_api_path_unresolved`, with no related test and no
   repair gap. A dev-dependency-only edge, a cfg-gated or nested `pub use`, a
   braced list not naming the owner, or a private owner keeps the plain
   `no_static_path`.
5. **Non-integration helper chain**: a transitive witness from ordinary Rust
   source keeps `rust_transitive_reach_unresolved`.

## Required Evidence

- `StaticLimitKind::RustIntegrationPublicApiPathUnresolved` in
  `crates/ripr/src/domain/language.rs`.
- Rust adapter selection in `crates/ripr/src/analysis/language/rust/mod.rs`.
- Unit guard proving integration paths and non-integration paths select
  different limitation kinds.
- Pure fixture goldens for `fixtures/rust_transitive_reach_positive/` and
  `fixtures/rust_transitive_reach_test_helper_chain/`.
- Honesty corpus assertions expecting
  `rust_integration_public_api_path_unresolved` for the two pure fixtures and
  the semver pinned external case.
- Output schema, static-limit documentation, spec index, doc-artifact ledger,
  LSP gap-artifact validation, and traceability entries updated.

## Test Mapping

- `crates/ripr/src/analysis/language/rust/mod.rs::tests::transitive_reach_limit_kind_names_integration_test_path`
- `crates/ripr/src/domain/language.rs::tests::static_limit_kind_wire_strings_are_stable`
- `crates/ripr/src/domain/language.rs::tests::static_limit_kind_describe_is_present_and_distinct`
- `crates/ripr/src/lsp/gap_artifacts.rs::tests::validation_accepts_rust_integration_public_api_static_limit_kind`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::facade_reexport_with_a_test_naming_the_facade_names_the_limit_and_keeps_the_class`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::reexports_that_cover_the_owner_fire`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::reexports_that_do_not_provably_cover_the_owner_are_unchanged`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::package_name_comes_from_the_manifest_not_the_directory`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::absent_or_dev_only_dependency_edge_keeps_the_definite_result`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::private_or_restricted_owner_never_fires`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::limited_graph_keeps_the_definite_result`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::test_in_the_owners_own_package_naming_the_facade_does_not_count`
- `crates/ripr/src/analysis/classify/facade_reach.rs::tests::paths_must_be_a_prefix_of_the_owners_path`
- `crates/ripr/src/analysis/classify/facade_reach.rs::tests::only_crate_root_unconditional_public_uses_count`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::baseline_without_a_tying_facade_test_keeps_the_repair_gap`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::narrowed_index_scopes_still_name_the_limit`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::facade_tests_tie_to_the_owner_by_glob_or_type`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::inherent_method_owners_are_named_by_their_type_not_the_method`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::owner_module_chain_must_be_public`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::binary_target_owner_never_fires`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::test_that_never_names_the_facade_crate_is_unchanged`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::doctest_of_another_module_sharing_a_name_token_is_not_related`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::doctest_in_the_owners_own_file_stays_related`
- `crates/ripr/src/analysis/language/rust/facade_reach_tests.rs::doctest_of_another_module_that_calls_the_owner_stays_related`
- `crates/ripr/src/analysis/classify/facade_reach.rs::tests::whole_crate_reexports_cover_any_owner`
- `crates/ripr/src/analysis/classify/facade_reach.rs::tests::facade_tests_tie_by_owner_name_type_or_glob`
- `crates/ripr/src/analysis/classify/facade_reach.rs::tests::pub_items_need_pub_attribute_free_declarations`
- `fixtures/rust_transitive_reach_positive/expected/check.json`
- `fixtures/rust_transitive_reach_test_helper_chain/expected/check.json`
- `cargo xtask check-evidence-promotion-honesty`
- `cargo xtask check-evidence-promotion-honesty --pinned-external --clone --case rust_semver_matches_greater_external_limitation`

## Implementation Mapping

| Component | Location |
|---|---|
| Static limit enum and text | `crates/ripr/src/domain/language.rs` |
| Integration-vs-generic selection | `crates/ripr/src/analysis/language/rust/mod.rs` |
| Facade re-export witness (#9982) | `crates/ripr/src/analysis/classify/facade_reach.rs` |
| Path predicate reused by selection | `crates/ripr/src/analysis/rust_index.rs` |
| LSP gap-artifact known-kind validation | `crates/ripr/src/lsp/gap_artifacts.rs` |
| Output contract docs | `docs/OUTPUT_SCHEMA.md` |
| Static limit docs | `docs/STATIC_LIMITS.md` |
| Pure fixture goldens | `fixtures/rust_transitive_reach_positive/`, `fixtures/rust_transitive_reach_test_helper_chain/` |
| Honesty corpus | `fixtures/evidence-promotion-honesty-corpus/corpus.json` |

## CI Proof

- `cargo test -p ripr analysis::language::rust::tests::transitive_reach_limit_kind_names_integration_test_path`
- `cargo test -p ripr domain::language::tests::static_limit_kind_wire_strings_are_stable`
- `cargo xtask fixtures rust_transitive_reach_positive`
- `cargo xtask fixtures rust_transitive_reach_test_helper_chain`
- `cargo xtask check-evidence-promotion-honesty`
- `cargo xtask check-evidence-promotion-honesty --pinned-external --clone --case rust_semver_matches_greater_external_limitation`
- `cargo xtask check-output-contracts`
- `cargo xtask check-static-language`
- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-traceability`
- `cargo fmt --check`

## Metrics

- Corpus invariant: integration-origin transitive witnesses emit
  `rust_integration_public_api_path_unresolved`, disclose witness and limitation
  detail, and remain non-promoted at `no_static_path`.
- Corpus invariant: semver pinned external remains not clean, not actionable,
  and within configured runtime/artifact budgets.
- Generic non-integration transitive witness behavior remains available as
  `rust_transitive_reach_unresolved`.
