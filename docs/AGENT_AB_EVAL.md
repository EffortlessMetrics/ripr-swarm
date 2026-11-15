# Agent A/B eval

Does giving a coding agent `ripr` make it write better tests, or cheaper ones?
This page records paired runs: the same agent, the same task, with and
without ripr. Governing issue: #12912.

## Result, 2026-11-15 (ripr 0.11.0 at `60114de3`)

72 runs: 12 tasks × 2 arms × 3 reps. Every run is listed in the appendix.

| | control | ripr | ripr / control |
|---|---|---|---|
| answer-key mutants caught | 100% | 100% | same |
| tokens per run (mean) | 58,744 | 63,944 | **1.09x** |
| tool calls per run (mean) | 7.5 | 10.7 | **1.43x** |
| wall seconds per run (mean) | 34.5 | 44.7 | 1.30x |
| test functions written (mean) | 12.7 | 10.8 | 0.85x |

Paired (same task, same rep): ripr costs +5,200 tokens per run (95% CI
+3,761 to +6,639) and +3.25 tool calls (95% CI +2.4 to +4.1).

**Reading.** Both arms hit the mutant ceiling on every task, including the
two harder multi-change tasks written for this rerun. So ripr showed no
detection benefit here. That is not evidence of no benefit: the
tasks are small, and the commit message alone was enough for the agent. The
cost side is measurable. ripr adds about 9% tokens (inside the 1.2x target)
and about 43% tool calls. On average 4.1 ripr commands per run.

Where the extra calls go:

- **Wasted first check (47% of ripr runs).** The agent runs bare
  `ripr check` first, gets `no_scope` because HEAD is the default base,
  then retries with `--base HEAD~1`.
- **DONE is unreachable on 7 of 12 tasks (#14821).** `verify=complete`
  appeared only on the single-change `ev_*` scenarios (15 of 36 runs
  overall). Every multi-change task ended `verify=refused`
  (`reason=unexposed_findings_remain`) on every rep, even with gaps=0 and
  tests that kill every mutant. Agents then spend explain and recheck rounds
  and end with a "STOP, not DONE" hand-off.
- **No signal on untested code (#14820).** On `hard_version`, FIND reported
  gaps=0 and a stop line while a new `compare` function had no test at all.
  The ten untested lines were counted as `no_static_path` limits. With a
  known-good test file the same task reported 6 `weakly_exposed` gaps, so
  the signal points the wrong way.

Per task (means over 3 reps; C = control, R = ripr):

| task | mutants | caught C/R | tokens C/R | tools C/R | ripr cmds | verify=complete |
|---|---|---|---|---|---|---|
| ab_invoice | 7 | 7/7 | 68,299/75,685 | 9.0/11.3 | 4.7 | 0/3 |
| ab_shipping | 5 | 5/5 | 56,864/65,016 | 6.7/11.3 | 4.3 | 0/3 |
| ab_slug | 5 | 5/5 | 61,107/71,260 | 8.0/11.3 | 4.7 | 0/3 |
| ab_window | 4 | 4/4 | 57,341/59,456 | 7.7/8.3 | 3.7 | 0/3 |
| ev_boundary_threshold | 1 | 1/1 | 54,368/57,685 | 6.0/9.0 | 3.3 | 3/3 |
| ev_changed_constant_return | 1 | 1/1 | 53,965/57,515 | 6.7/9.7 | 3.3 | 3/3 |
| ev_error_variant | 1 | 1/1 | 56,798/58,096 | 7.7/9.7 | 3.0 | 3/3 |
| ev_mixed_three_changes | 2 | 2/2 | 55,188/59,640 | 6.7/11.0 | 3.3 | 3/3 |
| ev_struct_field | 1 | 1/1 | 57,533/65,580 | 8.0/13.0 | 6.0 | 0/3 |
| ev_tested_control | 1 | 1/1 | 55,786/59,606 | 7.0/10.3 | 5.0 | 3/3 |
| hard_ratelimit | 5 | 5/5 | 62,795/66,902 | 7.7/11.7 | 4.7 | 0/3 |
| hard_version | 7 | 7/7 | 64,882/70,885 | 8.7/12.0 | 3.3 | 0/3 |

Compared with the 2026-11-08 pilot (#13051: n=1, larger model, 4 tasks):
1.09x tokens (unchanged) and 1.43x tool calls (was 2.17x).

## Method

**Tasks.** Each task is a small Cargo crate in a fresh git repo with two
commits. The second commit changes behavior and leaves a weak (smoke-only or
missing) test. The agent sees only the commit message.

- `ab_*` (4): the #13051 task crates. Base is a scaffold; head adds the feature
  and one smoke test.
- `ev_*` (6): the scenarios in `xtask/src/reports/agent_eval/scenarios.rs`.
  The answer key reverts each keyed changed line. `ev_tested_control` is
  already pinned by its existing test, as a control.
- `hard_*` (2): `ratelimit` (5 mutants) and `version` (7 mutants). These are
  multi-change commits with terse messages ("ratelimit: refill and take fixes").

**Controls.** For every task the original weak tests catch 0 mutants (except
`ev_tested_control`, which catches 1 of 1 by design), and a reference test file
catches all of them. So the scorer can tell good tests from weak ones.

**Arms.** One fresh agent per run, the same small fast model in both arms. The
prompt is identical except that the ripr arm adds the ripr binary path and
the `CLAUDE.md` block that `ripr init --agents claude` writes, as a
repository owner would have committed it. Agents must stay inside the trial
directory and must not edit `src/`. Every final tree left `src/` unmodified
(the scorer checks this). One ripr-arm run briefly wrote a scratch JSON file
outside the directory and removed it. That doesn't affect scoring.

**Scoring.** Each answer-key mutant is applied to `src/lib.rs` in a copy of the
agent's final tree, with a clean target directory. A mutant counts as caught
only when the crate compiles and `cargo test` reports a failing named test.
Tokens and tool calls are the harness's per-run totals. Tokens are dominated
by the shared system context (about 50k), so the paired delta is the useful
number.

**Excluded.** Two early `hard_*` runs used verbose commit messages that
spelled out the answer key. They were rerun with terse messages and left out
of the tables.

**Limits.** These are small first-party crates, with one model class and n=3
per cell. Nothing here measures real-repository behavior, larger diffs, or
whether ripr helps when the commit message is uninformative.

## Appendix: every run, 2026-11-15

`verify` is the last `verify=` value the ripr arm saw (`-` when the final check output was not captured). `wasted` marks a bare `ripr check` that returned `no_scope`.

| task | arm | rep | caught | tokens | tools | wall s | tests | ripr cmds | verify | wasted |
|---|---|---|---|---|---|---|---|---|---|---|
| ab_invoice | control | 1 | 7/7 | 68,294 | 9 | 65.8 | 23 |  |  |  |
| ab_invoice | control | 2 | 7/7 | 66,419 | 11 | 66.1 | 20 |  |  |  |
| ab_invoice | control | 3 | 7/7 | 70,184 | 7 | 74.3 | 25 |  |  |  |
| ab_invoice | ripr | 1 | 7/7 | 75,867 | 14 | 79.0 | 14 | 5 | refused | yes |
| ab_invoice | ripr | 2 | 7/7 | 84,930 | 13 | 97.8 | 18 | 6 | refused | no |
| ab_invoice | ripr | 3 | 7/7 | 66,258 | 7 | 51.2 | 15 | 3 | refused | no |
| ab_shipping | control | 1 | 5/5 | 56,341 | 6 | 25.2 | 14 |  |  |  |
| ab_shipping | control | 2 | 5/5 | 57,123 | 6 | 28.3 | 13 |  |  |  |
| ab_shipping | control | 3 | 5/5 | 57,128 | 8 | 25.2 | 15 |  |  |  |
| ab_shipping | ripr | 1 | 5/5 | 64,380 | 10 | 47.3 | 19 | 3 | refused | yes |
| ab_shipping | ripr | 2 | 5/5 | 68,183 | 14 | 59.1 | 10 | 6 | refused | yes |
| ab_shipping | ripr | 3 | 5/5 | 62,485 | 10 | 34.0 | 18 | 4 | refused | no |
| ab_slug | control | 1 | 5/5 | 64,881 | 9 | 57.9 | 19 |  |  |  |
| ab_slug | control | 2 | 5/5 | 59,393 | 8 | 40.6 | 17 |  |  |  |
| ab_slug | control | 3 | 5/5 | 59,048 | 7 | 32.0 | 15 |  |  |  |
| ab_slug | ripr | 1 | 5/5 | 75,740 | 14 | 80.0 | 21 | 7 | refused | no |
| ab_slug | ripr | 2 | 5/5 | 64,838 | 9 | 49.1 | 18 | 3 | refused | no |
| ab_slug | ripr | 3 | 5/5 | 73,201 | 11 | 86.6 | 24 | 4 | refused | yes |
| ab_window | control | 1 | 4/4 | 57,375 | 7 | 33.0 | 13 |  |  |  |
| ab_window | control | 2 | 4/4 | 58,822 | 10 | 38.3 | 12 |  |  |  |
| ab_window | control | 3 | 4/4 | 55,825 | 6 | 23.0 | 13 |  |  |  |
| ab_window | ripr | 1 | 4/4 | 59,595 | 8 | 32.0 | 11 | 4 | refused | yes |
| ab_window | ripr | 2 | 4/4 | 57,715 | 7 | 24.6 | 10 | 3 | refused | no |
| ab_window | ripr | 3 | 4/4 | 61,057 | 10 | 35.0 | 12 | 4 | refused | yes |
| ev_boundary_threshold | control | 1 | 1/1 | 53,736 | 5 | 15.0 | 8 |  |  |  |
| ev_boundary_threshold | control | 2 | 1/1 | 54,207 | 5 | 16.4 | 9 |  |  |  |
| ev_boundary_threshold | control | 3 | 1/1 | 55,161 | 8 | 20.8 | 8 |  |  |  |
| ev_boundary_threshold | ripr | 1 | 1/1 | 57,625 | 10 | 26.1 | 5 | 4 | complete | yes |
| ev_boundary_threshold | ripr | 2 | 1/1 | 58,447 | 10 | 25.8 | 5 | 3 | complete | no |
| ev_boundary_threshold | ripr | 3 | 1/1 | 56,984 | 7 | 23.1 | 5 | 3 | complete | no |
| ev_changed_constant_return | control | 1 | 1/1 | 53,022 | 5 | 13.8 | 4 |  |  |  |
| ev_changed_constant_return | control | 2 | 1/1 | 54,345 | 7 | 17.3 | 4 |  |  |  |
| ev_changed_constant_return | control | 3 | 1/1 | 54,529 | 8 | 19.5 | 4 |  |  |  |
| ev_changed_constant_return | ripr | 1 | 1/1 | 59,045 | 11 | 25.4 | 3 | 4 | complete | yes |
| ev_changed_constant_return | ripr | 2 | 1/1 | 58,172 | 10 | 28.7 | 3 | 3 | complete | no |
| ev_changed_constant_return | ripr | 3 | 1/1 | 55,328 | 8 | 22.5 | 3 | 3 | complete | no |
| ev_error_variant | control | 1 | 1/1 | 57,252 | 8 | 35.2 | 6 |  |  |  |
| ev_error_variant | control | 2 | 1/1 | 58,725 | 10 | 42.9 | 7 |  |  |  |
| ev_error_variant | control | 3 | 1/1 | 54,418 | 5 | 19.8 | 7 |  |  |  |
| ev_error_variant | ripr | 1 | 1/1 | 56,753 | 8 | 26.2 | 6 | 3 | complete | yes |
| ev_error_variant | ripr | 2 | 1/1 | 58,457 | 10 | 27.3 | 6 | 4 | complete | yes |
| ev_error_variant | ripr | 3 | 1/1 | 59,079 | 11 | 36.8 | 6 | 2 | complete | no |
| ev_mixed_three_changes | control | 1 | 2/2 | 54,882 | 7 | 20.9 | 7 |  |  |  |
| ev_mixed_three_changes | control | 2 | 2/2 | 56,489 | 7 | 26.2 | 13 |  |  |  |
| ev_mixed_three_changes | control | 3 | 2/2 | 54,194 | 6 | 14.3 | 7 |  |  |  |
| ev_mixed_three_changes | ripr | 1 | 2/2 | 60,084 | 11 | 31.7 | 4 | 3 | complete | no |
| ev_mixed_three_changes | ripr | 2 | 2/2 | 59,787 | 11 | 27.0 | 4 | 3 | complete | yes |
| ev_mixed_three_changes | ripr | 3 | 2/2 | 59,048 | 11 | 28.6 | 4 | 4 | complete | yes |
| ev_struct_field | control | 1 | 1/1 | 58,447 | 9 | 34.9 | 7 |  |  |  |
| ev_struct_field | control | 2 | 1/1 | 57,253 | 7 | 24.8 | 7 |  |  |  |
| ev_struct_field | control | 3 | 1/1 | 56,899 | 8 | 25.8 | 7 |  |  |  |
| ev_struct_field | ripr | 1 | 1/1 | 63,845 | 11 | 42.0 | 4 | 6 | refused | yes |
| ev_struct_field | ripr | 2 | 1/1 | 66,128 | 15 | 47.8 | 8 | 7 | refused | no |
| ev_struct_field | ripr | 3 | 1/1 | 66,767 | 13 | 42.8 | 9 | 5 | refused | no |
| ev_tested_control | control | 1 | 1/1 | 54,415 | 6 | 19.5 | 7 |  |  |  |
| ev_tested_control | control | 2 | 1/1 | 58,077 | 9 | 34.4 | 6 |  |  |  |
| ev_tested_control | control | 3 | 1/1 | 54,865 | 6 | 20.1 | 6 |  |  |  |
| ev_tested_control | ripr | 1 | 1/1 | 59,500 | 11 | 31.0 | 2 | 5 | complete | yes |
| ev_tested_control | ripr | 2 | 1/1 | 58,825 | 10 | 30.5 | 2 | 5 | complete | yes |
| ev_tested_control | ripr | 3 | 1/1 | 60,492 | 10 | 30.2 | 4 | 5 | complete | yes |
| hard_ratelimit | control | 1 | 5/5 | 63,706 | 8 | 62.4 | 22 |  |  |  |
| hard_ratelimit | control | 2 | 5/5 | 63,338 | 7 | 46.9 | 26 |  |  |  |
| hard_ratelimit | control | 3 | 5/5 | 61,340 | 8 | 48.6 | 20 |  |  |  |
| hard_ratelimit | ripr | 1 | 5/5 | 70,133 | 12 | 71.1 | 17 | 6 | - | no |
| hard_ratelimit | ripr | 2 | 5/5 | 65,119 | 10 | 49.0 | 15 | 4 | refused | yes |
| hard_ratelimit | ripr | 3 | 5/5 | 65,455 | 13 | 61.6 | 11 | 4 | - | no |
| hard_version | control | 1 | 7/7 | 66,421 | 10 | 53.5 | 30 |  |  |  |
| hard_version | control | 2 | 7/7 | 62,071 | 8 | 46.5 | 20 |  |  |  |
| hard_version | control | 3 | 7/7 | 66,155 | 8 | 51.1 | 15 |  |  |  |
| hard_version | ripr | 1 | 7/7 | 73,170 | 11 | 69.0 | 26 | 4 | - | no |
| hard_version | ripr | 2 | 7/7 | 71,878 | 14 | 69.1 | 29 | 3 | refused | no |
| hard_version | ripr | 3 | 7/7 | 67,607 | 11 | 59.8 | 16 | 3 | refused | no |
