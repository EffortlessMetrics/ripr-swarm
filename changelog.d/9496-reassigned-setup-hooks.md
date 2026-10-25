<!-- section: Fixed -->
- TypeScript/JavaScript preview captures setup-hook identity at registration,
  preserving earlier and lexically separate valid hooks while withholding
  callbacks affected by preceding writes, including unresolved local aliases
  of global roots. Immutable empty-object shadows and imported hook identities
  stay distinct; parameter-default writes are resolved before body-local
  declarations. Receiver race limits and the bounded
  Mocha abstention rule remain in place without adding hook oracle credit
  (#4266, #9496).
