# Fixture: cross-language-edge-registry-currentness-corpus

Spec: RIPR-SPEC-0379

Deterministic loading, currentness evaluation, and base/head movement fixtures for
#6484 on top of the schema corpus in `cross-language-edge-registry-corpus`.

Run:

```bash
cargo test -p ripr provider_contract::cross_language_edge::tests::currentness_corpus_cases -- --test-threads=1
```
