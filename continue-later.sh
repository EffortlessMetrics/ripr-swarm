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
run later-fmt cargo fmt --check || exit $?
run later-spec-format cargo xtask check-spec-format || exit $?
run later-spec-numbering cargo xtask check-spec-numbering || exit $?
run later-fixture-contracts cargo xtask check-fixture-contracts || exit $?
run later-traceability cargo xtask check-traceability || exit $?
run later-changelog cargo xtask check-changelog-fragments || exit $?
run later-whitespace git diff --check origin/main...HEAD || exit $?
