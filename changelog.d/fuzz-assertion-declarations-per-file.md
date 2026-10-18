<!-- section: Fixed -->
- `ripr check` no longer slows to a crawl on a test file with thousands of
  assertions. The file's type declarations were rescanned for every assertion,
  so one line of 20,000 `assert!` calls ran for over 30 seconds; they are now
  read once per file (#8645).
