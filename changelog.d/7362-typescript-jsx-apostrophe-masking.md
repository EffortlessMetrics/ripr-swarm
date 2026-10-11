<!-- section: Fixed -->
- TypeScript: an apostrophe in JSX text (`<b>Don't {p(1)}</b>`) no longer
  hides a later owner call from the observation guard; a quote opens a string
  only where an operand is expected (#7362). Rule 8's post-ladder demotion now
  runs in one hook after the reach/observe/discriminate ladder (#7365).
