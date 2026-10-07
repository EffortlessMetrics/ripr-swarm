<!-- section: Changed -->
- Verdict corpus: the required Rust gate now runs `cargo xtask verdict-corpus
  check-all` at Draft -> Ready and on main pushes, checking every
  `fixtures/<language>-verdict-corpus`, so a PR that moves a labeled verdict
  fails on that PR instead of drifting into main. Cases run in
  parallel (104 cases: 11 s sequential, 4 s on four cores). Subjects and
  cases are one file each under `subjects/` and `cases/`, the expected state
  is `expected/summary.json` plus one `expected/rows/<case>.json` per case,
  and `corpus_version` is gone, so parallel case PRs add files instead of
  conflicting on shared arrays. New `check --cases <id,...>` for the inner
  loop, `bless` to re-bless, and `split` to migrate a branch written against
  the one-file layout. The public-proof page assembles its
  verdict receipt from `expected/` (#6658).
