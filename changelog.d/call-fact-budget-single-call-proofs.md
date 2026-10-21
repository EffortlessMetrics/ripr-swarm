<!-- section: Fixed -->
- A source line with 256 or more distinct calls no longer lets the "this is the
  only call on the line" checks in boundary pairing and arm selection read a
  dropped call as absent. Those checks now report the line as unknown (#8651).
