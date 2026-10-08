<!-- section: Fixed -->
- Cleanup of a replaced classified-cache generation no longer deletes through a
  symlinked generation directory, so a planted link cannot steer a delete
  outside the cache (#6757).
