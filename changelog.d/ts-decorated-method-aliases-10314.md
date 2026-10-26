<!-- section: Fixed -->
- TypeScript: a decorated method's pass-through return is now also credited
  when the test constructs the class through a named-import alias
  (`import { Ledger as Book }`) or a default import of an
  `export default class`; classes the test rebinds locally still earn
  nothing (#10314).
