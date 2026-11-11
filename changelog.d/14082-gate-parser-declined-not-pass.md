<!-- section: Fixed -->
- `ripr gate check` and `ripr gate evaluate` no longer report `pass` when
  changed code was not judged: a changed file that could not be parsed
  (syntax error, nesting budget or non-UTF-8), or changed seams withheld
  because their static evidence is opaque or unknown. The decision now
  carries a warning that names the files or counts the seams, so the status
  reads `advisory`; review-comments discloses the count as
  `analysis_scope.static_limited_omitted_seams` (#14082).
