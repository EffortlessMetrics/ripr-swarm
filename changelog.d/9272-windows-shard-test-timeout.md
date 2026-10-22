<!-- section: Fixed -->
- The sharded Windows advisory workflow runs nextest with a `windows-advisory` profile that terminates a test after 6 minutes, so a hung test becomes a named TIMEOUT row and the shard still uploads its log (#9272).
