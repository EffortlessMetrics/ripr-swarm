<!-- section: Fixed -->
- Classified-seam cache loads now read through the encoded shard byte ceiling, so a corrupt or hostile oversized cache file degrades to a rebuild instead of being allocated whole into memory.
