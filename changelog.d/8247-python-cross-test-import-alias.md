<!-- section: Fixed -->
- A Python import alias now credits only the test that imports it. An
  uncalling test that imported the changed function under a name another
  test uses for a different function (`from src.shop import fee as quote`
  beside `assert quote(10) == 30` for `src.fixed.fixed`) no longer lets that
  other test's assertion count as observing the owner, so the finding is no
  longer reported `exposed` (#8247).
