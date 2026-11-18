<!-- section: Changed -->
- `ripr check` no longer says "statically linked related test(s)" for tests
  that only share a name or file with the change. When every finding reads no
  static test path and every related test is such a match, the closing note
  now counts them as `N related test(s) with no established call path to the
  changed code`, which agrees with the finding's own `Related test` line. Any
  other mix keeps the earlier wording. Human text only; JSON, the agent format
  and classification are unchanged (#15051).
