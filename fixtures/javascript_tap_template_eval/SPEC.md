# Fixture: javascript_tap_template_eval

Spec: RIPR-SPEC-0085
Spec: RIPR-SPEC-0108

## Given

Direct eval inside template interpolation replaces each callback's tap
`equal` method. The eval string contains the context spelling, but no AST
reference to that context. The native changed suite exits successfully.

## When

```bash
cargo xtask fixtures javascript_tap_template_eval
```

## Then

Two related tests retain unknown oracle evidence. The finding stays
`weakly_exposed` and the preview emits no repair packet.

## Must Not

- Hide direct eval by masking template-literal contents.
- Credit replaced matchers or report `exposed` static evidence.
