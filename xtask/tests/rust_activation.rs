//! Keep the #13123 Bash activation contract in ordinary workspace tests.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn sourced_rust_activation_preserves_identity_and_parent_shell() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("bash")
        .arg(root.join("tools/test-activate-rust.sh"))
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("Bash must be available for Rust activation tests: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "Rust activation tests failed ({}):\n{stdout}{stderr}",
            output.status
        ));
    }
    let cases = stdout.lines().find_map(|line| {
        line.strip_prefix("Passed ")?
            .split_whitespace()
            .next()?
            .parse::<usize>()
            .ok()
    });
    // Case-transform and nameref controls additionally run when Bash supports them.
    if cases.is_none_or(|count| count < 46)
        || !stdout.contains("Bash activation cases with controlled tool fixtures.")
    {
        return Err(format!(
            "Rust activation harness did not report its behavioral controls:\n{stdout}{stderr}"
        ));
    }
    Ok(())
}

#[test]
fn installed_rustup_activation_keeps_the_exact_fixture_compiler() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("bash")
        .args(["--noprofile", "--norc", "-c"])
        .arg(
            r#"
set -euo pipefail
compiler=${RUSTC-${CARGO_BUILD_RUSTC-rustc}}
native_rustc=$("$compiler" -vV)
native_cargo=$(cargo -vV)
release=$(printf '%s\n' "$native_rustc" | sed -n 's/^release: //p')
host=$(printf '%s\n' "$native_rustc" | sed -n 's/^host: //p')
[[ $release =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && -n $host ]]
exact=$release-$host
installed=$(rustup which --toolchain "$exact" rustc) || {
    printf 'Native activation control requires the already installed exact SDK: %s\n' "$exact" >&2
    exit 1
}
installed_identity=$("$installed" -vV)
[[ $installed_identity == "$native_rustc" ]]
cargo_home=${CARGO_HOME-${HOME-}/.cargo}
rustup_home=${RUSTUP_HOME-${HOME-}/.rustup}

# Select explicitly in this fresh test shell, independent of build-time overrides.
unset RUSTC CARGO_BUILD_RUSTC RUSTUP_TOOLCHAIN RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER \
    CARGO_BUILD_RUSTC_WRAPPER CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER
source "$1/tools/activate-rust.sh" "$exact"
check='
set -euo pipefail
[[ $RUSTUP_AUTO_INSTALL == 0 && $RUSTUP_TOOLCHAIN == "$3" ]]
[[ $CARGO_HOME == "$4" && $RUSTUP_HOME == "$5" ]]
[[ $(command -v rustc) == "$RUSTC" && $(command -v cargo) == "${RUSTC%/*}/cargo" ]]
actual_rustc=$(rustc -vV)
absolute_rustc=$("$RUSTC" -vV)
actual_cargo=$(cargo -vV)
selected=$(rustup which rustc)
selected_rustc=$("$selected" -vV)
[[ $actual_rustc == "$1" && $absolute_rustc == "$1" && $selected_rustc == "$1" && $actual_cargo == "$2" ]]
printf "Native activation verified at %s for %s\n" "$6" "$RUSTUP_TOOLCHAIN"
'
"$BASH" --noprofile --norc -c "$check" native-root \
    "$native_rustc" "$native_cargo" "$exact" "$cargo_home" "$rustup_home" root

fixture=$(mktemp -d "${TMPDIR:-/tmp}/ripr-native-activation.XXXXXXXX")
[[ $fixture == /* ]] || fixture=$PWD/$fixture
trap 'rm -rf -- "$fixture"' EXIT
mkdir -p "$fixture/nested"
printf '%s\n' '[toolchain]' 'channel = "1.0.0"' > "$fixture/rust-toolchain.toml"
(
    builtin cd -- "$fixture/nested"
    if unactivated=$(env -u RUSTUP_TOOLCHAIN rustup which rustc 2> "$fixture/unactivated.stderr"); then
        unactivated_identity=$("$unactivated" -vV)
        [[ $unactivated_identity != "$native_rustc" ]]
    else
        grep -F '1.0.0' "$fixture/unactivated.stderr" > /dev/null
    fi
    printf '%s\n' 'Opposing pin counterfactual verified without automatic installation'
    "$BASH" --noprofile --norc -c "$check" native-nested \
        "$native_rustc" "$native_cargo" "$exact" "$cargo_home" "$rustup_home" nested
)
printf 'Native compiler identity:\n%s\nNative Cargo identity:\n%s\n' "$native_rustc" "$native_cargo"
"#,
        )
        .arg("ripr-native-activation")
        .arg(&root)
        .current_dir(&root)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env_remove("BASH_ENV")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("Bash must be available for native Rust activation: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || !stdout.contains("Native activation verified at root for ")
        || !stdout.contains("Opposing pin counterfactual verified without automatic installation")
        || !stdout.contains("Native activation verified at nested for ")
    {
        return Err(format!(
            "Native Rust activation control did not complete ({}):\n{stdout}{stderr}",
            output.status
        ));
    }
    println!("{stdout}");
    Ok(())
}
