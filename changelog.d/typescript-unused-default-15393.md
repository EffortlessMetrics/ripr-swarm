<!-- section: Fixed -->
- TypeScript preview: a changed default parameter value no longer reads
  `exposed` when every test call passes that argument, so the default never
  applies. tinycolor's `wcag2 = { level: 'AA', size: 'small' }` on its own
  signature line now reads `reachable_unrevealed` and names the observed
  calls; a call that omits the argument or passes `undefined` keeps the
  verdict (#15393).
