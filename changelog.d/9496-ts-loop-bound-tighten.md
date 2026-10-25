<!-- section: Fixed -->
- The TypeScript abstention for a changed counted-loop header now fires only
  when the asserted call count belongs to a mock that may reach the owner: the
  mock is handed to it directly, through a carrier or helper, or passed to a
  call the rule cannot show is unrelated. An unrelated call count no longer
  hides a real gap, a spacing-only header edit is ignored, and a callback
  parameter or local that rebinds the mock's name is not the mock (#9496).
