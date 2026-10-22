<!-- section: Fixed -->
- TypeScript findings also stop reading `exposed` when the only tested inputs
  pass the identity for an added factor or term (`return a;` becoming
  `return a * b;` with `b == 1`) or for an inverse operator swap (`/` to `*`).
  A related test that names the owner in a way ripr cannot read (`.map(owner)`,
  a generic call, an alias) keeps the credit (#9571, RIPR-SPEC-0248).
