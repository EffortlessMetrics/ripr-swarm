# Fixture: collection_argument_raw_rebound

Spec: RIPR-SPEC-0094

## Given

The neutral push is followed by a rebinding of the same root using its equivalent raw identifier `r#b`.

## When

The public check path reads the exact fixture and its literal assertion.

## Then

Both collection effect findings remain `weakly_exposed`. The neutral, wrong-value and deleted-push sources compile and run the named test successfully: the reset removes the changed effect.

## Must Not

- Treat raw identifier spelling as a different binding.
- Credit an assertion after intervening mutation or rebinding.
