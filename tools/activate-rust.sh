#!/usr/bin/env bash
# Source this file in Bash; it cannot activate a parent shell when executed.
# This boundary only selects installed tools. It does not install or qualify them.

if [ -z "${BASH_VERSION:-}" ]; then
    printf '%s\n' 'activate-rust: source this file from Bash.' >&2
    return 2 2>/dev/null || exit 2
fi
if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    printf '%s\n' 'activate-rust: source this file with --pinned, an exact release, or stable.' >&2
    exit 2
fi

_ripr_activate_rust() {
    local repo_root selector pin_line pin_pattern cargo_home rustup_home rustup_bin
    local compiler compiler_id exact_compiler exact_id current_compiler release host exact cargo_bin sysroot
    local llvm_bin llvm_cov llvm_profdata llvm_status llvm_cov_version llvm_profdata_version llvm_release llvm_pattern
    local variable declaration command_name new_path path_tail path_part prefix path_keep
    local -a prefixes

    if (( $# != 1 )); then
        printf '%s\n' 'activate-rust: explicitly pass --pinned, an exact release, or stable.' >&2
        return 2
    fi
    # Validate declaration metadata before reading values or committing exports.
    for variable in CARGO_HOME RUSTUP_HOME RUSTUP_TOOLCHAIN RUSTUP_AUTO_INSTALL RUSTC PATH LLVM_COV LLVM_PROFDATA; do
        declaration=$(builtin declare -p "$variable" 2>/dev/null) || continue
        if [[ ! $declaration =~ ^declare[[:space:]]+(--|-x)[[:space:]] ]]; then
            printf 'activate-rust: %s must be an ordinary scalar, optionally exported; repair its shell attributes explicitly or use a fresh shell. No activation was applied.\n' "$variable" >&2
            return 1
        fi
    done
    for command_name in cargo rustc; do
        if builtin declare -F "$command_name" >/dev/null; then
            printf 'activate-rust: %s is a shell function; remove it explicitly or use a fresh shell before activation.\n' "$command_name" >&2
            return 1
        fi
        if builtin alias "$command_name" >/dev/null 2>&1; then
            printf 'activate-rust: %s is a shell alias; remove it explicitly or use a fresh shell before activation.\n' "$command_name" >&2
            return 1
        fi
    done
    repo_root=$(dirname -- "${BASH_SOURCE[0]}") || return 1
    # An explicit relative path bypasses CDPATH without assigning to it.
    [[ $repo_root == /* ]] || repo_root=./$repo_root
    repo_root=$(builtin cd -- "$repo_root/.." && builtin pwd -P) || return 1
    selector=$1
    if [[ $selector == --pinned ]]; then
        selector=
        pin_pattern='^[[:space:]]*channel[[:space:]]*=[[:space:]]*"([^"]+)"[[:space:]]*(#.*)?$'
        while IFS= read -r pin_line || [[ -n $pin_line ]]; do
            if [[ $pin_line =~ $pin_pattern ]]; then
                if [[ -n $selector ]]; then
                    printf '%s\n' 'activate-rust: rust-toolchain.toml has multiple channel entries.' >&2
                    return 1
                fi
                selector=${BASH_REMATCH[1]}
            fi
        done < "$repo_root/rust-toolchain.toml" || return 1
    fi
    if [[ $selector != stable && ! $selector =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[[:alnum:]_.-]+)?$ ]]; then
        printf 'activate-rust: unsupported selector %q; use --pinned, an exact stable release, or stable.\n' "$selector" >&2
        return 2
    fi

    if [[ -z ${HOME-} && ( -z ${CARGO_HOME+x} || -z ${RUSTUP_HOME+x} ) ]]; then
        printf '%s\n' 'activate-rust: set HOME or both absolute CARGO_HOME and RUSTUP_HOME values.' >&2
        return 1
    fi
    cargo_home=${CARGO_HOME-${HOME-}/.cargo}
    rustup_home=${RUSTUP_HOME-${HOME-}/.rustup}
    if [[ $cargo_home != /* || $rustup_home != /* ]]; then
        printf '%s\n' 'activate-rust: CARGO_HOME and RUSTUP_HOME must be nonempty absolute paths; keep the same homes for fixture subprocesses.' >&2
        return 1
    fi
    for variable in RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER CARGO_BUILD_RUSTC_WRAPPER CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER; do
        if [[ -n ${!variable-} ]]; then
            printf 'activate-rust: %s is set; unset it explicitly before selecting an unwrapped compiler.\n' "$variable" >&2
            return 1
        fi
    done
    rustup_bin=$cargo_home/bin/rustup
    if [[ ! -x $rustup_bin ]]; then
        rustup_bin=$(type -P rustup) || {
            printf '%s\n' 'activate-rust: rustup is missing from the selected Cargo home and PATH.' >&2
            return 1
        }
    fi
    compiler=$(CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" RUSTUP_AUTO_INSTALL=0 \
        "$rustup_bin" which --toolchain "$selector" rustc) || {
        printf 'activate-rust: install the selected toolchain once: rustup toolchain install %q --profile minimal --component rustfmt --component clippy\n' "$selector" >&2
        return 1
    }
    if [[ $compiler != /* || ! -x $compiler ]]; then
        printf '%s\n' 'activate-rust: rustup did not resolve an executable absolute rustc path.' >&2
        return 1
    fi
    compiler_id=$("$compiler" -vV) || return 1
    release= host= llvm_release=
    while IFS= read -r pin_line; do
        case $pin_line in
            'release: '*) release=${pin_line#release: } ;;
            'host: '*) host=${pin_line#host: } ;;
            'LLVM version: '*) llvm_release=${pin_line#LLVM version: } ;;
        esac
    done <<< "$compiler_id"
    if [[ $compiler_id != rustc\ * || ! $release =~ ^[0-9]+\.[0-9]+\.[0-9]+$ || ! $host =~ ^[[:alnum:]_]+(-[[:alnum:]_]+)+$ || ! $llvm_release =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        printf '%s\n' 'activate-rust: rustc -vV did not identify an exact stable release and host.' >&2
        return 1
    fi
    exact=$release-$host
    if [[ $selector != stable && $selector != "$release" && $selector != "$exact" ]]; then
        printf 'activate-rust: requested %s but the installed compiler identifies as %s.\n' "$selector" "$exact" >&2
        return 1
    fi
    exact_compiler=$(CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" RUSTUP_AUTO_INSTALL=0 \
        "$rustup_bin" which --toolchain "$exact" rustc) || {
        printf 'activate-rust: freeze this release once: rustup toolchain install %q --profile minimal --component rustfmt --component clippy\n' "$exact" >&2
        return 1
    }
    if [[ $exact_compiler != /* || ! -x $exact_compiler ]]; then
        printf '%s\n' 'activate-rust: the exact release did not resolve an executable absolute rustc path.' >&2
        return 1
    fi
    exact_compiler=$(cd -- "$(dirname -- "$exact_compiler")" && printf '%s/%s\n' "$(pwd -P)" "${exact_compiler##*/}") || return 1
    exact_id=$("$exact_compiler" -vV) || return 1
    if [[ $exact_id != "$compiler_id" ]]; then
        printf '%s\n' 'activate-rust: requested and exact-release rustc identities differ; no activation was applied.' >&2
        return 1
    fi
    current_compiler=${RUSTC-}
    if [[ $current_compiler == /* && -x $current_compiler ]]; then
        current_compiler=$(cd -- "$(dirname -- "$current_compiler")" && printf '%s/%s\n' "$(pwd -P)" "${current_compiler##*/}") || return 1
    fi
    if [[ -n $current_compiler && $current_compiler != "$exact_compiler" ]]; then
        printf 'activate-rust: RUSTC selects %q; unset it explicitly to select %q.\n' "$RUSTC" "$exact_compiler" >&2
        return 1
    fi
    if [[ -n ${CARGO_BUILD_RUSTC-} && $CARGO_BUILD_RUSTC != "$exact_compiler" ]]; then
        printf '%s\n' 'activate-rust: CARGO_BUILD_RUSTC selects another compiler; unset it explicitly before activation.' >&2
        return 1
    fi
    sysroot=$("$exact_compiler" --print sysroot) || return 1
    if [[ $sysroot != /* ]]; then
        printf '%s\n' 'activate-rust: rustc did not report an absolute sysroot.' >&2
        return 1
    fi
    sysroot=$(cd -- "$sysroot" && pwd -P) || return 1
    cargo_bin=$(CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" RUSTUP_AUTO_INSTALL=0 \
        "$rustup_bin" which --toolchain "$exact" cargo) || return 1
    if [[ $cargo_bin != /* || ! -x $cargo_bin ]]; then
        printf '%s\n' 'activate-rust: rustup did not resolve an executable absolute Cargo path.' >&2
        return 1
    fi
    cargo_bin=$(cd -- "$(dirname -- "$cargo_bin")" && printf '%s/%s\n' "$(pwd -P)" "${cargo_bin##*/}") || return 1
    if [[ $exact_compiler != "$sysroot/bin/rustc" || $cargo_bin != "$sysroot/bin/cargo" ]]; then
        printf '%s\n' 'activate-rust: rustc, Cargo and sysroot do not identify the same installed toolchain.' >&2
        return 1
    fi

    llvm_bin=$sysroot/lib/rustlib/$host/bin
    llvm_cov=$llvm_bin/llvm-cov
    llvm_profdata=$llvm_bin/llvm-profdata
    llvm_status='unavailable (install llvm-tools-preview for this exact toolchain before coverage)'
    if [[ -x $llvm_cov && -x $llvm_profdata ]]; then
        llvm_cov_version=$("$llvm_cov" --version) || return 1
        llvm_profdata_version=$("$llvm_profdata" --version) || return 1
        llvm_pattern='LLVM version ([0-9]+\.[0-9]+\.[0-9]+)'
        if [[ ! $llvm_cov_version =~ $llvm_pattern || ${BASH_REMATCH[1]-} != "$llvm_release" ]]; then
            printf '%s\n' 'activate-rust: selected llvm-cov does not report the compiler LLVM version; no activation was applied.' >&2
            return 1
        fi
        if [[ ! $llvm_profdata_version =~ $llvm_pattern || ${BASH_REMATCH[1]-} != "$llvm_release" ]]; then
            printf '%s\n' 'activate-rust: selected llvm-profdata does not report the compiler LLVM version; no activation was applied.' >&2
            return 1
        fi
        llvm_status='version probes passed; coverage execution NOT_RUN'
    fi

    prefixes=("$sysroot/bin" "$llvm_bin" "$cargo_home/bin")
    for prefix in "${prefixes[@]}"; do
        if [[ $prefix == *:* ]]; then
            printf 'activate-rust: PATH directory %q contains a colon; choose a Cargo home and physical toolchain path without colons, including symlink targets. No activation was applied.\n' "$prefix" >&2
            return 1
        fi
    done
    new_path=$sysroot/bin:$llvm_bin:$cargo_home/bin
    path_tail=${PATH-}
    while :; do
        path_part=${path_tail%%:*}
        path_keep=1
        for prefix in "${prefixes[@]}"; do
            if [[ $path_part == "$prefix" ]]; then
                path_keep=0
                break
            fi
        done
        if (( path_keep )); then new_path=$new_path:$path_part; fi
        if [[ $path_tail != *:* ]]; then break; fi
        path_tail=${path_tail#*:}
    done

    # Commit parent-shell changes together, after every fallible tool probe.
    export CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" RUSTUP_TOOLCHAIN="$exact" \
        RUSTUP_AUTO_INSTALL=0 RUSTC="$exact_compiler" PATH="$new_path" \
        LLVM_COV="$llvm_cov" LLVM_PROFDATA="$llvm_profdata"
    printf 'Activated installed Rust %s\nCARGO_HOME=%s\nRUSTUP_HOME=%s\nRUSTC=%s\n%s\n' \
        "$exact" "$cargo_home" "$rustup_home" "$exact_compiler" "$exact_id"
    printf 'LLVM coverage tools: %s\n' "$llvm_status"
    printf '%s\n' 'Builds, tests, editor host, and fresh-task restoration: NOT_RUN.'
}

if _ripr_activate_rust "$@"; then
    unset -f _ripr_activate_rust
    return 0
else
    unset -f _ripr_activate_rust
    return 1
fi
