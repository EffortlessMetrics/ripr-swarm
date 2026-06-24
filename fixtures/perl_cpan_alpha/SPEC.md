# Perl CPAN Alpha Fixture

Campaign 31 E1 convergence proof (ripr-swarm#1379).

## Three-outcome fixture

```text
Makefile.PL  (future — not yet needed for the alpha proof)
lib/Pricing.pm
t/pricing.t
```

### Outcome 1: Actionable (weak oracle on a changed boundary)

The boundary diff changes `>=` to `>` in `calculate_discount`. The test file
has `ok(calculate_discount(100))` — a weak smoke oracle that reaches the
changed owner but doesn't pin the exact boundary.

Expected ripr result: `WeaklyExposed` Finding with a concrete missing
discriminator (`$amount > 100`), a related test (`pricing.t`), and a
candidate repair packet (subject to the shared validator).

### Outcome 2: Already observed (exact oracle discriminates the boundary)

The test file also has `is(calculate_discount(100), 90)` — an exact oracle
that pins the boundary. If ripr sees both oracles, the exact oracle
discriminates the changed behavior.

Expected ripr result: no repair packet (the gap is already discriminated).

### Outcome 3: Limited (dynamic dispatch blocks actionability)

The dynamic-dispatch diff introduces `my $method = 'calculate_discount';
return shift->$method();` — a dynamic boundary.

Expected ripr result: `StaticUnknown` Finding with a named limitation
(dynamic_dispatch boundary).

## Usage

This fixture is the input for the two-binary proof:

```bash
perllsp ripr-facts --root . --base origin/main --head HEAD --out facts.json
ripr check --languages perl --perl-facts facts.json --json
```

The integration test (E1) runs both commands + asserts the three outcomes.
