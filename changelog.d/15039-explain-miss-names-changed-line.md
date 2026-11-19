<!-- section: Changed -->
- `ripr explain <file:line>` and `ripr context --at <file:line>` now say
  which reason applies when the file has no finding in a complete run: the
  diff changes no text line of the file, the named line is not one the diff
  changed (the nearest changed lines are listed), or the line changed and
  has no finding. The message used to list all three and leave the
  choice to the reader (#15039).
