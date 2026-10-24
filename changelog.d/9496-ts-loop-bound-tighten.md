<!-- section: Fixed -->
- The TypeScript abstention for a changed counted-loop header now fires only
  when the asserted call count belongs to a mock the test passes to the
  owner, and ignores a spacing-only header edit, so an unrelated call count no
  longer hides a real gap (#9496).
