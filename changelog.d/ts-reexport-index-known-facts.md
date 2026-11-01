<!-- section: Fixed -->
- On a TypeScript workspace larger than the in-memory facts cache (2,048
  entries), the re-export index no longer re-parses every file the cache
  dropped while phase 1 ran. It builds from the export facts phase 1 already
  derived, so a warm `ripr check` on VS Code (5,328 files) drops from 91 s to
  36 s with a byte-identical report (#11676).
