<!-- section: Fixed -->
- A TypeScript or JavaScript test that imports an owner as a member of a literal `require()` (`const stringify = require("./lib").stringify;`, also `var`, a renamed local and `require("./lib")["stringify"]`) now relates to that owner like the destructured spelling does, instead of reading `no_static_path` (#9188).
