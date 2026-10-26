<!-- section: Changed -->
- Dev and test builds optimize SHA-256 (`sha2` and its digest stack). The repair-binding driver test hashes the ~500 MB debug test binary three times. It drops from about 80 s to 10 s and no longer exceeds its 30 s child deadline on loaded CI shards (#10729).
