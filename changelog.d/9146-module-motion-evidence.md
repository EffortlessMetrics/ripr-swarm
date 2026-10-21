<!-- section: Fixed -->
- Preserve inline unit-test identity when an owner moves into an ordinary child
  module, and withhold direct observer credit for private child functions that
  the observing module cannot access (#4680, #9146).
  Default library and binary observers use their own automatic-target discovery
  flag, so disabling the other target kind does not change their identity.
