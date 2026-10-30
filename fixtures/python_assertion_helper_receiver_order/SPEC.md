# Fixture: python_assertion_helper_receiver_order

Spec: RIPR-SPEC-0238

## Given

A helper reads the second receiver before that receiver calls the changed method. The first receiver calls it earlier; that call must not supply the second receiver's temporal identity.

## When

The expanded counter update changes to `+=`.

## Then

One `Stats.record_write` finding remains `weakly_exposed`.

## Must Not

Promote a helper comparison without the executed receiver and retained field identity. Deleting the changed write leaves this control satisfied. RIPR-SPEC-0108 independently checks the class and nonempty finding count.
