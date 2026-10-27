# Fixture: canonical proptest singleton property

Spec: RIPR-SPEC-0117

## Given

A library threshold changes from `>` to `>=`. A separate ordinary Cargo
integration target uses the actual locked crates.io proptest 1.11.0 package.
Its `Just(100i32)` strategy supplies the boundary, and a direct
`proptest::prop_assert_eq!` expects 90 from the resolved library owner.

## When

`registered_proptest_profile.rs` invokes public `check_workspace`, renders its
JSON, and independently compiles/lists/runs the actual `boundary` Cargo test
target. Hosted preparation fetches this exact lockfile before the offline
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
property credit after a `tests/.cargo/config.toml` runner override without
changing Rust, manifests or lockfile. The warm result must equal an independent
cold result, and removing the override must recover the preserved positive.
The equivalent ordinary assertion with `100i32` must also be strongly gripped
in repository analysis. Off-boundary and opaque inputs remain unclosed;
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
Numeric-prefix casts, methods,
ranges and computed expressions are outside the added scalar reading.

The wrong owner deliberately passes with zero generated cases, with an early
successful return inside a diagnostic expression, and with a mere nonzero
assertion. Those controls must never receive the positive static credit.
An independent ordinary assertion remains useful beside an unsupported
property. All counts and exits are test obligations, not a claim that this
document itself supplies an executed receipt.

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
