<!-- section: Fixed -->
- TypeScript and JavaScript: a method or constructor of a class that a test
  imports with `const { Cart } = require('./cart')` (or
  `require('./cart').Cart`) is related to the test that constructs it, as it
  is for `import { Cart }`, instead of reading `no_static_path`. A `var`
  require that another file-level declaration re-declares is no longer read
  as an import (#14088).
