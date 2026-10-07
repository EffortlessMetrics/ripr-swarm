<!-- section: Changed -->
- A warm `ripr check` on a large workspace is faster: the scan that names
  where a trusted assertion macro may be redefined now runs on the rayon
  pool, and same-file helper crediting skips parsing test files whose tests
  call no candidate helper. On ripr-swarm the warm check drops from about
  8.1 s to about 6 s with byte-identical JSON (#5363).
