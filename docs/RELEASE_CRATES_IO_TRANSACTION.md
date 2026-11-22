# crates.io release transaction

Owner: [ripr#1649](https://github.com/EffortlessMetrics/ripr/issues/1649).
Status: decision core only. Nothing in this slice reads crates.io or uploads to it.

crates.io publication is not atomic across packages, and a published version
can never be replaced. The transaction therefore uploads one package at a
time, in dependency order, only when the registry does not have it, and never
twice under one authorization.

## What exists

`xtask/src/release_crates_io_transaction/plan.rs` holds the typed request, the
classification of observed registry state and `decide`.

`cargo xtask release-crates-io-transaction-plan --request <request.json>
--authorization <authorization.json> --inventory <inventory.json>` evaluates
supplied files and prints the decision. It exits nonzero for a stop.

Not built yet: the transport that observes the registry and downloads the
published bytes, the upload itself, the attempt record and the receipt.

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
any package in conflict              -> stop immutable_conflict, before any upload
every package exact                  -> complete (nothing uploaded)
first package that is not exact:
  ambiguous                          -> stop registry_unavailable
  upload already attempted           -> stop attempt_consumed
  crates_io_publish not allowed      -> stop not_authorized
  absent                             -> publish that package
```

Only the first non-exact package is considered, so a dependent is never
uploaded before its dependency is public with the exact bytes. A timeout, a
rate limit or index propagation delay is `hold_ambiguous`, not absence.

`attempt_consumed` means an upload was started under this authorization and
the registry still does not show the package. The outcome of that upload is
unknown, so a retry needs a successor authorization.

The plan also reports an aggregate: `nothing_published`,
`partial_publication_hold` when some packages are public and others are not,
or `package_set_published_exact`.

## Proof

`cargo test -p xtask release_crates_io_transaction` runs 8 tests. One sweeps
all 144 combinations of six registry states for each of two packages and four
attempt records, and checks that an upload is proposed only for an absent,
unattempted package whose dependency is exact, and never beside a conflict.

These tests exercise the decision against stand-in inventories. They do not
show that a real registry read produces a correct inventory, or that the
attempt record survives an interrupted upload; both belong to the transport
slice.
