# Fixture: python_raises_bare_owner_outside_body_no_credit

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

`parse` changes its blank-input raise message from `empty level` to
`blank level`. The unchanged test imports `parse` bare and calls only
`other("")` inside `pytest.raises(ValueError)`; the owner name `parse`
appears solely in the comparison appended after the `with` body.

## When

`cargo xtask fixtures python_raises_bare_owner_outside_body_no_credit`

## Then

The one finding remains `weakly_exposed`. A token that appears only
outside the exception body (in the appended comparison, before the
`with`, in a `match=` pattern, or inside a body string) does not
establish that the body observes the changed owner.

## Must Not

Report `exposed` or `strong_oracle_observes_owner_name` credit for an
owner named only outside the exception body. The governed RIPR-SPEC-0108
honesty corpus independently forbids that promotion (#14267).
