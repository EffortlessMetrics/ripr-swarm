# Perl CPAN Alpha Fixture

Campaign 31 E1 convergence proof (ripr-swarm#1379).

## Scope

Three-outcome CPAN-style fixture proving the Perl repair-routing loop
produces honest Findings or named limitations from real Perl source.

## When

The producer (`perllsp ripr-facts`) emits a fact packet from a CPAN-style
project. The consumer (`ripr check --perl-facts`) reads it and produces
honest results.

## Then

### Outcome 1: Actionable (weak oracle on a changed boundary)

The boundary diff changes `>=` to `>` in `calculate_discount`. The test file
has `ok(calculate_discount(100))` — a weak smoke oracle that reaches the
changed owner but does not pin the exact boundary.

Expected ripr result: `WeaklyExposed` Finding with a concrete missing
discriminator, a related test, and a candidate repair packet subject to the
shared validator.

### Outcome 2: Already observed (exact oracle discriminates the boundary)

The test file also has `is(calculate_discount(100), 90)` — an exact oracle
that pins the boundary.

Expected ripr result: no repair packet (the gap is already discriminated).

### Outcome 3: Limited (dynamic dispatch blocks actionability)

The dynamic-dispatch diff introduces `my $method = 'calculate_discount';
return shift->$method();` — a dynamic boundary.

Expected ripr result: `StaticUnknown` Finding with a named limitation
(dynamic_dispatch boundary).

## Must Not

- Emit a public repair packet for the dynamic-dispatch case.
- Emit a public repair packet without the shared validator passing.
- Crash or abort the whole report on any of the three outcomes.
- Credit a weak oracle as `exposed` without the shared validator.
