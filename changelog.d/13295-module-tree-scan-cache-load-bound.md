<!-- section: Fixed -->
- The module-tree scan cache bundle (`scans.json`) is now loaded through a byte ceiling (default 32 MiB, `RIPR_MODULE_TREE_SCAN_CACHE_BYTES`) instead of an unbounded read; an oversized bundle degrades to a cold rebuild rather than being allocated whole (#13295).
