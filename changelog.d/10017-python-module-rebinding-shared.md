<!-- section: Fixed -->
- Python test extraction no longer copies every module-level name into every test. A test file of N tests held N copies of N names, so about 8,000 tests took 13 s and 3.5 GB and a 1 MB test file was killed for memory; the same file now takes 2 s and about 90 MB, and the report is unchanged (#10017).
