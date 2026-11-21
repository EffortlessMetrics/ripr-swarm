<!-- section: Fixed -->
- TypeScript and JavaScript: a barrel that imports a name and exports it in a
  separate statement (`import { Help } from './help'; export { Help };`, the
  shape of commander.js's `index.js`) is followed like `export { Help } from
  './help'`, so a test that imports the name from the barrel relates to the
  changed function or class instead of reading `no_static_path`. Renamed,
  default and namespace imports resolve the same way; a name imported from
  another module, from a package, or as a type is not credited (#15940).
