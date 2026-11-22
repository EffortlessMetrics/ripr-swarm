# Fixture: python_patched_callee_reachable_h

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

`rate_for` changes its return from `1` to `3`. The only related test calls
`tax_for`, which calls `rate_for`, but replaces `rate_for` itself with a
`monkeypatch.setattr` stand-in before asserting, so no related test ever runs
the changed code (the patched-callee shape of #13632).

## When

`cargo xtask fixtures python_patched_callee_reachable_h` analyzes the
changed return through the real CLI.

## Then

One `reachable_unrevealed` finding: the tests reach the changed owner through
its caller but reveal nothing about the edit, because every related test
replaces the owner with a stand-in.

## Must Not

Credit the stand-in's asserted value as a discriminator for the changed
owner, or promote the finding to `exposed` or `weakly_exposed`. The
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
