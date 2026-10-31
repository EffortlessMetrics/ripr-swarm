# Fixture: python_bool_owner_unrelated_patching

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A `-> bool` owner whose boundary comparison changes from `>` to `>=`. The test file imports
`mock` and uses `monkeypatch.setenv` and a `MagicMock`, but never patches the callee; its tests
are literal-argument bare asserts on the owner.

## When

The changed comparison is analyzed.

## Then

One `exposed` finding: the bare asserts are exact pins, as they were before the file imported `mock`.

## Must Not

Withhold the #9542 credit because the file mentions `mock` or `monkeypatch` for other purposes.
Negative counterparts: `python_bool_owner_patched_callee` and `python_bool_owner_escaped_patch_target`.
