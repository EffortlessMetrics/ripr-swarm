<!-- section: Fixed -->
- TypeScript preview: a rounding-function swap (`Math.round` becoming
  `Math.floor`) tested only through a `for (const [a, b] of rows)` loop over a
  literal table of whole-cent rows no longer reads `exposed`; the loop's rows
  are folded one by one, so the finding reads `weakly_exposed` (#9653).
