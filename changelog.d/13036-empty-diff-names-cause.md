<!-- section: Changed -->
- An empty `ripr check` range on the comparison base now says the diff is empty instead of blaming the language, prints the paste-ready `ripr check --base HEAD~1`, and a range with changed files but no checkable behavior hints that the base probably already includes the production change (#13036).
