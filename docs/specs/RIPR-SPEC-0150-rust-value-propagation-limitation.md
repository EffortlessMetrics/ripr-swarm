# RIPR-SPEC-0150: Rust bounded value-propagation limitation

Status: accepted

Issue: #3215

## Contract

When a changed Rust `let` binding has a right-hand side containing `map_or`
and one of the bounded operation families `find`, `rfind`, or `len_utf8`, and
the same owner function later compares that binding in an equality predicate,
RIPR may emit:

```text
static_limit_kind: rust_value_propagation_unresolved
class: static_unknown
```

The finding must retain related tests when they are already discovered. It
must disclose the last established binding, the first unresolved edge into
the equality predicate, the analyzer route
`analysis/rust-value-propagation`, and a limitation-only non-claim.

## Required guards

- the changed line must parse as a simple `let` binding;
- the operation-family and `map_or` checks apply to the changed right-hand
  side, not arbitrary owner text;
- the compared identifier must be a whole identifier in the same owner body;
- a missing owner or missing related tests produces no new limitation;
- unrelated helper, loop, dynamic, and non-equality shapes remain unchanged;
- classification is never promoted and no repair packet or verify command is
  synthesized.

## Proof

The production-path Rust adapter fixture covers the `rfind`/`map_or` binding
feeding an equality predicate with two exact related tests. Negative controls
cover a non-family `len` binding and must not receive this limitation. The
stable enum wire string and LSP accepted-kind catalog are tested separately.

## Non-goals

This slice does not resolve value propagation, prove the equality boundary,
change probe-family inference, execute mutations, calculate rates, or create
repair/test instructions. Those are later bounded slices of #3215.
