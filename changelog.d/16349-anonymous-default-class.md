<!-- section: Fixed -->
- TypeScript and JavaScript: methods of an anonymous default-exported class
  (`export default class { ... }`, including `export default class extends
  Base`, and `module.exports = class { ... }`) now relate to tests that
  construct the class through the default import or a whole-module `require`
  under any local name, as their named twins do; before, a change in such a
  method read `no_static_path` (#16349).
