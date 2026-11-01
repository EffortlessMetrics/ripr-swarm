<!-- section: Internal / CI -->
- Add a Kani bounded-model-checking lane: `cargo xtask kani` reports VERIFIED, FAILED or NOT-ESTABLISHED per harness and never passes by absence, a nightly and merge-ready `kani.yml` workflow, `cfg(kani)` check-cfg, and a smoke harness for the actionability predicate (RIPR-SPEC-0371) (#11787).
