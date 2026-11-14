<!-- section: Fixed -->

- `ripr explain` and `ripr context`: a `file:line` miss whose path differs from a finding's file only in letter case now says paths match case-sensitively and lists that file's lines with a correctly cased `file:line` that selects one finding. Selection itself stays case-sensitive (#14416).
