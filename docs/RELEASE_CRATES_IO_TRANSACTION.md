# crates.io release transaction

Owner: [ripr#1649](https://github.com/EffortlessMetrics/ripr/issues/1649).
Status: decision core and read-only observer. Nothing here uploads to crates.io.

crates.io publication is not atomic across packages, and a published version
can never be replaced. The transaction therefore uploads one package at a
time, in dependency order, only when the registry does not have it, and never
twice under one authorization.

## What exists

`xtask/src/release_crates_io_transaction/plan.rs` holds the typed request, the
classification of observed registry state and `decide`. `observe.rs` holds
the read-only registry observer.

`cargo xtask release-crates-io-transaction-observe --request <request.json>`
reads the registry and prints the inventory. `cargo xtask
release-crates-io-transaction-plan --request <request.json> --authorization
<authorization.json> --inventory <inventory.json>` evaluates supplied files
and prints the decision. It exits nonzero for a stop.

Not built yet: the upload itself, the attempt record and the receipt.

## Observation

The observer issues https GET requests to three fixed hosts and holds no
credential. It refuses a request with violations before any read. For each
package it asks two independent sources about the exact version: the registry
API and the sparse index a dependent resolves through.

| API | Index | Reported as |
| --- | --- | --- |
| The registry's own not-found answer | No file, or no line for the version | `absent` |
| Version listed | Version listed, same checksum | `present`, with the archive downloaded |
| Version listed | Not listed, or the reverse | `unavailable` |
| Version listed | Listed with another checksum | `unavailable` |
| Any other status, no answer, unparseable content, another crate or spelling, or a version listed twice, from either source | | `unavailable` |

A version listed in one source and not the other is propagation in progress.
It is neither absent nor resolvable, so it holds the transaction.

For a present version the observer downloads the archive and reports its
SHA-256 only when that hash equals the checksum both sources list. A failed
download, or bytes that differ from the registry's own checksum, leave the
hash unset, which the plan holds as ambiguous. A version is reported yanked
when either source says so.

The inventory's attempt record is always empty here. It is not a registry
fact; the upload side owns it. A plan built from this output alone therefore
cannot show `attempt_consumed`.

## Request

The request names the registry (`crates.io` only), the candidate commit and
tree, the ship-packet and stable-decision digests, and the ordered packages.
Each package has a name, a `MAJOR.MINOR.PATCH` version, the size and SHA-256
of the exact `.crate` archive, and the first-party packages it requires.

- Packages are listed in dependency order. A package may require only
  packages listed before it.
- Names are unique under the registry's own rule, which ignores letter case
  and treats `-` and `_` as the same character.
- The canonical package list must hash to `package_set_sha256`.
- Unknown and missing JSON fields are refused.

`request_id` is the SHA-256 of the request's canonical JSON, as bare lowercase
hex. The release authorization packet
([ripr#1651](https://github.com/EffortlessMetrics/ripr/issues/1651)) names that
value as its `transaction_request_id`.

## Authorization

`decide` takes an already-verified authorization. It refuses one that names
another request, or whose subjects are not exactly
`<name>-<version>.crate` with the requested SHA-256 for every package. An
upload needs the `crates_io_publish` operation. Verifying that an
already-complete set is exact needs none.

## Decision

Each call reads one fresh inventory and returns at most one step.

| Registry state of a package | Classification |
| --- | --- |
| The registry answered that the version does not exist | `absent_admitted` |
| Present, not yanked, downloaded bytes hash to the requested SHA-256 | `exact_existing` |
| Present with other bytes, or yanked | `immutable_conflict` |
| Present but bytes unreadable, unavailable, unobserved or observed twice | `hold_ambiguous` |

```text
attempt record names a package
  outside the set                    -> stop inventory_rejected
any package in conflict              -> stop immutable_conflict, before any upload
any package ambiguous                -> stop registry_unavailable, before any upload
every package exact                  -> complete (nothing uploaded)
first package that is not exact:
  upload already attempted           -> stop attempt_consumed
  crates_io_publish not allowed      -> stop not_authorized
  absent                             -> publish that package
```

An upload is proposed only when every package in the set reads as absent or
exact. An unknown state can hide a conflict, so a support package is not
uploaded while the facade's state is unread. Among those, only the first
non-exact package is considered, so a dependent is never uploaded before its
dependency is public with the exact bytes. A timeout, a rate limit or index
propagation delay is `hold_ambiguous`, not absence.

The attempt record is matched by registry name, so `ripr_contract` and
`Ripr-Contract` consume the attempt for `ripr-contract`.

`attempt_consumed` means an upload was started under this authorization and
the registry still does not show the package. The outcome of that upload is
unknown, so a retry needs a successor authorization.

The plan also reports an aggregate: `nothing_published`,
`partial_publication_hold` when some packages are public and others are not,
or `package_set_published_exact`.

## Proof

`cargo test -p xtask release_crates_io_transaction` runs 14 tests. One sweeps
all 144 combinations of six registry states for each of two packages and four
attempt records, and checks that an upload is proposed only for an absent,
unattempted package whose dependency is exact, while every other package
reads as absent or exact.

The decision tests use stand-in inventories. The observer tests use scripted
HTTP answers and check each row of the observation table.

Observed against crates.io on 2026-11-22, read-only: the command reported a
published version as present with a downloaded hash equal to the registry's
checksum, and an unpublished crate as absent. Not observed: a version in
propagation, a yanked version, and whether the attempt record survives an
interrupted upload, which belongs to the upload slice.
