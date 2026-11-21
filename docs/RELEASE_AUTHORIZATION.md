# Release authorization packets

An owner ruling written as issue prose and a tag-push event are different
objects. A workflow receives a ref, an actor and an event payload; it does not
receive permission because someone earlier wrote a sentence on GitHub. This
page describes the machine object that carries one owner decision
([ripr#1650](https://github.com/EffortlessMetrics/ripr/issues/1650),
[ripr#1651](https://github.com/EffortlessMetrics/ripr/issues/1651)): the
packet, the comment record that holds it, the authorizer policy, and the
read-only verifier.

Binding a packet to a tag push or continuation request, reserving it and
consuming it belong to
[ripr#1652](https://github.com/EffortlessMetrics/ripr/issues/1652) and are not
part of this contract.

## Commands

```bash
cargo xtask release-authority render-packet --input <packet-draft.json> [--out <dir>]
cargo xtask release-authority render-revocation --input <revocation-draft.json> [--out <dir>]
cargo xtask release-authority verify-record --repository <owner/name> --issue <number> --comment <id> --expected-body-sha256 <hex> --bindings <bindings.json> [--fixture <dir>] [--out <dir>]
```

`render-packet` reads a draft, which is a packet without `authorization_id`,
checks it against the profile matrix, computes the identity and writes
`authorization-packet.json` and `authorization-comment.md`. `render-revocation`
does the same for a revocation. Neither posts anything. Posting the comment is
a separate action of the authorized maintainer account.

`verify-record` re-reads one exact comment and writes
`authorization-receipt.json` and `authorization-receipt.md`. It exits zero only
for an `authorized` record read live from GitHub. Its GitHub access is three
`GET` endpoints through `gh api -X GET`: the repository, one issue comment and
the comment listing of one issue. It creates no ref, comment or release and
requests no environment or credential.

`--fixture <dir>` replays saved API responses (`repository.json`,
`comment-<id>.json`, `issue-<number>-comments.json`) through the same verifier.
A replay can show any disposition, but its receipt carries
`"live_authority": false` and the command exits nonzero. A replay is never
authority.

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
packet. A body containing a credential-shaped value is refused and the value
is not echoed in the receipt.

## Authorizer policy

[`policy/release-authorizers.toml`](../policy/release-authorizers.toml) lists
who may issue a packet for the public source repository. The numeric GitHub
user id is the identity. The login is checked for currentness: an id whose
login no longer matches the policy is refused until the policy is reviewed. A
display name, email, author association, repository membership or comment
location is never read. A packet cannot add its issuer or widen a profile
list. Verification after the policy's `review_after` date is refused.

## Verification

Given a repository, issue number, comment id and expected body SHA-256, the
verifier evaluates these checks in order and stops at the first that does not
pass:

1. `locator`, `repository_identity`: the observed repository id and name equal
   the locator and the policy.
2. `record_location`, `record_body_digest`, `record_unedited`: the exact
   comment, on the exact issue, with the expected bytes, never edited.
3. `secret_scan`, `packet_block`, `packet_canonical_bytes`,
   `packet_fields_and_profile`, `authorization_id`: one marked canonical packet
   that satisfies its profile and hashes to its own id.
4. `record_agreement`, `issuer_identity`, `authorizer_policy`: the packet names
   the place it was found, the comment author is the packet issuer, and that
   user id is a current policy authorizer for the profile.
5. `bindings`: every identity in the packet equals the value in the supplied
   bindings file. The packet does not vouch for its own candidate, evidence,
   workflow, transaction or subjects.
6. `policy_ruling`: for `public_rc`, the cited ruling comment is re-read as
   exact bytes written by a policy authorizer. Its prose is not parsed.
7. `validity_window`, `supersession`: the evaluation time is inside the window
   and no authorized later record supersedes or revokes the packet.

The bindings file is a JSON object with `profile`, `version`, `source_commit`,
`source_tree`, `workflow_path`, `workflow_blob`, `workflow_commit`,
`plan_schema`, `plan_digest`, `transaction_kind`, `transaction_schema`,
`transaction_tool`, `transaction_request_id`, `event_ref`, `evidence` and
`subjects`.

### Dispositions

| Disposition | Meaning |
| --- | --- |
| `authorized` | Every check passed |
| `denied` | Issuer, policy or cited ruling refused, or the packet is not yet valid |
| `expired` | The validity window has elapsed |
| `superseded`, `revoked` | An authorized later record names this packet |
| `mismatched` | The record or a bound identity is not the expected one, or the comment was edited |
| `malformed` | No single canonical packet that satisfies its profile |
| `unavailable` | An observation could not be completed |

An HTTP 403, 404, 429 or 5xx, a TLS or transport failure, a timeout, a response
that does not parse, a comment without a complete author observation and an
issue with more comments than the bounded index reads (1,000) are all
`unavailable`. A 404 is not treated as proof of deletion.

### Supersession and revocation

Only canonical records change a packet's standing. A later packet supersedes
an earlier one when it names that `authorization_id` in `supersedes`, has a
higher `sequence`, stays within the same repository, programme, record issue,
profile and transaction kind, and is itself an unedited record by a policy
authorizer. A revocation is a record with marker
`<!-- ripr-release-authorization-revocation:v1 -->` and schema
`ripr-release-authorization-revocation/v1`.

A comment that says "cancelled", an issue closure, a label, a reaction or an
edit revokes nothing. A later record that names the packet but is malformed,
edited, unauthorized or outside the boundary is listed in the receipt under
`ignored_candidates` and changes nothing. Revocation after an immutable public
mutation stops unused operations; it does not undo the completed object.

## Receipt

The receipt (`ripr-release-authorization-receipt/v1`) holds the locator, the
repository, comment, issuer and policy observations, the packet's profile,
identities, operation and channel matrices, subjects, environment and
credential names, executors, the supersession observations, every check with
its state, the disposition and reasons. `external_mutation_attempts` and
`credential_requests` are always `false`.

`receipt_id` is the SHA-256 of the canonical receipt. It does not depend on
the working directory or on the order in which GitHub enumerates comments.
The comment id, body digest and issuer id are evidence and are part of it.

## Claim boundary

An `authorized` receipt establishes that one exact, unedited record by a
policy authorizer was read from GitHub and matches the supplied inputs. It
does not bind that record to an event, reserve or consume it, create a tag,
request a credential or publish anything. It is static evidence about a
record and makes no claim about runtime or mutation-test outcomes.

The schemas of the transactions a packet names (GitHub, crates.io,
marketplaces) and of the release plan are owned by their own issues. This
verifier treats their schema names, request ids and digests as exact opaque
identities and compares them with the supplied bindings.
