# Fixture: typescript_vitest_for_context_skip

Spec: RIPR-SPEC-0243

Issue: #10760

## Given

A literal Vitest `.for` boundary row calls the real second-argument
TestContext's `skip()` before its assertion. Vitest skips the test regardless
of the changed boundary output; the positive sibling fixture retains an
independent native skip control.

## When

Run `ripr check --root input --diff ../diff.patch --format json --mode fast`.

Observed with the sibling fixture's pinned Vitest 4.1.11 dependency graph,
Node 24.19.0 on Linux, 2026-11-06: exit 0, one registered subject, zero
passes, zero failures, one skipped. This is no executed positive witness.

## Then

The finding remains `no_static_path` with named partial-extraction disclosure.
The bounded positive `.for` producer refuses the context skip instead of
extracting a live discriminator. The independent honesty corpus forbids
promotion even if this golden is re-blessed incorrectly.

## Must Not

- Credit the assertion after `ctx.skip()`.
- Apply the ordinary first-parameter context shortcut to the row.
- Treat a skipped native subject as executed positive evidence.
