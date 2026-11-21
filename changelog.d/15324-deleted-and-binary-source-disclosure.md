<!-- section: Fixed -->
- `ripr check` now names deleted files whenever the result has no findings,
  not only when nothing else changed, and says the result covers only the
  other changed files. A source file that git printed as binary (a `-diff` or
  `binary` attribute in `.gitattributes`, or a NUL byte) no longer reads as a
  complete analysis: the outcome carries a limitation naming the file, the
  likely cause and how to recover (#15324).
