# Fixture: python_exception_dotted_module_no_credit

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

The physical file is named `fx.core.py`. Its one-line edit replaces a
reachable `raise EmptyError()` with the inert sibling `TooLongError()`.
The unchanged test writes `from fx.core import parse, TooLongError` and
uses `pytest.raises(TooLongError)` around the matching literal-input call.
Python resolves that import through a package named `fx`; it does not
interpret a dot inside the filename as a package boundary. The fixture
contains no such package, so the import fails on both versions.

## When

`cargo xtask fixtures python_exception_dotted_module_no_credit`

## Then

Keep the one `weakly_exposed` finding and the existing broad-error/weak
oracle. The changed source file cannot establish the imported class or
callable identity. The finding-specific exception reading grants no credit.

## Must Not

Report `exposed`, emit `exception_variant_read` credit, or report a clean
result merely because a normalized filename resembles the import spelling.
RIPR-SPEC-0108 independently forbids this false promotion for #12952.
