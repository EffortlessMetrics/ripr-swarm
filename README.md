# Immutable evidence packet for #8132

Source Draft PR: https://github.com/EffortlessMetrics/ripr-swarm/pull/8135

Source head: `720e27d14c46e8a48129dcc0d185272bbc4ca1cc`

Source tree: `7b804108c45e50d00e189d1027aba31ba432516a`

Archive SHA-256: `e00d6315b72a99a34f073b82230e26e4e1b2478316745531f527bd835931d07a`

Download `8132-producer-evidence.tar.gz`, verify against `SHA256SUMS`, extract into a fresh directory, then run `sha256sum -c SHA256SUMS` inside the extracted packet. The packet contains logs, JSON receipts, the test-only red patch and provenance; no executable, cache or source checkout.

Native red/green and selector evidence passed as described in provenance. Workspace Clippy was interrupted on storage admission; full local precommit and independent/hosted qualification remain unestablished. Evidence-only commit, separate from the frozen source branch.
