# Fixture: javascript_tap_context_escape

Spec: RIPR-SPEC-0085
Spec: RIPR-SPEC-0108

## Given

Two tap callbacks pass their contexts through tagged-template interpolation
to a helper that replaces `equal` with a no-op. Their comparisons cannot
discriminate the changed `stringify` predicate. The native suite passes
after the edit, independently of the static result.

## When

```bash
cargo xtask fixtures javascript_tap_context_escape
```

## Then

Both tests remain related, with unknown oracle evidence. The finding stays
`weakly_exposed`, and the preview emits no repair packet.

## Must Not

- Hide a context escape by masking template-literal contents.
- Credit either replaced matcher or promote the finding to `exposed`.
- Confuse a native pass with established static discrimination.
