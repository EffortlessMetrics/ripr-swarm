<!-- section: Fixed -->
- `ripr agent verify` reads a repo-exposure artifact's producer stamp
  (`schema_version` and `artifact.producer.version`) before deserializing the
  full shape, so a missing or older RIPR producer fails closed with a PATH
  substitution diagnostic instead of serde `missing field` (#5115).
