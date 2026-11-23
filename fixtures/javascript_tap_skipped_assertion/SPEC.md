# Fixture: javascript_tap_skipped_assertion

Spec: RIPR-SPEC-0085
Spec: RIPR-SPEC-0108

## Given

A tap test reaches the #9187 owner but its equality assertion has
`{ skip: true }`. Native tap can exit successfully when the comparison is
false, so this call supplies no admitted discriminator.

## When

```bash
cargo xtask fixtures javascript_tap_skipped_assertion
```

## Then

The test stays related with unknown oracle evidence. The finding must not
be `exposed`, and preview output must not emit a repair packet.

## Must Not

- Credit the skipped equality as a strong oracle.
- Lose the real owner call while withholding its assertion credit.
- Treat a static class as a native runner result.
