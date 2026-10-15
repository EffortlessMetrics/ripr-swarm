#!/usr/bin/env bash
set -u
export CARGO_HOME=/workspace/.cargo RUSTUP_HOME=/workspace/.rustup
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_TARGET_DIR=/workspace/qualification-7915/target
export CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cd /workspace/qualification-7915/repo
run() {
  local name=$1; shift
  printf '%q ' "$@" > "../logs/$name.command"; printf '\n' >> "../logs/$name.command"
  "$@" > "../logs/$name.log" 2>&1
  local rc=$?
  printf '%s\n' "$rc" > "../logs/$name.exit"
  printf '%s exit=%s\n' "$name" "$rc"
  return "$rc"
}
run doctor-named cargo xtask worktree doctor || exit $?
run strict-lint cargo clippy --locked -p xtask --all-targets -- -D warnings || exit $?
run check-fast cargo xtask check-fast || exit $?
run preflight cargo policy preflight || exit $?
run ci-policy env RIPR_POLICY_PREFLIGHT_RECEIPT=target/ripr/reports/policy-preflight.json cargo xtask ci-policy || exit $?
