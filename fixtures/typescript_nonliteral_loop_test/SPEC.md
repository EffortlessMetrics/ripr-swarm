# Fixture: typescript_nonliteral_loop_test

Spec: RIPR-SPEC-0243

## Given

The changed predicate includes age 18. The only test that calls `isAdult`
is registered inside `for (const age of loadAges())`, a loop over data static
analysis cannot enumerate. The test asserts both sides of the boundary.

## When

`cargo xtask fixtures typescript_nonliteral_loop_test` analyzes the diff.

## Then

The test is indexed, so the finding has a related test and no partial
test-extraction disclosure. Its assertions earn no credit: the finding is not
`exposed`, and it carries the `typescript_conditional_registration`
limitation naming the loop (#10501).

## Must Not

- Credit an oracle from a registration that may run zero times.
- Report the looped test as unextracted.
- Claim how many times the loop runs.
