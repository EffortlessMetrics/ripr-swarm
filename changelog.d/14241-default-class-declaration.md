<!-- section: Fixed -->
- TypeScript and JavaScript: a method or constructor of a class declared as
  `class Cart {}` and exported by a later `export default Cart;` or
  `export { Cart as default };` is related to the test that default-imports
  and constructs it, as it is for `export default class Cart {}`, instead of
  reading `no_static_path` (#14241).
