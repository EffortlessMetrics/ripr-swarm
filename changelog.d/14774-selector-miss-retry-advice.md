<!-- section: Fixed -->
- A `ripr explain` / `ripr context --at` `file:line` miss now suggests
  retrying only lines that select exactly one finding, and names lines that
  several findings share with a pointer to probe ids instead of a retry that
  refuses as ambiguous. A `file:line:column` selector on an extensionless
  file such as `Makefile:3:5` now gets the drop-the-column hint (#14492,
  #14511).
