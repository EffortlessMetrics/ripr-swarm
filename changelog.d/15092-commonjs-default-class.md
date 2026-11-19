<!-- section: Fixed -->
- TypeScript and JavaScript: a method or constructor of a class that is a
  CommonJS module's whole export (`class Cart {}` then `module.exports =
  Cart;`) is related to the test that loads it with `const Cart =
  require('./cart')` and constructs it, instead of reading `no_static_path`
  (#15092).
