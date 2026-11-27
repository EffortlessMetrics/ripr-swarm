<!-- section: Fixed -->
- A document outside the editor's analyzed workspace root (for example a file
  from a second repository) no longer gets a paused-diagnostics marker that
  says to save the file and refresh, which can never work; the marker, its
  log line and hover now say the file is outside the analyzed workspace root
  and to open its repository in its own editor window or session, under reason
  `outside_analyzed_root` (#15462).
