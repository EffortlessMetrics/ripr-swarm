<!-- section: Fixed -->
- `ripr lsp` publishes one withdrawal marker per document version when an
  open file is rewritten on disk (for example by `git checkout`) and reloaded
  into the buffer, instead of the same marker twice. A buffer that equals the
  saved file ripr has not analyzed yet is now reported as
  `saved_content_not_yet_analyzed` ("the saved file changed since ripr last
  analyzed it") rather than told to save a file that is already saved
  (#15316).
