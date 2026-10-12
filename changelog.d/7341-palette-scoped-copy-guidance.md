<!-- section: Fixed -->
- Palette invocations of the diagnostic-scoped targeted-test commands
  (`ripr: Write Targeted Test - Copy Suggested Assertion`, `Copy Brief`,
  `Open Best Related Test`) now explain how to run the command from a ripr
  diagnostic via Quick Fix, instead of claiming "No ripr ... is available
  for this diagnostic" for evidence the handler never evaluated (#7341).
  A code-action target that genuinely carries no payload keeps the
  original missing-evidence message.
