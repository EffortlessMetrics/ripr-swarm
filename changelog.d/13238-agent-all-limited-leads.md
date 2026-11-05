<!-- section: Fixed -->
- `ripr check --format agent` no longer says "nothing more is fixable from the test side" when every finding is static-limited. It now lists the unconfirmed findings (location and id, no invented missing value or test; bounded, with a `+N more` count) and a `next=ripr explain` pointer (#13238).
