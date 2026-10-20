<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0243 rule 13): a suite wrapped in a same-file
  factory function and invoked at top level, such as
  `runAverageSuite("average", average)`, is now read as the suite it
  expands to, so a changed owner it exercises reads `exposed` instead of
  `no_static_path`. Only a parameter-simple, non-exported, singly declared
  factory called from the top level expands; defaults, rest or destructured
  parameters, a title built from a parameter, reassignment and `\u` escapes
  still refuse (#8950).
