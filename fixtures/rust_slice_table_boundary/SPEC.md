# Fixture: immutable slice table boundary

Spec: RIPR-SPEC-0001

## Given

The real retained `authored-grid-early` corpus subject with the committed
`grid-early-property` edit applied. Its property test passes immutable literal
slices of lengths 1, 2 and 3 to `mean_latency` on an eager table path.

## When

`ripr check` analyzes the changed `samples.len() == 0` predicate.

## Then

The finding is `weakly_exposed`, names the missing `samples.len() == 0`
discriminator, and lists observed lengths 1, 2 and 3. Its returned-value sink
comes from the existing propagation authority.

## Must Not

Borrow unrelated neighboring tests' literal zero as an empty input. Evaluate
slice elements, sums, min/max, helper calls, macros or buffer arithmetic.
