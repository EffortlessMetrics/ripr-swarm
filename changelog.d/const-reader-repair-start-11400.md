<!-- section: Fixed -->
- A changed `const` whose every same-file reader is just a `format!` of it is now reported as the change to those readers' returned values, so `ripr check` and `ripr repair` offer the same repair when a test checks that value only loosely; any other shape keeps its previous answer with no repair offered (#11400).
