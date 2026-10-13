# Fixture: same_name_use_imported_owner

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #6544

## Given

`celsius::snap` and `fahrenheit::snap` share a name. The test module
imports `use super::celsius::snap;` and pins `assert_eq!(snap(17), 20)`. The
diff rewrites the return of `celsius::snap` (`(v + 5) / 10 * 10` becomes
`(5 + v) / 10 * 10`). Mutants `(v + 2) / 10 * 10`, `(v - 5) / 10 * 10` and
`(v + 5) * 10 * 10` all fail the test (rustc 1.95).

## When

`cargo xtask fixtures same_name_use_imported_owner` runs `ripr check` on the diff.

## Then

The return value reads `exposed` through the owner-return pin. The
`use` import settles the bare `snap(17)` call on `celsius::snap`, so the
same-named `fahrenheit::snap` no longer blocks the pin.

## Must Not

- Read the return `weakly_exposed` (the pre-fix false actionable).
- Pin `fahrenheit::snap` from this test; a test that imports the other
  function, or calls a bare `snap` with no import, still pins neither.
