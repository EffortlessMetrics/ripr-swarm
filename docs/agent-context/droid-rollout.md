# Droid Rollout Checklist

Use this agent checklist when preparing or reviewing a PR that copies the
`ripr` Factory Droid setup to another repository. The canonical human guide is
[Roll out Factory Droid review](../how-to/roll-out-droid.md); keep this file
thin and link back to that guide instead of duplicating every detail.

Automatic PR review was retired in `ripr-swarm` under #5086, and #4842 retired
manual mention-triggered review. The retained workflow setup covers scheduled
security scans; do not copy or restore either development review lane.

## Inventory

Record these target-repository facts before editing:

```text
Repo:
Default branch:
Factory Droid GitHub App installed:
Actions enabled:
FACTORY_API_KEY scoped:
MINIMAX_API_KEY scoped:
Existing Droid workflows:
Existing workflow allowlist:
Existing AGENTS.md or repo instructions:
Existing validation commands:
Public/fork PR posture:
Security/release sensitivity:
```

Stop before opening the rollout PR if the required app install or secrets are
not ready.

## Expected Files

Most target repositories should receive or adapt:

```text
.github/workflows/droid-security-scan.yml
.factory/skills/review-guidelines/SKILL.md
.factory/rules/droid-review.md
docs/agent-context/review-invariants.md
docs/agent-context/droid-smoke-tests.md
AGENTS.md
```

Small repositories may shorten the review guidance, but they still need the
repair-queue review posture, clean-review inspection record, finding format,
notification rules, and local validation commands.

## Required Invariants

Check for these before marking a rollout PR ready:

```text
show_full_output: false on every Droid action step
upload_debug_artifacts: false on every Droid action step
approved EffortlessMetrics/droid-action-safe SHA
GitHub CLI available on PATH before Droid action starts
no direct Factory-AI/droid-action use for BYOK workflows
no raw $HOME/.factory/** or droid-prompts/** artifact upload
quoted heredoc for settings.json
literal ${MINIMAX_API_KEY} inside settings.json
empty ANTHROPIC_AUTH_TOKEN step env override
empty ANTHROPIC_BASE_URL step env override
no reasoning_effort
custom:MiniMax-M3-0
review_depth: shallow
action refs pinned to immutable SHAs
repo-scoped concurrency for scheduled security scan
cancel-in-progress: false
security scan contents: read
security_scan_schedule: true
security_scan_days: 7
security_severity_threshold: medium
security_block_on_critical: true
security_block_on_high: false
```

If the target repository has a workflow allowlist or shell-line budget policy,
update it for every Droid workflow. Do not invent that policy only for Droid
unless the target repository explicitly wants the governance.

The retained `ripr` permission shape for the scheduled security scan is
`contents: read` (plus `pull-requests: write`,
`issues: write`, and `actions: read`). That posture landed in #2498 after
dropping `contents: write` and `id-token: write`. Do not expand the baseline
rollout to `contents: write`; keep the working `ripr` permissions until a
focused permission-test PR proves a write requirement.

Historical mention-workflow safeguards included a trusted-actor check,
same-repository PR check, `pull_request` rather than `pull_request_target`, and
`contents: read`. These describe the retired wrapper; they are not setup
requirements or permission to restore it.

## Validation

For `ripr` changes to Droid workflow policy or rollout guidance, run:

```bash
cargo xtask check-droid-review-config
cargo xtask check-workflows
cargo xtask check-doc-index
cargo xtask markdown-links
cargo xtask check-pr
```

For target repositories, use the repository's own workflow and policy checks,
then run only the retained scheduled/manual security-scan smoke path from
[Droid smoke tests](droid-smoke-tests.md). The mention-review procedure there is
historical, not a live rollout step.

## Non-Goals

Do not include these in the baseline rollout:

```text
review_depth: deep
self-hosted or VPS runners
pull_request_target
secrets-backed Droid jobs for fork PR code
comment post-processing to remove Factory wrapper mentions
global permission reductions that have not been tested in ripr
raw Droid debug artifact upload
```
