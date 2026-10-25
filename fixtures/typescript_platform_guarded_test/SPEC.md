# Fixture: typescript_platform_guarded_test

Spec: RIPR-SPEC-0243

## Given

The changed predicate includes age 18. The only test that calls `isAdult`
is registered under `if (isWindows) { ... }`, a platform guard static
analysis cannot evaluate. The test asserts both sides of the boundary.

## When

`cargo xtask fixtures typescript_platform_guarded_test` analyzes the diff.

## Then

The test is indexed, so the finding has a related test and no partial
test-extraction disclosure. Its assertions earn no credit: the finding is not
`exposed`, and it carries the `typescript_conditional_registration`
limitation naming the guard (#10438).

## Must Not

- Credit an oracle from a registration that may never run.
- Report the guarded test as unextracted.
- Claim the guard holds or does not hold on any machine.
