<!-- section: Fixed -->
- TypeScript: an apostrophe in JSX text (`<b>Don't {p(1)}</b>`) no longer
  hides a later owner call from the observation guard. A quote opens a string
  only where an operand is expected or after a keyword such as `as`,
  `satisfies` or `from`, and whitespace now separates words (`x as`) (#7362).
  Rule 8's post-ladder demotion runs in one hook after the
  reach/observe/discriminate ladder (#7365).
