<!-- section: Internal / CI -->
- The hosted shard verifier validates XML before retaining complete suite elements and checks each receipt's four ordered input blobs against the expected checkout commit, rejecting malformed XML and shared incorrect hashes and duplicate scalar context keys without retaining earlier successful receipts (#8195).
