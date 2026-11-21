# GitHub release transaction

Owner: [ripr#1644](https://github.com/EffortlessMetrics/ripr/issues/1644).
Status: decision core only. Nothing in this slice reads GitHub or writes to it.

The transaction publishes one tag, one release and one exact set of assets for
one release candidate. It never replaces, moves or deletes a public object, and
it resumes only when everything already present is exact.

## What exists

`xtask/src/release_github_transaction/` holds three parts:

| Part | Role |
| --- | --- |
| `model.rs` | The typed request and the reasons it can be refused |
| `inventory.rs` | Classification of observed tag, release, asset and provenance state |
| `plan.rs` | `decide`: the single next step, completion, or a terminal stop |

`cargo xtask release-github-transaction-plan --request <request.json>
--authorization <authorization.json> --inventory <inventory.json>` evaluates
supplied files and prints the decision. It exits nonzero for a stop.

Not built yet: the transport that observes GitHub and performs a step, the
loop that drives steps to completion with a receipt, and the retirement of
`release-upload-assets`, which still creates a release on any failed view and
uploads with `--clobber`. Do not use that command for a release.

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
  and their canonical list must hash to `subject_inventory_sha256`.
- The four other channels (crates.io, npm, VS Marketplace, Open VSX) are
  fields that must be `false`.
- Unknown and missing JSON fields are refused.

`request_id` is the SHA-256 of the request's canonical JSON, as bare lowercase
hex. The release authorization packet
([ripr#1651](https://github.com/EffortlessMetrics/ripr/issues/1651)) names that
value as its `transaction_request_id`. The request does not name the
authorization back, because each digest would then depend on the other.

## Authorization

`decide` takes an already-verified authorization: its identifier, the request
ID it names, its allowed operation names and its `(name, sha256)` subjects. It
refuses an authorization that names another request or another subject set.
Verifying a packet is the verifier's job and is not done here.

| Step | Operation required |
| --- | --- |
| Create the tag | `create_annotated_tag` |
| Create the draft release, and finalize it | `github_prerelease_create` (`public_rc`) or `github_release_create` (`stable`) |
| Upload a subject | `github_asset_upload` |
| Verify an already-complete public release | none |

## Decision

Each call reads one fresh inventory and returns at most one step.

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
| Tag, releases or an asset's bytes could not be read | `remote_unavailable` |
| Tag is lightweight or at another commit | `remote_conflict` |
| Release on another tag, or title, body or prerelease flag differs | `remote_conflict` |
| Two releases for the tag, or a release whose tag is absent | `remote_conflict` |
| An expected asset with other bytes, another size, a case-variant name, or attached twice | `remote_conflict` |
| An asset the request does not expect | `remote_conflict` |
| Provenance for any subject missing or unreadable | `attestation_not_verified` |
| Public release missing an expected subject | `partial_public` |
| The step's operation is not allowed | `not_authorized` |

An asset is exact only when its downloaded bytes hash to the expected SHA-256.
Name, size and API metadata are not enough. `partial_public` is an incident:
nothing is uploaded to a finalized release.

## Proof

`cargo test -p xtask release_github_transaction` runs 13 tests. One sweeps all
800 combinations of tag, release, asset and provenance state the model can
observe and checks, against the classification alone, that a mutation is
proposed only when the object it writes is absent and everything present is
exact.

These tests exercise the decision against stand-in inventories. They do not
show that a real GitHub read produces a correct inventory; that belongs to the
transport slice.
