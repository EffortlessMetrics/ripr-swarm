# Droid Smoke Tests

## Retired mention-review procedure

Automatic PR review was retired under #5086, and the trusted mention wrapper
`droid.yml` was retired under #4842. Opening, synchronizing or reopening a PR,
and posting `@droid review` or `@droid security`, must not start those retired
workflows. The former mention smoke procedure is historical only: it checked
trusted-actor admission, MiniMax BYOK review, severity labels and repair advice.
It is not a live test instruction or permission to restore the wrapper.

The separate scheduled/manual security scan remains available. For changes to
that retained workflow or its policy, run `cargo xtask check-droid-review-config`
locally before an explicitly requested live scan. For cross-repository rollout,
start with the [Droid rollout checklist](droid-rollout.md) and the human
[Roll out Factory Droid review](../how-to/roll-out-droid.md) guide; neither
re-enables mention review in this repository.

## Full security scan

Implemented by `.github/workflows/droid-security-scan.yml`.

Triggers:
- `workflow_dispatch`
- weekly Monday 08:00 UTC schedule

Expected:
- scan uses `custom:MiniMax-M3-0`;
- scan window is 7 days;
- severity threshold is `medium`;
- critical findings block (`security_block_on_critical: true`);
- high findings do not block (`security_block_on_high: false`);
- no secrets are printed in output;
- `show_full_output: false` is set;
- `upload_debug_artifacts: false` is set;
- no raw Droid debug artifact is uploaded.

Validate after triggering manually:
- no secrets appear in workflow logs;
- findings include severity and fix direction.

## Artifact hygiene

After changing any Droid workflow, inspect one completed run's artifact list and
confirm:
- no raw artifact named `droid-review-debug-<run_id>` was uploaded;
- if sanitized debug artifacts were explicitly enabled for a private manual run,
  the artifact name is `droid-review-debug-sanitized-*`;
- sanitized debug artifacts do not contain expanded provider keys, GitHub
  tokens, authorization headers, or raw prompt files;
- generated Factory settings (`~/.factory/settings.json`) keep
  `${MINIMAX_API_KEY}` unexpanded in the heredoc;
- `show_full_output: false` is in effect for all Droid action steps;
- `upload_debug_artifacts: false` is in effect for normal secrets-backed runs.

## Rollout smoke sequence

For the retained security-scan rollout, follow the checklist first, then use an
explicit manual Droid Security Scan to confirm `custom:MiniMax-M3-0` and artifact
hygiene. The former same-repository PR mention checks are historical and are not
part of the active rollout.
