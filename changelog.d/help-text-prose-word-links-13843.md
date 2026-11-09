<!-- section: Fixed -->
- Editing a string literal in a struct field (help text such as
  `summary: "Show capability status per language"`) no longer relates
  unrelated tests whose assertions or names merely share its words. A word
  found only inside the changed literal now relates a test only when one of
  the test's own string literals equals that literal. On a help-text edit in
  ripr's own CLI metadata, the related set drops from 2,563 rows to 514, led by the
  file's own tests, and the next step becomes inspect-first instead of
  "strengthen the related assertion" (#13843).
