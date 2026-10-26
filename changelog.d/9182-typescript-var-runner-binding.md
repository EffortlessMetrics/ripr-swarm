<!-- section: Fixed -->
- TypeScript/JavaScript: a CommonJS test runner bound with `var` or `let`
  (`var test = require("tape");`) that the file never reassigns now registers
  its tests like the `const` form. Before, every test in such a file was
  dropped and the owners it covered read `no_static_path` (#9182).
