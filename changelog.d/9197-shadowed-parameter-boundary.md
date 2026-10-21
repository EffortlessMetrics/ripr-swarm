<!-- section: Fixed -->
- A comparison operand that only name-matches an owner parameter is no longer
  credited with the test's observed input when the owner rebinds or mutates the
  name (`let x = x * 2`, `x += 5`, a shadowing pattern or closure parameter)
  before the compared line (#9197).
