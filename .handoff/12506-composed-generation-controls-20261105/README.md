# Composed generation test controls

Status: prepared and independently source-reviewed; **not compiled or executed**. This evidence branch is not the integration candidate and does not change the product source.

Basis: history/#7490 refresh `2fc0b8f98d7f0fe3ca1dbb242427776a6ae5fa3a`, tree `4d74ab87fcd57a2a635489657316b23c6c9aac6e`; `seam_cache.rs` blob `9beab6b27a0802b7ba07c3c662e41fbc9354ed48`. The separate owner's proposed generation patch came from evidence commit `36ef2786db5094eb60c6a458f0ca4cf423b185c9`, `.handoff/011-blocker-repairs-20261105/composed-cache-generation.patch` (SHA256 `76b03ade079e22b07d1687de29116f3cdf3c6dc7686f21b1397618a53907bf48`).

Apply `owner-generation.patch` to the basis first, then `test-controls.patch` (SHA256 `e653337e376c9d3a0eb4a6a5a0062f7ee213ebc64c62b2d644662d2780f00fb5`). Combined `seam_cache.rs` SHA256 is `4f1ecd98339db7f07a29007323dd562778f6ffe2c43e8d5e16f76f29ab43b2ee`. The follow-up is test-only. It checks valid current manifests against stale shard identities and exercises the production compact constructor with the workspace's full key in single and sharded forms. It refuses a nonempty `RIPR_CACHE_DIR` before fixture cache writes.

Independent source review found no blocking issue; the bounded cache-read helper and oversized-entry regression from #13183 remain unchanged. The predecessor payloads are synthesized, the workspace fixture has no source files, and the former sharded outer-key fixture uses the current inner codec. No historical-cache, analyzer, release qualification, or test-pass claim follows. The 27 generated-output conflicts and combined #12010 integration remain the owner's work. Preserve all existing holds.
