<!-- section: Fixed -->
- Cache: the TypeScript and Python fact caches now prune entries left by older ripr builds and older cache schemas, the same way the Rust cache does. `ripr cache status` reports the reclaimable bytes and entries (`reclaimable_size_bytes`, `reclaimable_entry_count`; status schema `0.3`) (#7047).
