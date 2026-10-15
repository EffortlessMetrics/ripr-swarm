# PR #8004 corpus and live-receiver repair

Source candidate: `43f00b54eeb239c240bb71553c30a7bde3a4e080`, tree
`b520b108591cf046c3498e6558604c9f6101eac7`. This packet adds evidence only;
its commit is not another implementation candidate or an independent review.

The candidate preserves original head `15ca898`, adds repair `cd141db8`, and
merges inspected main `67866bc9` because of one actual test-file conflict.
`conflict-resolution.json` verifies the common prefix and both complete test
appends. The inherited collection-field suite passes. Later main `42b064fe`
adds an unrelated TypeScript change; its clean textual composition tree
`3ee0987f` was not executed. The branch was not restacked for that movement.

## Corpus adjudication

Both exact base `89ac47a6` and pristine head `15ca898` were built cleanly and
run on identical case inputs. The five cases move from exposed/credited to
weakly_exposed/gap. See `base-head-comparison.json`, the two partial corpus
reports, and the complete raw finding JSON under the three revision folders.
CLI related-test rendering is bounded; an absent serialized test row does not
establish absence from the full relation inventory.

- `bytesize-as-kib-div`: `ByteSize::gb(43).as_kib()` uses an arbitrary
  lowercase associated factory, outside the nominal constructor convention.
- `bytesize-as-mb-div`: `ByteSize::kib(938948).as_mb()` has the same boundary.
- `lib-rstest-fixture-injected-receiver`: `account: Account` is a typed
  parameter, outside this contract's still-visible simple local bindings.
- `serde-derive-rename-field-upper`: bare imported `UpperCase` in a table loop.
- `serde-derive-rename-variant-lower`: bare imported `LowerCase` in a table loop.
  The guard does not resolve the enum glob import or these pattern scopes.

These are conservative identity refusals with a real accuracy cost. No runtime
truth, ideal verdict or acceptable-verdict list was changed. The five observed
snapshot rows remain `false_actionable`, checked by `snapshot-invariants.json`.
They are not reclassified as acceptable advice. No blanket bless was run.

## Minimal source repair and controls

The legacy text prefilter rejected a qualified unit variant and borrowed an
expired foreign sibling `let`, hiding a valid later call on the actual outer
receiver. The shared AST check is now the inherent receiver authority in both
consumers. Arbitrary factories, parameters, unsupported patterns, wrong types
and item shadows retain their refusals.

`red-controls.diff` and `red-inherent-units.log` retain the pre-fix executed
assertion failures: one pass/two failures, no setup or compiler failure. At the
repaired committed head, three receiver units pass, including full/compact
parity and supported/unsupported boundary controls. Seven production receiver
integration tests, five Linux native-path tests and seventeen CLI rerun tests
pass. The separately inherited field suite passes one test over its fixture
matrix. These selected tests are not a full workspace-test result.

## Terminal local checks

Rust checks use Rust/Cargo 1.95, default features, one job, incremental off and
dev/test debug 0. The corpus helper additionally builds `lang-perl` for the
Perl cases, then restores the default binary. Test invocations use `--locked`;
xtask's internal builds use the unchanged repository lockfile without forcing
that flag. Corpus/scenario batches additionally use Rayon threads 2.

| Command | Terminal observation |
| --- | --- |
| `cargo test --locked -p ripr --lib inherent_receiver -- --nocapture` | 3 passed, 10,927 filtered |
| `cargo test --locked -p ripr --lib field_projected_collection_repo_seam_retains_indexed_target -- --nocapture` | 1 passed, 10,929 filtered |
| `cargo test --locked -p ripr --test private_inherent_receiver -- --nocapture` | 7 passed |
| `cargo test --locked -p ripr --test native_path_roots -- --nocapture` | 5 passed on Linux |
| `cargo test --locked -p ripr --test cli_smoke rerun -- --nocapture` | 17 passed, 503 filtered |
| `cargo xtask verdict-corpus check-all` | pass: Perl 36, Python 69, Rust 485, TypeScript 169 cases |
| `cargo xtask check-fast` | pass: 15 gates, no skips; independently checked 24 paths, merge-base 67866bc9 |
| `cargo xtask goldens check` | fail: one inherited JSON semantic drift and its two human renders; zero fixture errors |
| `cargo xtask dogfood` | pass; two analyzer scenarios produce 1 and 2 findings, plus nonempty documented action/report controls |
| honesty / traceability / doc-artifacts / spec-format | pass; individual command logs retained |

The golden failure is not hidden or re-blessed. On unchanged
`owner_return_pin_fallible_literal` inputs, all three retained binaries produce
identical complete parsed JSON with `weakly_exposed`; see
`inherited-golden-comparison.json` and `inherited-golden/`. This reproduces the
inherited limitation directly on the exact base, rather than inferring it from
an earlier narrative. The original doctor failure for a behind branch is also
retained; it did not authorize restacking. The actual content conflict did.

The required historical hosted run 38660627400 executed merge `2fa3df5b`
(head15ca + main9543a9b) and failed at the five corpus rows. Its retained job-log
excerpt corrects an inherited statement: 15,980 tests ran/passed **with 2
skipped**, and one doctest result had **1 ignored**. It is not qualification of
this new head. Artifact IDs 11939958384 and 11940815369 remain intact upstream.

## Scope, recovery and next owner

Source is pushed to the original PR branch and #8004 remains Draft. Rook owns
fresh independent exact-head review, explicit disposition of remaining macro
shadow/helper last-hop/same-name wrapper concerns, and eventual hosted
qualification. The repaired expired-shadow thread was not resolved. Parser
cost remains the separate #8005 follow-up. No merge/release/deploy, protection
bypass or external-thread resolution was performed.

After a reported executor disconnect, direct commands confirmed the same
clean head and original live validation session. No second writer, new repair
attempt or repeated expensive validation was started. All subsequent actions
continued that session or performed the named small inherited-golden replay.

`environment.json` records the tool paths and fixed inputs. `executables.json`
records the three actually retained executable digests and source identities;
the binaries themselves remain in the executor proof directory and are not
uploaded into Git. This is source/check evidence, not a distributable binary or
release acceptance packet. No private transfer URLs or credentials are present.

Lane resources are retained for the next review/reproduction: candidate and
base worktrees, serialized mutable build output, the three executable witnesses,
and raw logs. The shared root checkout, its Git common store and original target
were not moved, cleaned or deleted. Available space remains above the admitted
reserve. The existing source candidate and historical evidence remain intact.
