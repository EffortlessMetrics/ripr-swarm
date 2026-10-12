# Stability and versioning policy

This document states what `ripr` considers a compatibility promise and what
it does not. It governs the Rust library API, the machine-readable output
contracts, and the CLI surface. Human-readable prose, log lines, progress
indicators, and internal diagnostics are never a compatibility promise.

For the 0.11 release line (`version = "0.11.0"`, Rust edition 2024,
MSRV 1.95) this policy is descriptive: it records the promises the
maintainers intend to keep. Nothing here widens the recorded surface in
`policy/public_api.txt` into an endorsement; that file stays a recording
until each entry is triaged (see Residuals).

## Rust library API

Cargo semver applies to the `ripr` crate version:

- A patch release (`0.11.x`) never removes or incompatibly changes a public
  item, and never changes the meaning of a stable output contract version.
- A minor release (`0.x.0`) may extend the API and mint new output contract
  versions. It must not break the endorsed-stable subset below.
- A major release (`1.0.0` and later majors) is the only release allowed to
  break the endorsed-stable subset, and must document each break in the
  changelog under a `Changed` section naming the old and new shape.

The endorsed-stable subset is the integration surface named in
`crates/ripr/src/lib.rs`: `CheckInput`, `check_workspace`, `CheckOutput`,
`explain_finding`, `collect_context`, the public field types required to
use those entrypoints (`Mode`, `OutputFormat`, and the other root-level
field types documented there), and the domain re-exports documented in the
crate root for editor tooling, CI automation, and custom reporting. A
public field type required by an endorsed entrypoint inherits its promise:
a minor release must not break consumers through those types either.
Everything else reachable through `pub mod` — the twenty modules recorded
in `policy/public_api.txt` — is recorded-but-unendorsed: present and
nameable, but subject to narrowing, moving, or removal in a minor release
with a changelog note. New public items land as unendorsed; promotion to
endorsed-stable needs an explicit maintainer decision recorded here.

`provider_contract` is excluded from the generic additive rule below.
It is versioned separately by its `ripr_*_v1` schema versions, and
unknown fields fail closed (proven by the unknown-field rejection
tests): an added field breaks old readers, so it must mint a new
provider schema version. Old readers reject new writers loudly rather
than misreading them; that fail-closed behavior is itself stable.

## Output contracts

Each machine-readable contract carries its own `schema_version` namespace;
there is no single product-wide number. The contract table and per-contract
bump rules live in `docs/OUTPUT_SCHEMA.md`:

- Outside `provider_contract`, additive changes (new optional fields,
  new nested objects consumers may ignore) keep the contract version.
  `#[serde(default)]` covers absent fields only: it never makes an old
  reader accept a new enum spelling or a retyped value.
- A new enum spelling, a removed or retyped field, or a changed meaning of
  an existing value mints a new contract version — unless supported old
  readers have an explicit unknown-preserving variant or custom decoder for
  that spelling, proven by an old-reader/new-spelling compatibility test.
  The changelog names the old and new versions plus the migration.
- A version denotes one envelope shape, selected by any documented
  discriminator. Distinct versions for success and refusal documents are
  preferred for new contracts. The one grandfathered exception is
  `ripr agent stub --json` 0.1, where the `state` field (`ready` vs
  `refused`) selects the shape; consumers must dispatch on `state`, never
  on the version alone.
- Consumers select on `schema_version`, never on shape familiarity: one
  version denotes one set of field meanings, and only the version tells
  a same-shaped successor apart. A consumer that keys on a familiar
  field while ignoring the version silently inherits whatever meaning
  the newest producer gives that shape.

SARIF output follows the SARIF 2.1.0 standard envelope; standard fields
track the standard, and `ripr`-specific properties follow the additive /
breaking rules above.

## CLI surface

Stable: command names (including `ripr mcp` and `ripr lsp`: the names
stay even while their behavior is unstable), flag spellings, exit-code
meanings, and the JSON documents selected by `--format json` / `--json`
flags (each under its own contract version). Integrations must consume
those documents, never human-readable stdout: prose, tables, progress
lines, and suggestion text may change in any release, including patches.

Unstable: `ripr mcp` behavior (bounded read-only adapter; envelopes are
versioned per tool but the tool set may grow), `ripr lsp` behavior
(experimental sidecar), `ripr doctor` prose, and every `cargo xtask`
command (repository tooling, not product surface).

## Breaking-change process

1. Name the break in the PR body (old shape, new shape, affected
   contracts) and in `changelog.d/` under `Changed`.
2. Mint new contract versions for every affected output family; keep
   emitting the old version alongside, but only when the producer can do so
   without lying about provenance (otherwise cut over, loudly).
3. Update this policy's endorsed lists when the break touches them.
4. The release notes repeat the migration in one place per break.

## Known collector blind spots

`cargo xtask check-public-api` records module-level items with a bare `pub`
visibility chain from the crate root. It deliberately does not collect
public struct fields, enum variants, trait items, or associated `impl`
functions; a `pub use` records the bound name without resolving it; glob
re-exports are recorded as opaque entries. Consumer impact: a semver break
confined to one of those shapes (a removed struct field, a narrowed enum,
a changed trait method) passes the gate silently today. Until the collector
closes a blind spot, contributors must call out such changes in the PR body
under a `Semver` heading, and reviewers must treat a missing heading on a
public-shape diff as a finding.

## Residuals

- Triage `policy/public_api.txt` entry by entry into endorsed-stable or
  explicitly-unstable with a removal/narrowing plan per unstable item; the
  gate enforces the endorsed set once triage lands.
- Close each collector blind spot above, or keep it disclosed here with its
  consumer impact; a deliberately-added public item without policy handling
  must fail a named gate.
- Revisit this policy before 1.0: the endorsed subset and the breaking-change
  process above are the 1.0 compatibility contract once declared.
