<!-- section: Fixed -->
- TypeScript and JavaScript: when a class defines the same method twice, the
  earlier body (which a later same-named member replaces and never runs) no
  longer relates to the tests that call the method; a change there reads
  `no_static_path` instead of borrowing the live method's tests (#16261).
