<!-- section: Fixed -->
- `cargo xtask check-droid-review-config` now requires Droid job
  `contents: read`, rejects `contents: write` and `id-token: write`, and
  fails only when rollout docs list `contents: write` as a required
  permission rather than a dropped or must-not-add note (#7501).
