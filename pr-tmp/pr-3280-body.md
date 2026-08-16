## Production delta

Closes #3280 — C1 of #3212, governed by **RIPR-SPEC-0151** (new, `proposed`): every finding now carries a producer-owned `source_currentness` disposition naming which revision owns the actionable source.

- **`domain/probe.rs`** — `SourceCurrentness` (`candidate_current` | `base_deleted` | `moved_or_renamed` | `unresolved_subject`), `SOURCE_CURRENTNESS_VALUES`, and a non-optional `Finding.source_currentness` with `#[serde(default)]` so pre-field artifacts read back as the explicit unknown, never a fabricated disposition.
- **`analysis/probes/diff.rs`** — `resolve_probe_source_currentness` from the diff evidence that seeded the probe: `after`-carrying → `candidate_current`; removed-only → `base_deleted`, or `moved_or_renamed` when the same trimmed expression re-appears among the file's added lines; evidence-less → the explicit unknown.
- **`analysis/language/rust.rs`** — diff loop resolves the disposition per probe; repo mode is `candidate_current` by construction. Preview producers (TS/Python/Perl) explicitly emit the unresolved unknown.
- **`output/json/report.rs`** — the field is always emitted as the final finding field; the preceding trailing-comma chain simplified accordingly.
- **Registration** — `policy/output_contracts.txt` (+ gate kind), `docs/OUTPUT_SCHEMA.md`, spec + README + doc-artifacts, `.ripr/traceability.toml`, 3 public-API baseline entries.

**Recorded coordinate unchanged** (review-hardened): an earlier revision of this PR recorded the base-side line on removed-only probes; adversarial review proved in the golden corpus that the base coordinate feeds the new-file flow/value classifiers (a `propagate` unknown→`yes` flip, a fabricated flow sink, a confidence change on the removed-line fixture) and contradicts the #1222 RANK-1 fixture contract. The disposition alone carries revision semantics in this slice; consumer re-coordination of deleted-side evidence (base identity, projections, edit-target suppression, SARIF/LSP/context-packet mirroring) is the #3212 projection slice (#3281).

## Evidence delta

- 4 new resolver tests (deleted-tail, moved-expression, added-seam, evidence-less, coordinate-stability, content-addressed-id guard) + 2 domain tests (wire vocabulary, pre-field artifact back-compat). **Discriminating proof**: resolver neutered → 3 tests fail; restored → green.
- **176 golden fixtures re-blessed** citing the spec; corpus diff vs `main` verified **exactly additive** — the only non-field lines are trailing commas (`"language": "rust",`).
- Zero classification/stage/confidence/count drift: the spec's guard is "the golden corpus diff is exactly the additive field."
- `cargo xtask goldens check`, `fixtures`, `dogfood`, `precommit`, `check-output-contracts`, `check-traceability`, `check-spec-format/-numbering`, `check-doc-artifacts/-index`, `check-static-language`, `check-no-panic-family`, `check-public-api`, `check-architecture`, and the rest of the battery pass; full lib suite 4280 green; `--all-features` compiles.

## Acceptance matrix (#3280)

| Acceptance | Status |
| --- | --- |
| Deleted function tail yields `base_deleted` | ✅ resolver test + corpus (12 `base_deleted` findings) |
| Added/modified seam yields `candidate_current` | ✅ resolver test + corpus (76) |
| Reused line + different expression does not inherit base identity | ✅ content-addressed-id test |
| Rename/move only with evidence; else moved/unresolved | ✅ moved-rule test; explicit unknown pinned |
| Mixed diff: deleted-side evidence + separate candidate-current finding | ✅ corpus fixtures |
| Existing findings retain classifications/identities | ✅ corpus diff additive-only; ids unchanged (content-addressed) |

## Non-claims

- No gate/ledger/diagnostic/repair policy consumes the field yet (C2 = #3281, including `moved_or_renamed` family-match tightening).
- Recorded coordinates unchanged; `multi_hunk_removed_line_wrong_target`'s SPEC.md (#1222) remains authoritative.
- Preview-language findings stay `unresolved_subject` until their producers resolve.

Candidate head: `caf744e4` (base `origin/main` @ `723414bb`, includes the #3288/#3290 rebase).
