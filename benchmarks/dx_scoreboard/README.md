# Developer-experience scoreboards

`cargo xtask dx-scoreboard` measures whether ripr is something a developer
would happily reach for, using numbers a developer feels rather than internal
counts. Each metric belongs to one board, carries a target bar, and has a
regression margin. The report lands at
`target/ripr/reports/dx-scoreboard.{json,md}`.

The definitions live in [`scoreboards.toml`](scoreboards.toml). Changing a
target, margin, or corpus pin is a reviewed edit to that file.

## Boards

| Board | What a developer feels | Metrics |
|---|---|---|
| `speed` | How long until ripr says something useful, and whether the edit-check loop stays interactive | cold `ripr pilot` time and peak memory, warm `ripr check` time and peak memory, per corpus repository |
| `ci` | What it costs to adopt ripr in CI | lines in the workflow `ripr init --ci github` writes, whether compiling ripr is its only install route, prebuilt install time (ingested from the install harness) |
| `trust` | Whether ripr is ever confidently wrong | commands that exit 0 on a missing repository, hostile-repository journeys (`crates/ripr/tests/hostile_repos.rs`) that neither find nor refuse cleanly, self-contradicting findings, false-verdict, false-actionable, false-exposed, false-silent and abstention rates (kept separate: fewer abstentions or false gaps are not evidence that credited verdicts are right) on the hand-checked verdict corpus (derived from the committed rows under `fixtures/rust-verdict-corpus/expected/rows/` by the `verdict-corpus:` source), false-actionable, false-exposed and false-silent rates on the authored TypeScript verdict corpus (derived the same way from `fixtures/typescript-verdict-corpus/expected/rows/`), the same three rates on the authored Python verdict corpus (derived from `fixtures/python-verdict-corpus/expected/rows/`), the oracle accuracy and overclaim rate on Rust corpus cases labeled with a spec-adopted related-test oracle kind and strength (targets 1.0 and 0; kept apart from the verdict rates, derived from the same rows), and the share of in-scope numbered spec acceptance examples (minus waived) that a verdict-corpus case cites (target 1.0; coverage, not correctness), mutation spot-check agreement on discriminator and gap claims plus join coverage (ingested), judged-panel false actionable rate |
| `paste` | Whether a printed command works when pasted | printed commands that break or run injected code under a hostile repository path, printed commands that drop the repository root |
| `first_run` | The new-developer journey from install to first useful result | install seconds (source build), time to first useful result, walk seconds per crate, failed steps, steps over budget, friction events, `*_unknown` verdicts (all ingested) |
| `agent` | Whether an agent using only ripr's help closes a real test gap quickly and without being misled | fix success, mutants caught, ripr commands and tool steps to fix, false weak findings left after the fix, stale re-checks, white-box tests written only for ripr, and how many of the stub calls `check` suggested produced a stub (ingested from the agent-as-user harness); share of `ripr agent stub` tests that compile and fail at their own todo, share of gap locations that get a stub, and `observer_required` refusals (ingested from the stub evaluation, source `agent-stub`) |
| `corpus` | Whether ripr keeps working on the shared pinned Rust corpus | per repository: whether the diff-scoped `ripr check` reached `analyzed`, and its wall time (ingested from `cargo xtask rust-corpus smoke`) |
| `ranking` | Whether `ripr pilot` sends people to the right places first | pooled top-5 and top-10 precision, confirmed and refuted counts, top-10 scored share, top-10 distinct-function share and top-10 pick count of pilot's picks against the checked-in mutation answer key (ingested from `cargo xtask pilot-ranking score`; see `benchmarks/pilot_ranking/README.md`) |

The rollup counts metrics that meet their target, fall below it, are not
measured, have a failed instrument, or regressed. A per-repository view groups
every per-repo sample, so a repository we intend to integrate with reads as
its own row set.

## Corpus

The corpus is a fixed list of real repositories pinned to exact commits. Each
pin is a commit whose own change touches production Rust, so the warm
`ripr check --base HEAD~1` measures a real small edit. Entries marked
`heavy = true` (ripr-swarm itself) run only with `--include-heavy`.
Checkouts live under `target/ripr/dx-scoreboard/corpus/`; cloning is opt-in:

```bash
cargo xtask dx-scoreboard --clone                  # all boards, light corpus
cargo xtask dx-scoreboard --clone --include-heavy  # what the nightly runs
cargo xtask dx-scoreboard --boards ci,paste        # seconds, no corpus
```

`--ripr-bin <path>` measures an existing binary; otherwise the command builds
release ripr first, because developers run release builds.

## How each instrument works

- **Cold pilot.** Fresh `RIPR_CACHE_DIR`, no `target/ripr` in the checkout,
  then the exact invocation the generated CI workflow runs:
  `ripr pilot --root <repo> --out <dir> --mode ready --max-seams 5`. The
  sample counts only when pilot exits 0 and `pilot-summary.json` says
  `complete`; otherwise it is `incomplete` and cannot meet its target.
- **Warm check.** `ripr check --root <repo> --base HEAD~1 --format json` twice
  on the cache pilot left behind; the second run is measured.
- **Peak memory.** `VmHWM` from `/proc/<pid>/status`, sampled in the same
  10 ms loop that enforces the deadline. Linux only; a lower bound.
- **Workflow size.** `ripr init --root <tiny crate> --ci github`, then the
  line count of `.github/workflows/ripr.yml`. The source-build flag is 1 only
  when `cargo install ripr` is the workflow's sole route; a fallback behind a
  prebuilt release download does not count.
- **False clean on bad input.** Twelve commands pointed at a repository that
  does not exist, run from an unrelated directory. Each should exit nonzero.
- **Self-contradictions.** Rules over the pilot `repo-exposure.json` and the
  warm check JSON:
  - `R1` a seam says reach is `no` while it lists related tests;
  - `R2` a finding is `no_static_path` while it lists related tests;
  - `R3` a finding's evidence says related tests were found while it lists
    none.
- **Paste safety.** A two-commit fixture crate at a path containing a space,
  `$(touch PWNED)`, an apostrophe and double quotes. `check`, `check
  --format human-full`, `pilot`, `first-pr` and `doctor` run against it from
  another directory. Every printed `ripr <command> … --option …` line (plain
  lines, backtick spans, bash fences; PowerShell forms skipped) is replayed
  under bash with `ripr` replaced by an argv recorder. A line is **unsafe** if
  bash rejects it, the canary file appears, or the root reaches ripr split or
  rewritten. It is **unbound** if it runs but neither passes the root nor
  changes into it.

## Ingesting other harnesses

The scripted first-run journey, the install harness and the mutation spot-check measure their
metrics elsewhere and hand them over with `--ingest <file>`:

```json
{
  "schema_version": "ripr-dx-scoreboard-input-v1",
  "source": "first-run",
  "evidence": "link or path to the run",
  "metrics": [
    {"id": "first_run.friction_events", "value": 3},
    {"id": "first_run.time_to_first_useful_result_s", "value": 412, "repo": "serde", "completed": true}
  ]
}
```

`source` must match the metric's `ingest:<source>` in `scoreboards.toml`, so
an ingest file cannot overwrite a number this command measures. `completed:
false` records a journey that never reached a useful result.

These native receipts are also accepted as-is:

- `first_run.v1` from the scripted first-run harness: time to first useful
  result (install step plus steps through the first successful `check`; not
  emitted when no install step was timed), friction events, and `*_unknown`
  verdicts.
- `first_run_row.v1` JSON Lines (one row per case, step and metric) from the
  first-run walk. Its gates map onto scoreboard metrics: nonzero `exit` rows
  are failed steps; `secs`, `stdout_lines` and `workflow_lines` rows over
  their own `budget` are over-budget steps; `friction_count` sums to friction
  events and any rise regresses; a missing `exit` row counts as a failed step;
  walk seconds per crate regress above 1.5x the same crate's baseline plus
  0.5 s (applied to each crate's total, not to every step). Malformed rows
  reject the file instead of counting as zero.
  `verdict` rows feed the `*_unknown` count, which is `review_on_change`: a
  changed verdict list is printed under "For review" and never fails the gate.
- `ripr-mutation-spot-check-v2` from the mutation spot-check: the agreement
  rate of the `claims_discriminator` and `claims_no_discriminator` families
  over `canonical_precise` records (operator mutants joined by `seam_id` or
  span containment to a predicate or return seam), and join coverage as
  `canonical_precise` over every runtime record. These compare static claims
  with real mutation outcomes on a sample; they are evidence about ripr's
  calibration, not a mutation result for the corpus. A
  `ripr-mutation-spot-check-v1` receipt is refused: its `seam_precise`
  operator-text pairings are a different population. For the same reason a
  baseline row whose samples were not ingested from a v2 receipt is reported
  as not comparable instead of gating a v2 value against it. The committed
  baseline has no measured value for these rows yet.
  These rates pool every repository in the receipt, so each row's evidence
  ends with its population as one JSON array: every repository's name,
  checkout revision, cargo-mutants version, `mutant_set_sha256` (a digest of
  the sorted mutant names, so selection arguments count even when unrecorded),
  `cargo_mutants_args` and `mutant_timeout_secs` (both `null` for a supplied
  `mutants.out`, whose run settings the harness cannot see; a shorter timeout
  turns slow caught mutants into unscoreable timeouts). The gate compares a row only against a
  baseline over the same population; a swapped repository, a moved revision,
  a different mutant set, timeout or cargo-mutants run reports "not
  comparable" with both populations, because the pooled rate can move with
  no verdict changing. Repository order does not matter. A population with an
  unrecorded (`null`) cargo-mutants version is never comparable, even to
  another unrecorded one, because it could hide an instrument change. A
  receipt repository without a name, revision or mutant-set digest, or with
  non-string arguments, an empty or non-string version, or a timeout that is
  not a positive integer, is refused.
  The receipt also carries the precision of `ripr pilot`'s top ten
  recommendations per repository (a recommendation is confirmed when a mutant
  on its line, or else in its function's body, was missed).
- `ripr-rust-corpus-smoke-v1` from `cargo xtask rust-corpus smoke`: per
  repository, `corpus.not_analyzed` (0 when the run reached `analyzed`, 1 when
  it failed closed, timed out or broke) and `corpus.check_ms` (the time of an
  analyzed run; any other run's time is incomplete). A repository the smoke
  could not run (`not_fetched`, `spawn_failed`) is incomplete on every
  metric, so against a baseline that analyzed it the gate reports lost
  completion rather than a pass.
- `ripr-pilot-ranking-v1` from `cargo xtask pilot-ranking score`: the
  pooled `ranking` rows, tagged with the corpus version as their repository so
  a baseline from another corpus version is uncompared. A run that could not fetch or score one of the
  pinned crates, or scored only a `--repo` subset, marks every row incomplete, so the gate reports lost
  completion instead of comparing a different population.

### The ranking lane

`.github/workflows/pilot-ranking.yml` runs nightly, on demand, and once when a
pull request touching pilot ranking, seam grading or the answer key leaves
draft. It never runs cargo-mutants; the labels are committed:

```bash
cargo xtask pilot-ranking fetch --allow-network
cargo xtask pilot-ranking score --ripr target/release/ripr
cargo xtask dx-scoreboard --boards ranking --ingest target/ripr/reports/pilot-ranking.json \
  --baseline metrics/dx-scoreboard/pilot-ranking-baseline.json --gate
```

### The corpus lane

`.github/workflows/rust-corpus.yml` runs the fast tier on demand and the full
tier nightly, then gates the receipt against
`metrics/dx-scoreboard/corpus-<tier>-baseline.json`:

```bash
cargo xtask rust-corpus fetch --allow-network --tier fast
cargo xtask rust-corpus smoke --tier fast --ripr target/release/ripr
cargo xtask dx-scoreboard --boards corpus \
  --ingest target/ripr/reports/rust-corpus-smoke.json \
  --baseline metrics/dx-scoreboard/corpus-fast-baseline.json --gate
```

A repository that analyzed in the baseline and now fails closed or times out
fails the lane; one that failed closed in both does not. To accept a changed
status (a manifest re-pin, or a fail-closed refusal that is now intended),
replace the baseline with the new `dx-scoreboard.json` in the same PR, which
puts the change in front of a reviewer.

## Targets versus the regression gate

A **target** is the bar a developer would feel. Missing it is a gap to close
and never fails the command.

The **gate** (`--gate --baseline <earlier dx-scoreboard.json>`) fails when a
metric, or any repository's own sample of it, is worse than the baseline by
more than `max(regression_pct% of baseline, regression_floor)` (their sum for
`regression_additive` metrics), when a repository stops completing, or when
an instrument breaks. The worst value is compared over the repositories both
reports measured, so a repository new to the corpus is listed as "not in
baseline" instead of failing the gate; one that does not complete still fails.
A repository that completes again after an incomplete baseline sample has no
value to compare, so it is listed as "completed again" and left out of the
compared worst.
Metrics the baseline measured that a run cannot compare, such as ingested
metrics without a receipt, are listed as not compared. Wall-time and memory
metrics compare only against a baseline from the
same runner class: host, OS, architecture, CPU count and, on Linux, the CPU
model from `/proc/cpuinfo` (for example
`github-hosted-linux-x86_64-4cpu-amd-epyc-7763-64-core-processor`), or
`RIPR_DX_RUNNER_CLASS` when set. Hosted runners with the same CPU count use
more than one CPU model, and wall time on identical code differed by about
1.7x between runs, so the model is part of the key. Counts and line totals
compare across runners.

`metrics/dx-scoreboard/baseline.json` is the committed baseline. To move it,
commit a newer report after reviewing why the numbers changed. A baseline
recorded on one runner class leaves speed metrics uncompared on another; the
report says so instead of passing them.

The scoreboard and fast-corpus baselines are reports from hosted runs on
`github-hosted-linux-x86_64-4cpu-amd-epyc-7763-64-core-processor`, the model
most hosted runs drew. Two EPYC 7763 runs of the same ripr code stayed within
5.2% of each other on every speed and memory sample (margins are 15% for
memory and 25% for time), and fast-corpus check times moved at most 14 ms. The
full-corpus job drew a different model on each of three runs (EPYC 9V74, Xeon
6973P, Xeon Platinum 8370C), so its baseline is the latest of those and its
check times compare only when that model recurs. A nightly that lands on
another model still gates counts and completion; its wall-time and memory
rows read "runner class differs". The nightly passes no `--ingest`, so the
scoreboard baseline holds no mutation spot-check or first-run values; a local
run that ingests those reports leaves them uncompared. To re-record, take the
`dx-scoreboard.json` printed in the lane's log group or uploaded artifact.

The nightly `.github/workflows/dx-scoreboard.yml` runs the full corpus with
the gate and uploads the report as an artifact.

The `first_run` board is measured by `.github/workflows/first-run-install.yml`,
which runs nightly and on demand (never on a pull request). It does a cold
`cargo install ripr --locked` from crates.io, walks the first-run path, and
ingests the receipt. It gates against
`metrics/dx-scoreboard/first-run-baseline.json`; until that file exists the
lane records the report ungated. To start gating, dispatch the workflow on
`ubuntu-latest`, review the `dx-scoreboard.json` artifact, and commit it as the
baseline file. Install seconds compare only against a same-runner baseline.

## Claim boundary

Numbers hold for the recorded revision, binary, runner class, and pinned
corpus. Targets are proposed bars, not product guarantees. Verdict metrics are
static and do not claim runtime mutation outcomes.

## Corpus-rate case populations

Verdict and oracle-rate samples carry `population` (`ripr-verdict-case-population-v1`):
`rate` and one entry per case with `case_id`, `definition_sha256`, `numerator`
and `denominator`. Zero-eligible contributions remain in the evidence. The
corpus owner supplies the contributions; findings, rather than cases, supply
the contradiction-rate denominator. The producer validates corpus definitions,
actual retained-file pins, complete expected-row coverage and row/definition
agreement. It loads and hashes each corpus once for all selected rates.
Perl packet-provenance projections retain every case identity, with `0/0`
contributions from the other packet cohort. Unsupported or absent projections
fail explicitly; they do not manufacture zero rates.
The two `oracle_accuracy` projections use the corpus owner's oracle summary;
unlabeled cases retain `0/0` contributions. Case and row oracle labels must
agree, and the stored observations must reproduce the row through the oracle
owner's scoring rules. This checks committed records, without recovering
finding file/line identities or original JSON truncation counts.

The whole-corpus value still determines the absolute target status. Regression
compares fractions over cases with the same ID and scoring-definition hash in
both observations, using the existing margins with exact integer cross-products
and the parsed margins' shortest round-tripping decimal values,
so floating subtraction does not turn an exactly allowed change into regression. JSON
`baseline.common` records both fractions and case count; `new_cases`,
`removed_cases`, `changed_definitions`, `added` and `removed` report composition
separately. Markdown names both the common-case comparison and the additions.
Missing, duplicate, inconsistent or incomparable population evidence enters
`gate.failed_comparisons` and fails `--gate`. It is not an unmeasured pass.
Spec-example coverage has its separate existing producer and floor contract.

Scoring identity includes corpus schema and projection policy, case/subject ID,
anchor, edit kind, diff bytes, truth state, ideal/acceptable verdicts, mutant
replacement/outcome semantics, and the subject origin, upstream pin and actual
retained-file hashes. Ordered sets are normalized.
Perl identities also include verified packet bytes, packet provenance and the
pinned fact producer. Other languages retain their original definition hashes.
Oracle-rate identities additionally bind the expected oracle kind, strength
and test selector. Oracle labels do not change verdict-rate identities.
Reasoning, spec citations,
labeling observations, replay commands, failing-test annotations, equivalence
reviews and `mutated_line` replay scaffolding are provenance rather than this
scoring identity. This hash does not assert full replay equivalence.

`testdata/7552-original-baseline-with-population.json` preserves the original
committed baseline with metadata-only evidence for its five Rust samples, from
revision
`10e56371911a4e6ef8020fbcd7e5f2692fc9b7f1`: corpus blob
`10b9f397e2c39f4f819b1f68659b5ff1dbf28675`, report blob
`0b038c45bb6b2fa25faf8708c57cf41e58522e6d`. Removing the two added sample fields
reconstructs the original baseline bytes (SHA256
`2df8bc3f48d6ec1384464d7baacfe8e31a24553dcfc019c6b4f9dfbacf667ea5`). All original
values, source details, revision and thresholds are preserved. #7552 enriched
the five original Rust operational samples with the same evidence and accepted
no refreshed values: a rate refresh needs its own policy decision and
attributable observation provenance. #7555 proposed a refresh and was held for
that decision. The later
`3961078c56b8c89258af56068fdf989f97b7d776` report has eight differing rows and is
not a replacement. Historical per-case contradiction denominators are absent
and were not reconstructed. Legacy TypeScript/Python observations lacked
attributable population evidence and failed comparison explicitly until
#8035 recorded them (below).

#7325 is that policy decision: the owner accepted the retained Rust abstention
rise, and the five Rust verdict-corpus entries of the operational baseline
now hold the trust-board observation at
`2af3f38a1de75ca48d5d84555a839d50710159b6` (false verdicts 29/477, false
actionable 26/236, abstention 177/477, false silent 0/241, false exposed
3/241). Each refreshed sample carries `population_provenance` with kind
`committed_expected_rows`, that revision, the `corpus.json` blob and the
`expected/rows` tree. The witness file above is unchanged; a test pins the
operational baseline to it in every other field, and the public-proof
scoreboard copy to the operational baseline byte for byte.

#8035 records the remaining thirteen trust comparisons at
`76651d1cdf9242b301c9ddf28c442838ccf73c59` with the same provenance shape,
approved in #8035: TypeScript false actionable 5/101, false exposed 0/68,
false silent 0/68; Python false actionable 0/35, false exposed 0/34, false
silent 0/34; Perl (producer packets) false verdict 8/27, false actionable
5/15, false exposed 3/12, false silent 0/12, abstention 8/27; and the Rust
corpus oracle accuracy 44/44 and overclaim rate 0/44. Each sample names its
language corpus `corpus.json` blob and `expected/rows` tree. No target or
threshold changed.

`testdata/7552-rust-abstention.json` freezes committed expected records at
`6da363f9b630845d82d8a073c42429d90cd03732`, rather than executing the analyzer.
It pins the actual retained regression, 58/203 to 74/203 (+16/203, above the
unchanged 0.02 margin), separately from 55/162 added abstentions and the whole
129/365 value. It is a regression-test witness, not a new baseline. The repair
does not make that gate green or change analyzer classifications (#7553).
