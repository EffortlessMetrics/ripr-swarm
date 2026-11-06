# Fixture: typescript_vitest_for_assertion_bindings

Spec: RIPR-SPEC-0243

Issue: #10760

## Given

One `.for` callback reads a bare `expect` whose imported binding was assigned
outside the callback. Another reads an unqualified ambient `expect`, without
an assertion import or a bound TestContext. Neither can borrow stable imported
Vitest assertion authority merely because its spelling is `expect`.

## When

Run `ripr check --root input --diff ../diff.patch --format json --mode fast`.

## Then

Keep both registrations and owner reach, but withhold the unqualified exact
assertions and any `exposed` promotion. Assertion-like source stays unresolved;
withheld assertions must not become a claim that no assertions exist.
The independent honesty corpus pins non-promotion and a nonempty finding.

The pre-repair candidate reported `exposed` for this exact two-file binding
control. Its retained executable SHA-256 is
`b8d837be5cc0a32b86104b5d7e9daaaaea47a1e159ce5d0ca432bcb848364895`.
This is a static provenance refusal, not a native behavioral discrimination
receipt: assigning an ESM import is invalid runtime setup, and ambient globals
are outside the qualified profile. The separate positive fixture owns the
valid native baseline / changed-owner / restoration evidence.

## Must Not

- Reuse an imported assertion's authority after a known write.
- Infer a Vitest global assertion without the bounded import/context binding.
- Withdraw the separately bound second TestContext's genuine expect solely
  because another binding with the same spelling is written.
- Count collection errors or unexecuted subjects as caught behavioral faults.
