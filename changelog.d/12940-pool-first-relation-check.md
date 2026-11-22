<!-- section: Fixed -->
- `review-comments` (and the PR comment and CI packet paths built on it)
  spends less time deciding which caller-file seams relate to an edited test.
  The check resolves owner calls only for the edited tests, keeps each
  function's answer across its seams, and no longer reparses a file for every
  enum-variant receiver lookup. On a 53-file ripr-swarm diff that edited 22
  tests, the check over 45,207 seams now finishes in 347 s; before, it did not
  finish inside the 600 s default. That diff still ends in the typed
  `limited_timeout` receipt, because 4,803 seams remain to evaluate (#12940).
