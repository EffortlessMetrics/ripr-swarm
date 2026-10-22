<!-- section: Fixed -->
- The LSP work-done progress no longer drops the first stage report
  (`loading input`) when it is produced before the client answers
  `window/workDoneProgress/create`; it is queued and sent after `Begin` in
  producer order (#9362).
