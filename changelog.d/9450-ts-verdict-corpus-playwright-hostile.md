<!-- section: Internal / CI -->
- The TypeScript verdict corpus gains 27 runtime-labeled cases in two new
  packages: Playwright Test specs of pure functions and hostile inputs (CRLF,
  non-ASCII, a minified line, skipped and commented-out assertions). They add
  6 false verdicts, which stay visible in the rate rather than being waived
  (#9450).
