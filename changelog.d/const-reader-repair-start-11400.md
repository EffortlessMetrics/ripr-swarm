<!-- section: Fixed -->
- A changed `const` whose every same-file reader returns it is now reported as the change to those readers' returned values, so `ripr check` and `ripr repair` offer the same repair when a test checks that value only loosely; a constant ripr can evaluate keeps its evaluated answer, and anything less certain stays unknown with no repair offered (#11400).
