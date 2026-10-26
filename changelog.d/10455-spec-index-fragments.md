<!-- section: Changed -->
- A new spec registers its index row in `docs/spec-index.d/RIPR-SPEC-NNNN.md` instead of appending to the `docs/specs/README.md` table; `check-spec-numbering` and `check-doc-index` read the README and the fragments, so two PRs that each add a spec no longer conflict on the README tail (#10455).
