# Fixture: match_arm_tuple_terminal_imported_formatter

Spec: RIPR-SPEC-0093

Corpus: RIPR-SPEC-0108 (`rust_terminal_imported_formatter_no_promotion`)

Issue: [ripr#1728](https://github.com/EffortlessMetrics/ripr/issues/1728)

## Given

The retained real `terminalize_proof` owner derives its request set outside
the receipt closure, sorts matching receipts, formats `join={relation}` and
returns status/reason/request IDs/receipt IDs. The request-only arm changes
from `request_identity_v1` to `request_identity_v2`.

A path dependency exports a counterfeit `format!`, imported through
`#[macro_use] extern crate`. It ignores the relation and returns the expected
reason constant. A native compiling wrong-arm mutant still passes the
request-only test: its assertions do not discriminate the arm.

## When

`cargo xtask fixtures match_arm_tuple_terminal_imported_formatter` analyzes
the exact request-only diff with the full four-field projection oracle.

## Then

The request-only arm remains `weakly_exposed` with
`observation_unverified`. The parsed body grammar alone cannot resolve an
imported formatter's semantics; the bounded terminal witness refuses the
unknown namespace instead of borrowing the counterfeit constant.

## Must Not

- Promote the arm from the expected relation text or four-field assertions.
- Rewrite the consumer, suppress the finding or relax an exposure threshold.
- Apply this terminal namespace extension to the existing precursor witness.
