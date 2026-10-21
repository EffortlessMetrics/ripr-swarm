<!-- section: Fixed -->
- Suggested Python verify commands now use the host's shell grammar: on
  Windows hosts, paths and selectors that need quoting are wrapped in double
  quotes (the cmd / PowerShell common denominator) with the pytest node id
  inside the quotes, instead of POSIX single-quote escaping that cmd and
  PowerShell misparsed. POSIX hosts keep the existing sh quoting unchanged
  (#8752).
