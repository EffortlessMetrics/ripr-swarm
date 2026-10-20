<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0243 rule 14): a `describe` or `test` call whose
  callee the file, or an enclosing `describe` body, declares as its own
  function (`const describe = () => {}`, `function test() {}`) no longer
  donates its assertions. A suite registered through such a no-op never
  runs, so a changed owner it names now reads `no_static_path` instead of
  `exposed`. Imports, `require` members, `test.extend(...)` and other
  bindings are unchanged (#8959).
