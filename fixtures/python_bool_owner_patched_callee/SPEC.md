# Fixture: python_bool_owner_patched_callee

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A `-> bool` owner whose boundary comparison changes from `>` to `>=`. The only test is a
literal-argument bare assert on the owner, but another test in the same file patches the callee
by name with `mock.patch("rules.is_adult")`.

## When

The changed comparison is analyzed.

## Then

One `weakly_exposed` finding: the callee may be rebound, so the bare assert is not credited
as an exact pin.

## Must Not

Credit a bare-bool assert as an exact pin when the test file patches the callee by name.
A file that only patches other names (`monkeypatch.setenv`) is credited; that positive
control is pinned by unit tests in `bool_oracle_tests.rs`. The honesty corpus independently
rejects promotion after an incorrect re-bless.
