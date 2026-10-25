<!-- section: Fixed -->
- A TypeScript or JavaScript test that imports an owner by picking a member off a `const` `require()` namespace (`const lib = require("./lib"); const stringify = lib.stringify;`, also `lib["stringify"]` and `const { stringify } = lib`) now relates to that owner like the destructured spelling does, instead of reading `no_static_path` (#10447).
