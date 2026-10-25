<!-- section: Fixed -->
- TypeScript/JavaScript preview captures setup-hook identity at registration,
  preserving earlier and lexically separate valid hooks while withholding
  callbacks affected by preceding writes. Receiver race limits and the bounded
  Mocha abstention rule remain in place without adding hook oracle credit
  (#4266, #9496).
