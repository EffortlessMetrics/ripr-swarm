#!/usr/bin/env bash
# Behavioral proof for the sourced Bash boundary; never installs or runs real Rust.
set -euo pipefail

repo_root=${BASH_SOURCE[0]}
if [[ $repo_root == */* ]]; then
    repo_root=${repo_root%/*}
    repo_root=${repo_root:-/}
else
    repo_root=.
fi
[[ $repo_root == /* ]] || repo_root=./$repo_root
repo_root=$(builtin cd -- "$repo_root/.." && builtin pwd -P)
test_root=$(mktemp -d "${TMPDIR:-/tmp}/ripr-activate.XXXXXXXX")
[[ $test_root == /* ]] || test_root=./$test_root
test_root=$(builtin cd -- "$test_root" && builtin pwd -P)
trap 'rm -rf -- "$test_root"' EXIT
bash_bin=$(type -P bash)
system_path=$PATH
host=x86_64-unknown-linux-gnu
exact=1.95.0-$host
cargo_home=$test_root/cargo\ home
rustup_home=$test_root/rustup\ home
fixture_repo=$test_root/repo\ with\ spaces
mkdir -p "$cargo_home/bin" "$rustup_home/toolchains" "$fixture_repo/tools" \
    "$fixture_repo/fixtures/nested" "$test_root/wrong tools" "$test_root/home"
cp "$repo_root/tools/activate-rust.sh" "$fixture_repo/tools/activate-rust.sh"
printf '%s\n' '[toolchain]' 'channel = "1.95.0"' > "$fixture_repo/rust-toolchain.toml"
printf '%s\n' '[toolchain]' 'channel = "1.90.0"' > "$fixture_repo/fixtures/nested/rust-toolchain.toml"

cat > "$cargo_home/bin/rustup" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'rustup %s | auto=%s | cargo=%s | rustup=%s\n' "$*" "${RUSTUP_AUTO_INSTALL-}" "$CARGO_HOME" "$RUSTUP_HOME" >> "$TEST_TRACE"
[[ ${RUSTUP_AUTO_INSTALL-} == 0 ]] || exit 91
[[ $# == 4 && $1 == which && $2 == --toolchain ]] || {
    printf '%s\n' 'unexpected rustup operation' >> "$TEST_TRACE"
    exit 92
}
case $3 in
    stable) selected=stable ;;
    1.95.0|1.95.0-x86_64-unknown-linux-gnu)
        [[ ${TEST_MISSING_EXACT-0} != 1 ]] || exit 93
        selected=1.95.0-x86_64-unknown-linux-gnu ;;
    *) exit 94 ;;
esac
[[ $4 == rustc || $4 == cargo ]] || exit 95
printf '%s/toolchains/%s/bin/%s\n' "$RUSTUP_HOME" "$selected" "$4"
SH
chmod +x "$cargo_home/bin/rustup"

for selected in stable "$exact"; do
    selected_root=$rustup_home/toolchains/$selected
    mkdir -p "$selected_root/bin" "$selected_root/lib/rustlib/$host/bin"
    cat > "$selected_root/bin/rustc" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
script_dir=${0%/*}
sysroot=$(builtin cd -- "$script_dir/.." && builtin pwd -P)
case "$*" in
    '-vV')
        [[ ${TEST_COMPILER_FAIL-0} != 1 ]] || exit 71
        commit=exact-compiler
        if [[ $sysroot == */stable && ${TEST_ALIAS_DRIFT-0} == 1 ]]; then commit=changed-compiler; fi
        release=1.95.0
        if [[ ${TEST_RELEASE_DRIFT-0} == 1 ]]; then release=1.94.0; fi
        printf 'rustc %s (fixture)\nbinary: rustc\ncommit-hash: %s\nhost: x86_64-unknown-linux-gnu\nrelease: %s\nLLVM version: 21.1.0\n' "$release" "$commit" "$release" ;;
    '--print sysroot')
        if [[ ${TEST_SYSROOT_DRIFT-0} == 1 ]]; then sysroot=$sysroot/lib; fi
        printf '%s\n' "$sysroot" ;;
    *) exit 72 ;;
esac
SH
    cat > "$selected_root/bin/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ ${RUSTUP_TOOLCHAIN-} == 1.95.0-x86_64-unknown-linux-gnu ]]
[[ ${RUSTUP_AUTO_INSTALL-} == 0 ]]
[[ $CARGO_HOME == "$TEST_CARGO_HOME" && $RUSTUP_HOME == "$TEST_RUSTUP_HOME" ]]
[[ $RUSTC == "$RUSTUP_HOME/toolchains/$RUSTUP_TOOLCHAIN/bin/rustc" ]]
[[ $PWD == "$TEST_REPO/fixtures/nested" ]]
printf '%s\n' 'fixture inherited exact compiler and homes'
SH
    for tool in llvm-cov llvm-profdata; do
        cat > "$selected_root/lib/rustlib/$host/bin/$tool" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ $* == --version ]] || exit 81
[[ ${TEST_LLVM_FAIL-0} != 1 ]] || exit 82
printf 'LLVM (fixture):\n  LLVM version %s\n' "${TEST_LLVM_VERSION-21.1.0}"
SH
        chmod +x "$selected_root/lib/rustlib/$host/bin/$tool"
    done
    chmod +x "$selected_root/bin/rustc" "$selected_root/bin/cargo"
done
for tool in llvm-cov llvm-profdata cargo; do
    cat > "$test_root/wrong tools/$tool" <<'SH'
#!/usr/bin/env bash
printf '%s\n' 'wrong PATH tool invoked' >> "$TEST_TRACE"
exit 89
SH
    chmod +x "$test_root/wrong tools/$tool"
done

cat > "$fixture_repo/dirname" <<'SH'
#!/usr/bin/env bash
printf '%s\n' 'caller dirname executable invoked' >> "$TEST_TRACE"
exit 97
SH
chmod +x "$fixture_repo/dirname"

tests_run=0
run_case() {
    local name=$1 body=$2
    if ! env -i HOME="$test_root/home" CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" \
        PATH="$test_root/wrong tools:$system_path" TEST_TRACE="$test_root/trace" \
        TEST_REPO="$fixture_repo" TEST_CARGO_HOME="$cargo_home" TEST_RUSTUP_HOME="$rustup_home" \
        "$bash_bin" --noprofile --norc -c 'set -euo pipefail; source_file=$TEST_REPO/tools/activate-rust.sh; '"$body" \
        > "$test_root/output" 2>&1; then
        printf 'FAIL: %s\n' "$name" >&2
        cat "$test_root/output" >&2
        exit 1
    fi
    tests_run=$((tests_run + 1))
    printf 'PASS: %s\n' "$name"
}

# A failed activation must not change set/unset state, exports, PATH, options or cwd.
failure_check='
before=$(declare -p CARGO_HOME RUSTUP_HOME RUSTUP_TOOLCHAIN RUSTUP_AUTO_INSTALL RUSTC PATH LLVM_COV LLVM_PROFDATA 2>/dev/null || :)
options_before=$SHELLOPTS
flags_before=$-
directory_before=$PWD
if source "$source_file" "${selector---pinned}"; then exit 41; fi
after=$(declare -p CARGO_HOME RUSTUP_HOME RUSTUP_TOOLCHAIN RUSTUP_AUTO_INSTALL RUSTC PATH LLVM_COV LLVM_PROFDATA 2>/dev/null || :)
[[ $before == "$after" && $options_before == "$SHELLOPTS" && $flags_before == "$-" && $directory_before == "$PWD" ]]
if builtin declare -F _ripr_activate_rust >/dev/null; then exit 42; fi
'

run_case 'default pin; parent exports; fresh nested fixture; spaces; matching LLVM' '
cd "$TEST_REPO/fixtures/nested"
set -- unrelated-caller-argument "second caller argument"
source "$source_file" --pinned
[[ $# == 2 && $1 == unrelated-caller-argument && $2 == "second caller argument" ]]
[[ $RUSTUP_TOOLCHAIN == 1.95.0-x86_64-unknown-linux-gnu ]]
[[ $(command -v rustc) == "$RUSTC" ]]
[[ $(command -v llvm-profdata) == "$LLVM_PROFDATA" ]]
[[ $(command -v llvm-cov) == "$LLVM_COV" ]]
[[ $LLVM_COV == "$TEST_RUSTUP_HOME/toolchains/$RUSTUP_TOOLCHAIN/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-cov" ]]
[[ $(cargo) == "fixture inherited exact compiler and homes" ]]
if builtin declare -F _ripr_activate_rust >/dev/null; then exit 42; fi
'
relative_source_check='
cd "$TEST_REPO"
set -- unrelated-caller-argument "second caller argument"
cdpath_before=$(declare -p CDPATH)
options_before=$SHELLOPTS; flags_before=$-; directory_before=$PWD
source tools/activate-rust.sh --pinned
[[ $(declare -p CDPATH) == "$cdpath_before" ]]
[[ $options_before == "$SHELLOPTS" && $flags_before == "$-" && $directory_before == "$PWD" ]]
[[ $# == 2 && $1 == unrelated-caller-argument && $2 == "second caller argument" ]]
[[ $RUSTUP_TOOLCHAIN == 1.95.0-x86_64-unknown-linux-gnu && $(command -v rustc) == "$RUSTC" ]]
if builtin declare -F _ripr_activate_rust >/dev/null; then exit 42; fi
cd "$TEST_REPO/fixtures/nested"
[[ $(cargo) == "fixture inherited exact compiler and homes" ]]
'
run_case 'relative source preserves exported CDPATH and caller state' 'export CDPATH=.; '"$relative_source_check"
run_case 'relative source preserves readonly CDPATH and caller state' 'export CDPATH=.; readonly CDPATH; '"$relative_source_check"
run_case 'bare source name keeps caller state and resolves the repository pin' '
builtin cd -- "$TEST_REPO/tools"
set -- unrelated-caller-argument "second caller argument"
options_before=$SHELLOPTS; flags_before=$-; directory_before=$PWD
source activate-rust.sh --pinned
[[ $options_before == "$SHELLOPTS" && $flags_before == "$-" && $directory_before == "$PWD" ]]
[[ $# == 2 && $1 == unrelated-caller-argument && $2 == "second caller argument" ]]
[[ $(command -v rustc) == "$RUSTC" ]]
builtin cd -- "$TEST_REPO/fixtures/nested"
[[ $(cargo) == "fixture inherited exact compiler and homes" ]]
'
for command_name in dirname cd pwd; do
    if [[ $command_name == dirname ]]; then native_call='command dirname'; else native_call="builtin $command_name"; fi
    run_case "$command_name function stays intact and cannot alter path resolution" "
$command_name() { builtin printf '%s\n' 'caller $command_name output'; $native_call \"\$@\"; }
function_before=\$(builtin declare -f $command_name)
set -- unrelated-caller-argument \"second caller argument\"
options_before=\$SHELLOPTS; flags_before=\$-; directory_before=\$PWD
source \"\$source_file\" --pinned
[[ \$(builtin declare -f $command_name) == \"\$function_before\" ]]
[[ \$options_before == \"\$SHELLOPTS\" && \$flags_before == \"\$-\" && \$directory_before == \"\$PWD\" ]]
[[ \$# == 2 && \$1 == unrelated-caller-argument && \$2 == \"second caller argument\" ]]
[[ \$(command -v rustc) == \"\$RUSTC\" ]]
builtin cd -- \"\$TEST_REPO/fixtures/nested\"
[[ \$(cargo) == \"fixture inherited exact compiler and homes\" ]]
"
done
run_case 'cd and pwd aliases stay intact and cannot alter path resolution' '
shopt -s expand_aliases
alias cd="builtin printf \"caller cd output\n\"; builtin cd"
alias pwd="builtin printf \"caller pwd output\n\"; builtin pwd"
aliases_before=$(builtin alias cd pwd)
set -- unrelated-caller-argument "second caller argument"
options_before=$SHELLOPTS; flags_before=$-; directory_before=$PWD
source "$source_file" --pinned
[[ $(builtin alias cd pwd) == "$aliases_before" ]]
[[ $options_before == "$SHELLOPTS" && $flags_before == "$-" && $directory_before == "$PWD" ]]
[[ $# == 2 && $1 == unrelated-caller-argument && $2 == "second caller argument" ]]
[[ $(command -v rustc) == "$RUSTC" ]]
shopt -q expand_aliases
builtin cd -- "$TEST_REPO/fixtures/nested"
[[ $(cargo) == "fixture inherited exact compiler and homes" ]]
'
run_case 'empty PATH component never invokes the caller directory dirname' '
builtin cd -- "$TEST_REPO"
export PATH=:$PATH
set -- unrelated-caller-argument "second caller argument"
options_before=$SHELLOPTS; flags_before=$-; directory_before=$PWD
source tools/activate-rust.sh --pinned
[[ $options_before == "$SHELLOPTS" && $flags_before == "$-" && $directory_before == "$PWD" ]]
[[ $# == 2 && $1 == unrelated-caller-argument && $2 == "second caller argument" ]]
[[ $(command -v rustc) == "$RUSTC" ]]
[[ $(< "$TEST_TRACE") != *"caller dirname executable invoked"* ]]
builtin cd -- "$TEST_REPO/fixtures/nested"
[[ $(cargo) == "fixture inherited exact compiler and homes" ]]
'
run_case 'stable freezes exact release and stays idempotent' '
options_before=$SHELLOPTS; flags_before=$-; directory_before=$PWD
source "$source_file" stable
first_path=$PATH
first_compiler=$RUSTC
source "$source_file" stable
[[ $RUSTUP_TOOLCHAIN == 1.95.0-x86_64-unknown-linux-gnu && $RUSTC == "$first_compiler" && $PATH == "$first_path" ]]
[[ $options_before == "$SHELLOPTS" && $flags_before == "$-" && $directory_before == "$PWD" ]]
'
run_case 'explicit release and explicit host select identical tools' '
source "$source_file" 1.95.0
first_path=$PATH
source "$source_file" 1.95.0-x86_64-unknown-linux-gnu
[[ $PATH == "$first_path" ]]
'
run_case 'ordinary scalar export targets remain supported' '
RUSTUP_TOOLCHAIN=previous
source "$source_file" --pinned
[[ $RUSTUP_TOOLCHAIN == 1.95.0-x86_64-unknown-linux-gnu ]]
'
run_case 'symlinked home keeps caller spelling and resolves physical tools' '
ln -s "$RUSTUP_HOME" "$TEST_REPO/rustup alias"
export RUSTUP_HOME="$TEST_REPO/rustup alias"
export RUSTC="$RUSTUP_HOME/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc"
source "$source_file" --pinned
[[ $RUSTUP_HOME == "$TEST_REPO/rustup alias" ]]
[[ $RUSTC == "$TEST_RUSTUP_HOME/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc" ]]
'
mkdir -p "$test_root/home/.cargo" "$test_root/home/.rustup"
cp -R "$cargo_home/bin" "$test_root/home/.cargo/bin"
cp -R "$rustup_home/toolchains" "$test_root/home/.rustup/toolchains"
run_case 'default homes without shell profiles' '
unset CARGO_HOME RUSTUP_HOME
source "$source_file" --pinned
[[ $CARGO_HOME == "$HOME/.cargo" && $RUSTUP_HOME == "$HOME/.rustup" ]]
'
run_case 'missing HOME refuses without changing or closing the caller' 'unset HOME CARGO_HOME RUSTUP_HOME; '"$failure_check"
run_case 'invalid selector leaves parent unchanged' 'selector=nightly; '"$failure_check"
run_case 'empty selector leaves parent unchanged' 'selector=; '"$failure_check"
run_case 'uninstalled selector leaves parent unchanged' 'selector=9.99.0; '"$failure_check"
run_case 'stable requires exact release installed once' 'export TEST_MISSING_EXACT=1; selector=stable; '"$failure_check"
run_case 'stable identity drift leaves parent unchanged' 'export TEST_ALIAS_DRIFT=1; selector=stable; '"$failure_check"
run_case 'explicit compiler release drift leaves parent unchanged' 'export TEST_RELEASE_DRIFT=1; '"$failure_check"
run_case 'native compiler failure leaves parent unchanged' 'export TEST_COMPILER_FAIL=1; '"$failure_check"
run_case 'sysroot mismatch leaves parent unchanged' 'export TEST_SYSROOT_DRIFT=1; '"$failure_check"
run_case 'existing RUSTC drift requires explicit removal' 'export RUSTC=/different/compiler; '"$failure_check"
run_case 'Cargo-config compiler env drift requires explicit removal' 'export CARGO_BUILD_RUSTC=/different/compiler; '"$failure_check"
for command_name in cargo rustc; do
    run_case "$command_name function is refused and preserved" "
$command_name() { printf '%s\n' foreign-$command_name; }
export -f $command_name
function_before=\$(declare -f $command_name)
$failure_check
[[ \$(declare -f $command_name) == \"\$function_before\" ]]
[[ \$(bash --noprofile --norc -c $command_name) == foreign-$command_name ]]
"
    run_case "$command_name alias is refused and preserved" "
shopt -s expand_aliases
alias $command_name='printf foreign-$command_name'
alias_before=\$(alias $command_name)
$failure_check
[[ \$(alias $command_name) == \"\$alias_before\" ]]
shopt -q expand_aliases
"
done
for wrapper in RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER CARGO_BUILD_RUSTC_WRAPPER CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER; do
    run_case "$wrapper requires explicit removal" "export $wrapper=/a/wrapper; $failure_check"
done
run_case 'relative Cargo home leaves parent unchanged' 'export CARGO_HOME=relative; '"$failure_check"
run_case 'empty Rustup home leaves parent unchanged' 'export RUSTUP_HOME=; '"$failure_check"
run_case 'colon in Cargo home PATH entry leaves parent unchanged' '
ln -s "$CARGO_HOME" "$TEST_REPO/cargo:alias"
export CARGO_HOME="$TEST_REPO/cargo:alias"
'"$failure_check"
run_case 'colon in physical sysroot PATH entry leaves parent unchanged' '
cp -R "$RUSTUP_HOME" "$TEST_REPO/rustup:copy"
export RUSTUP_HOME="$TEST_REPO/rustup:copy"
'"$failure_check"
run_case 'safe home spelling cannot hide a colon in the physical sysroot' '
cp -R "$RUSTUP_HOME" "$TEST_REPO/rustup:physical"
ln -s "$TEST_REPO/rustup:physical" "$TEST_REPO/rustup safe alias"
export RUSTUP_HOME="$TEST_REPO/rustup safe alias"
'"$failure_check"
run_case 'readonly export leaves parent unchanged' 'readonly LLVM_COV=sentinel; '"$failure_check"
run_case 'integer export leaves parent unchanged' 'declare -i RUSTUP_TOOLCHAIN=0; '"$failure_check"
run_case 'array export leaves parent unchanged' 'declare -a LLVM_COV=(sentinel second); '"$failure_check"
# Older Bash cannot create these attributes; the portable core still runs there.
if (( BASH_VERSINFO[0] >= 4 )); then
    run_case 'case-transform export leaves parent unchanged' 'declare -u RUSTUP_TOOLCHAIN=sentinel; '"$failure_check"
fi
if (( BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 3) )); then
    run_case 'nameref home is refused before reading or changing its referent' '
home_target=$CARGO_HOME
declare -n CARGO_HOME=home_target
'"$failure_check"'
[[ $home_target == "$TEST_CARGO_HOME" ]]
'
fi
run_case 'failed LLVM probe leaves parent unchanged' 'export TEST_LLVM_FAIL=1; '"$failure_check"
run_case 'wrong LLVM version leaves parent unchanged' 'export TEST_LLVM_VERSION=19.0.0; '"$failure_check"
run_case 'direct execution refuses to claim parent activation' '
if bash "$source_file"; then exit 43; fi
[[ -z ${RUSTC+x} && -z ${RUSTUP_TOOLCHAIN+x} ]]
'
run_case 'missing explicit selector is refused' '
if source "$source_file"; then exit 45; fi
[[ -z ${RUSTC+x} && -z ${RUSTUP_TOOLCHAIN+x} ]]
'

mv "$rustup_home/toolchains/$exact/lib/rustlib/$host/bin/llvm-cov" "$test_root/saved-llvm-cov"
run_case 'missing optional LLVM cannot reuse a wrong PATH override' '
export LLVM_COV=/wrong/llvm-cov LLVM_PROFDATA=/wrong/llvm-profdata
output=$(source "$source_file" --pinned)
# Command substitution is deliberately a different shell; activate this shell too.
source "$source_file" --pinned
[[ $output == *"LLVM coverage tools: unavailable"* && $output == *NOT_RUN* ]]
[[ $LLVM_COV == "$TEST_RUSTUP_HOME/toolchains/$RUSTUP_TOOLCHAIN/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-cov" ]]
[[ ! -e $LLVM_COV ]]
if "$LLVM_COV" --version; then exit 44; fi
'

trace=$(< "$test_root/trace")
[[ $trace != *'unexpected rustup operation'* && $trace != *'wrong PATH tool invoked'* ]]
printf '%s\n' "Passed $tests_run Bash activation cases with controlled tool fixtures. Real Rust builds/tests and fresh-task restoration were NOT_RUN."
