# ripr targeted-test outcome report

Status: advisory

Inputs:
- before: `fixtures/boundary_gap/calibration/before-targeted-test.repo-exposure.json`
- after: `fixtures/boundary_gap/calibration/after-targeted-test.repo-exposure.json`

## Reviewer Receipt

What changed:
- Static evidence for \`67fc764ba37d77bd\` at src/lib.rs:2 is the selected receipt thread.
- The selected seam is \`predicate_boundary\` and moved \`unchanged\`.
- Receipt buckets: moved 0, unchanged 1, regressed 0, new 0, removed 0.

What RIPR flagged before:
- Before snapshot: \`predicate_boundary\` at src/lib.rs:2 was classified \`weakly_gripped\`.

Focused proof added outside RIPR:
- new observed value: 100
- related test count increased by 1

Verification movement:
- After snapshot: selected seam moved from \`weakly_gripped\` to \`weakly_gripped\` with direction \`unchanged\`.

What remains weak or unknown:
- Selected seam remains \`weakly_gripped\` in the after snapshot.
- Selected seam did not change static class.

Reviewer should believe:
- This receipt compares the supplied before and after static repo-exposure snapshots by seam_id.
- The listed movement is advisory static evidence movement for review context.
- RIPR did not add the focused test or output proof; the external change is represented only by the after snapshot.

Reviewer should not believe:
- This is not runtime mutation confirmation.
- This is not coverage adequacy or a code-correctness guarantee.
- This is not merge approval, source-edit automation, generated tests, or provider/model execution.

## Summary

| Bucket | Count |
| --- | ---: |
| moved | 0 |
| unchanged | 1 |
| regressed | 0 |
| new | 0 |
| removed | 0 |

## Grip Counts

| Class | Before | After |
| --- | ---: | ---: |
| seams_total | 1 | 1 |
| strongly_gripped | 0 | 0 |
| weakly_gripped | 1 | 1 |
| ungripped | 0 | 0 |
| reachable_unrevealed | 0 | 0 |
| activation_unknown | 0 | 0 |
| propagation_unknown | 0 | 0 |
| observation_unknown | 0 | 0 |
| discrimination_unknown | 0 | 0 |
| opaque | 0 | 0 |
| intentional | 0 | 0 |
| suppressed | 0 | 0 |

## Moved

None.

## Unchanged

- `67fc764ba37d77bd` src/lib.rs:2 weakly_gripped -> weakly_gripped (unchanged)
  - new observed value: 100
  - related test count increased by 1

## Regressed

None.

## New

None.

## Removed

None.

This report compares two static repo-exposure snapshots. It is advisory and does not run mutation testing, edit source, generate tests, claim runtime correctness, claim coverage adequacy, or approve a merge.
