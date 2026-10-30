<!-- section: Fixed -->
- Persisted analysis caches now distinguish analyzer binaries built from identical
  sources with different compiled language features. Older identities recompute;
  matching builds retain separate warm entries (#9898).
