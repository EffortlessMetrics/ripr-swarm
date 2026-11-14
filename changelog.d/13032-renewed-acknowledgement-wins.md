<!-- section: Fixed -->
- A structured gate acknowledgement renewed by a later row now applies even when
  an earlier row for the same gap is expired or over `max_matches`; the first
  limitation is reported only when no row applies (#13032).
