<!-- section: Fixed -->
- TypeScript and JavaScript: a changed method of a class assigned to a
  CommonJS named export (`exports.Cart = class { ... }`,
  `module.exports.Cart = class ...`) is now reported and related to tests that
  `require` `Cart`; before, the run reported no behavioral change at all. A
  named export assigned again later in the file (by a class or any value) leaves
  the earlier definition without credit (#16275).
