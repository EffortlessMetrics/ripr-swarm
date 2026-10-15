# Independent exact-head / comparative review — issue #8132

Repository: EffortlessMetrics/ripr-swarm. Review date: 2026-10-15 UTC.
Scope: independently review #8135 and separately compare #8142 against the governing issue, rather than importing either candidate's precision oracle as the other's contract.

## Frozen subjects

| Subject | Commit | Tree |
|---|---|---|
| #8135 | `7d246801183e22bcf78e6c2efd556e387e27b1e2` | `10fbc4170b485e97d83e30d8d759254bc66777a2` |
| #8142 | `33be7c306c147e3b862451891164d73b2faaea91` | `b0f6ab1bb5a9d21fa1f4a43410406de4304ca44c` |
| Hosted integration basis | `ef12e0258a63e2476fcf9509d867152d6b36df80` | `d5e31485b68d173068906fc8c23baca7983d0c43` |
| #8135 hosted merge | `a8bc5c7439916f7b95b48ed4e961828caf50b725` | `39d150d9aeffefc32ff0e0b82221b153b62b10c0` |
| #8142 hosted merge | `e1f804a5c981ec69f906c2edcf9cf44459a38cb5` | `0ec38b460475d0cbba6d40dfc4d655b4047706d4` |

The merge objects' actual parents are the listed main and respective PR head. `subjects.txt` retains parents. Both submitted refs remain untouched. No ownership transfer was accepted by this review. Issue comment 6196600959 controls the overlap; #8142's body asserting successor ownership is not an acceptance record.

Production ledger blobs: #8135 `63f5d736dd30fd7f2091a2b695f00d79b6e8308f`; #8142 `a8a35d132c20cd3b156f3c914fd985449bc45f94`.
Additive experiment trees: #8135 `6299e24c4cd06a3d11f7a588a27dd2c231bfed2b`; #8142 `6711ad8c1b2f5458d512bfd0f51598ddd25da329`. Production is unchanged except an appended test-only module declaration. Exact reconstruction patches, commands, inputs and outputs are retained. These are experiments, not replacement candidates.

## Contract and inspected surfaces

Issue #8132 permits binding file/name/line to the same selected candidate **or withholding precision when identity is unresolved**. It requires retained runnable/oracle selection, deterministic order and complete rendered ledger output. It does not require maximum precision for every valid same-file namesake.

Read root AGENTS.md, .agents/skills/review-pr, factory review/rust/workflow/security guidance, repository review invariants, validation/resource policy, architecture/engineering/scoped-PR context, RIPR-SPEC-0028 and relevant output-schema entries. Read current issue coordination, both complete PR changes, PR bodies/reviews/threads and #8114/#8144 status.

Followed placement in `analysis/language/python/related_tests.rs`, selected evidence emission in `classify.rs`, Python repair card, check JSON cap/true pre-cap total in `output/json/report.rs`, ledger projection and optional-line serialization, agent eligibility/source anchor, and LSP `related_information_for_gap` consumption. Other-language line resolution is unchanged. No new command, dependency, credential, policy, schema field or support-tier promotion. Neither candidate edits the selector/diagnostics fences.

The real producer emits one selected file/name pair and a selected line when full candidates contain same-file/name duplicates. This makes #8142's selected-evidence authority credible on valid output. Its source is more permissive than #8135 for missing/invalid/undercounted totals and malformed unmatched rows, and uses the final evidence key values. Source differences alone were not treated as false-line reproductions.

## Independent observations

Pristine #8135: **116 ledger + 5 selector tests passed**, no ignored tests. Its additive review matrix: **8 tests passed**. #8142's unchanged production with the additive matrix: **122 ledger tests passed** (114 submitted/existing + 8 review functions), no ignored tests.

The review reused #8135's real parser → analysis → selector → card → capped check JSON → full ledger-record fixtures, including 16 cap/order/namesake combinations, 11 lossy inventories and 16 complete-record controls. Expectations were adjudicated separately for #8142. Additional evidence probes cover duplicate/conflicting keys, order, missing rows, valid complete/capped precision and corrupted totals after removing selected-line evidence. Full records are compared, including source anchor, commands, advisory eligibility and other fields, with repeated projection in the real cap matrix.

`8135-observations.json` and `8142-observations.json` contain 67 named observations each. The extractor verified **identical finding JSON inputs for all 67 pairings**; `comparison.json` preserves their correspondence. Raw logs retain complete JSON, plus baseline selected-first/second and original cap tests. The extractor uses JSON raw decoding because test-harness status text can follow a JSON record on the same log line; the original bytes are retained.

| Stimulus | #8135 line | #8142 line | Adjudication |
|---|---:|---:|---|
| Ordinary selected second | 60 | 60 | Required same-candidate positive |
| Valid 9-row inventory capped to 8; selected pytest28 / visible unittest25 | absent | 28 | Both meet #8132; 25 is forbidden |
| Valid complete same-file namesake fixture | absent | 22 | Conservative omission / valid evidence precision; both permitted |
| Ordinary record with absent/invalid/under total | absent | 60 | Observed tolerance difference; this fixture's 60 remains correct |
| Capped fixture, selected-line evidence removed, total absent/null/text/negative/fraction/under-counted | absent | 25 | Reproduced wrong line on **mutated JSON**, not current producer output |
| Capped fixture, append conflicting `suggested_test_line: 25` | absent | 25 | Reproduced final-key sensitivity on mutated evidence |
| Same conflict prepended before real line28 | absent | 28 | Conflicting evidence is order-dependent in #8142 |
| Duplicate identical line28 | absent | 28 | No false line; rejecting identical duplicates is not automatically mandatory |
| Unique row60 plus injected conflicting evidence99 | 60 | absent | #8135 trusts complete row; #8142 rejects disagreement |
| No related rows, valid selected evidence triple | absent | 28 | #8142 evidence fallback is broader than truncation alone; triple still supplies identity |

The corrupt-cap wrong-line probes require both loss of selected-line evidence and loss/corruption of the count, or an injected contradictory evidence key. Unmodified producer output did not reproduce a #8142 wrong line in this bounded matrix. Do not relabel these as a current valid-producer P1. They establish a robustness limitation if adopting an unqualified damaged-record fail-closed guarantee. A proposed repair could require explicit total parity/intact row identities for row fallback and unambiguous coherent selected evidence, while preserving valid evidence precision. No repair was authored here.

## Concern dispositions

- **Original #8135 P1, review5535095430/discussion4284518435:** technically superseded by the pinned repair. Independent native tests and full-record cap experiments no longer borrow line25. The historical finding was correct on 720e27; its thread remains unresolved per instruction.
- **#8135 discussion4285261393, complete-inventory namesake precision:** valid optional enhancement, not a required #8132 repair and not a basis for a new P1. Preserve complete-inventory/row-identity guards if pursued. The conservative matrix oracle intentionally tests the current permitted policy.
- **#8135 cleanup-on-panic, discussion4283973393:** still valid, non-blocking test hygiene. The temporary directory cleanup follows the assertion-containing closure and is bypassed by panic. No thread resolved. #8142's Drop guard improves normal panic cleanup; not a producer-contract discriminator.
- **#8142 space-prefixed identity, discussion4285218163:** repaired on the exact head by rejecting all-whitespace identity without changing comparison bytes; submitted precision test passes.
- **#8142 capped namesake concern/discussion4285218425:** valid selected-evidence authority is supported by source and the independent real-pipeline matrix; conservative #8135 withholding is not mandatory for #8142.
- **#8142 absent/malformed totals, ignored malformed unmatched rows, duplicate/conflicting evidence:** measured tolerance/robustness differences, with the mutated false-line limitations stated above. Not evidence that the current producer emits bad totals, malformed identities or duplicate triples. The matched missing/malformed identity controls withhold as observed; intact selected evidence can independently retain precision when row resolution fails.
- **Prior automated coverage/guard questions:** total comes from the real producer and is asserted through cap boundaries; count/identity guards are exercised separately by truthful-total controls; negative/non-integer selected row lines withhold; blank card identity withholds without byte trimming; AnalysisOptions growth fails compilation rather than silently bypassing tests. Module placement/extra helper suggestions do not establish a source defect. Default crate features include lang-python; the independent tests executed it. Retained 16-case matrix covers capped records both with and without visible namesakes.
- **Later #8142 automated review5536339481 / its 16:21 classification:** the module contains five `#[test]` functions, not twelve sibling tests; ten withhold cases are loop cases. Only selected-second is registered under RIPR-SPEC-0028. Broader registration is optional traceability hygiene here, not a reproduced behavior defect. Feature-gating is intended and default features include lang-python; native execution confirms coverage. No helper unification is required.
- **#8142 review5536109500:** author self-review remains distinct from this independent exercise and is not an approval. Its hosted-evidence gap has a concrete failing lane diagnosis now.
- **Overlap/ownership:** both candidates survive; no accepted transfer or supersession. The body claim alone cannot choose a winner.

## Hosted failure and integration boundary

Independently fetched both actual job logs and checkout identities:
- #8135 run38763943788/job116522145229 checked out `a8bc5c743…`.
- #8142 run38764853670/job116525279809 checked out `e1f804a5c…`.

Both Rust-only `cli_smoke` runs: **477 passed / 1 failed**, `structural_workspace_markers::scalar_marker_preserves_cargo_uv_git_and_unmarked_root_boundaries`, line182, uv marker selects Python but binary lacks lang-python; Cargo exit101. These runs are failed, not green. #8144 remains open. Existing #8114 remains open at `d9b57ef0b2877efc664944bd1f66a2a7bc7cad2b`; its actual patch skips uv when lang-python is disabled. No duplicate repair or rerun was created.

Main was ef12 at initial inspection and advanced by one #8140 TypeScript-only commit to `a184e8145f91d63eeb5c8290509c2ea643fde350` (tree `c7055ebcb27ef27699720539552bf1140cbba918`) at the pre-publication refresh. The new delta leaves Python, output, LSP, manifests and Cargo configuration byte-identical. Both candidates compose cleanly with that main: #8135 tree `d6d26ff540d8a661a2c9485dde5a3ad075a9f289`; #8142 tree `aa07226a03e04d42e1d56f3940173766fc9aa36f`. These latest compositions are inspected, not runtime-qualified; unrelated main movement does not invalidate the existing affected-path checks. Currentness refresh confirmed both submitted heads unchanged and the same substantive thread set. Python producer/card/check-JSON/ledger blobs are unchanged between each head and its named actual merge; the later-main delta affecting those paths is empty. LSP diagnostics changed on main, so focused diagnostics integration checks are required separately from head-only producer tests.

Pristine actual hosted merge checkouts ran independently in this review: #8135 `a8bc5c743…`: **116 ledger + 5 selector + 11 gap-diagnostic tests passed**; #8142 `e1f804a5c…`: **114 ledger + 5 selector + 11 gap-diagnostic tests passed**; no ignored tests. These establish focused local integration on the named merge trees, not green hosted CI. The first diagnostic namespace filter selected zero tests (exit0), retained as selection/instrument non-evidence; the discovered `gap_record_diagnostic` filter selected the 11 actual tests and passed on both trees.

Pre-publication check-run reads returned 27/27 current entries for #8135 and 40/40 for #8142 (per_page=100). Both retain the failed Rust-only lane. #8142 now has newly admitted run `38768377324`, with `Ripr Rust Small on GitHub Hosted / Rust gates` job `116538021623` **in progress**. This supersedes the earlier missing-admission statement, not the historical failed run. No terminal Rust Small Result or checkout-level qualification is claimed for that live run; no CI admission or rerun was performed by this review. Required-policy/aggregate completeness is not independently cleared.

## Disposition and next ownership recommendation

**#8135: REVIEW_INCOMPLETE for promotion; no blocking producer defect found on this exact head.** Original P1 repaired; optional precision and cleanup remain non-blocking. Hosted qualification remains failed/incomplete as recorded; complete required-check clearance is not established or waived by these local checks.

**#8142: comparative REVIEW_INCOMPLETE.** The valid-producer contract passed this bounded comparison with useful added precision. The damaged-record failure modes above must stay explicit; this review does not manufacture a valid-producer P1 from them or silently accept its replacement claim.

Recommendation, **not acceptance**: retain #8135 as the primary candidate for this claim, given its earlier coordination and explicit fail-closed inventory behavior; retain #8142 as comparative evidence and the precision/cleanup alternative until the issue coordinator records one candidate disposition. Complete-inventory evidence precision can be considered as a later bounded enhancement; it is not a condition of accepting #8135's current #8132 repair. Coordinate the existing #8114/#8144 gate repair and obtain fresh exact-head hosted qualification before promotion. Do not combine two competing implementations or delete either branch.

No submitted branch push, Ready/close/merge, review-thread resolution, bot contact, release/deployment, credential/policy/security change or destructive cleanup. Local test sources/targets and unique proof are retained for reconstruction. Full hosted gate-matrix completeness, broad fixture/golden/dogfood runs, live editor/pytest, Windows, combined #8096 runtime and release acceptance are not claimed. Strict Clippy/check-fast/precommit in the verified author packet remain retained author evidence, not an independently repeated lint run.

Packet freeze: 2026-10-15T16:35:30.250580+00:00. All review-owned Cargo drivers are terminal. The isolated mutable target (~853 MiB), four review-owned source checkouts and unique proof remain retained; original /workspace/ripr-swarm and its pre-existing cache remain untouched. No destructive cleanup. The separately admitted hosted run belongs to its existing PR workflow and remains pending.
