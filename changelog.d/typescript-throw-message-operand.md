<!-- section: Fixed -->
- TypeScript: a change confined to the thrown message argument, such as `"limit " + max` becoming `"limit "`, now counts as message-only under RIPR-SPEC-0243 rule 4 unless both sides read the same message. A string `toThrow` payload credits only when it matches exactly one of the old and new messages, so `toThrow("limit ")` no longer reads `exposed` for a change it cannot see (#6654).
- TypeScript: a literal `.rejects.toBe`/`toEqual`/`toStrictEqual` reason now credits a changed throw only when it equals the thrown message, not when it is a substring of it, since the equality passes on neither version otherwise (#6654).
- TypeScript: a literal `.rejects` equality no longer credits a line that throws an `Error`, since a rejected `Error` never equals a string (#7944).
