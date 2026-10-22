<!-- section: Fixed -->
- TypeScript preview: an edited conditional arm is no longer credited when the
  test imports the enum root of the compared member path (`Level.High`) from a
  module other than the owner's, since the same text can name a different enum
  (#9624).
