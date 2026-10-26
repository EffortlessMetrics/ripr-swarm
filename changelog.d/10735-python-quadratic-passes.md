<!-- section: Changed -->
- Python diff mode is faster on large workspaces: per-class fact grouping and package re-export lookup are now indexed instead of scanning every owner against every class or exporter, and the foreign-module index scans each file in the parallel stage. Django draft analysis drops from about 14 s to about 9 s with byte-identical output (#10735).
