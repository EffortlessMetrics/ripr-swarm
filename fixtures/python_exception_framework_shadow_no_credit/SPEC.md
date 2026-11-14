# Fixture: python_exception_framework_shadow_no_credit

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

`parse` changes one reachable raise from the local sibling `EmptyError` to
`TooLongError`. The unchanged test imports `pytest` canonically and calls
`parse('abcdefghijklmnop')` inside `pytest.raises(TooLongError)`.
The import resolves to the workspace's `pytest.py`, whose context manager
suppresses both exceptions. That test therefore passes with either raise.

## When

`cargo xtask fixtures python_exception_framework_shadow_no_credit`

## Then

The one finding remains `weakly_exposed`, with a `broad_error` / weak oracle.
The local framework shadow prevents the finding-specific exception identity
credit. This is the framework-custody refusal for #12952.

## Must Not

Report `exposed`, emit `exception_variant_read` credit, or claim a clean
report because the import has the familiar `pytest` spelling. The governed
RIPR-SPEC-0108 honesty corpus independently forbids that promotion.
