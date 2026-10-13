# Fixture: rust_mixed_direct_macro_reach

Spec: RIPR-SPEC-0117

Example 6; #7542.

## Given

The preserved authored example has a weak direct `inner_is_positive` test and
an exact `outer_triples` test behind the opaque `call_inner!` path. The current
owner expression is `3 * x`, a rewrite of `x * 3`.

## When

`cargo xtask fixtures rust_mixed_direct_macro_reach` analyzes the retained diff.

## Then

Keep `weakly_exposed`, direct reach `yes` and existing related-test rows. Name
`rust_macro_reach_unresolved` and the `outer_triples` witness. Missing advice
names the limitation, and the next step inspects that unresolved test route.

## Must Not

- Promote reach, classification or the witness relation.
- Treat a weak direct test as evidence that no other test discriminates.
- Emit a canonical repair packet, verification/receipt command or test-writing route.
- Infer macro expansion, hygiene, runtime coverage or mutation adequacy.
