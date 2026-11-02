# Fixture: rust_indirect_owner_item_scope

Spec: RIPR-SPEC-0240
Honesty contract: RIPR-SPEC-0108. Issue: #10162.

## Given

A nested block defines another `inc`. Outside that block, a weak direct
assertion and a strong iterator assertion reach the real owner.

## When

The owner changes from `x + 1` to `x + 2`.

## Then

The return-value finding remains `static_unknown` with
`rust_indirect_owner_call_unresolved`. The nested declaration cannot hide the
outer function-value reference. Native execution passes before the edit and
fails afterward at the map assertion (`[3, 4]` versus `[2, 3]`).

## Must Not

- Report an actionable gap by extending the nested item's scope.
- Credit an iterator call whose result the analyzer does not follow.
- Claim general Rust name resolution or macro expansion.
