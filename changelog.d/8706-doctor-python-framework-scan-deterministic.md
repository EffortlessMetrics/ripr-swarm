<!-- section: Fixed -->
- Make `ripr doctor`'s Python test-framework scan deterministic: each scanned
  directory's entries are sorted by name and only admitted `.py` test
  candidates count toward the per-directory cap, so 64 non-test entries can
  no longer starve the scan of every test file or flip the reported
  framework set between runs and machines (#8706).
