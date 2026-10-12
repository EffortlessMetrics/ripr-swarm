<!-- section: Fixed -->
- The #5830 transitive caller walk is now memoized per owner
  `FunctionId` for the whole classification run instead of once per
  probe, so a function with several changed probes walks its callers
  once. Verdicts are unchanged: `verdict-corpus check` and goldens are
  byte-identical before and after (#7024).
