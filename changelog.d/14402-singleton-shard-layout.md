<!-- section: Internal / CI -->
- `cargo xtask verify-test-shards` retains evidence extracted directly into the
  download directory when only one shard artifact matches. Ambiguous layouts
  and incomplete shard cover still fail verification (#14402).
