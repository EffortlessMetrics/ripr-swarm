<!-- section: Internal / CI -->
- The hosted test shards build and run their test binaries without debug info. Shard 3 of a merge-group run swapped up to 9.8 GB while linking the debug-info test binaries and hit its 30-minute build limit, ejecting the entry; a failing test still reports its panic location (#11566).
