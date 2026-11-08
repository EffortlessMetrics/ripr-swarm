# Mutation spot check

`cargo xtask mutation-spot-check` checks static grip verdicts against real
cargo-mutants outcomes on real repositories. For each checkout it writes a
`repo-exposure-json` snapshot, takes a cargo-mutants `mutants.out` directory
(supplied with `--mutants-out`, or produced with `--run-mutants`), and joins
the two through `ripr calibrate cargo-mutants`, so the join has one owner.
cargo-mutants records no source revision, so before scoring the harness checks
that every mutant diff's original lines still match the checkout. If any
differ, or a mutant has no diff (cargo-mutants before 27), it refuses the
directory and says why.

There is no committed corpus. The harness takes checkouts, records each
checkout's `HEAD`, and reports against those revisions. The first run used:

| Repo | URL | Revision |
| --- | --- | --- |
| rust-hex | https://github.com/KokaKiwi/rust-hex | `e25a8701f7f6b5ec4acedffcb4a45cfb69cc34eb` |
| semver | https://github.com/dtolnay/semver | `280ebcb6edac3aa4cdc545dbff8a26c5ac4861fe` |
| strsim-rs | https://github.com/rapidfuzz/strsim-rs | `dacc84c0dc61eff0ee0ff66962bcf2e17018ad26` |
| bytesize | https://github.com/bytesize-rs/bytesize | `66a3715e33369e99cb428bcd5aee32291f8e0a94` |
| humantime | https://github.com/tailhook/humantime | `76c8929b4cc286f675322475a8e1841f35bafc57` |

```bash
cargo install cargo-mutants --locked
git clone https://github.com/dtolnay/semver ../corpus/semver
git -C ../corpus/semver checkout 280ebcb6edac3aa4cdc545dbff8a26c5ac4861fe
cargo xtask mutation-spot-check --repo semver=../corpus/semver --run-mutants --jobs 3
```

The receipt is `target/ripr/reports/mutation-spot-check.{json,md}`.

Large repositories take hours to mutate in full. `--mutants-arg <name>=<arg>`
passes one argument to that repository's cargo-mutants run, and can be
repeated. Use it to narrow the run (`--file`, `--re`) or to choose packages
(`--package`, `--workspace`, which can widen it). The receipt records the
arguments, so the run can be reproduced, and the scoreboard marks rates from
runs with extra arguments. Each repository also records the
`mutant_timeout_secs` it ran with. A supplied `--mutants-out` records
`cargo_mutants_args: null` and `mutant_timeout_secs: null`, because the
harness cannot see how that run was invoked. Every repository also records `mutant_set_sha256`, a digest of the
sorted mutant names, so two receipts over different mutant selections never
read as the same population on the scoreboard.
Each value is one argument (`--re=decode`, not `--re decode`). Options the
harness owns or that would stop it writing outcomes, such as `--output`,
`--jobs`, `--timeout-multiplier`, `--in-place`, `--manifest-path` or
`--list-files`, are refused, as are their short forms, including bundles such
as `-vj8`.

Package selection follows cargo-mutants. In a workspace whose root is a
package, it mutates only that package unless given `--workspace` or
`--package`. In a virtual workspace it mutates the `default-members`, or every
package when there are none. Zola's root is a package, so reaching
`components/site` needs `--workspace`:

```bash
cargo xtask mutation-spot-check --repo zola=../corpus/zola --run-mutants \
  --mutants-arg zola=--workspace --mutants-arg zola=--file=components/site/src/queue.rs
```

## Refreshing the corpus

`.github/workflows/mutation-spot-check.yml` is the one place third-party crate
build scripts and tests run for this harness. It clones the pinned checkouts in
`benchmarks/mutation_spot_check/corpus.json` (name, URL, revision, optional
`mutants_args`), runs `cargo xtask mutation-spot-check --run-mutants` on a hosted
runner with read-only permissions and no secrets, and uploads the receipt as the
`mutation-spot-check` artifact. It runs weekly and on `workflow_dispatch`
(optionally limited to a comma-separated `repos` list). It is advisory: it never
writes the default branch, and its numbers gate nothing.

The harness writes the receipt only when the whole run finishes, and the job is
capped at 350 minutes, so a run that times out uploads nothing. If the first
full run does, split the manifest into smaller complete manifests and rerun each.
A `repos` batch is for trying a crate, not for promotion: before promoting, check
that the receipt's repository names and revisions exactly match `corpus.json`,
and never promote a filtered receipt while the manifest still lists more crates.

To promote a run, download the artifact, replace
`metrics/public-proof/mutation-spot-check.json` with it, carry over the recorded
top-level `timing_observation` (a fresh run does not measure time), run
`cargo xtask mutation-spot-check --rederive-summaries metrics/public-proof/mutation-spot-check.json`
and `cargo xtask public-proof --refresh-receipts`, and open a reviewed PR. To add
a crate, append it to `corpus.json` with a full 40-character revision (the first five keep their recorded pins). The README table
above lists the first five; the manifest is the current population.

## Derived summary rows

The v2 receipt also carries three additive rows derived from
`outcomes_by_disposition_and_grip_class` (no schema bump; consumers that read
other keys are unaffected):

- `recall_of_missed`: runtime-missed mutants across every disposition, counted
  by ripr grip class (`ungripped`, `activation_unknown`, `strongly_gripped`,
  `opaque`, `weakly_gripped`, `no_joined_seam`) and two labelled shares: the
  strict `ungripped_share_of_missed`, and `flagged_or_unknown_share_of_missed`
  (ungripped, weakly_gripped, activation_unknown and opaque, i.e. a gap or an
  honest unknown rather than a confirmed gap), with `not_flagged_missed`
  (strongly_gripped and no_joined_seam) for the remainder. `by_language`
  splits the same figures: Rust is established; Python is `not_established`
  until a real-mutant corpus exists and carries no numbers; TypeScript and Python are
  established only by a Stryker report or a cosmic-ray dump (see below) and say so with their
  `agreement_method`.
- `seam_coverage`: mutants total, caught and missed counts, `canonical_precise`
  and no-containing-seam counts, and their shares of all recorded mutants.

- `replacement_criterion`: the stated "replaces cargo-mutants" criterion, one
  row per criterion with `threshold`, `observed`, `status` (`met`, `not_met`,
  `not_established`, or `informational` for the unthresholded strict recall)
  and `scope`, plus an overall `verdict` (`replaces` only when every
  thresholded criterion is met and none is `not_established`; otherwise
  `not_yet` listing `not_met` and `not_established` ids). Thresholds are
  bars from the release ladder in docs/METRICS.md (R2: precision 0.97,
  flagged-or-unknown recall 0.80) or agent-chosen technical bars with the
  rationale in each row's `threshold_source` (scored coverage 0.50; ripr wall
  time at most 0.10 of full cargo-mutants). The time criterion reads the
  top-level `timing_observation` (`ripr_seconds`, `cargo_mutants_seconds`,
  crate, revision, tool versions, machine); it stays `not_established` when
  absent. The recorded observation is one crate (rust-hex), one diff, one run.
  Python/TypeScript agreement stays `not_established` in this criterion (both are scored separately by the synthetic-diff method), so `replaces` is unreachable until a corpus exists (fail-closed). A `replaces`
  verdict is not suite adequacy; scope is Rust on the recorded crates.

All are advisory, bound to the recorded revisions, and not suite adequacy. To
refresh a recorded receipt without re-running mutation testing:
`cargo xtask mutation-spot-check --rederive-summaries metrics/public-proof/mutation-spot-check.json`.

## What is scored

The receipt is `ripr-mutation-spot-check-v2`. `ripr calibrate cargo-mutants`
(calibration schema 0.2) owns the join; the harness gives every runtime record
one disposition and scores only `canonical_precise` records:

- the calibration joined the mutant by `seam_id` or by `span_containment` (the
  unique innermost seam span holds the mutated range);
- the mutant is a cargo-mutants `BinaryOperator` or `UnaryOperator`;
- the seam is a `predicate_boundary` or `return_value`;
- the outcome is `caught` or `missed`.

Every other record is excluded with its reason, and `canonical_precise` plus
the exclusions always equals the mutant count:

| Exclusion | Meaning |
| --- | --- |
| `file_line_only` | the calibration's line fallback joined a seam without a span; compatibility evidence, not a precise pair |
| `ambiguous_span_overlap` | equal or crossing innermost seam spans hold the mutant, so no single seam does |
| `ambiguous_file_line` | the line fallback found several span-less seams on the line |
| `unmatched_<reason>` | no seam joined; the reason is the calibration's `unmatched_reason` (`no_containing_seam`, `no_seam_on_line`, ...) |
| `unsupported_genre` | not an operator mutant, including `FnValue` whole-body replacements |
| `unknown_genre` | the mutant is missing from `mutants.json`, so its genre is unknown |
| `unsupported_seam_kind` | joined to a seam kind other than predicate or return value |
| `unscoreable_outcome` | timeout, unviable or another outcome that is not caught or missed |
| `unknown_join_method` | a join method this harness does not recognize |

| Grip class | Claim | Caught mutant | Missed mutant |
| --- | --- | --- | --- |
| `strongly_gripped` | a discriminator exists | agree | overclaim |
| `ungripped`, `reachable_unrevealed` | no discriminator exists | false gap | agree |
| `weakly_gripped`, unknown classes | no claim one mutant settles | unscored | unscored |

v1 scored `seam_precise` pairs: an operator mutant whose original operator
token appeared in the expression of a seam on the same line. v2 keeps that
check only as `operator_token_diagnostic` (`present`, `absent`,
`expression_unavailable`, `no_original_operator`) over the canonical precise
records. It never vetoes a canonical join, so a disagreement stays visible for
investigation instead of dropping an established pair.

Timeouts and unviable mutants are never scored. Claims are limited to the
recorded revisions, the cargo-mutants version in `outcomes.json`, and this
join rule. The report is advisory and is not a suite adequacy measure.

## TypeScript: real Stryker mutants (synthetic-diff verdict join)

The hosted run installs with `--no-package-lock`, so transitive dependencies
resolve fresh each week; the receipt records the package revision and Stryker
version but not resolved dependency versions, so weekly results can drift for
reasons outside ripr. The lane is advisory.

`ripr check --format repo-exposure-json` lists no seams for TypeScript (or
Python), which are diff-first, so the Rust seam-span join cannot score them.
`--stryker-report <name>=<mutation.json>` (with the matching
`--repo <name>=<checkout>`) scores them differently:

1. Read Stryker's JSON report (`files{<path>:{source,mutants[]}}`). Each
   mutant's `location` (1-based line and column, end exclusive, UTF-16
   columns) and `replacement` turn the report's `source` into the mutated
   source. A file whose report `source` differs from the checkout is excluded
   whole as `source_mismatch`.
2. Synthesize a one-hunk unified diff from the original to the mutated text.
3. In a scratch copy of the checkout (without `node_modules`, `.git`, ...)
   write the mutated file and a `ripr.toml` with
   `[languages] enabled = ["rust", "typescript"]` (`ripr check` enables only
   Rust unless told otherwise), then run
   `ripr check --diff <hunk> --root <scratch> --format json`.
4. Read back the finding whose probe lies on the mutated lines.

Runtime side: Stryker's detected status is `caught`; its undetected and
no-coverage statuses are `missed`;
everything else (`Timeout`, `CompileError`, `Ignored`, `RuntimeError`,
`Pending`) is `unscoreable_outcome`. Verdict side, tallied under the grip
classes of the table above (the raw token stays in `verdict_token`):

| ripr classification | Tallied as | Claim | Caught mutant | Missed mutant |
| --- | --- | --- | --- | --- |
| `exposed` | `strongly_gripped` | a discriminator exists | agree | overclaim |
| `reachable_unrevealed`, `no_static_path` | `ungripped` | no discriminator exists | false gap | agree |
| `weakly_exposed` | `weakly_gripped` | none one mutant settles | unscored | unscored |
| `infection_unknown` | `activation_unknown` | none | unscored | unscored |
| `propagation_unknown`, `static_unknown` | `opaque` | none | unscored | unscored |

A pair is `canonical_precise` only when exactly one classification and owner
lie on the mutated lines. Exclusions, so that `canonical_precise` plus them
equals the mutant count: `non_target_language_file`, `source_mismatch`,
`unscoreable_outcome`, `invalid_location`, `noop_mutant`, `over_mutant_cap`
(`--max-mutants`, alias `--stryker-max-mutants`, default 1500; when more mutants are eligible, an even
deterministic stride across the sorted report is run and the rest are
excluded; the receipt's `sampling` records `eligible` and `selected`),
`ripr_check_failed`, `analysis_incomplete`, `no_finding_on_mutated_lines`,
`ambiguous_verdict`, and `language_adapter_unavailable` /
`language_scope_unsupported` when ripr names that limitation for an empty
result.

The receipt stays `ripr-mutation-spot-check-v2`; these repos add
`language: "typescript"`, `agreement_method: "synthetic_diff_verdict"` and the
mutation tool identity. Their numbers are reported under the top-level
`synthetic_diff_verdict` row and in `recall_of_missed.by_language.typescript`
(which becomes `established` only with at least 30 scored claim pairs, i.e.
`canonical_precise` `strongly_gripped` or `ungripped` with a caught or missed
outcome; below that it is `insufficient_sample` with the counts and no share;
both carry the same `agreement_method`), and
are excluded from the Rust `scored_families`, `seam_coverage`,
`replacement_criterion` and pilot rows. The
`python_typescript_real_mutant_agreement` criterion stays `not_established`
(Python and TypeScript are measured only by the synthetic-diff
method, not the span-join agreement it is defined over). A mutated line is not a seam and the TypeScript
analysis is a syntax-first preview, so the figures say how often a diff-time
verdict agrees with a real mutant, not suite adequacy.

`.github/workflows/mutation-spot-check-typescript.yml` runs it weekly on the
pinned packages of `corpus-typescript.json` (Stryker 9.6.1 with the vitest
runner), uploads the receipt and a per-package Stryker exit-status file, and
ends with a step that fails the job when the receipt is missing or a package
failed. First hosted run (2026-11-08, Stryker 9.6.1): defu and ufo installed, ran
under Stryker and were scored (1,107 mutants); scule was dropped because its
vitest crashes the Stryker vitest runner. Figures bind to the recorded
revisions and are not promoted to the public receipt yet.
The join code (`xtask/src/reports/mutation_spot_check/synthetic_diff.rs`) is
language-generic; another mutation tool only needs a report parser like
`stryker.rs`.

## Python: real cosmic-ray mutants (same join)

`--cosmic-ray-dump <name>=<file>` (with the matching `--repo <name>=<checkout>`)
scores Python through the same synthetic-diff join. The file is the output of
`cosmic-ray dump <session.sqlite>`: one `[work_item, result]` JSON array per
line. The parser (`cosmic_ray.rs`) reads `module_path`, `start_pos`/`end_pos`
(`[line, column]`, 1-based line, 0-based code-point column) and the result's
`worker_outcome`, `test_outcome` and `diff`. cosmic-ray records no replacement
text, so the replacement is recovered from the diff's one removed/added line
pair against the checkout's own file, and the columns are converted to the
join's UTF-16 form. Every original-side line of the diff's hunk (context and
removed, at the hunk header's line numbers) must match the checkout, so a dump
from another revision is not scored. A dump does not name its producer, so
`--cosmic-ray-version <v>` records the version in `mutation_tool.version`
(null when omitted); the workflow passes the corpus pin. A mutant whose range does not reproduce its diff, or that
spans several lines, is not guessed at: its outcome token is
`unlocatable_diff` or `unlocatable_multiline`, so it lands in
`unscoreable_outcome` and stays countable in the receipt.

Runtime side: cosmic-ray reports a test timeout as a normal run with output `timeout`; that is `timeout`, never `caught`. A mutant that breaks import or collection also exits non-zero and reads as detected, so `caught` here is an upper bound on tests discriminating. A normal run whose tests detected the mutant is `caught` and a
normal run whose tests did not is `missed`; incompetent (`unviable`), timeout,
abnormal/exception (`runtime_error`), `no-test`, skipped and pending results
are never scored. One run scores one synthetic language: `--stryker-report` and
`--cosmic-ray-dump` together are refused, because the
`synthetic_diff_verdict` row is a single aggregate. Everything else about the
receipt, exclusions and claim boundary is as for TypeScript, with
`language: "python"` and `recall_of_missed.by_language.python`.

`.github/workflows/mutation-spot-check-python.yml` runs it on demand and
weekly on the pinned packages of `corpus-python.json` (cosmic-ray 8.8.0, local
distributor, pytest). Third-party code (the packages and cosmic-ray) runs only
there, never in an agent sandbox. **That workflow and corpus have not run**:
no package is confirmed to install, pass its baseline or finish a session, and
the cosmic-ray dump field names were taken from its source, not from a real
dump. The first run is the first evidence; it uploads the raw dumps beside the
receipt so a parser mismatch can be diagnosed.

## Python: real mutmut mutants (same join)

`--mutmut-dump <name>=<file>` (with the matching `--repo <name>=<checkout>`)
scores Python mutmut (3.x) outcomes through the same join (#13842). mutmut
has no machine-readable export of a mutant's location, so the workflow writes
the file: for each mutant listed by `mutmut results --all true` it records
`mutmut show <name>` as one `{"name", "status", "diff"}` JSON line. That diff
compares the original function with the mutant function from mutmut's own
module, so it is the real mutation, but its line numbers count from the
function and a method is dedented to column 0. The parser (`mutmut.rs`) finds
the hunk's original side (context and removed lines) in the checkout at one
constant indent (a line inside a multi-line string may also match verbatim)
and requires exactly one placement, so code repeated verbatim in two places
(`unlocatable_ambiguous`) is never scored, and neither is a hunk with no
placement (`unlocatable_diff`: a dump from another revision, or a package not
indented with 4 spaces, since `mutmut show` re-renders blocks at 4 spaces per
level). The replaced range is the smallest span (common prefix and
suffix removed) between the dump's own removed and added lines, mapped onto
the checkout (a line matched through the indent shifts by it, a line inside a
multi-line string does not), so multi-line mutants are located exactly; a diff with several changed hunks
is `unlocatable_multihunk`. A record with an empty diff has no path, so it is
counted by status in the receipt's `mutation_tool.records_without_diff` rather
than dropped. `--mutmut-version` records the version; the workflow passes the
corpus pin.

`mutmut apply` is deliberately not used. In mutmut 3.8.1 it replaces the first
function of the mutant's name anywhere in the module (any class's `__init__`,
or an `@overload` stub), and on the first hosted run it garbled the file for
about a third of itsdangerous and cachetools mutants.

Status mapping: mutmut's detected status is `caught` and its undetected status
is `missed`; `timeout`, `no tests`, `skipped`, `not checked`, `suspicious`,
`segfault` and the type-checker rejection (the tests never ran) are never
scored. As with cosmic-ray, a mutant that breaks import also reads as detected,
so `caught` is an upper bound on tests discriminating. One run scores one tool:
`--mutmut-dump` together with `--cosmic-ray-dump` or `--stryker-report` is
refused.

The `mutmut` job of `.github/workflows/mutation-spot-check-python.yml` runs it
weekly beside cosmic-ray (or alone with `tool=mutmut`) on the pinned packages of
`corpus-python-mutmut.json` (isodate, itsdangerous, tomli, python-slugify, and
cachetools at the cosmic-ray revision for a cross-tool comparison). Each package
must pass its own tests before mutmut runs, and mutmut must pass them again
under its trampoline: humanize was dropped because it does not (see the corpus
note), and tomli and python-slugify each skip one test that cannot run there. The recall figure the scoreboard
reads is `recall_of_missed.by_language.python`: of the runtime-missed mutants with a
scoreable verdict, the strict share ripr calls a gap and the share it calls a
gap, weak or unknown.

## Pilot top recommendations

The receipt also runs `ripr pilot --max-seams 10` on each checkout and judges
its ranked recommendations against the same outcomes, because a wrong top
recommendation is the first error a developer meets. A recommendation is:

| Verdict | Rule |
| --- | --- |
| confirmed | on a predicate or return seam whose expression occurs once on its line, a viable operator mutant starting inside that expression was missed (`seam` tier); with none, a viable non-`FnValue` mutant on the recommended line was missed (`line` tier, coarse: it can belong to another expression on that line); with none on the line, a whole-body (`FnValue`) mutant of the innermost function containing the line was missed (`owner` tier) |
| refuted | every such mutant was caught |
| unscored | no tier has a caught or missed mutant |

`pilot_top_recommendations.precision` is confirmed over confirmed plus
refuted, with counts split by tier (`seam`, `line`, `owner`) and grip class. The
owner tier is coarser: a caught whole-body mutant shows the function's tests
notice when the function does nothing, not that they discriminate the seam.
If `ripr pilot` fails, times out, or writes a summary whose status is not
`complete` (pilot's own budget ran out) on a repository, that repository's entry
carries `unavailable` with the reason and counts in `unavailable_repos`; its
mutation results still feed the rest of the report. The dx-scoreboard reads
the precision as `trust.pilot_top_recommendation_precision`, and publishes no
pilot row for a run with any unavailable repository, since that run measured
a smaller population than its baseline.

## Atuin regression

`atuinsh/atuin@90f590b9235556363ffb5b2c66728f8af3c27afe`, 27 cargo-mutants
27.1.0 records (`crates/atuin-ai/src/context.rs`,
`crates/atuin/src/logs/otel/enabled.rs`,
`crates/atuin/src/command/client/store/rebuild.rs`). The reduced inputs are
`fixtures/boundary_gap/calibration/span-containment-atuin/`, and
`atuin_multi_seam_lines_score_from_the_canonical_join` pins the result.

| Bucket | v1 (file/line join, operator text) | v2 (canonical join) |
| --- | ---: | ---: |
| Mutants | 27 | 27 |
| Scored | 1 `seam_precise` | 7 `canonical_precise`, all `span_containment` |
| Joined, not scored | 5 | 6: 5 `file_line_only`, 1 `unsupported_seam_kind` (`context.rs:78` `delete !` inside a call seam) |
| Ambiguous | 16 `ambiguous_file_line` | 6 `ambiguous_span_overlap` |
| Unmatched | 5 | 8 `no_containing_seam` |

The 7 scored mutants are 2 overclaims (`context.rs:72` `&& → ||` and
`otel/enabled.rs:62` `delete !`, both on `strongly_gripped` predicates) and 5
agreeing misses on `ungripped` predicates. The fixture's seams predate #5569;
a live run on a tree with #5569 reads those 5 seams as `opaque` (their reach is
unresolved), so they stay `canonical_precise` but unscored and only the 2
overclaims score. Receipt: `mutation-spot-check-v2` run of 2026-10-04. The 6 span-overlap ties are
`context.rs:40` (a predicate and a return seam) and `context.rs:85` (a return
and a call seam), where two seams share one span; they stay excluded even when both seams make the same claim, so the
hand-found `context.rs:40` overclaim from #5335 is not in the score.

`semver@280ebcb6` `src/display.rs:20` is the negative case: the `+ → *` and
`+ → -` mutants sit beside the call seam that starts on that line, so they are
`unmatched_no_containing_seam` and never scored
(`fixtures/boundary_gap/calibration/span-containment-semver-display/`).
