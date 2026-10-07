<!-- section: Changed -->
- A cold `ripr pilot` is faster: each test-target admission reads shared
  directories once and skips resolving the path when no entry on it is a
  symlink, which roughly halves the filesystem calls. Every admission still
  checks that the indexed files are current (#5361).
