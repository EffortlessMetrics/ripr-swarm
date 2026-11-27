<!-- section: Fixed -->
- TypeScript and JavaScript: a CommonJS module's private helper that the
  module's export calls now relates to that export's tests, as its ESM twin
  does (`module_entry_call`, at most `static_unknown`). Before, it read
  `no_static_path`. This covers `module.exports = { other }`, `exports.other =
  other`, `module.exports = other`, an inline whole-export function or arrow,
  and TypeScript `export = other` (#16369).
