# Source-file identity for a line collision

Spec: RIPR-SPEC-0158

## Given

A direct caller observes a JSON-indexed coverage receipt. An unrelated test
in another source file asserts on ledger text at the same numeric line.

## When

The absent arm changes from an empty `Some` to `None`.

## Then

The direct receipt values retain `src/lib.rs` as their source. The ledger
value retains `src/decoy.rs`. Both observed-value arrays agree. The existing
line-keyed assertion map and differing-text provenance remain available.
The changed return remains `weakly_exposed` with weak discrimination.

## Must Not

Borrow the unrelated assertion's file for an owner value, infer the source
file from the changed probe, or turn JSON indexing or metadata into stronger
oracle admission, classification, or runtime proof.
