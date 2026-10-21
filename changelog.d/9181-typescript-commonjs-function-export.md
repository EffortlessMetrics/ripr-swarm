<!-- section: Fixed -->
- TypeScript/JavaScript: a CommonJS module whose whole export is a function
  (`module.exports = function (...) {}` or `module.exports = name;`) is now
  related to the tests that `require()` it and call the binding
  (`const parse = require("../"); parse(...)`), as the `export default` form
  already was. Before, such a module (minimist, rfdc, fast-safe-stringify) read
  `no_static_path` while its own suite failed every mutant of the line (#9181).
