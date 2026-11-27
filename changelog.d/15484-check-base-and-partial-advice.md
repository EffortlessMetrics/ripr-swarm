<!-- section: Fixed -->
- `ripr check` now names a fix that works for three dead-end messages. An
  unresolved `--base` that is a commit id names `git fetch <remote> <id>` (or
  the full-id form for an abbreviated id, plus `git fetch --unshallow` in a
  shallow clone), and one spelled like a version tag (`v1.2.3`) names
  `git fetch --tags`, instead of branch fetch advice that never brings either
  in. The remote-tracking advice is a capitalized sentence that quotes
  `origin/topic` when that short name resolves to the fetched branch. A
  partial run whose whole diff exceeds the analysis-cost limits also says to
  narrow the diff with a PATH argument or a nearer `--base`, since raising the
  budgets alone adds one file per run (#15484).
