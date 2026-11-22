# Fixture: currentness_matrix_python_comment_replaced_code

Spec: RIPR-SPEC-0156

## Given

A Python module with two owners. The diff removes code from both:

- `capped_total` loses its guard `if total > CAP:` and the guarded
  `total = CAP`. One comment line is added where they stood (#15913). The
  diff pairs the comment with the removed guard, but the added run holds no
  code, so it replaced nothing.
- `clamped_double` is the control: two statements are rewritten as a comment
  and one statement. The added run holds code, so the comment keeps its
  pairing and the statement carries the change.

## When

```bash
cargo xtask fixtures currentness_matrix_python_comment_replaced_code
```

## Then

The removed guard and its body are removed-only probes on `capped_total`,
with `source_currentness` `base_deleted`, naming the removed code. They read
as the same deletion reads when no comment is added. The comment is no
finding. `clamped_double` keeps its ordinary `candidate_current` finding on
the rewritten return and has no base-side record.

## Must Not

- Report the comment text as a changed behavior, a discriminator or a gap
  id: before #15913 the comment was a `predicate` probe graded `exposed`.
- Emit a head-actionable gap, repair target, or diagnostic for a
  `base_deleted` record.
- Give a removed line of `clamped_double` a base-side record: its added run
  holds code.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
