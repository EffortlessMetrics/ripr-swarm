# Fixture: currentness_matrix_typescript_comment_replaced_code

Spec: RIPR-SPEC-0156

## Given

A TypeScript module with two owners. The diff removes code from both:

- `cappedTotal` loses its guard `if (total > CAP) {`, the guarded
  `total = CAP;` and the closing bracket. One comment line is added where
  they stood (#15913). The diff pairs the comment with the removed guard,
  but the added run holds no code, so it replaced nothing.
- `clampedDouble` is the control: two statements are rewritten as a comment
  and one statement. The added run holds code, so the comment keeps its
  pairing and the statement carries the change.

## When

```bash
cargo xtask fixtures currentness_matrix_typescript_comment_replaced_code
```

## Then

The removed guard and its body are removed-only probes on `cappedTotal`,
with `source_currentness` `base_deleted`, as in
`currentness_matrix_typescript_deleted_statement`. They read as the same
deletion reads when no comment is added. `clampedDouble` keeps its ordinary
`candidate_current` finding on the rewritten return and has no base-side
record.

## Must Not

- Leave the removed guard with no record: before #15913 a comment is never a
  probe and the guard it paired with was dropped.
- Emit a head-actionable gap, repair target, or diagnostic for a
  `base_deleted` record.
- Give a removed line of `clampedDouble` a base-side record: its added run
  holds code.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
