<!-- section: Fixed -->
- A repair attempt that starts from a producer-owned new-test proposal now records the proposal it follows and credits only the file it names; creating any other file, or a proposal made stale by an already-existing file, is refused (#13203).
