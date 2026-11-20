#!/usr/bin/env bash
set -euo pipefail
evidence="$(mktemp -d "$RUNNER_TEMP/ripr-packed-shards.XXXXXX")"
printf 'path=%s\n' "$evidence" >> "$GITHUB_OUTPUT"
workspace_identity() {
  git rev-parse HEAD
  git status --porcelain=v1 --untracked-files=all
  git for-each-ref --format='%(refname):%(objectname)'
  git config --local --list
  git hash-object Cargo.lock .config/nextest.toml .github/workflows/rust-gates.yml .github/workflows/rust-test-shards.yml
}
checkout_status="$(git status --porcelain=v1 --untracked-files=all)"
if [ -n "$checkout_status" ]; then
  echo "::error::Packed partitions require an initially clean checkout"
  exit 1
fi
workspace_identity > "$evidence/baseline"
reports=(
  target/ripr/reports/discarded-matcher-cli-5713
  "target/nextest/ci/python-symlink-source-admission-cli-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.txt"
  target/ripr/reports/repair-card-usability.json
  target/ripr/reports/repair-card-usability.md
  target/ripr/reports/blind-journey-execute.json
  target/ripr/reports/blind-journey-execute.md
)
finish=0
for partition in 1 2; do
  workspace_identity | cmp -s "$evidence/baseline" - || {
    echo "::error::Workspace identity changed before partition $partition"
    exit 1
  }
  stage="$evidence/shard-$partition"
  mkdir -p "$stage/nextest/ci" target/nextest/ci
  rm -f target/nextest/ci/{junit.xml,run-context.txt,shard-*.status,list-*.txt}
  partition_temp="$(mktemp -d "$RUNNER_TEMP/ripr-partition-$partition.XXXXXX")"
  partition_config="target/nextest/ci/packed-partition-$partition.toml"
  python3 - "$partition_temp" > "$partition_config" <<'PYCONF'
import json
import sys
print("[env]")
for key in ("TMPDIR", "TMP", "TEMP"):
    print(f"{key} = {{value = {json.dumps(sys.argv[1])}, relative = false, force = true}}")
PYCONF
  for report in "${reports[@]}"; do
    if [ -e "$report" ] || [ -L "$report" ]; then
      echo "::error::Stale shard report before partition $partition: $report"
      exit 1
    fi
  done
  (
    set -euo pipefail
    checkout_sha="$(git rev-parse HEAD)"
    {
      printf 'repository=%s\ncheckout_sha=%s\nrun_id=%s\nrun_attempt=%s\n' "$GITHUB_REPOSITORY" "$checkout_sha" "$GITHUB_RUN_ID" "$GITHUB_RUN_ATTEMPT"
      printf 'profile=ci\nshard=%s/2\ncommand=cargo nextest run --workspace --profile ci --partition hash:%s/2 --config target/nextest/ci/packed-partition-%s.toml\n' "${partition}" "${partition}" "${partition}"
      cargo nextest --version
      rustc --version
      cargo --version
      printf 'input_blobs (Cargo.lock, nextest.toml, rust-gates.yml, rust-test-shards.yml):\n'
      git hash-object Cargo.lock .config/nextest.toml .github/workflows/rust-gates.yml .github/workflows/rust-test-shards.yml
    } > target/nextest/ci/run-context.txt
    # The partition lists let `verify` prove the shards are a disjoint cover.
    # Listing reuses the build step's binaries; shard 1 also lists the whole
    # workspace. The lists are taken before the run, from the same binaries
    # the run executes: taken after it, shard 4 spent 4.3 to 6.1 minutes here
    # on every run (4 to 16 seconds on the other shards), with the cause
    # hidden by the discarded stderr (#12915).
    cargo nextest list --workspace --profile ci --partition hash:${partition}/2 --color never --config "$partition_config" --message-format oneline 2> /dev/null | sort > target/nextest/ci/list-${partition}.txt
    if [ "$partition" = 1 ]; then
      cargo nextest list --workspace --profile ci --color never --config "$partition_config" --message-format oneline 2> /dev/null | sort > target/nextest/ci/list-all.txt
    fi
    set +e
    export RIPR_PACKED_TEST_PARTITION="$partition"
    cargo nextest run --workspace --profile ci --partition hash:${partition}/2 --config "$partition_config"
    test_exit=$?
    set -e
    printf 'nextest_exit=%s\n' "$test_exit" >> target/nextest/ci/run-context.txt
    printf '%s\n' "$test_exit" > target/nextest/ci/shard-${partition}.status
    if [ "$test_exit" -eq 0 ] && ! grep -Eq '<testsuites [^>]*tests="[1-9][0-9]*"' target/nextest/ci/junit.xml; then
      echo "::error::nextest shard ${partition} succeeded without a fresh JUnit report naming at least one test"
      exit 1
    fi
    exit "$test_exit"
  ) &
  pid=$!
  if wait "$pid"; then
    :
  else
    finish=1
  fi
  for file in target/nextest/ci/{junit.xml,run-context.txt,shard-*.status,list-*.txt}; do
    if [ -f "$file" ]; then cp -- "$file" "$stage/nextest/ci/"; fi
  done
  if [ -d target/nextest/ci/build-metrics ]; then
    cp -R -- target/nextest/ci/build-metrics "$stage/nextest/ci/"
  fi
  for report in "${reports[@]}"; do
    if [ -e "$report" ]; then
      mkdir -p -- "$stage/$(dirname "${report#target/}")"
      mv -- "$report" "$stage/${report#target/}"
    fi
  done
  workspace_identity | cmp -s "$evidence/baseline" - || {
    echo "::error::Workspace identity changed during partition $partition"
    exit 1
  }
done
exit "$finish"
