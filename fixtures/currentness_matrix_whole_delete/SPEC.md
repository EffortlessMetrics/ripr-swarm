# Fixture: currentness_matrix_whole_delete

Spec: RIPR-SPEC-0156

## Given

A production owner `price` changed in `src/lib.rs` (boundary predicate
narrowed from `>=` to `>`), plus a whole-file delete of `src/gone.rs`
in the same diff. The input tree retains the deleted file so the diff
applies cleanly; the deletion is the diff's statement of the change.

## When

```bash
cargo xtask fixtures currentness_matrix_whole_delete
```

or:

```bash
ripr check --root fixtures/currentness_matrix_whole_delete/input --diff fixtures/currentness_matrix_whole_delete --mode fast
```

## Then

A whole-file deletion is disclose-not-probe: no probe, no finding, and
no repair target is emitted for the deleted file, and the run discloses
the deletion as a named non-claim (deleted behavior has no candidate-side
code to analyze). The changed production owner in the same diff stays
actionable (`candidate_current`), proving the deletion adds no
obligation while the real seam keeps its teeth (#3212 matrix row 3).

## Must Not

- Emit any probe, finding, or repair route for the deleted file.
- Present the deleted file as clean or as a candidate edit target.
- Drop the production owner's ordinary classification.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
