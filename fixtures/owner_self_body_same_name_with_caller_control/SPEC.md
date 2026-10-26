# Fixture: owner_self_body_same_name_with_caller_control

Spec: RIPR-SPEC-0108

## Given

The same change and unrelated test as `owner_self_body_same_name_no_unseen_reach`,
plus a sibling method `peek` whose body calls `self.get()`. The files differ only
by that caller.

## When

```bash
cargo xtask fixtures owner_self_body_same_name_with_caller_control
```

## Then

Reach stays undecided (not `no_static_path`): `peek` names the owner outside its
own body, so something may reach it unseen. This is the positive control for
#10160; a same-named call on another receiver in a different function reads the
same way, since the scan does not tell receivers apart.

## Must Not

- Treat a caller outside the owner's body as the owner's own recursion.
- Promote to `exposed` from the unrelated same-file test.
- Claim runtime mutation adequacy.
