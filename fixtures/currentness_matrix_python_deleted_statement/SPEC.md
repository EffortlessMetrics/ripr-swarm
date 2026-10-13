# Fixture: currentness_matrix_python_deleted_statement

Spec: RIPR-SPEC-0156

## Given

A Python module with three owners. The diff deletes a behavior line from two
of them and rewrites the third:

- `capped_total` loses `total = min(total, CAP)` inside an uneven replacement
  block: the indented comment above it is re-added at column 0 (#7587).
  Content pairing pairs the re-added comment with the old comment, so no
  added line pairs with the deleted clamp.
- `floored_total` loses `total = max(total, FLOOR)` with no added line at all.
- `doubled` is an even block: its comment and its return are both rewritten.

## When

```bash
cargo xtask fixtures currentness_matrix_python_deleted_statement
```

## Then

Each deleted behavior line that no added line pairs with is a removed-only
probe on its owner, with `source_currentness` `base_deleted`. It is retained
as labelled base-side evidence, as the Rust adapter does for
`currentness_matrix_tail`, and is never a head-actionable gap. The rewritten
return of `doubled` pairs with its own old line and keeps its ordinary
`candidate_current` classification.

A base-side record keeps the classifier's ordinary static read of the deleted
line, as Rust base-side records do. That read is labelled evidence, not a
verdict on the candidate: the human report hides both records as base-side
evidence and starts from `doubled`.

## Must Not

- Report a deleted Python behavior line as nothing to analyze: the same diff
  read no record for either deletion before #7587.
- Emit a head-actionable gap, repair target, or diagnostic for a
  `base_deleted` record.
- Give a deleted line a record when an added line pairs with it.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
