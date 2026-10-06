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
runs with extra arguments. A supplied `--mutants-out` records
`cargo_mutants_args: null`, because the harness cannot see how that run was
invoked. Every repository also records `mutant_set_sha256`, a digest of the
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
