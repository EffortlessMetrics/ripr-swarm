# False-verdict ledger

Tracking issue: #9430, linked from the 0.11 tracker (#9271). Measured on main plus this change
(2026-10-22) with `cargo xtask verdict-corpus check-all`.

| Language | False | Cases |
| --- | --- | --- |
| Rust | 13 | 558 |
| TypeScript | 33 | 309 |
| Python | 25 | 191 |
| Perl | 1 | 60 |

Every false row is listed once in the `[[known_false]]` tables of
`fixtures/<language>-verdict-corpus/spec-coverage.toml`, each with its owning
issue. `verdict-corpus check` fails on a false row that is not listed, and on a
listed row that is no longer false, so a bless cannot absorb a new false
verdict and a fixed row cannot return unnoticed. Fix a row in the case, the
spec or the analyzer. Never exclude or reclassify it.

## Rows

| Case | Outcome | Root cause | Fix, in | Owner | Pinned by |
| --- | --- | --- | --- | --- | --- |
| `regex-syntax-max-scalar-two-byte` (Rust) | false actionable | The match-arm selector domain through a bounded caller loop is not derived. | Analyzer | #5503 | Row, with `regex-syntax-word-byte` as the negative |
| `jest-mined-private-impl-dispatcher` (TS) | false actionable | A module-private impl behind an exported dispatcher reads as a gap and the repair names a private symbol. | Analyzer: follow the exported caller | #6655 | Row |
| `perl-testmore-tier-far-from-boundary` (Perl) | false exposed | No row-owned predicate discriminator fact, so inputs far from the changed threshold look aligned. | Producer fact, then consumer | #8979 (perl-lsp-swarm#17237, #5562) | Row |

### Rows from the language-shape coverage batches

| Group | Cases | Owner |
| --- | --- | --- |
| Rust exact pin read as gap | `shape-thread-join-result`, `shape-sort-direction-truncate`, `shape-let-else-branch-pinned`, `-closure-returned-adder`, `-collect-result-short-circuit`, `-impl-trait-return-filter` | #9440 |
| Rust async owner | `shape-async-fn-block-on` | #9443 |
| Rust generated and fn-value owners | `shape-hostile-owner-as-path-value`, `-owner-through-fn-pointer`, `-cfg-attr-test`, `-macro-defined-owner` | #9442 |
| Rust const-generic and cast | `shape-cast-truncation-mask` | #9444 |
| TS credit on unreached arms | `shape-nullish-middle-operand`, `-template-plural-boundary`, `-async-catch-fallback-unexercised`, `-static-factory-rounding` | #9437 |
| TS silent enum member edit | `shape-enum-member-value-unread` | #9436 |
| Python language shapes | nine `py-shapes-*` cases | #9412 |
| Python idioms | `py-idioms-caplog-message`, `-caplog-level` (#9424), `-approx-constant-within-tolerance` (#9428), `-unittest-setup-sign` (#9412) | see each |
| TS Playwright and hostile | `pw-expect-soft-second-term`, `pw-step-heavy-parcel-threshold`, `pw-tothrow-anchored-regex`, `pw-loop-registered-rounding-mode` (#9434), `pw-fixme-clamp-upper-bound` (#9436) | see each |
| TS mocha and node:test | nine false actionable rows (`mocha-*`, `node-*`) | #9496 |
| TS skip and unexercised bound credited | `mocha-it-skip-sole-pin`, `mocha-clamp-unexercised-lower-arm`, `node-t-skip-inside-test`, `node-first-attempt-nonnegative-bound` | #9497 |
| TS operator grid | `ts-grid-*` (eleven rows) | #9515, #9532, #9533, #9534 |
| Python operator grid | `py-grid-*` (seven rows): the original three bare-boolean rows, then `-arithmetic-property`, `-arithmetic-table`, `-delete-helper`, `-returns-property` | #9542 (three), #9594 (four) |
| Python hostile inputs | `py-hostile-*` (five rows) | #9415, #9419, #9423, #9426 |

## Gate

- `known_false_gate` in `xtask/src/reports/verdict_corpus_known_false.rs`
  runs inside `verdict-corpus check` for a whole-corpus run, which
  `check-all` runs in CI (`rust-gates.yml`, "Verdict corpora"). A case subset
  never trips it.
- Each fix removes its entry in the same PR. The entry's removal is the
  regression test: if the row turns false again, `check` reports an unlisted
  false verdict.
