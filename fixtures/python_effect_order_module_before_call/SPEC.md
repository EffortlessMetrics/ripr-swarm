# Fixture: python_effect_order_module_before_call

Spec: RIPR-SPEC-0028

Governing claim: #10178 shared statement-order follow-up.

## Given

A direct module-object field assignment runs after the test has already asserted on the imported object. The assertion and call share a physical line, but the assertion is the earlier Python statement.

## When

`cargo xtask fixtures python_effect_order_module_before_call` analyzes the single changed field assignment.

## Then

Exactly one changed-field finding is `weakly_exposed`. Native Python runs the same test against the old and new assignments; both variants pass, and the changed assignment executes in both. The assertion reads the state before the owner call, so it does not discriminate the changed value. RIPR-SPEC-0108 independently pins the finding count, non-promotion and repair-packet denial.

## Must Not

Treat the pre-call imported-object read as an observation of the later write, erase the changed-field finding, or derive execution order from equal line numbers. Call-then-assert controls retain their exposure reading and distinguish the old and new values.
