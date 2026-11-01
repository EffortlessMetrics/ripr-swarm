<!-- section: Fixed -->
- A warm TypeScript `ripr check` no longer parses every test file again to
  decide whether the extractor dropped a registration shape. The verdict is a
  function of the file's bytes, so it is stored beside the test extraction (fact
  store schema 6) and served on later runs. Warm VS Code (5,328 files) drops
  from 36 s to 26 s with an identical report (#11676).
