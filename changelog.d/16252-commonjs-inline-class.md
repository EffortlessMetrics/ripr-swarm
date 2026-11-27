<!-- section: Fixed -->
- TypeScript and JavaScript: a changed method of a class expression that is a
  CommonJS module's whole export (`module.exports = class Cart { ... }`) is
  now reported, and relates to the test that loads it with `const Cart =
  require('./cart')` and constructs it. It used to produce no finding at all
  (#16252).
