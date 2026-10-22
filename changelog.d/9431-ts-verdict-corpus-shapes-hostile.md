<!-- section: Internal / CI -->
- The TypeScript verdict corpus gains 63 runtime-labeled cases in four new
  packages: language shapes (classes, generics, async, enums, barrels), jest
  decorators, Playwright Test specs and hostile inputs (CRLF, non-ASCII,
  minified lines, skipped and commented-out assertions). They add 16 false
  verdicts to the corpus, which stay visible in the rate rather than being
  waived (#9431).
