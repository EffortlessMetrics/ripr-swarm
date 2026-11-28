<!-- section: Fixed -->
- TypeScript and JavaScript: after `module.exports = other;` (or `export =
  other;`), a named `require` or namespace member of another function in the
  same file (`const { total } = require(...)`) no longer relates to it as
  `exposed`; `require()` returns `other`, so that binding reads `undefined`
  unless the file adds the property (#16366).
