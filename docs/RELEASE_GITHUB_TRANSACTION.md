# GitHub release transaction

Owner: [ripr#1644](https://github.com/EffortlessMetrics/ripr/issues/1644).
Status: decision core and read-only observer. Nothing here writes to GitHub.

The transaction publishes one tag, one release and one exact set of assets for
one release candidate. It never replaces, moves or deletes a public object, and
it resumes only when everything already present is exact.

## What exists

`xtask/src/release_github_transaction/` holds four parts:

| Part | Role |
| --- | --- |
| `model.rs` | The typed request and the reasons it can be refused |
| `inventory.rs` | Classification of observed tag, release, asset and provenance state |
| `plan.rs` | `decide`: the single next step, completion, or a terminal stop |
| `observe.rs` | The read-only GitHub observer that produces an inventory |

`cargo xtask release-github-transaction-observe --request <request.json>`
reads GitHub and prints the inventory. `cargo xtask
release-github-transaction-plan --request <request.json> --authorization
<authorization.json> --inventory <inventory.json>` evaluates supplied files
and prints the decision as JSON with
`schema: ripr.release.github_transaction.decision.v1`. It exits nonzero for a
stop.

Not built yet: the transport that performs a step,
the loop that drives steps to completion with a receipt, and the retirement of
`release-upload-assets`, which still creates a release on any failed view and
uploads with `--clobber`. Do not use that command for a release.

## Observation

The observer runs `gh api --hostname github.com --method GET` and, for
provenance, `gh attestation verify`. Neither writes to GitHub. It refuses a request with violations before any read.

| Read | Reported as |
| --- | --- |
| `repos/<repo>` does not answer this repository with `permissions.push = true` | tag and releases `unavailable`; nothing else is read |
| Tag ref answers 404 | `absent` |
| Tag ref points at a tag object whose target is a commit | `present`, annotated, with that tag object's id and that commit |
| Tag ref points straight at a commit, or at a tag of a tag | `present`, not annotated |

The plan compares the reported tag object id with the authorization's
`expected_tag_object`, so another annotated tag at the candidate is a conflict.
| Every release whose tag name matches ignoring case, drafts included | listed, with every asset from the paged asset endpoint |
| An expected asset, fully uploaded, downloaded with the listed length | SHA-256 of the downloaded bytes |
| An expected asset that is still uploading, fails to download, or downloads with another length | bytes `unavailable` |
| An asset the request does not expect | listed by name, not downloaded |
| Any other status, no answer, unparseable JSON, a missing field, or a listing that does not end within 50 pages | `unavailable` |

The write-permission check exists because GitHub answers 404 for a repository
a token cannot read and omits draft releases for a token that cannot write.
Without it a weak token would read an existing draft as absent.

Provenance is checked with `gh attestation verify` over the downloaded bytes
of a subject, and only when exactly one asset of that exact name downloaded
with the subject's SHA-256. The command binds the repository, the exact
certificate identity `https://github.com/<signer_workflow>@<source_ref>`
(`--cert-identity`; `--signer-workflow` would match a regular-expression
prefix), the source ref, the candidate commit as source digest, and refuses
self-hosted runners. The bytes are written to a temporary directory readable
only by the current user, since a draft's assets are not public yet. Its JSON output must also
name the subject with its digest. Anything else, including a subject that is
not uploaded yet, is `unavailable`: `gh` does not separate "no attestation"
from "could not check", so the observer never reports `missing`.

## Request

The request names the repository, mode (`public_rc` or `stable`), product
version, candidate commit and tree, the exact tag, title, body digest,
prerelease flag, ship-packet digest, stable-decision digest (stable only) and
the ordered expected subjects with size and SHA-256.

- The tag is an explicit field. It is checked against the mode and version but
  never derived from them: `public_rc` requires `v<version>-rc.<ordinal>` with
  `prerelease=true` and no stable decision; `stable` requires `v<version>` with
  `prerelease=false` and a stable decision.
- Subjects are plain file names in strict name order, unique ignoring case,
  and their canonical list must hash to `subject_inventory_sha256`. Each
  subject's `kind` is one of the GitHub asset classes a GitHub authorization
  packet may carry: `server_archive`, `server_manifest`, `checksums`, `vsix`,
  `attestation_bundle`.
- `provenance.signer_workflow` is `<repository>/.github/workflows/<file>.yml`
  and `provenance.source_ref` is `refs/heads/<name>` or `refs/tags/<name>`.
  The signing run's commit must be the candidate.
- `candidate_tree` is format-checked and bound into the request ID. No
  observation is compared with it: the tag names the commit, and the commit
  names its tree.
- The four other channels (crates.io, npm, VS Marketplace, Open VSX) are
  fields that must be `false`.
- Unknown and missing JSON fields are refused.

`request_id` is the SHA-256 of the request's canonical JSON, as bare lowercase
hex. The release authorization packet
([ripr#1651](https://github.com/EffortlessMetrics/ripr/issues/1651)) names that
value as its `transaction_request_id`. The request does not name the
authorization back, because each digest would then depend on the other.

## Authorization

`decide` takes an already-verified authorization: fields copied from the
verified packet, each compared with the request. Any difference stops with
`authorization_mismatch`.

| Field | Packet source | Must equal |
| --- | --- | --- |
| `profile` | `profile` | `public_rc` for `public_rc`, `stable_core` for `stable` |
| `transaction_kind` | `transaction.kind` | `github` |
| `transaction_request_id` | `transaction.request_id` | the request ID |
| `source_commit` | `source.commit` | `candidate_sha` |
| `event_ref` | `event.ref` | `refs/tags/<tag>` |
| `expected_tag_object` | the annotated tag object the authorization event pushes ([ripr#1652](https://github.com/EffortlessMetrics/ripr/issues/1652)) | 40 lowercase hex; see below |
| `subjects` | `subjects[]` as `{name, class, digest}` | the request subjects as `{name, kind, "sha256:" + sha256}`, as a whole set |
| `allowed_operations` | the allowed operation names | checked per step, below |

Digests stay in the packet's `sha256:<64 hex>` form; the request holds bare
hex and the comparison adds the prefix. A GitHub packet's subject catalog is
limited to the five GitHub asset classes, so no class is filtered out: a
packet subject the request does not list is a mismatch.

The tag object is the authorization event, and its bytes derive from the
packet, so its ID is known before the push. An annotated tag at the candidate
with any other object ID (another tagger, date or message) is a conflict, not
the authorized tag. The expected ID lives on the authorization rather than the
request because the packet names the request ID; the reverse would be a
digest cycle.

Verifying a packet is the verifier's job and is not done here. The
`--authorization` file is accepted as given for offline planning only. When the
transport lands, the authorization must be built from a live verifier record
(not a fixture replay), and a file path stays test-only.

| Step | Operation required |
| --- | --- |
| Create the tag | `create_annotated_tag` |
| Create the draft release, and finalize it | `github_prerelease_create` (`public_rc`) or `github_release_create` (`stable`) |
| Upload a subject | `github_asset_upload` |
| Verify an already-complete public release | none |

## Decision

Each call reads one fresh inventory and returns at most one step. The
inventory records the repository and tag name it observed, even when nothing
was found, and the signer workflow and source ref its provenance was verified
against. An inventory for any other target or provenance policy stops with
`inventory_mismatch` before classification, so an empty snapshot of another
tag cannot read as "absent" for this one, and a verdict under one signer
cannot finalize a release that requires another.

```text
tag absent, no release          -> create the tag
tag exact, no release           -> create the draft release
draft exact, a subject absent   -> upload the first absent subject
draft exact, all subjects exact,
  every provenance verified     -> finalize
public exact, all exact,
  every provenance verified     -> complete (nothing written)
```

Everything else stops:

| Observation | Stop |
| --- | --- |
| The inventory observed another repository, tag or provenance policy than the request names | `inventory_mismatch` |
| Tag, releases or an asset's bytes could not be read | `remote_unavailable` |
| Tag is lightweight, at another commit, or another tag object than the authorized one | `remote_conflict` |
| Authorization profile, kind, request, commit, ref, tag object or subject set differs | `authorization_mismatch` |
| Release on another tag, or title, body or prerelease flag differs | `remote_conflict` |
| Two releases for the tag, or a release whose tag is absent | `remote_conflict` |
| An expected asset with other bytes, another size, a case-variant name, or attached twice | `remote_conflict` |
| An asset the request does not expect | `remote_conflict` |
| Provenance for any subject missing, unreadable, or observed more than once | `attestation_not_verified` |
| Public release missing an expected subject | `partial_public` |
| The step's operation is not allowed | `not_authorized` |

An asset is exact only when its downloaded bytes hash to the expected SHA-256.
Name, size and API metadata are not enough. `partial_public` is an incident:
nothing is uploaded to a finalized release.

## Proof

`cargo test -p xtask release_github_transaction` runs 25 tests. One sweeps all
1,000 combinations of tag, release, asset and provenance state the model can
observe and checks, against the classification alone, that a mutation is
proposed only when the object it writes is absent and everything present is
exact. Because the sweep trusts the classification, classification errors are
caught by the named conflict cases instead: removing the tag-object comparison
fails the "foreign annotated tag object at the candidate" case.

The decision tests use stand-in inventories. The observer tests use scripted
HTTP answers and check each row of the observation table, including that a
failed or partial read is never reported as absent or as a shorter list, and
that bytes other than the subject's are never offered for verification.

Observed against GitHub on 2026-11-22 with gh 2.89.0, read-only: the
`--include` output shape the parser expects, a 404 for a missing tag ref, and
an asset download whose SHA-256 matched the digest GitHub lists for it. The
command was also run end to end against an existing draft release with no
tag: it reported the tag absent, listed the one draft with its 13 assets, and
downloaded and hashed only the two the request expected. Not observed: an
annotated tag, since the repository read had none; whether GitHub returns a
release body byte-for-byte as submitted, which `body_sha256` depends on; and a
successful provenance verification. From the machine these checks ran on
`gh attestation verify` could not initialize a Sigstore verifier, so only its
refusal path ran, and the JSON shape `names_subject` reads is taken from gh's
documentation rather than a real verification.
