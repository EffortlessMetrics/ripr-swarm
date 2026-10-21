# Path to a great default-tool experience

Tracking issue: #9278. The 0.11 cut has its own tracker; this page is the
experience bar only, and says per gap whether it blocks 0.11 or can follow it.

Bar: ripr is the tool a developer reaches for by default. That means a fast
and light inner loop, verdicts that are trustworthy and say why and what to do
next when they refuse, frictionless install and first run, and behavior that
holds on real-world and hostile inputs. Every gap gets a scoreboard row with a
trend and a regression gate. Fixes keep the actionability flip fail-closed.

Measured on main 4d91ae82, 2026-10-21, release build on a 4-core Linux
container. Numbers below are one run each, not benchmarks.

## What meets the bar today

- Warm loop on small repos: `cargo xtask first-run` steps all at or under
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
| Related tests not credited on real crates (aliases, trait dispatch, closures, integration paths, cfg) | dogfood umbrella; macro_rules table fix in #9208 | #8999, #6925 | #6925 yes, rest after |
| Weak reach lends a strong oracle from unrelated tests; retention shows file-location rows first | dogfood runs on bytes, nom | #9169, #9175, #8654 | after, unless a false actionable verdict results |
| First impression: all three first-run crates end `static_unknown` | first-run walk | #9276 | after |
| Large-workspace speed: warm check 5.1 s vs 2 s target; cold pilot 77.6 s vs 30 s and 1.59 GB vs 1 GB | `metrics/dx-scoreboard/baseline.json` | #5363, #7220 | after |
| Editor refresh latency (earlier reports of 17 to 22 s on a large workspace) | re-measurement owned by the "Editor refresh latency" thread | #5317, #9156, #9158 | 0.11 if it still reproduces |
| Oversized diff is all-or-nothing; generated files count toward the cap | 200k-line repro | #9277 | after |
| Dependabot PRs cannot pass admission or fixture identity checks | four PRs on 2026-10-21 | #9259, #9261 | after |
| Perl and corpus false verdicts (Perl 1/60; Rust 5/499, TypeScript 3/178, Python 0/80) | `verdict-corpus report` | #8979 and the corpus threads | yes |
| Install and CI routes: Windows signal on a current head, wheelhouse route, packaged routes on the exact candidate | `0.11-readiness.md` | #2769, #1703 | yes |

## Not established

LSP and editor latency on this build is not measured here. Install routes that
need a published release cannot be checked before the cut.
