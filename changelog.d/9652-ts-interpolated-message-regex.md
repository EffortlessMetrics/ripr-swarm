<!-- section: Fixed -->
- TypeScript preview: an anchored `toThrow(/^message$/)` separates the
  versions of a thrown message that is one template literal with
  interpolations (`` `not a count: ${text}` `` becoming `` `not a number:
  ${text}` ``) when its text can be the whole message of exactly one version
  for some fill of the holes. Unanchored, flagged and substring payloads keep
  the old reading (#9652).
