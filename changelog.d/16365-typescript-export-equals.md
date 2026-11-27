<!-- section: Fixed -->
- TypeScript: a module that ends with `export = Name;` and a test that loads it
  with `import x = require("./m")` now read like their CommonJS twins
  (`module.exports = Name;`, `const x = require("./m")`), so a test that calls
  the exported function or constructs the exported class relates to a change
  in it; before, both read `no_static_path` (#16365).
