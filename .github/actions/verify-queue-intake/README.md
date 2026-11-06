# Source-only Hermes H-intake validator

This directory contains an unwired validation component for the retained
`hermes.queue-intake.v2` contract. It performs no network I/O and does not
create an Actions job, check, credential, queue entry, or merge authorization.
`VALID_AGAINST_SUPPLIED_OBSERVATIONS` means only that the provided records
satisfy the structural and currentness checks. A caller can fabricate those
records; the result is not authentication or proof that a native entry exists.

A future protected caller must execute the verifier from independently trusted
source and obtain its configuration from an accepted control generation. It
must supply the exact repository, PR, H, current entry, numeric publisher App
ID and App/check names, accepted producer-generation digest, and complete
reviewed producer closure. There are no default publisher values. The closure
must include the `.github` tree and another producer root, with exact object
identities and modes. The caller must also verify those objects against actual
G and establish the complete native queue composition; JSON shape and matching
hash strings alone cannot establish either.

The reader callbacks must use authenticated, bounded GitHub API observations
for the exact requested H and live PR/entry. They must not use candidate files,
self-declared trust, unscoped endpoints, stale cached snapshots, or fallback
values. The caller supplies a timezone-aware trusted clock and its own required
intervening metadata observation. The verifier reads the check list twice,
brackets the reads with current-entry observations, refuses changed bindings,
and rechecks expiry after the last read. Readers must impose their own timeouts.
No sequence of reads is atomic; the protected publisher must revalidate before
consequential use or a final result. The admission `base_sha` is the base at H
admission and is not required to equal a later G base.

Local controls: `python3 .github/actions/verify-queue-intake/test_queue_intake.py`.
These fixture tests do not prove live App identity, installation permissions,
transport authentication, Git object custody, queue activation, or timing.
