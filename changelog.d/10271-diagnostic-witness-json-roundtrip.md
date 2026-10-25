<!-- section: Fixed -->
- Public `DiagnosticWitness` JSON consumers can deserialize an omitted
  `limitations` field as an empty list; serialization and rejection of `null`
  or malformed lists stay unchanged (#10271).
