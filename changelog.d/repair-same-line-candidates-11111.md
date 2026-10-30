<!-- section: Fixed -->
- `ripr repair` without an item, when several changes can start a repair, now
  lists them in plain words: a change ripr can repair two ways is named once
  and each choice says what a test must check, the command `ripr check`
  printed is named as the one to run, and the prompt no longer shows internal
  terms (`choose_item`, `select_item`) or a PowerShell note about a
  placeholder retry line (#11111).
