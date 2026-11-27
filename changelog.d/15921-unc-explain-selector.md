<!-- section: Fixed -->
- `ripr explain <file:line>` selects a finding stored under a Windows verbatim
  UNC path (`\\?\UNC\server\share\...`) from the plain share path
  (`\\server\share\...` or `//server/share/...`), the spelling ripr prints and
  users type. The lookup reported no match (#15921).
