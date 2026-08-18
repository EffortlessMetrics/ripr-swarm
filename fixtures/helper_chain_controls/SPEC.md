# Fixture: helper_chain_controls

Spec: RIPR-SPEC-0159

## Given

Three fail-closed controls. The computed-argument control calls its
helper with `input.trim()` — a computed argument, not a literal or
parameter. The same-name control defines `is_word_start` in a second
module, so the callee name is not unique and neither chain may
transfer. The tests call only the entry functions with exact values.

## When

```bash
cargo xtask fixtures helper_chain_controls
```

## Then

No helper-owned probe relates through a chain: every control keeps the
pre-transfer output (the lexical transitive-reach limitation, or the
ordinary direct analysis), and no transferred input row or call-operand
evaluation appears for these shapes.

## Must Not

- Transfer through a computed argument or a non-unique callee name.
- Promote any control past its fail-closed class on call reach alone.
