# Fixture: python_exception_stale_type_no_credit

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

`parse` changes its raised exception from `ValueError("m")` to
`TypeError("m")`. The unchanged test still expects the OLD class:
`pytest.raises(ValueError, match="m")`. That test fails once the raise changed,
yet its `match=` pattern accepts the unchanged message.

## When

`cargo xtask fixtures python_exception_stale_type_no_credit`

## Then

The one finding is `weakly_exposed`; its missing-discriminator text names the
stale `ValueError` and the new `TypeError`.

## Must Not

Report `exposed` from `match=` alone. The governed RIPR-SPEC-0108 honesty
corpus independently forbids that promotion (#13622).
