<!-- section: Internal / CI -->
- The hosted test shards write their nextest partition lists before the test run instead of after it. Listing after the run took shard 4 between 4.3 and 6.1 minutes on every measured run, against 4 to 16 seconds on the other shards, and that shard bounds the required test phase (#12915).
