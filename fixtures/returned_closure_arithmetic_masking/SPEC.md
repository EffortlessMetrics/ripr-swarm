# Fixture: returned closure arithmetic masking

Spec: RIPR-SPEC-0265

Rule 5 governs returned-closure pins. RIPR-SPEC-0108 governs evidence promotion.

## Given

Some exact assertions run returned closures but do not discriminate the changed
operator at the supplied values: a captured offset, unit input, an identifier
spelled like a literal, and an uppercase helper call.

A computed result is overwritten or shadowed before the assertion, a returned
closure is replaced before invocation, or qualified/imported same-name rivals
answer an assertion after the owner result is discarded.
Other controls leave the returned closure unapplied.

## When

The public check path analyzes the eleven owner-tail hunks in `diff.patch`.
The same-name rival bodies remain outside those hunks.

## Then

All eleven return-value
findings must remain present and must not be promoted to `exposed`. Public API
controls live in `returned_closure_pin.rs`; native original/mutated fixtures
and successful direct/qualified/captured-value controls are retained in the
review proof.

## Must Not

- Treat shared owner/closure names as an applied discriminating pin.
- Promote input-masked arithmetic or an unsupported uppercase callable.
- Borrow a discarded owner call or captured-name token for a replacement result,
  replacement closure, or same-name rival assertion.
- Infer that a zero capture means the closure body never ran.
