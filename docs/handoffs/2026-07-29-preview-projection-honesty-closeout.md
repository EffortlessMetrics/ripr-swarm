# Campaign 36 Closeout: Preview Projection Honesty

Date: 2026-07-29  
Campaign: `preview-projection-honesty`  
Status: complete

## Delivered

- #2743 (`17560690002de21a4615a92ac13053d64b1346fe`) documented that JSON
  object member order is semantic-insensitive and excluded exact-byte
  canonical artifacts from that claim.
- #2746 (`8783582935d7ff0b23a68710c90727bc0ac57ad2`) carried the Campaign 36
  plan, non-goals, acceptance criteria, and proof path into the repository.
- #2753 (`fd3a44771e401e074abeec7833f04e12968926a1`) merged final head
  `06e097e6a6ed41c4a2a5e02e67ef3bb4208362f6`, retaining every configured Bun
  bridge profile across human, JSON, SARIF, and GitHub output. The first
  profile remains the compatibility alias; multi-profile JSON adds `profiles[]`.
  Profile placements are associated by producer order, so profiles sharing a
  test file retain distinct placement reasons.

Issues #2716 and #2744 are closed.

## Acceptance and proof

- Blob plus `copy_to_unshared` focused fixtures prove two profiles survive the
  card projection and keep separate placement reasons.
- Human, GitHub, JSON/SARIF, and preview-card tests passed; the focused
  cross-renderer suite passed 8 tests and the preview-card suite passed 14.
- `cargo fmt --all -- --check` and
  `cargo clippy -p ripr --lib --tests -- -D warnings` passed locally.
- Hosted required Rust gates, workspace tests, coverage, UB review, dependency
  review, and source-of-truth checks passed on the final head. The required
  `Ripr Rust Small Result` check passed before auto-merge.
- Golden, dogfood, output-contract, static-language, no-panic, and fixture
  contract checks passed during the lane; hosted required gates revalidated
  the final pushed head after review fixes.

## Claim boundary

The projection remains static preview evidence. It does not claim runtime Bun
execution, mutation outcomes, test adequacy or coverage, public repair-packet
authority, default blocking, or release readiness. The campaign did not add a
new Bun taxonomy, discovery engine, runtime, dependency, or broad refactor.

## Verification gaps and disposition

The local full `cargo xtask check-pr` attempt did not complete because
unrelated Cargo jobs held shared build locks. A local full-library attempt
reported unrelated Windows timing, LSP, and path-normalization failures; the
hosted workspace test gate passed, so those findings are not attributed to
this campaign. GitHub's automated review rate limit prevented a CodeRabbit
review; available review threads were addressed and resolved, including the
profile-placement identity bug.

No Campaign 36 follow-up issue is needed. Runtime Bun support, broader profile
discovery, and non-preview repair-packet promotion remain intentionally outside
this campaign.
