<!-- section: Fixed -->

- `agent verify` on `check-findings-json` artifacts now peeks producer identity before typed deserialization, so a missing or older `producer.version` fails with the PATH-substitution diagnostic instead of an opaque serde `missing field` error (#5115).
