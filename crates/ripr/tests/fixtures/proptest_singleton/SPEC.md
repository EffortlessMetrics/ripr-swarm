# Fixture: canonical proptest singleton property

Spec: RIPR-SPEC-0117

## Given

A library threshold changes from `>` to `>=`. A separate ordinary Cargo
integration target uses the actual locked crates.io proptest 1.11.0 package.
Its `Just(100i32)` strategy supplies the boundary, and a direct
`proptest::prop_assert_eq!` expects 90 from the resolved library owner.

## When

`registered_proptest_profile.rs` runs an isolated child that invokes public
`ripr::cli::run`, loads the fixture's configuration, and captures the complete
JSON for diff and repository-exposure checks. It independently compiles, lists
and runs the actual `boundary` Cargo test target. Hosted preparation fetches
this exact lockfile before the offline
checks; missing resolution is an instrument/setup failure, never a skipped
positive. The native controls disable failure persistence consistently so a
previous failure cannot replay a case in the deliberately zero-cases row.

## Then

The static positive must be `exposed` on cold and warm reads. It is a file-level
macro with an explicitly suffixed integer input; unsuffixed integers, inline
modules, raw-spelled declarations/bindings and `prop_assert_ne!` remain
unsupported. Raw-spelled library module/type shadows must retain the macro
limitation and supply no generated test or observed activation input row.
Changes to runner
environment, Cargo `[env]`, target registration, source, lock or provider must
revoke the corresponding premise. The compiled positive must collect exactly
`exact_boundary`, pass the original owner, and fail the same assertion with
only the owner changed back to `>` (100 observed, 90 expected).

A repository-exposure run must demonstrate a classified-cache hit, then lose
property credit after a `tests/.cargo/config.toml` `[env]` override of
`PROPTEST_CASES` without changing Rust, manifests or lockfile. The warm result
must equal an independent
cold result, and removing the override must recover the preserved positive.
The equivalent ordinary assertion with `100i32` must also be strongly gripped
in repository analysis. Ordinary associated/receiver calls with identical
`100i32` arguments retain their existing empty missing-discriminator list while
activation remains unknown; different argument text gains no new scalar values.
Off-boundary and opaque inputs remain unclosed;
ordinary and property calls through shadowing library modules/types, including
raw-spelled declarations, supply no activation values for the real owner.
New typed ordinary rows must pass the existing free-owner binding checks before
observed-value emission and equality pairing. Every property candidate passes
that authority before any evidence stage, including a property with an unused
parameter and literal-only call arguments. A shadowed property supplies no
related test for any seam of the real owner.
The body-level owner proof requires every same-name call to bind the owner:
a real-owner call at 101 cannot borrow the boundary from a different callable
at 100. Replacing only that different callable with the real owner restores
the positive. The covered owner path is the canonical library-qualified one;
registration alone does not establish bare, associated or unresolved paths.
Changing `mock_expectation_strength` on a direct bool property whose owner name
contains `expect_` exercises the real post-registration policy mutation. A
registered fact without an exact matching receipt supplies no repository
relation or observed value, including through an unused `Just` parameter and
unsuffixed literal arguments. Restoring the default policy restores the same
canonical property evidence; a shadowed owner remains uncredited in both states.
Numeric-prefix casts, methods,
ranges and computed expressions are outside the added scalar reading.

The wrong owner deliberately passes with zero generated cases, with an early
successful return inside a diagnostic expression, and with a mere nonzero
assertion. Those controls must never receive the positive static credit.
An independent ordinary assertion remains useful beside an unsupported
property. All counts and exits are test obligations, not a claim that this
document itself supplies an executed receipt.

Four additional acceptance controls start with an admitted property, revoke one
premise, and restore the original evidence. A literal `[patch.crates-io]`
replacement uses a consistent lockfile and successful offline Cargo metadata.
A canonical dev dependency plus a build dependency must resolve both edge kinds
to the real provider yet remain outside the admitted dependency contexts.
An unreferenced `tests/support/property.rs` must not borrow registration from
the real, independent ordinary test target. `#[cfg(all())]` must collect and
pass one actual native test while remaining outside this reader's cfg grammar.
Each refusal checks absent property relations, assertion/value/activation
evidence and unknown/none oracle fields, not only the final classification.

The internal index controls compare fresh, cold/warm file-cache,
reconstructed-role and incremental-addition paths through actual seam
inventory, evidence and classification consumers. Reconstruction first checks
that local role normalization alone grants no execution ancestry or property
relation, then runs the ordinary composition authority over the complete
fixture corpus and finalizes the index before classification. The positive
requires the exact generated TestFact, full receipt source/line offset/name
scopes, one related property, boundary 100 and established stages. With Rust
bytes fixed, root Cargo `[env] PROPTEST_CASES='0'` and a changed provider checksum
must revoke the same authority through reconstruction and incremental reuse;
restoring each premise must recover the original classified predicate. A
separately named ordinary witness retains its complete facts. Raw cached
parsing and its wire round-trip retain the unresolved macro and never store
promoted property authority.

## Offline metadata diagnosis (#10864)

In fresh Rust finding analysis (with diff/fast CLI acceptance), a supported
property invocation whose provider verification cannot obtain offline locked
Cargo metadata retains its existing
unknown stages and zero generated-test/value credit. Its existing macro
limitation names the observed operational failure and recommends a manual
same-root, same-environment metadata command. No package is inferred from an
exit code. Capture failures point to temporary storage; a refused toolchain
path must not be recommended for execution.

The public child regression pairs JSON and human-full guidance, repeats the
unavailable state without purging the diff cache, refuses to attach this advice
to unsupported syntax, preserves an independent ordinary assertion, and
restores the initial finding when the original Cargo environment is restored.
That environment-transition control is not the separate installed same-Cargo-home
missing-archive restoration acceptance, which still requires execution.

Diagnostic context is analysis-local and bound to invocation source bytes; it
is not serialized in raw parser facts or classified-seam caches. This slice
does not add operational guidance to full/compact repository-seam output and
does not claim its cache-freshness acceptance. Existing classified-cache keys
and provider admission remain unchanged.

## Must Not

- Treat package/macro spelling, zero-test success or compilation failure as
  affirmative provider, registration or discrimination evidence.
- Turn `prop_assert*` into generally trusted non-returning macros.
- Infer random sampling, shrink coverage, model strength, repair readiness,
  or a native mutation outcome from static exposure.
- Claim the unchanged Perl-LSP position/model cohort or close the full #6643
  QuickCheck, bool-helper and explicit-runner acceptance from this fixture.

For local reproduction, prepare dependencies with
`cargo fetch --locked --manifest-path crates/ripr/tests/fixtures/proptest_singleton/input/Cargo.toml`,
then run the explicit `registered_proptest_profile` integration target.
