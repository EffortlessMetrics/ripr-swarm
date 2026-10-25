# Path to a great default-tool experience

Tracking issue: #9278. The 0.11 cut has its own tracker (#9271); this page is the
experience bar only, and says per gap whether it blocks 0.11 or can follow it.

Bar: ripr is the tool a developer reaches for by default. That means a fast
and light inner loop, verdicts that are trustworthy and say why and what to do
next when they refuse, frictionless install and first run, and behavior that
holds on real-world and hostile inputs. Every gap gets a scoreboard row with a
trend and a regression gate. Fixes keep the actionability flip fail-closed.

Sources differ by row. The first-run timings were measured on main 4d91ae82
on 2026-10-21 (release build, 4-core Linux container, one run each, not
benchmarks). The speed figures come from the committed
`metrics/dx-scoreboard/baseline.json` (revision 10e56371, hosted runner,
marked runner-dependent) and are the worst repo of four, always `ripr-swarm`
itself; serde, ripgrep and regex meet their targets. Corpus rates are the
coordinator's figures of 2026-10-21 and may differ by a case or two from
the 0.11 readiness report (kept in project files, not this repo; tracker #9271).

## Re-score on main a6909e94 (2026-10-25)

Release build, 4-core Linux container, one run each unless stated; these are
observations, not benchmarks. The 2026-10-21 figures further down are kept
as the previous point on each trend.

| Dimension | 2026-10-21 | 2026-10-25 | Verdict |
|---|---|---|---|
| Inner loop, small crates | first-run steps at or under 0.4 s | still at or under 0.34 s (doctor 0.07 to 0.27, check 0.17 to 0.22, pilot 0.14 to 0.34) | meets |
| Inner loop, `ripr-swarm` itself | warm check 25 to 87 s on product Rust diffs | one-line edit under `crates/ripr`: 27 s first, then 15.1 s and 15.0 s against a 2 s target | misses; owned by the warm-check work |
| Verdict trust, Rust | 5/499 false verdicts | 5/636 (0.8%), contradictions 0/804, spec examples 202/205, oracle accuracy 44/44, operator grid 21/21 | rate down from 1.0% to 0.8%; the five remaining rows were not re-triaged for owners here |
| Verdict trust, other languages | TS 3/178, Python 0/80, Perl 1/60 | coordinator figures of 2026-10-24: TS 9/351, Python 2/224, Perl 2/81 (counts rose with the corpus) | owned by the corpus threads |
| Adoption, first impression | 2 of 3 first-run crates ended `static_unknown` | all three end `exposed` (semver, fastrand, bytesize); generated workflow 174 lines and installs the published release | #9276 looks fixed on main; confirm and close |
| Hostile input | oversized diff all-or-nothing (#9277) | `--path`/`--exclude` shipped (#9330); an empty filter is an error; `@generated` files are skipped; 20,000-deep parentheses, a 50 MB single line (34 s), a symlink loop, BOM with CRLF, a Unicode path, no manifest, a syntax error and 3,000 nested modules all finish without a panic | meets, with the gap below |
| Hostile input, size scaling | not measured; no gate fails when time or memory stops being linear in input size | probe of 2026-10-25 (debug build, one run each): Python production file 3.6 s/MB, Rust production 4.7 s/MB, both linear; TypeScript test file 5 MB single line 187 s before #10330 and about 20 s after; Python test file with 8,000 tests 13 s and 3.6 GB before #10359 and 2 s and 93 MB after; Rust test file 20 s/MB (110 s at 4 MB), time 2.1x to 2.5x per doubling, unexplained (#10363); 3,000-deep nesting and 17 MB files end in 0.1 s with a named limitation in all three; Perl not established without a producer (#10364) | misses; gate and trend row owned by #10362, TypeScript residuals on #10017 |
| Agent loop | not on this page | last measured 2026-10-04 on an older build: 17 to 21 `ripr` commands and 19 to 22 tool steps to close one gap (targets 10 and 15); no baseline compare gates it | unmeasured on main and ungated (#10183) |

Two measurement traps found on the way. `cargo xtask first-run` runs the
`ripr` on `PATH` unless `--ripr <path>` is passed; without it the walk
measured the published 0.10.0 (1,957-line workflow, `static_unknown`), not
main. And its friction count includes the intended development-version note
on `init --ci`, so that column cannot reach zero before 0.11.0 is published.

New gap, found by the hostile pass: with an unparseable `Cargo.toml`, the JSON
report says `analysis_complete: true` and `limitations: []` while stderr says
the path-dependency graph is limited and dependents may be missed. The
disclosure never reaches machine consumers (#10182).

## What meets the bar today

- Warm loop on small repos (single manual walk, not fixture-covered): `cargo xtask first-run` steps all at or under
  0.4 s (doctor 0.07 s, check 0.18 to 0.21 s, pilot 0.13 to 0.30 s,
  init_ci 0.02 s). Cold release build 6m06s.
- Refusals name the cause and the next step: outside a Git work tree, an
  unresolvable `--base`, `diff_scope_oversized`, and a non-UTF-8 file (reported
  as a named limitation while the real finding still prints).

## Gaps

Pick one row, comment on its issue with the branch and PR, and keep to one PR
per bullet in the issue. Acceptance criteria live in the issue.

| Gap | Evidence | Issue | 0.11 |
|---|---|---|---|
| Related tests not credited on real crates (aliases, trait dispatch, closures, integration paths, cfg) | dogfood umbrella; macro_rules table fix is PR #9208, open at time of writing | #8999, #6925 | #6925 yes, rest after |
| Weak reach lends a strong oracle from unrelated tests; retention shows file-location rows first | dogfood reports in #8999, #9175, #9169 (unverified here) | #9169, #9175, #8654 | after, unless a false actionable verdict results |
| First impression: all three first-run crates end `static_unknown` | first-run walk; on main a6909e94 all three end `exposed` | #9276 (verdict rows gated on a main build by `first-run-main-baseline.json`) | after |
| Large-workspace speed on `ripr-swarm` itself: warm check 5.1 s vs 2 s target; cold pilot 77.6 s vs 30 s and 1.59 GB vs 1 GB (baseline). Measured on main 4d91ae82 in the 4-core container: a warm check of a two-line Rust diff took about 25 s and a cold pilot 125 to 129 s. Re-measured on main afcfbf87 after #9406, #9517 and #9576 (one cold run, 4-core shared container, total wall time varies by tens of seconds between runs): cold pilot 157 s, peak RSS 1.2 GB | `metrics/dx-scoreboard/baseline.json`; container runs, one each, not benchmarks. Cold pilot phases on afcfbf87: seam evidence evaluation 112 s (heavy tail, owned by the cold-pilot evidence work), file-fact cache 21 s (17 s of it parse), cache store 11 s (was about 22 s serial; sharded writes #9406, overlapped planning #9517); the finalized cold index holds 374 MB live (was 428 MB, #9576), which does not move the peak inside evidence evaluation. Warm check on main 360798ec (4-core shared container, second run of the scoreboard recipe) is bimodal by diff: a commit with no Rust 0 s, two xtask files 3 s, tokio's pinned commit 0 s and cargo's pinned commit 3 s (168 MB), but commits editing product Rust under `crates/ripr` 49, 51 and 87 s; one 41 s run spent about 93 CPU-s, of which parsing 17 s (3,882 parses of 2,560 distinct sources) and the workspace-wide macro-binding scan 13 s (#9727 appeared to take about 11% off the wall time, one comparison on a noisy container). So it varies with workspace and diff together: serde, ripgrep and regex meet the bar, cargo's 3 s misses it, and this repository's 41 to 87 s misses it by far. The heavy `cargo` and `ripr-swarm-2026-10` rows now track a fast and a slow Rust-editing case; the `serde-no-rust` and `cargo-no-rust` rows (#10329) pin commits that touch no Rust and enter the baseline at its next hosted refresh, and all numbers here are one run each | #5363, #7220 | after |
| Editor refresh latency: the 17 to 22 s reports were an unanswered `workDoneProgress/create`, fixed by #9156 | lane re-measurement on serde: about 0.6 s, down from 30.7 s before the fix; refresh settles in 0.25 to 2.1 s; first diagnostics about 2.4 ms after `didOpen` | #5317, #9156, #9158 | closed by #9156 for serde-sized workspaces; large-workspace refresh follows the speed row |
| Oversized diff is all-or-nothing; generated files count toward the cap | 200k-line repro | #9277 (closed by #9330: `--path`/`--exclude`) | done |
| Dependabot PRs cannot pass admission or fixture identity checks | four PRs on 2026-10-21 | #9259, #9261 | after |
| Perl and corpus false verdicts (Perl 1/60; Rust 5/499, TypeScript 3/178, Python 0/80) | `verdict-corpus report` | #8979 and the corpus threads | yes |
| Install and CI routes: Windows signal on a current head, wheelhouse route, packaged routes on the exact candidate | the 0.11 readiness report (kept in project files, not this repo; tracker #9271) | #2769, #1703 (its milestone label says 0.12.0; confirm) | yes |

## Not established

Editor latency was measured by another lane on serde, not on `ripr-swarm`
itself. Install routes that need a published release cannot be checked before
the cut.
