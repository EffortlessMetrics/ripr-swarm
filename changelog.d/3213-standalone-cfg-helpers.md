<!-- section: Fixed -->
- Directly test-gated Rust helpers stay evidence-only without adding executable
  tests or production probes; parser and lexical normalization share the cfg
  predicate authority. The Rust cache schema versions advance, so existing
  `target/ripr` caches are recomputed once and receipt
  `source_subject.corpus.cache_schema_version` reads `1.83` in this release (#3213, #8479, #NNNN).
