<!-- section: Fixed -->
- TypeScript/JavaScript: a CommonJS module whose whole export is one function
  (`module.exports = function (...) {}`, an arrow, or `module.exports = name;`
  of a single top-level function declaration) is now related to the tests that
  `require()` it and call the binding (`const parse = require("../");
  parse(...)`), as the `export default` form already was. The relation applies
  only when the file assigns `module.exports` exactly once and never uses
  `module` any other way (no alias, compound write, `Object.assign`, `eval` or
  `with`), and the test binding is not rebound. Before, such a module (minimist)
  read `no_static_path` while its own suite failed every mutant of the line
  (#9181).
