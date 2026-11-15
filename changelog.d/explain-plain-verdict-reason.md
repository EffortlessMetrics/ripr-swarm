<!-- section: Changed -->
- `ripr explain` now says in plain words why a finding got its verdict: the
  `Static exposure` section adds the same `Why <verdict>:` line the check
  summary prints (for example `Why weak: a related test reaches this change,
  but no assertion tells the old and new behavior apart`). For findings ripr
  could not settle statically (unclear findings and unresolved call edges),
  the "Why this verdict" block no longer repeats the
  paragraph-long Next step or the stop-reason list the finding block already
  shows; it points at Next step instead. `Tests examined: 1 listed (3
  assertion row(s) of 3)` now reads `Tests examined: 1 (3 assertions)`.
  Verdicts, classes, evidence and next steps are unchanged;
  `--format human-full` is unchanged (#14811).
