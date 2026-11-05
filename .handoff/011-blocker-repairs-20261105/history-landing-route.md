# History-preserving landing route inspection

Observed 2026-11-05. No settings changed; no CI requested or merge attempted.

## Live evidence

- Repository metadata: merge commits enabled, squash enabled, rebase disabled.
- Live main: f4bc18a361360a3d33f6f7e561991504b6f3464a.
- Active branch ruleset 18845973 targets default branch. It blocks deletion/non-fast-forward, requires PRs and resolved review threads, permits merge/squash/rebase, has no bypass actors and reports current_user_can_bypass=never. No merge_queue or required_linear_history rule appears. The only other returned repository/parent ruleset is the tag-only release-transaction-pins rule.
- Branch metadata reports required context Ripr Rust Small Result, app 15368, enforcement non_admins. This partial branch metadata is not a complete classic-protection inventory.
- Full classic protection read returns 403 Resource not accessible by integration. Shell gh read also returned Forbidden. No alternative credentials or permission changes attempted.
- At the actual live main, routed-rust.yml still requires queued G to have exactly one parent, that parent equal queue base, and the squash subject to name its sole PR. Relevant workflow/admission/merge-flow source is unchanged from dc4771c5 to live main.

## Minimal supported option

A one-transaction user exception to queue-only CI for the exact history-sync PR, using the native history-sync pull_request merge-ready route, with source S and destination B/H/G bindings, all mandatory non-diff qualification, review and late head/eligibility checks. On success, use the protected expected-head true merge (`--merge --match-head-commit H`), never admin/bypass. Current visible merge settings already permit this; no settings mutation is indicated by available evidence. The concrete successor head will change as remaining repairs/outputs/current-main composition finish, so do not request a stale-head approval.

Ordinary repairs remain queue-only. The exception is a single transaction, not a persistent configuration change, so there is no settings restoration to perform for this option. It does not lift promotion trust/release selection holds. If classic protection requires queue-only landing, this option must stop rather than bypass it; its exact field is unknown because the endpoint is forbidden. Obtain an authorized current read before proposing any protection change.

## Why alternate squash composition cannot preserve ancestry

Source S is not an ancestor of destination B. A queue squash G with sole parent B has ancestor set ancestors(B) plus G, regardless of H's extra parents. It therefore cannot add S's history. Landing another history-preserving predecessor first merely moves the same exceptional transaction earlier. A tag retaining S is not S ancestry on main. A native queue profile admitting true history merges would be a different workflow/queue contract requiring its own decision and qualification, a larger change than this transaction exception.

## Authority still pending

The current user authorizes queue-only engineering qualification, not this legacy CI exception or any protection/settings change. No exception is inferred from repository admin role, prior temporary-setting history, or the release urgency. Request the precise transaction exception only once the candidate is concrete/reviewable. No release publication authority is inferred here.
