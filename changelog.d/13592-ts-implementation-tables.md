<!-- section: Fixed -->
- TypeScript: a test suite that loops over a file-level table of namespace
  imports (`const impls = { js: cryptoJS, node: cryptoNode };` then
  `for (const [name, { digest }] of Object.entries(impls))`) now relates each iteration's bare `digest(...)` call to every listed
  module's `digest`, and module-entry reach continues from it. Before, the
  loop binding only shadowed the name, so what the export calls read
  `no_static_path` (#13592, unjs/ohash).
