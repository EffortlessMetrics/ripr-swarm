<!-- section: Fixed -->
- TypeScript preview: a changed literal return in one arm of a switch or
  if-chain that returns a distinct literal from each arm no longer reads
  `exposed` when every exact assertion expects another arm's result. Changing
  geolib's `case 1: return 'NNE'` under tests that expect only `'WSW'`,
  `'ENE'` and `'NE'` now reads `infection_unknown` and names the unpinned arm
  result; a test that expects `'NNE'` keeps `exposed` (#15306).
