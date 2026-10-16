# Native diff fan-out #8169: preserved technical evidence

Candidate PR: https://github.com/EffortlessMetrics/ripr-swarm/pull/8201
Candidate head: `6a47f5bf094e5b010845b544f9464333a8b90d7d`
Candidate tree: `2a15b485aa9684ff9ea05159e429641433dd550b`
Frozen validation basis: `b772d9e13cafcdc84a19ea77d9b90fbc6cb91ecc`

This orphan technical-evidence branch contains no candidate source or workflows.
It preserves the existing raw receipts and exact measurement script, the existing
20 paired samples (also projected as CSV), source challenge, bounded validation
logs and relevant reports. No benchmark or validation was rerun for this archive.

`manifest.json` gives every payload's byte count and SHA-256, provenance and
explicit omissions. `SHA256SUMS` additionally binds the manifest. Resolve these
files by this evidence commit SHA, never by an assumed mutable branch tip.

The raw receipt paths and environment are historical producer-side locators,
not a claim those files exist on a new executor. The measurement script retains
its original paths. Adapt them deliberately for a later reproduction and keep
that run separate from this evidence. The large binary is intentionally absent;
its recorded hash is metadata, not archived executable bytes. Caches, secrets,
raw native payload files and unrelated scratch files are not included. The full
1.8 MiB traceability report is omitted; its original hash and a bounded relevant
excerpt remain. Other file references in the original receipt may be omitted;
consult this package manifest to determine what was actually preserved.

Local evidence does not establish hosted integration, native Windows execution,
real UB/release performance, supply-chain clearance or independent merge review.
The historical source challenge is scoped pre-publication feedback. Twenty paired
comparisons had equal native member bytes. Performance measurements used the dev
profile, uncontrolled OS caches/host load and Python timeout polling; they imply
no general 3x guarantee. Existing downstream qualification ownership and #8180's
inherited ZIP supply-chain ownership remain unchanged. PR stays Draft; fresh
independent review is owned by the parent coordinator.
