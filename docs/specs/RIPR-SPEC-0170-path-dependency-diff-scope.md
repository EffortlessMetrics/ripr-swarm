# RIPR-SPEC-0170: path-dependency diff-scope expansion

Status: proposed

Owner:

Created: 2026-08-31

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues:

- #2665 (path dependencies not followed), slice C of the program
- #2970 (expand diff scope via reverse-dependency graph)
- #2968/#3037 (slice A: captured path-dependency edges)
- #2969 (slice B: forward/reverse adjacency)

Linked PRs:

Support-tier impact:

- No tier promotion. Diff-scope expansion widens which files Draft/Fast
  index; it adds no runtime evidence and does not change any support
  tier. [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md) remains the
  tier authority.

Policy impact:

## Problem

In the modes whose diff selection narrows to changed packages (Draft/Fast
with unchanged tests included), a diff that touches only crate `a` indexes
`a`'s files. A behavior change in a path dependency can surface in every
crate that depends on it, but the tests of those dependents are dropped from
the index entirely, so no cross-crate evidence can even be considered. The
forward/reverse path-dependency adjacency built in #2969 was disclosure-only;
no analysis surface consumed it.

## Behavior

In the package-narrowing selections (Draft/Fast with
`include_unchanged_tests`), the Rust adapter expands the changed-package
scope with the package roots of the manifests that reach a changed package
through the reverse path-dependency adjacency:

- the base scope is never dropped: expansion only ever adds packages;
- the walk is transitive (a change in `a` reaches `b` when `b` depends on
  `a` and `c` when `c` depends on `b`) and deterministic (`BTreeMap`/
  `BTreeSet`-ordered);
- cycles are cut by the walk and disclosed on the graph surfaces; the
  reachable set itself stays exact for the walked edges;
- selections without package narrowing are unchanged: Instant stays
  changed-files-only, Deep/Ready stay whole-workspace, and
  `include_unchanged_tests = false` stays changed-files-only in every mode;
- a `limited` graph (partial edge inventory) still walks its connected edges
  and discloses that dependents may be missing; an `unavailable` graph (no
  manifest scan) discloses that the expansion did not run and the scope
  stays the changed packages. Neither state may read as a complete reach;
- a `complete` graph needs no disclosure: the walk covered the full captured
  edge inventory, and a complete-but-empty graph truthfully adds nothing;
- the diff-index scope cap keeps its fail-closed behavior unchanged, and the
  expansion counts toward it: a diff whose dependent reach pushes the index
  over the cap fails closed with `diff_scope_oversized` exactly as before.

The manifest inventory is read locally; no Cargo invocation, no registry
metadata, and no network access is added.

## Non-Goals

No forward-direction scope expansion (a changed crate's own dependencies do
not enter scope through this behavior). No classification change: the
cross-crate package-prefix guard, its uniqueness bypass, and the
`workspace_index_complete` derivation keep their semantics — expansion
participates in that derivation only through the selection it actually
produced. No cross-crate test-oracle crediting change (#2664 owns that). No
change to `input_changed` naming, parity decisions, or any report/JSON
schema. No registry or external dependency resolution.

## Required Evidence

- a disclosure line naming the `limited` boundary (partial edge inventory,
  dependents may be missing) or the `unavailable` boundary (expansion did
  not run, scope stays the changed packages) whenever the scope decision
  consults a graph in one of those states;
- end-to-end evidence that a Draft-mode diff touching only `a` surfaces the
  dependent crate's test as related evidence through the dependency edge,
  and that the same workspace without the edge does not;
- selection-level evidence that the unrelated crate stays out and that
  non-narrowing selections ignore the expansion.

## Acceptance Examples

- Workspace `a <- b <- c` (path dependencies) plus unrelated `d`: a Draft
  diff touching only `a/src/lib.rs` brings `b`'s and `c`'s package files
  into scope; `d` stays out; a `Draft` finding for `a`'s changed owner may
  name `b`'s integration test as related evidence. Expanding from `c`
  (which nothing depends on) adds nothing.
- The same workspace without `b`'s dependency edge: `b` stays out of scope.
- A workspace whose manifest inventory cannot be parsed fully: the graph is
  `limited`, connected edges still participate, and the disclosure names the
  partial inventory.
- A directory with no `Cargo.toml` at all: the graph is `unavailable`, the
  expansion did not run, and the disclosure says the scope stays the changed
  packages.

## Test Mapping

- `crates/ripr/src/analysis/workspace/path_dependencies.rs::tests::expansion_reaches_dependents_of_changed_packages_only`
- `crates/ripr/src/analysis/workspace/path_dependencies.rs::tests::expansion_adds_nothing_for_unmapped_or_empty_changed_roots`
- `crates/ripr/src/analysis/workspace/path_dependencies.rs::tests::expansion_discloses_limited_and_unavailable_graph_states`
- `crates/ripr/src/analysis/workspace/path_dependencies.rs::tests::package_root_and_manifest_identities_round_trip`
- `crates/ripr/src/analysis/workspace/select.rs::tests::dependent_packages_enter_draft_and_fast_selection`
- `crates/ripr/src/analysis/workspace/select.rs::tests::empty_dependent_roots_reproduce_the_unchanged_selection`
- `crates/ripr/src/analysis/workspace/select.rs::tests::dependent_roots_do_not_change_non_narrowing_selections`
- `crates/ripr/src/analysis/language/rust.rs::tests::draft_diff_scope_reaches_path_dependent_tests_through_the_dependency_edge`
- `crates/ripr/src/analysis/language/rust.rs::tests::draft_diff_scope_stays_narrow_without_the_path_dependency_edge`
- `crates/ripr/src/analysis/language/rust.rs::tests::instant_mode_does_not_expand_scope_through_path_dependencies`

## Implementation Mapping

- `crates/ripr/src/analysis/workspace/path_dependencies.rs` —
  `reverse_dependent_scope_expansion`, `PathDependencyScopeExpansion`,
  package-root/manifest identity mapping
- `crates/ripr/src/analysis/workspace/select.rs` —
  `select_rust_files_for_mode_with_dependent_packages`
- `crates/ripr/src/analysis/language/rust.rs` — Draft/Fast gating,
  disclosure emission, selection wiring

## Metrics

- unit_test_pass_rate
- golden_fixture_pass_rate
