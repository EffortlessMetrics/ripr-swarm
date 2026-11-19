<!-- section: Fixed -->
- An integration test that imports its library through one root glob (`use money::*;`) and calls a crate-root entry such as `cash_total(103)` is now related to the changed code behind that entry, as `use money::cash_total;` already was. A second glob, a glob of a library module, or an entry the crate root does not define or re-export keeps the call unrelated (#15073).
