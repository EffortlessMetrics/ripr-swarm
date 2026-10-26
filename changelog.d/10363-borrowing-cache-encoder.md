<!-- section: Fixed -->
- Writing and verifying a file-facts cache entry no longer deep-clones the file's facts and module-scope table first. On a 4 MB Rust test file the warm run is about 7% faster (median 3.76 s to 3.51 s) and cold about 5%, with byte-identical cache entries and `ripr check` output (#10363).
