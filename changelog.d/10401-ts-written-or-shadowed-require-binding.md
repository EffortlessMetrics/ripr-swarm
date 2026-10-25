<!-- section: Fixed -->
- A TypeScript or JavaScript test no longer relates to an owner through a `require()` binding the file writes again (`let s = require("./lib").s; s = other;`) or through a `require` the file defines itself, so those no longer read `weakly_exposed` (#10401).
