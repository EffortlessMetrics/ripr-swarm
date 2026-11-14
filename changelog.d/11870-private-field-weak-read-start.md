<!-- section: Fixed -->
- A changed `self.f = ..` write to a private or `pub(crate)` field now starts a repair from a weak relational read when a related test can name the field (an inline test module), in `ripr check` and `ripr repair` alike; a getter-only, unread, integration-test or otherwise unplaceable read still starts nothing (#11870).
