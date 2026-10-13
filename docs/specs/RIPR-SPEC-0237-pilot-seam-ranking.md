# RIPR-SPEC-0237: Pilot seam ranking

Status: proposed

Owner: product / output

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #5770 (pilot top recommendations cluster on one function)
- #5411 (pilot ranked unresolved reach as the top gap; closed)
- #5497 (rank canonical actionability, withhold unresolved-reach
  limitations; open)

Linked PRs:

- #6294 (one seam per owning function per round; open, head `aba66e8d`)
- #5946 (grade weak grip only when activation is established; scores
  pilot's top picks against real mutants; open)
- #5295 (mutation spot-check harness; merged)
- #5480 (change-first pilot ranking; open, edits the same lines)

Support-tier impact:

- No tier change. No seam changes class and no seam gains credit. Only
  the order of `top_actionable_seams[]` changes (the #6294 owner spread),
  and the Markdown "Also in this function" line counts the same ranked
  population as the list beside it, as #6294 does. Claim boundaries remain
  governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. `pilot-summary.json` fields are unchanged; only
  the array order changes. The Markdown line is prose.
- Class membership of the ranked set is RIPR-SPEC-0236's. This spec owns
  only the order among admitted seams and the "Also in this function"
  count.

## Problem

`ripr pilot` writes its top N seams to `pilot-summary.json`,
`pilot-summary.md` and the terminal. The order decides what a user reads
first and which seam `next.repair_command` targets. No spec states the
order. RIPR-SPEC-0009 lists three ranking tests and names
`output/pilot/ranking.rs`, but has no rule text. The only prose is the
`top_actionable_seams[]` note in `docs/OUTPUT_SCHEMA.md`. It omits the kind
and id tie-breaks, and it does not say the evidence keys are booleans, not
counts.

On main `bcb0be576` the order is a single sort by `RankKey` (class, then
three evidence booleans, then path, line, kind, id), cut at `--max-seams`.
Adjacent seams of one function share their tests, so they sort together
and fill the list. Measured and inferred behavior:

| Ranking input | Today's order | Basis | What it shows |
| --- | --- | --- | --- |
| weak `a::clone` at `src/a.rs` 10, 11, 12; weak `a::as_str` at 40; weak `b::parse` at `src/b.rs` 5; `--max-seams 3` | clone 10, clone 11, clone 12 | inferred from `RankKey`; the #6294 author reports its new test fails with the spread disabled | one function fills the list |
| five spot-check crates, top 10 each | 19 distinct functions in 50 picks (humantime 2, semver 3) | #6294 PR body; not reproduced here | same, in the field |
| same, with #6294's spread | 50 distinct functions in 50 picks | #6294 PR body; not reproduced here | the spread does what it says |
| `strongly_gripped`, `intentional`, `suppressed`, `opaque` | `[opaque]` | main test `pilot_ranking_excludes_solved_governed_classes` | opaque is ranked, last |
| #6294 head: weak and opaque seams in one function, the weak one listed | "Also in this function" counts the opaque seam | inferred from `actionable_in_owner`, which uses `class_rank` | the count matches `top_actionable_seams` and `actionable_seams_total` (decision 2) |

Test evidence: #6294 at `26b3ec51` passed
`cargo test -p ripr --lib -- output::pilot` (32 passed, exit 0) in the
class and pilot research run. The current head `aba66e8d` adds two tests
(`pilot_ranking_counts_owner_rounds_across_classes`,
`pilot_summary_md_names_unlisted_seams_on_an_owners_first_pick_only`);
that head was read, not run.

#6294's own text disagrees with itself on one point. Its PR body says
rounds "stay inside a class". Its code and its OUTPUT_SCHEMA text count an
owner's rounds across all classes ("a function already ranked in a higher
class counts as having its first"), and a test pins that. The visible
order differs only when one function has seams in more than one class.

## Behavior

### Ranked set

The ranking runs over the classified seams pilot analyzed, after the
`RIPR_PILOT_SEAM_BUDGET` cut (default 2000,
`analysis::apply_pilot_seam_budget`). A seam outside that budget is not
ranked and is not counted anywhere below.

Which classes enter the ranked set is RIPR-SPEC-0236's decision. The set
is every class with a rank in the class order below, `opaque` included.
`strongly_gripped`, `intentional` and `suppressed` are never ranked.
Ranking does not read `[severity.seams]`: `SeverityConfig::for_seam` is
called only by the LSP diagnostics, SARIF, the badge summary, the agent
brief and review comments (`output/review_comments.rs`), never by pilot or the seam inventory. A class set to `off` is still
ranked.

### Class order

1. Rank 0: `weakly_gripped`.
2. Rank 1: `ungripped`.
3. Rank 2: `reachable_unrevealed`.
4. Rank 3: `activation_unknown`, `propagation_unknown`,
   `observation_unknown` and `discrimination_unknown`, tied. No unknown
   class outranks another; the keys below separate them.
5. Rank 4: `opaque`, after every other ranked class (RIPR-SPEC-0236
   keeps it ranked).

Class order always leads. No evidence key, location or owner round moves
a seam above a seam of a lower rank.

### Rank key

Seams are first sorted ascending by this key. Every field after the class
is a tie-break for the field before it.

1. Class rank.
2. Has a missing discriminator: any `missing_discriminators` entry sorts
   first. A boolean; one entry and five entries tie.
3. Has a related test: any `related_tests` entry sorts first. A boolean.
4. Has a suggested assertion:
   `suggested_assertion_for_classified_seam` returns `Some`. A boolean.
5. Display path, compared as a byte-wise string after `display_path`
   (forward slashes on every platform). `src/a/b.rs` sorts before
   `src/ab.rs`, which sorts before `src/z.rs`.
6. Display line, numeric.
7. Seam kind string (`call_presence`, `error_variant`, ...), byte-wise.
8. Seam id, byte-wise.

### Owner identity

A seam's owning function is the pair (file path, owner string). The owner
string is the seam's `owner()`, the owner's symbol id. Two seams have the
same owner only when both parts are equal. The same function name in
another file is another owner. Two owner strings that differ in any
segment (module, impl or name) are different owners. No name-only
matching.

### Owner spread

After the rank-key sort, each seam gets an owner round:

1. Walk the seams in rank-key order.
2. A seam's round is the number of seams of the same owner that came
   before it in that walk, in any class.
3. The final order is a stable sort by (class rank, round).

So within each class, round 0 of every owner comes before round 1 of any
owner, and seams that share a class and round keep rank-key order.

Rounds count across classes. A function already listed for a
`weakly_gripped` seam enters the `ungripped` or unknown classes at round 1
or later, so other functions' first seams in those classes come first. An
owner whose seams all sit in one class sees the same order as per-class
rounds.

Within a class, the round outranks the evidence keys. An owner's second
seam with a missing discriminator follows another owner's first seam
without one.

One seam per owner per round (cap 1).

### Truncation and consumers

1. Truncation happens after the owner spread: the list is the first
   `--max-seams` seams of the final order, or all of them when fewer are
   ranked.
2. `--max-seams` is a positive integer, default 5. `0` is rejected
   ("requires a positive integer").
3. `pilot-summary.json` `top_actionable_seams[]` and the Markdown
   "Ranked Seams" list use one ranking and show the same seams in the same
   order.
4. The terminal shows the first seam only (`top_actionable_seams` with
   N = 1).
5. The first seam of the final order is always the first seam of the
   rank-key order: it has the lowest class rank and round 0. The owner
   spread therefore never changes the Top Recommendation, the terminal
   pick or `next.repair_command`, which all read the first seam.

### "Also in this function" line

`pilot-summary.md` adds one line under a listed seam when both hold:

1. the seam is its owner's first seam in the listed top N; and
2. N_more > 0, where N_more is the number of analyzed seams with the same
   owner whose class has a rank in the class order above (the ranked
   set, `opaque` included) and which are not in the listed top N.

The line reads
`   - Also in this function: {N_more} more actionable seam not listed here`
for N_more = 1, and `seams` for more than one.

"Actionable" here means actionable per pilot ranking: the same
population as `top_actionable_seams[]` and `actionable_seams_total`, so
the line and the list beside it agree. `strongly_gripped`, `intentional`
and `suppressed` are never counted. An `opaque` seam is counted; it is
not headline-eligible, and the line does not claim it is.

The line appears only in Markdown. JSON gains no field and the terminal
gains no line.

### Determinism

Given distinct seam ids, the final order depends only on the set of
ranked seams, not on their input order: the rank key is total, the round
walk follows the rank key, and the second sort is stable. Paths are
compared after `/` normalization, so the order is the same on Windows and
Unix.

### Composition with other rankings

A later ranking bucket that must lead the class (for example #5480's
change-first bucket) goes in front of the spread key:
(bucket, class rank, round). Rounds still count per owner across the whole
walk. Putting the bucket after the round would lose the bucket order.

### What the mutant measurements establish

Two PR bodies report pilot top-10 picks scored against cargo-mutants on
five crates (semver, bytesize, humantime, strsim-rs, rust-hex), using
#5946's join rule (the seam's own expression, then line, then function).
A pick is "confirmed" when a mutant there was missed by the crate's tests.

| Change | Confirmed / scored | Refuted | Unscored | Distinct functions |
| --- | --- | --- | --- | --- |
| main `5281d78` (before #5946) | 9 / 37 (24.3%) | 28 | not reported | not reported |
| #5946 classifier | 12 / 41 (29.3%) | 29 | 9 | 19 of 50 |
| #5946 + #6294 spread (cap 1) | 13 / 35 (37.1%) | 22 | 15 | 50 of 50 |
| #5946 + spread, cap 2 | 14 / 37 (37.8%) | 23 | not reported | not reported |

The rows are consistent: #6294's "before" (12 confirmed, 29 refuted) is
#5946's "after" (12 of 41).

They establish, for those five crates at the recorded revisions:

- the spread moves the top tens from 19 to 50 distinct functions;
- refuted picks fell from 29 to 22 while confirmed picks held (12 to 13);
- cap 1 and cap 2 differ by one confirmed pick.

They do not establish:

- a general precision gain. Part of the rise from 29.3% to 37.1% comes
  from six more unscored picks (no viable mutant), which leave the
  denominator;
- that any ranked seam is a real gap, or that a pick's class is right.
  Each mutant outcome is runtime evidence from cargo-mutants. ripr's
  output stays static and makes no runtime mutation claim;
- anything about crates outside the five, or about classes other than
  those ranked in those runs;
- per-class versus across-class rounds. The measurement does not
  separate them.

The numbers come from the PR bodies and were not reproduced for this
spec. Real mutation testing remains the independent authority.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **Owner rounds count across classes.** Adopted: one round counter per
   owner over the whole rank-key walk, as #6294's code, its OUTPUT_SCHEMA
   text and `pilot_ranking_counts_owner_rounds_across_classes` do at head
   `aba66e8d`. #5770's reason holds in every class: an owner's seams share
   its tests, so a second pick in the same function, in any class, is
   likely to be right or wrong together with the first. Rejected:
   per-class rounds, which give a function a fresh first pick in each
   class and contradict #6294's code and test. #6294's measurement
   (19 to 50 distinct functions) is of the across-class version: commit
   `26b3ec51` already counted rounds over the whole walk, and `aba66e8d`
   changed only a doc comment in `ranking.rs` and added tests. Per-class
   rounds were not measured.
2. **"Also in this function" counts the ranked set.** Adopted (project
   coordinator, 2026-10-04, after the #6294 thread explained its
   intent): the count covers every ranked class, `opaque` included, as
   #6294 does. It counts the same actionable population as
   `actionable_seams_total` and `top_actionable_seams[]`, whose
   `class_rank` includes `opaque`, so the note never disagrees with the
   list it sits beside. Rejected: counting headline-eligible classes
   only (`SeamGripClass::is_headline_eligible`), which would make the
   note's "actionable" mean something different from the list's.
3. **Unknown classes tie.** Adopted: the four unknown classes share rank
   3. The missing-discriminator key already lifts unknown seams that carry
   a hint, which #5946 keeps on `activation_unknown` seams. Rejected:
   ranking `activation_unknown` above the other unknown classes, which no
   measurement supports.
4. **`opaque` ranks last.** Adopted: rank 4, after every unknown class,
   as main does and RIPR-SPEC-0236 keeps. Rejected: ranking it with the
   unknown classes.
5. **The round outranks evidence within a class.** Adopted, as #6294
   does. Rejected: sort by evidence first and spread only among ties,
   which lets one owner's hinted seams fill the list again.
6. **Cap 1 per round.** Adopted from #6294's measurement: cap 2 adds one
   confirmed pick (37.8% vs 37.1%), within noise, and cap 1 gives the
   most distinct functions. Rejected: cap 2.
7. **Path order is pinned.** Adopted: byte-wise order of the display path
   is normative, so tests and goldens can pin it. Rejected: calling
   location order unspecified, which would let renderers drift.
8. **No JSON field for the owner count.** Adopted: Markdown only, as
   #6294 does. Rejected: a `top_actionable_seams[].unlisted_in_owner`
   field, a schema addition with no consumer.

## Required Evidence

- Every acceptance example below, as a unit test on
  `top_actionable_seams` or `render_pilot_summary_md`.
- A discriminating test for decision 1: example 13 differs between
  across-class and per-class rounds.
- A test for decision 2: example 21 shows "1 more actionable seam" for
  an unlisted `opaque` seam, so the line and `top_actionable_seams[]`
  count the same population.
- A permutation test: shuffling the input of example 13 does not change
  the output.
- No change to any seam's class, `actionable_seams_total`, the JSON field
  set or `next.repair_command` from this spec.
- A class set to `off` under `[severity.seams]` is still ranked. Order
  reads neither severity config nor headline eligibility.

## Non-Goals

- No change to which classes are ranked (RIPR-SPEC-0236).
- No change to seam classification (RIPR-SPEC-0005, RIPR-SPEC-0230).
- No change-first bucket (#5480); only its composition rule.
- No canonical-actionability admission or withholding (#5497).
- No runtime mutation claim, and no precision target for pilot.
- No ranking for `ripr check` findings or the agent brief
  (RIPR-SPEC-0010's ranking is separate).

## Acceptance Examples

Notation: `class file owner line [flags]`. Classes: `W` weakly_gripped,
`U` ungripped, `R` reachable_unrevealed, `AU` activation_unknown, `PU`
propagation_unknown, `DU` discrimination_unknown, `O` opaque, `SG`
strongly_gripped, `I` intentional, `X` suppressed. Flags: `m` has a
missing discriminator, `r` has a related test, `a` has a suggested
assertion; none means all three are absent. Kind is `predicate_boundary`
unless stated. Output lists `file:line` in final order. "today" is main
`bcb0be576`.

1. Class beats evidence. `U src/a.rs f 10 [m,r]`, `W src/z.rs g 99`;
   N = 5: `src/z.rs:99`, `src/a.rs:10` (unchanged).
2. Evidence order. `W src/d.rs f 10`, `W src/c.rs f 10`,
   `W src/a.rs f 10 [r]`, `W src/b.rs f 10 [m]`; N = 5: `b`, `a`, `c`,
   `d` (unchanged; the four seams have four owners because the files
   differ).
3. Excluded classes. `SG src/a.rs f 1`, `I src/b.rs g 1`,
   `X src/c.rs h 1`, `O src/d.rs k 1`; N = 5: `src/d.rs:1` (unchanged).
4. Full class order. `O src/a.rs f 1`, `PU src/b.rs g 1`,
   `AU src/c.rs h 1`, `R src/d.rs k 1`, `U src/e.rs p 1`,
   `W src/f.rs q 1`; N = 6: `f`, `e`, `d`, `b`, `c`, `a` (unchanged; `PU`
   before `AU` by path, not by class name).
5. Unknown tie by evidence. `AU src/c.rs f 1 [m]`, `PU src/b.rs g 1`;
   N = 2: `src/c.rs:1`, `src/b.rs:1` (unchanged).
6. Evidence is a boolean. `W src/b.rs f 5 [m]` with three missing
   discriminators, `W src/a.rs g 10 [m]` with one; N = 2: `src/a.rs:10`,
   `src/b.rs:5` (unchanged).
7. Evidence precedence. `W src/a.rs f 1 [a]`, `W src/b.rs g 1 [r]`,
   `W src/c.rs h 1 [m]`, `W src/d.rs k 1 [r,a]`; N = 4: `c`, `d`, `b`, `a`
   (unchanged).
8. Path and line order. `W src/z.rs f 1`, `W src/ab.rs g 1`,
   `W src/a/b.rs h 1`, `W src/a.rs p 10`, `W src/a.rs q 9`; N = 5:
   `src/a.rs:9`, `src/a.rs:10`, `src/a/b.rs:1`, `src/ab.rs:1`,
   `src/z.rs:1` (unchanged).
9. Kind tie-break. `W src/a.rs f 5` kind `return_value`, `W src/a.rs f 5`
   kind `predicate_boundary`; N = 1: the `predicate_boundary` seam
   (unchanged).
10. One pick per owner first. `W src/a.rs a::clone 10`, `11`, `12`,
    `W src/a.rs a::as_str 40`, `W src/b.rs b::parse 5`; N = 3:
    `src/a.rs:10`, `src/a.rs:40`, `src/b.rs:5` (today `10`, `11`, `12`).
11. Rounds after the first. Same input as 10; N = 5: `src/a.rs:10`,
    `src/a.rs:40`, `src/b.rs:5`, `src/a.rs:11`, `src/a.rs:12` (today
    `10`, `11`, `12`, `40`, `src/b.rs:5`).
12. Same name, other file. `W src/a.rs fmt 1`, `W src/a.rs fmt 2`,
    `U src/b.rs parse 1`, `W src/c.rs fmt 1`; N = 4: `src/a.rs:1`,
    `src/c.rs:1`, `src/a.rs:2`, `src/b.rs:1` (today `a:1`, `a:2`, `c:1`,
    `b:1`).
13. Rounds cross classes. `W src/a.rs f 1`, `U src/a.rs f 2`,
    `U src/b.rs g 1`, `U src/b.rs g 2`; N = 4: `src/a.rs:1`, `src/b.rs:1`,
    `src/a.rs:2`, `src/b.rs:2` (today `a:1`, `a:2`, `b:1`, `b:2`, which is
    also what per-class rounds would give); N = 2: `src/a.rs:1`,
    `src/b.rs:1`.
14. Round beats evidence. `W src/a.rs f 1 [m]`, `W src/a.rs f 2 [m]`,
    `W src/b.rs g 1`; N = 3: `src/a.rs:1`, `src/b.rs:1`, `src/a.rs:2`
    (today `a:1`, `a:2`, `b:1`).
15. Unknown classes share a round space. `AU src/a.rs f 1 [m]`,
    `PU src/a.rs f 2 [m]`, `DU src/b.rs g 3 [m]`; N = 3: `src/a.rs:1`,
    `src/b.rs:3`, `src/a.rs:2` (today `a:1`, `a:2`, `b:3`).
16. Distinct owner strings in one file. `W src/lib.rs A::fmt 1`,
    `W src/lib.rs A::fmt 2`, `W src/lib.rs B::fmt 10`; N = 3:
    `src/lib.rs:1`, `src/lib.rs:10`, `src/lib.rs:2` (today `1`, `2`,
    `10`).
17. Top pick never moves. Any input above with N = 1 gives the first seam
    of the rank-key order: example 14 gives `src/a.rs:1`, example 12
    gives `src/a.rs:1` (unchanged). The terminal shows that seam whatever
    `--max-seams` is.
18. Fewer ranked than N. `W src/a.rs f 1`, `SG src/a.rs f 2`,
    `U src/b.rs g 1`; N = 10: `src/a.rs:1`, `src/b.rs:1` (unchanged).
19. Also line, plural, first pick only. `W src/a.rs a::clone 10`, `11`,
    `12`, `13`, `W src/b.rs b::parse 5`; N = 3: listed `src/a.rs:10`,
    `src/b.rs:5`, `src/a.rs:11`. Markdown has exactly one
    `   - Also in this function: 2 more actionable seams not listed here`,
    under `src/a.rs:10` and before `src/a.rs:11`.
20. Solved seams are not counted. `W src/a.rs a::clone 10`, `11`, `12`,
    `W src/b.rs b::parse 5`, `SG src/b.rs b::parse 6`; N = 2: listed
    `src/a.rs:10`, `src/b.rs:5`; one line "2 more actionable seams" under
    `src/a.rs:10`, none under `src/b.rs:5`. N = 5: no line.
21. Opaque is counted. `W src/a.rs f 1`, `AU src/a.rs f 2`,
    `O src/a.rs f 3`, `W src/b.rs g 1`; N = 3: listed `src/a.rs:1`,
    `src/b.rs:1`, `src/a.rs:2`;
    `   - Also in this function: 1 more actionable seam not listed here`
    under `src/a.rs:1` (the unlisted `opaque` seam; same as #6294 head).
22. Opaque counted when unlisted. `W src/a.rs f 1`, `O src/a.rs f 9`,
    `W src/b.rs g 1`; N = 2: listed `src/a.rs:1`, `src/b.rs:1`;
    "1 more actionable seam" under `src/a.rs:1` (same as #6294 head).
23. Unknown classes are counted, singular. `W src/a.rs f 1`,
    `PU src/a.rs f 5`, `W src/b.rs g 1`; N = 2: listed `src/a.rs:1`,
    `src/b.rs:1`;
    `   - Also in this function: 1 more actionable seam not listed here`
    under `src/a.rs:1`.
24. Budget bounds the count. Classified in analysis order
    `W src/a.rs f 1`, `W src/b.rs g 1`, `W src/a.rs f 2`,
    `W src/a.rs f 3`, with a pilot seam budget of 2; N = 5: listed
    `src/a.rs:1`, `src/b.rs:1`; no line (the two later seams were not
    analyzed).
25. JSON matches Markdown. For example 19, `top_actionable_seams[]` holds
    the three listed seams in the same order, and no object has an
    owner-count field.
26. Input order does not matter. Every permutation of example 13's input
    gives example 13's output.

## Test Mapping

- Existing (main): `crates/ripr/src/output/pilot/tests.rs`
  - `pilot_ranking_prefers_actionable_class_order_before_tie_breakers`
    (example 1)
  - `pilot_ranking_uses_evidence_tie_breakers_then_stable_location`
    (example 2)
  - `pilot_ranking_excludes_solved_governed_classes` (example 3)
  - `pilot_terminal_prints_top_test_and_follow_up_commands` (example 17,
    terminal top 1)
- Existing (on #6294 head `aba66e8d`, not on main):
  - `pilot_ranking_takes_one_seam_per_owner_before_a_second` (examples
    10, 11)
  - `pilot_ranking_spreads_owners_without_crossing_class_order`
    (example 12)
  - `pilot_ranking_counts_owner_rounds_across_classes` (decision 1, with
    `opaque` seams)
  - `pilot_summary_md_names_unlisted_seams_on_an_owners_first_pick_only`
    (example 19)
  - `pilot_summary_md_counts_an_owners_unlisted_seams_once` (example 20)
- Existing: `crates/ripr/src/analysis/seam_inventory.rs`
  `apply_pilot_seam_budget_inner_truncates_when_above_limit` (budget cut
  for example 24).
- Existing: `crates/ripr/src/cli/commands/pilot.rs`
  `pilot_rejects_non_positive_max_seams`.
- Planned: one unit test per remaining example (4 to 9, 13 to 16, 18, 21
  to 26). Example 13 fails under per-class rounds.

## Implementation Mapping

- `crates/ripr/src/output/pilot/ranking.rs`: `class_rank`, `RankKey`,
  `top_actionable_seams`, `actionable_total`; on #6294,
  `spread_across_owners` and `actionable_in_owner` (decision 2 keeps
  its `class_rank` filter; the count is the owner's ranked seams not in
  the listed top N).
- `crates/ripr/src/output/pilot/render/complete.rs`: JSON and Markdown
  use `max_seams`; the terminal uses 1; `next.repair_command` reads the
  first seam; #6294 adds the Markdown line.
- `crates/ripr/src/cli/commands/pilot.rs`: `--max-seams` default 5,
  positive only; applies the pilot seam budget before rendering.
- `crates/ripr/src/analysis/seam_inventory.rs`:
  `apply_pilot_seam_budget`, `DEFAULT_PILOT_SEAM_BUDGET`.
- `crates/ripr/src/analysis/seams.rs`: `RepoSeam::owner`.
- `crates/ripr/src/output/path.rs`: `display_path` (`/` normalization).
- `docs/OUTPUT_SCHEMA.md`: `top_actionable_seams[]` note.

## Metrics

- `pilot_ranking_order_mismatches`: acceptance examples whose ordered
  output or "Also in this function" line differs from this spec; must be
  zero.
