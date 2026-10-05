# RIPR-SPEC-0230: Seam reach unknown, not ungripped

Status: proposed

Owner: product-analysis

Created: 2026-10-04

Linked issues: #5411 (pilot ranks unresolved reach as the top gap), #5295
(mutation spot check), #5334 (crediting that reach, out of scope here).
The `ripr check` side of the same rule is #5416. Numbered 0230 because
open #5512 holds RIPR-SPEC-0225 through 0229.

Support-tier impact:

- No tier change. `opaque` is an existing grip class; no schema value,
  JSON field or `schema_version` changes. Seams that read `ungripped`
  because reach was unresolved now read `opaque` and leave the headline
  gap count. [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact: none. No new process, network, file-policy or dependency
surface. The classified-seam cache generations move (`1.31`, `0.37`).

## Problem

A repo seam with no related test got reach `no`, and RIPR-SPEC-0005 rule 1
maps reach `no` to `ungripped`, the class `ripr pilot` ranks first and the
LSP reports as "No detected test grip". But "no related test" is often an
analyzer limit, not an established negative. ripr does not trace calls
through `pub` to `pub(crate)` helper chains past the direct-call relation,
through macros, or through trait dispatch (`to_string()` running
`Display::fmt`, `<` running `PartialOrd::partial_cmp`). The mutation spot
check (#5295) at semver `280ebcb6edac` found 22 of 23 seam-precise
`ungripped` mutants caught by the crate's tests. All 12 seams sit in
`Display`/`Ord`/`FromStr` impls or private parse helpers that integration
tests reach through the public API.

`ripr check` already names these limits on `no_static_path` findings
(RIPR-SPEC-0114, RIPR-SPEC-0118). The seam surface ignored them.

## Behavior

The rule: a seam is `ungripped` only when ripr established the negative.
When a seam has no related test, the evidence producer looks for an
unresolved candidate path, in this order:

1. A bounded transitive witness (RIPR-SPEC-0114/0118): a test calls an
   in-crate entry point whose name-matched call chain reaches the owner's
   name within `MAX_TRANSITIVE_DEPTH` hops.
2. A macro-reach witness (the existing `no_static_path` macro check).
3. Trait dispatch. The owner is a trait-impl method (`impl Trait for T`),
   a test body, or production code a test may run, names `T`, and the trait
   passes its gate (below).
4. Trait dispatch, one level down. The owner is called, within the bounded
   walk, by a trait-impl method that meets rule 3. A callee that is itself a
   gated trait's method must pass that trait's gate too, so `Display::fmt`
   calling `.fmt(f)` does not reach an unused `Debug::fmt`.
   This includes a trait method whose own type nothing names but which a
   named type's trait method delegates to (`self.inner.fmt(f)`).
   Each such method's callees join the test-reached set, so a type they
   name can root a further method, up to `MAX_TRANSITIVE_DEPTH` rounds.

"Production code a test may run" is the forward closure, by name over
non-macro call facts, of every test's callees within `MAX_TRANSITIVE_DEPTH`
hops. A mention is a capitalized identifier, or a primitive type name
(`u32`, `str`, `bool`), outside comments and string literals. A function's
own `fn name(` signature is not a call to `name` (#5577).

The trait gate (#5577). These std traits run only from syntax ripr can see,
and a test that only builds a value does not run them:

| Trait | Syntax that runs it |
| --- | --- |
| `Display` | `to_string`, a `{}` / `{name}` / `{:>8}` placeholder, insta `assert_snapshot!` / `assert_display_snapshot!` |
| `Debug` | `dbg!`, a `{:?}` / `{x:#?}` placeholder, `assert_debug_snapshot!`, expect_test `assert_debug_eq` |
| `PartialEq` | `==`, `!=`, `assert_eq!`, `assert_ne!`, `debug_assert_eq!`, `debug_assert_ne!`, `eq`, `ne`, `contains`, `dedup`, `dedup_by_key` |
| `PartialOrd`, `Ord` | ` < `, ` > `, `<=`, `>=`, `cmp`, `partial_cmp`, `lt`, `le`, `gt`, `ge`, `max`, `min`, `clamp`, `sort`, `sort_unstable`, `sort_by_key`, `sort_unstable_by_key`, `max_by_key`, `min_by_key`, `binary_search`, `select_nth_unstable`, `is_sorted`, `BTreeMap`, `BTreeSet`, `BinaryHeap` |
| `Hash` | `hash`, `hash_one`, `Hasher`, `BuildHasher`, `HashMap`, `HashSet`, `IndexMap`, `IndexSet` |
| `Clone` | `clone`, `cloned`, `clone_from`, `to_owned`, `to_vec`, `resize`, `extend_from_slice`, `vec![` |
| `Default` | `default`, `Default`, `unwrap_or_default`, `or_default`, `take` |
| `FromStr` | `parse`, `from_str`, `FromStr` |
| `Serialize`, `Deserialize`, serde `Visitor<'de>` | `serde`, the format crates `serde_json`, `serde_yaml`, `toml`, `bincode`, `postcard`, `ron`, `rmp_serde`, `ciborium`, `Serializer`, `Deserializer`, serde_test (`serde_test`, `assert_tokens`, `assert_ser_tokens`, `assert_de_tokens`), insta `assert_json_snapshot!`, `assert_yaml_snapshot!`, `assert_ron_snapshot!`, `assert_toml_snapshot!`, `assert_csv_snapshot!`, `assert_compact_json_snapshot!` |
| `Arbitrary` | `arbitrary`, `Unstructured`, `fuzz_target`, `proptest`, `prop_compose`, `arbitrary_with`, `quickcheck` |

A gated impl passes when a test, a helper in a test file, a test file's
`use` items (`use serde_json::to_string;`), or a generic test-reached
function (`fn render<T: Display>`, an `impl Trait` or `dyn` argument) uses
the syntax, or when other test-reached production code uses it in a body
that also names `T`. A method body that uses `self` or `Self` also counts for
its impl's self type and the types that type's fields or variants name, a few
levels deep (`self.to_string()`, `format!("{:?}", self.inner)`). A
`Display` impl that writes a `char` field with `{:?}` runs `char`'s `Debug`,
not every type's. Delegation within one trait passes the delegating impl's
gate: an `Outer` `Display` that calls `self.0.fmt(f)` reaches `Inner`'s
`Display`. A placeholder inside an `assert!`, `assert_eq!`,
`panic!`, `expect` or similar failure message does not count: it formats only
when the test fails. `assert_eq!` does not run `Debug` for the same reason.

Every other trait (`Drop`, `From`, `Iterator`, operator traits, a crate's own
traits) runs from syntax too common or implicit to gate, so the self-type
mention is enough. A trait impl for a primitive self type also needs the
trait's name in test-reached code (a `T: Encode` bound or `Encode::encode`),
since primitive names appear almost everywhere.

When any of these fires, reach is `opaque` (confidence low) and its summary
names the limit and the witness, for example:

```text
Reach unresolved (rust_integration_public_api_path_unresolved): `test_new`
(tests/test_identifier.rs:14) calls `new`, which may lead to `identifier`
through a call path ripr does not fully trace; no gap is reported
```

The transitive and macro summaries carry the existing `static_limit_kind`
wire value that `ripr check` emits for the same witness. Trait dispatch has
no `static_limit_kind` value and reads `(trait dispatch)`.

RIPR-SPEC-0005 rule 2 (any stage `opaque` gives `opaque`) then classifies
the seam `opaque`. It is not headline-eligible, pilot ranks it after every
gap class, the LSP shows it at information severity, and its evidence
record carries the reach summary as an `opaque_static_evidence` limitation.

When none fires, reach stays `no` and the seam stays `ungripped`.

Known limits. Those that widen dispatch make a seam read `opaque` where it
might not need to. Those that narrow it leave a seam `ungripped` that a test
may run, so each is named here:

- A lowercase self type that is not a primitive never counts as mentioned.
- The gate is per trait and name-wide within its scope: one test that uses
  `{:?}` lets every `Debug` impl of a test-used type dispatch.
- Narrows: the syntax table is a closed approximation. Syntax it does not
  list (a custom assertion macro that formats, a method of a generic
  `impl<T>` block that names no type parameter in its own signature) does
  not pass the gate, and the impl reads `ungripped`.
- Narrows: production code that uses the syntax on a value whose type its
  body never names, outside `self` and its fields (a local bound from a
  call's return value in a free function), does not count for that type.
- Narrows: a gated callee still waiting for its trait's syntax when the
  rounds reach `MAX_TRANSITIVE_DEPTH` is not reached.
- Widens: a test file's unused `use` item counts as syntax.
- Widens: the failure-message check looks back at most 4096 bytes for the
  enclosing call; a placeholder past that counts as a use.
- Comments and strings are stripped line by line, so a type named only inside
  a block comment or multi-line string still counts as a mention.
- Matching is by name, as in RIPR-SPEC-0114. A test that calls a common name
  such as `new` or `parse` pulls in every same-named function, so rule 3
  covers most ungated trait impls of types a crate's tests use, such as
  `Drop` impls the tests never observe. Those seams read unknown,
  not gripped.

The witness is a candidate path. It never becomes a related test and adds
no reach, activation, propagation, observation or discrimination credit.
Seams with related tests are unchanged.

## Non-Goals

- Crediting the unresolved reach (#5334). The seam reads unknown, not
  gripped.
- Changing `ripr check` findings (#5416 owns that surface).
- A new grip class or `static_limit_kind` value.
- Tracing trait dispatch, generics or macros precisely.

## Required Evidence

- A seam whose owner no test path reaches stays `ungripped` (reach `no`).
- A private helper behind a public function an integration test calls is
  `opaque` and names the transitive witness.
- A trait-impl method of a type a test uses is `opaque` and names the test.
- A trait-impl method of a type no test-reached code names stays
  `ungripped`.
- A helper run only from a trait-impl method of a test-used type is
  `opaque` and names the dispatch root.
- A gated trait's impl (`Debug`) of a test-used type stays `ungripped` when
  no test uses the trait's syntax, and when the only use is an assertion
  message; it reads `opaque` and names the syntax when a test uses it.
- A trait method reached by delegation (`.fmt(f)`) does not reach an unused
  gated impl of the same method name.
- A trait impl for a primitive reads `opaque` only when test-reached code
  names the trait.
- A test whose name matches a helper does not "call" that helper.
- Runtime-control integration tests (`cargo test -p ripr --test '*'`) pass
  unchanged.

## Acceptance Examples

At semver `280ebcb6edac`, `ripr check --format repo-exposure-json`:

| Grip class | Before | After |
| --- | --- | --- |
| `ungripped` | 505 | 1 |
| `opaque` | 0 | 504 |

The 504 moves: 321 transitive witnesses, 183 trait dispatch. The spot check
scored 12 semver seams (23 mutants). Each of the 12 had at least one mutant
the crate's tests caught, and all 12 now read `opaque`. Its one missed mutant
sits on one of those same seams, so no seam whose mutants all survived
changed class. The one remaining `ungripped` seam is
`impl Version::cmp_precedence`.

On the other spot-check repositories: rust-hex and strsim-rs have no
`ungripped` seams before or after. bytesize moves 2 of 5 (trait dispatch);
its 3 `ensure-no-std` seams, including a `GlobalAlloc` impl no test reaches,
stay `ungripped`. humantime moves 8 of 8 (trait dispatch).

## Test Mapping

- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::seam_reached_by_no_test_path_stays_ungripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::seam_behind_an_unresolved_transitive_path_is_opaque_not_ungripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::trait_method_of_a_type_tests_use_is_opaque_not_ungripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::trait_method_of_a_type_no_test_reaches_stays_ungripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::helper_run_only_through_trait_dispatch_is_opaque_not_ungripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::trait_method_reached_only_by_delegation_is_opaque_not_ungripped`
- `crates/ripr/src/analysis/test_grip_evidence/reach_limit.rs::tests::identifiers_keep_type_shaped_tokens_outside_comments_and_strings`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::transitive_reach_limit_kind_names_integration_test_path`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::macro_reach_limit_kind_names_direct_test_body_macro_path`

## Implementation Mapping

- `crates/ripr/src/analysis/test_grip_evidence/reach_limit.rs`: the rule,
  the type-mention and dispatch-root index, and the reach summaries
- `crates/ripr/src/analysis/test_grip_evidence.rs`: `reach_evidence` calls
  it for full and compact evidence
- `crates/ripr/src/analysis/test_grip_evidence/related_tests/context.rs`:
  the run-scoped transitive index, type-mention index and per-owner cache
- `crates/ripr/src/analysis/classify/transitive_reach.rs`: forward
  test-reached closure and the shared witness-to-`static_limit_kind` mapping
- `crates/ripr/src/analysis/seam_cache.rs`: cache generations

## Metrics

- `ungripped` seams whose reach rests on an unresolved path: 0 by
  construction
- semver `ungripped`: 505 before, 1 after

### Trait gate (#5577)

Against the same checkouts and cargo-mutants outputs, after #5946:

| Repository | `ungripped` | `opaque` | Moved seams |
| --- | --- | --- | --- |
| semver `280ebcb6edac` | 1 → 32 | 504 → 473 | 13 `Debug::fmt` (`Version`, `Error`), 18 serde `Serialize`/`Deserialize`/`Visitor` |
| bytesize `66a3715e` | 3 → 5 | 2 → 0 | 2 `Arbitrary` |
| humantime, rust-hex, strsim-rs, atuin `90f590b9` | unchanged | unchanged | none |

cargo-mutants missed every mutant in the functions that own the 33 moved
seams (16 on semver, 7 on bytesize) and caught none. The 12 spot-check
false-gap seams stay `opaque`. Of semver's trait-dispatch `opaque` seams,
those in functions where every mutant was missed drop from 32 to 1
(`FromIterator`, an ungated trait); those in functions where every mutant was
caught stay at 128. No other grip class moves.
