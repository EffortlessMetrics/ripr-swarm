<!-- section: Fixed -->
- Editing a string literal in a struct field (help text such as
  `summary: "Show capability status per language"`) no longer links
  thousands of unrelated tests whose assertions happen to mention the same
  words. A word that occurs in the change only inside a string literal now
  relates a test only when its assertion quotes that literal whole. On a
  help-text edit in ripr's own CLI metadata, related tests drop from 2,563
  to the file's own 8, and the next step becomes inspect-first instead of
  "strengthen the related assertion" (#13843).
