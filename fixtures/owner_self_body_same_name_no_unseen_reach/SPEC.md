# Fixture: owner_self_body_same_name_no_unseen_reach

Spec: RIPR-SPEC-0108

## Given

`Wrapper::get` changes its guard from `>= 10` to `> 10`. Its body calls
`self.inner.get()`, which is `Cell::get`, a different method that shares the
name. No other source names `get`, and the only test, `other_test`, exercises an
unrelated function in the same file.

## When

```bash
cargo xtask fixtures owner_self_body_same_name_no_unseen_reach
```

## Then

The finding is `no_static_path`: the only naming of `get` sits inside the
owner's own body, and that body cannot run before the owner is entered, so it is
not an unseen caller (#10160).

## Must Not

- Keep weak reach because the owner's own body names `get`.
- Report `weakly_exposed` or `reachable_unrevealed` from the unrelated test.
- Claim runtime mutation adequacy.

Discriminator pair: `owner_self_body_same_name_with_caller_control` differs only
by `pub fn peek(&self) -> u32 { self.get() }`.
