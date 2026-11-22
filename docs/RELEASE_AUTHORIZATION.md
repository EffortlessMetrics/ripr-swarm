# Release authorization packets

An owner ruling written as issue prose and a tag-push event are different
objects. A workflow receives a ref, an actor and an event payload; it does not
receive permission because someone earlier wrote a sentence on GitHub. This
page describes the machine object that carries one owner decision
([ripr#1650](https://github.com/EffortlessMetrics/ripr/issues/1650),
[ripr#1651](https://github.com/EffortlessMetrics/ripr/issues/1651)): the
packet and the comment body that holds it.

Reading a posted record back from GitHub, the authorizer policy and the
verification receipt are the second half of ripr#1651 and are not part of this
page yet. Binding a packet to a tag push or continuation request, reserving it
and consuming it belong to
[ripr#1652](https://github.com/EffortlessMetrics/ripr/issues/1652).

## Commands

```bash
cargo xtask release-authority render-packet --input <packet-draft.json> [--out <dir>]
```

`render-packet` reads a draft, which is a packet without `authorization_id`,
checks it against the profile matrix, computes the identity and writes
`authorization-packet.json` and `authorization-comment.md`. It posts nothing.
Posting the comment is a separate action of the authorized maintainer account.

## The packet

A packet is one canonical JSON object with schema
`ripr-release-authorization-packet/v1`. Canonical means keys in byte order, no
insignificant whitespace, standard string escapes, printable ASCII text and
plain unsigned integers. Bytes that are not already in that form are refused,
which also refuses duplicate keys.

`authorization_id` is `sha256:` over the canonical packet with the
`authorization_id` field removed. Every field is always present. A field that a
profile does not use is an explicit `null`, an empty array or `not_authorized`.

| Field | Content |
| --- | --- |
| `schema`, `tool` | Packet schema and renderer version |
| `repository`, `programme_issue`, `record` | Repository id and name, the release programme issue, the issue that holds the record |
| `profile` | `reserved_probe`, `public_rc`, `stable_core` or `stable_channel_continuation` |
| `issuer` | Expected GitHub user id and login |
| `sequence`, `nonce`, `validity`, `supersedes` | Ordering, uniqueness, the validity window, and the one prior authorization this packet replaces |
| `version`, `source` | Exact version and source commit and tree |
| `evidence` | Digests of the ship packet, readiness, qualification, public-RC verification, cohort, stable decision and predecessor receipt, and the cited policy ruling |
| `workflow` | Workflow path, blob, commit, plan schema and plan digest |
| `transaction` | Kind (`github`, `crates_io`, `vs_marketplace`, `open_vsx`, `npm`, `probe`), schema, tool and request id |
| `event` | `annotated_tag_push` with the exact ref, or `exact_channel_continuation` |
| `allowed_operations`, `denied_operations` | Together they name every operation in the closed set |
| `channels` | `authorized` or `not_authorized` for each of the seven channels |
| `subjects` | Class, name and SHA-256 of each package or asset |
| `environments` | Per channel: environment name and credential names. Never a value |
| `executors` | GitHub user or app ids allowed to execute |
| `consumption`, `invalidation`, `stop_conditions`, `non_claims` | One-use policy and the fixed v1 statement lists |

### Profiles

Each (profile, transaction kind) pair is a closed boundary. A packet may use
less than its boundary; anything outside it is refused.

| Profile | Transaction | Channels | Event | Evidence required |
| --- | --- | --- | --- | --- |
| `reserved_probe` | `probe` | none | tag `v0.0.0-ripr-trigger-probe.<nonce>` | none; window at most 24 hours |
| `public_rc` | `github` | `git_tag`, `github_prerelease` | tag `v<X.Y.Z-rc.N>` | ship packet, readiness, qualification, cited policy ruling |
| `stable_core` | `github` | `git_tag`, `github_release` | tag `v<X.Y.Z>` | the above plus public-RC verification, cohort, stable decision |
| `stable_core` | `crates_io` | `crates_io` | continuation | same as stable GitHub |
| `stable_channel_continuation` | `vs_marketplace` or `open_vsx` | that one channel | continuation | stable evidence plus predecessor receipt |

Consequences of the matrix:

- A probe cannot name a public channel, a subject, an environment or a
  credential.
- A release candidate cannot carry a `.crate`, crates.io, a stable release, a
  marketplace or npm.
- Crates.io authority and tag or GitHub authority are separate packets. Neither
  implies the other.
- A continuation carries one channel, one environment and one VSIX. An npm
  continuation is refused in v1 because no npm transaction is ruled and
  implemented.
- A channel and the operation that mutates it are authorized together or not
  at all. An environment is bound to one authorized channel, and neither an
  environment name nor a credential name is shared.
- `branch_or_existing_ref_update` and `signing_key_operation` are in the
  operation set so that every packet denies them explicitly. No profile allows
  them.

## The record

The durable record is one GitHub issue comment:

````text
<!-- ripr-release-authorization:v1 -->
```json
{canonical packet on one line}
```

Context for people. Never parsed.
````

The body must hold exactly one marker of this family, immediately followed by
a `json` fence with a single line, and must name the packet schema once.
Prose, an unmarked block, two blocks or a marker of another version is not a
packet. A packet containing a credential-shaped value is refused at render
time and the value is not echoed.

## Claim boundary

A rendered packet is a canonical object and a comment body on local disk. It
is not a posted record, establishes nothing about who issued it, and
authorizes nothing: no command in this contract reads GitHub, creates a ref,
requests a credential or publishes. It is static data and makes no claim about
runtime or mutation-test outcomes.

The schemas of the transactions a packet names (GitHub, crates.io,
marketplaces) and of the release plan are owned by their own issues. A packet
carries their schema names, request ids and digests as exact opaque
identities.
