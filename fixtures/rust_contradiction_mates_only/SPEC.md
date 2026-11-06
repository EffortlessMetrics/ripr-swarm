# Fixture: rust_contradiction_mates_only

Spec: RIPR-SPEC-0108
Honesty contract: RIPR-SPEC-0108. Issue: #12412 (no-over-withhold control).

## Given

A changed owner (`is_adult`) whose same-file tests never call it — they
reach only `clamp_to_range` and `discounted_total` — and no contradictory
test exists.

## When

The owner changes from `age > 18` to `age >= 18`.

## Then

The findings read `no_static_path`: file-mates share only the owner's file,
so proximity never credits reach, and the #12412 credited-reach gate must
not over-withhold this honest refusal into a contradiction disclosure.

## Must Not

- Credit proximity as reach: the class must stay `no_static_path`.
- Emit an activation-credit-withheld disclosure; nothing is withheld here.
