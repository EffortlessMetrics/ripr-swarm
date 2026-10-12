# Span Containment Calibration: Atuin

Pinned real-world regression for the `span_containment` join (#5486) and the
runtime span import (#5485). The inputs are reduced metadata, not source:

- `runtime-mutants.json`: the 27 cargo-mutants 27.1.0 outcomes from
  `atuinsh/atuin` at `90f590b9235556363ffb5b2c66728f8af3c27afe`, reduced to
  mutant name, file, function span, mutant span, replacement, genre and
  outcome.
- `repo-exposure.json`: the `repo-exposure-json` 0.4 seams from the same
  checkout that start on a mutant's line or contain a mutant span, reduced to
  identity, kind, location, span and grip class. Seam expressions are omitted.

The reduction keeps every join decision of the full snapshot: the full and
reduced runs produce the same matches, ambiguities and unmatched reasons.

## Expected joins

| Bucket | Count | Meaning |
| --- | ---: | --- |
| `span_containment` | 8 | the unique innermost seam span contains the mutant |
| `file_line` | 5 | no span contains the mutant; one span-less seam starts on its line |
| `ambiguous_span_overlap` | 6 | two seams with equal spans contain the mutant (`context.rs:40`, `context.rs:85`) |
| `ambiguous_file_line` | 0 | |
| unmatched `no_containing_seam` | 8 | seam spans exist in the file and none contains the mutant |

With the span ends removed (line-only evidence) the same inputs give 16
`ambiguous_file_line` records. With spans, none remain: 6 join by containment,
3 join a span-less seam by the line fallback, 1 is `no_containing_seam`, and 6
are equal-span ties.

## Update Command

```bash
ripr calibrate cargo-mutants --mutants-json fixtures/boundary_gap/calibration/span-containment-atuin/runtime-mutants.json --repo-exposure-json fixtures/boundary_gap/calibration/span-containment-atuin/repo-exposure.json --format json > fixtures/boundary_gap/calibration/span-containment-atuin/mutation-calibration.json
```

Use `--format md` for `mutation-calibration.md`.
