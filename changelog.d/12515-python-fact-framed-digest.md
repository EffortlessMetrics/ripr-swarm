<!-- section: Fixed -->
- Python fact-cache entries are framed with a digest header over the stored
  body bytes, so a warm `ripr check` verifies integrity over the bytes it
  already read instead of re-serializing every decoded envelope (#12515).
