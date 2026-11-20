//! Execute the advisory lane's production script, including failed instruments.
#[path = "../test_support.rs"]
mod test_support;

fn workflow() -> Result<String, String> {
    std::fs::read_to_string(
        test_support::workspace_root()?.join(".github/workflows/future-clippy.yml"),
    )
    .map_err(|e| e.to_string())
}

const EXTRACT: &str = "Extract documented coverage upgrade recipe";
const NATIVE: &str = "Run documented coverage upgrade with native SDKs";
const VERIFY: &str = "Verify documented coverage upgrade execution";

fn coverage_document() -> Result<String, String> {
    std::fs::read_to_string(test_support::workspace_root()?.join("docs/ci/coverage.md"))
        .map_err(|e| e.to_string())
}

fn workflow_step(text: &str, name: &str) -> Result<String, String> {
    let marker = format!("      - name: {name}\n");
    let mut matches = text.match_indices(&marker);
    let (start, _) = matches
        .next()
        .ok_or_else(|| format!("missing workflow step: {name}"))?;
    if matches.next().is_some() {
        return Err(format!("duplicate workflow step: {name}"));
    }
    let tail = &text[start + marker.len()..];
    let end = tail.find("\n      - ").unwrap_or(tail.len());
    let blocks = repo_policy::extract_workflow_run_blocks(&tail[..end]);
    match blocks.as_slice() {
        [block] => Ok(block.text.clone()),
        _ => Err(format!("expected one run block: {name}")),
    }
}

fn step(name: &str) -> Result<String, String> {
    workflow_step(&workflow()?, name)
}

#[test]
fn pinned_component_and_failure_evidence_are_wired() -> Result<(), String> {
    let text = workflow()?;
    assert!(text.contains(
        "uses: dtolnay/rust-toolchain@035fceb3c12fb0c6a9461b52775e16473133e591 # 1.95.0\n"
    ));
    assert!(text.contains("components: clippy\n"));
    assert!(step("Run future Clippy (advisory)")?.contains("cargo +1.95.0 clippy "));
    let (_, summary) = text
        .split_once("- name: Write step summary\n")
        .ok_or("missing summary")?;
    assert!(summary.starts_with("        if: always()\n"));
    assert!(summary.contains("SCAN_OUTCOME: ${{ steps.scan.outcome }}"));
    assert!(text.contains("id: scan\n"));
    let (_, upload) = text
        .split_once("- name: Upload future-clippy log\n")
        .ok_or("missing upload")?;
    assert!(upload.starts_with("        if: always()\n"));
    assert!(upload.contains("target/future-clippy.log"));
    assert!(upload.contains("target/future-clippy-status.txt"));
    assert!(upload.contains("if-no-files-found: error"));
    assert!(!text.contains("lane never fails"));
    Ok(())
}

#[test]
fn stable_compatibility_requires_matching_tools_and_focused_commands() -> Result<(), String> {
    const SETUP: &str = "      - uses: dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable\n        with:\n          toolchain: stable\n          components: clippy, llvm-tools-preview\n";
    const CONTROLS: &str = "cargo +stable test --locked -p repo-policy --test future_clippy";
    const PIN: &str = "ripr_cargo_for 1.95.0 test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture";
    const STABLE: &str = "ripr_cargo_for stable test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture";
    const FRONTDOOR: &str = "ripr_cargo_for stable test --locked -p repo-policy --test frontdoor";
    const CLIPPY: &str =
        "ripr_cargo_for stable clippy --locked -p repo-policy --all-targets -- -D warnings";
    const INSTALL: &str = "rustup toolchain install 1.95.0 --profile minimal --component clippy --component llvm-tools-preview";
    let job = |text: &str, name: &str| {
        text.split_once(&format!("\n  {name}:\n")).map(|(_, tail)| {
            tail.lines()
                .take_while(|line| {
                    line.is_empty()
                        || line.starts_with("    ")
                        || line.trim_start().starts_with('#')
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
    };
    let prepared = |text: &str, document: &str| {
        let (Some(stable), Some(pinned)) =
            (job(text, "stable-repo-policy"), job(text, "future-clippy"))
        else {
            return false;
        };
        let pinned_guard = pinned.lines().find(|line| line.starts_with("    if:"));
        if pinned_guard.is_none()
            || stable.lines().find(|line| line.starts_with("    if:")) != pinned_guard
            || !stable.contains("    env:\n      RUSTUP_TOOLCHAIN: stable\n")
            || stable
                .lines()
                .filter(|line| line.trim_start().starts_with("RUSTUP_TOOLCHAIN:"))
                .count()
                != 1
            || !stable
                .lines()
                .any(|line| line == "    runs-on: ubuntu-latest")
            || !stable.lines().any(|line| line == "    timeout-minutes: 15")
            || stable.matches("uses: dtolnay/rust-toolchain@").count() != 1
            || stable.lines().any(|line| {
                line.starts_with("        if:")
                    || line.starts_with("      - if:")
                    || line.trim_start().starts_with("continue-on-error:")
            })
        {
            return false;
        }
        let Some((before, after)) = stable.split_once(SETUP) else {
            return false;
        };
        if !repo_policy::extract_workflow_run_blocks(before).is_empty() {
            return false;
        }
        let blocks = repo_policy::extract_workflow_run_blocks(after);
        let names = ["Check stable workflow controls", EXTRACT, NATIVE, VERIFY];
        if blocks.len() != names.len()
            || !blocks.iter().zip(names).all(|(block, name)| {
                workflow_step(text, name)
                    .is_ok_and(|selected| selected.trim_end() == block.text.trim_end())
            })
            || blocks[0].text.trim() != CONTROLS
            || !blocks[1].text.contains("rm -f \"$recipe\" \"$recipe.md\"")
            || !blocks[2].text.contains("export RUSTC=\"$compiler\"")
            || !blocks[2].text.contains("export PATH=\"${RUSTC%/*}:$PATH\"")
            || !blocks[2]
                .text
                .contains("native_status=(\"${PIPESTATUS[@]}\")")
            || !blocks[3]
                .text
                .contains("recipe_completed source_sha=$GITHUB_SHA")
        {
            return false;
        }
        [
            "<!-- ripr:coverage-upgrade:start -->",
            "<!-- ripr:coverage-upgrade:end -->",
            INSTALL,
            "rustup update stable",
            "rustup component add --toolchain stable clippy llvm-tools-preview",
            PIN,
            STABLE,
            FRONTDOOR,
            CLIPPY,
        ]
        .iter()
        .all(|expected| {
            document
                .lines()
                .filter(|line| line.trim() == *expected)
                .count()
                == 1
        })
    };
    let text = workflow()?;
    let document = coverage_document()?;
    assert!(
        prepared(&text, &document),
        "native recipe proof must be wired"
    );
    for (label, from, to) in [
        (
            "missing LLVM component",
            "components: clippy, llvm-tools-preview",
            "components: clippy",
        ),
        ("wrong toolchain", "toolchain: stable", "toolchain: 1.95.0"),
        (
            "wrong fixture activation",
            "RUSTUP_TOOLCHAIN: stable",
            "RUSTUP_TOOLCHAIN: 1.95.0",
        ),
        (
            "conditional preparation",
            "          components: clippy, llvm-tools-preview\n",
            "          components: clippy, llvm-tools-preview\n        if: false\n",
        ),
        (
            "ignored failures",
            "    name: stable-repo-policy\n",
            "    name: stable-repo-policy\n    continue-on-error: true\n",
        ),
        (
            "omitted executed controls",
            CONTROLS,
            "echo controls omitted",
        ),
        ("omitted extractor", EXTRACT, "omitted extractor"),
        ("omitted native runner", NATIVE, "omitted native runner"),
        ("omitted verifier", VERIFY, "omitted verifier"),
    ] {
        let changed = text.replace(from, to);
        assert_ne!(changed, text, "{label} must change the real workflow");
        assert!(!prepared(&changed, &document), "{label} must reject");
    }
    for command in [INSTALL, PIN, STABLE, FRONTDOOR, CLIPPY] {
        assert!(
            !prepared(&text, &document.replace(command, ":")),
            "missing {command} must reject"
        );
    }
    let duplicate = format!("{text}\n      - name: {EXTRACT}\n        run: true\n");
    assert!(
        !prepared(&duplicate, &document),
        "duplicate step must reject"
    );
    let late_setup = text.replacen(SETUP, "", 1) + SETUP;
    assert!(
        !prepared(&late_setup, &document),
        "tools must precede proof"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
mod behavior {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::{Command, Output};

    use super::{EXTRACT, NATIVE, VERIFY, coverage_document, step};

    const CARGO: &str = r#"#!/bin/bash
printf '%s\n' "$@" > "$ARG_LOG"
case "$SCAN_CASE" in
  missing) echo "error: the 'cargo-clippy' binary is not installed for toolchain '1.95-x86_64-unknown-linux-gnu'" >&2; exit 1 ;;
  compile) echo 'error[E0308]: mismatched types' >&2; echo '{"reason":"build-finished","success":false}'; exit 101 ;;
  empty) exit 0 ;;
  completion-only) echo '{"reason":"build-finished","success":true}'; exit 0 ;;
  warning) echo 'warning: clippy::duration_suboptimal_units' >&2 ;;
esac
echo '{"reason":"compiler-artifact","fresh":true}'
if [[ "$SCAN_CASE" != truncated ]]; then
  echo '{"reason":"build-finished","success":true}'
fi
if [[ "$SCAN_CASE" == late-failed ]]; then
  echo '{"reason":"build-finished","success":false}'
fi
echo '    Finished dev profile' >&2
"#;

    // These tools model selection and failure semantics; they are not native Rust proof.
    const RECIPE_RUSTUP: &str = r#"#!/bin/bash
set -e
if [[ $1 == which && $# == 4 && $2 == --toolchain ]]; then
  [[ "$SCAN_CASE" != which ]] || exit 74
  case "$3" in
    1.95.0) sdk="$FIXTURE_ROOT/pinned SDK"; test -f "$FIXTURE_ROOT/pin-installed" || exit 73 ;;
    stable) sdk="$FIXTURE_ROOT/stable SDK" ;;
    *) exit 75 ;;
  esac
  selected="$sdk/bin/$4"
  if [[ "$SCAN_CASE" == foreign-clippy && $4 == cargo-clippy ]]; then
    selected="$FIXTURE_ROOT/pinned SDK/bin/cargo-clippy"
  fi
  test -f "$selected" || exit 73
  printf '%s\n' "$selected"
  exit 0
fi
test "${RUSTUP_AUTO_INSTALL:-}" = 0
printf '%s\n' "$*" >> "$FIXTURE_ROOT/provision-events"
case "$*" in
  'toolchain install 1.95.0 --profile minimal --component clippy --component llvm-tools-preview')
    [[ "$SCAN_CASE" != provision ]] || exit 66
    touch "$FIXTURE_ROOT/pin-installed" ;;
  'update stable') [[ "$SCAN_CASE" != update ]] || exit 71 ;;
  'component add --toolchain stable clippy llvm-tools-preview')
    [[ "$SCAN_CASE" != components ]] || exit 72 ;;
  *) exit 76 ;;
esac
"#;

    const RECIPE_RUSTC: &str = r#"#!/bin/bash
set -e
case "$0" in
  "$FIXTURE_ROOT/pinned SDK/bin/rustc") sdk="$FIXTURE_ROOT/pinned SDK"; release=1.95.0; llvm=22.1.2 ;;
  "$FIXTURE_ROOT/stable SDK/bin/rustc") sdk="$FIXTURE_ROOT/stable SDK"; release=1.99.1; llvm=23.1.1 ;;
  *) exit 85 ;;
esac
case "$*" in
  -vV)
    [[ "$SCAN_CASE" != compiler ]] || exit 65
    printf 'rustc %s fixture\nrelease: %s\nLLVM version: %s\n' "$release" "$release" "$llvm"
    [[ "$SCAN_CASE" == host ]] || echo 'host: fixture-host' ;;
  '--print sysroot')
    [[ "$SCAN_CASE" != sysroot ]] || exit 67
    printf '%s\n' "$sdk" ;;
  *) exit 68 ;;
esac
"#;

    const RECIPE_LLVM: &str = r#"#!/bin/bash
[[ "$*" == --version ]] || exit 68
if [[ $0 == */llvm-cov && "$SCAN_CASE" == cov ]]; then exit 69; fi
if [[ $0 == */llvm-profdata && "$SCAN_CASE" == llvm ]]; then exit 70; fi
case "$0" in
  "$FIXTURE_ROOT/pinned SDK/"*) echo 'LLVM fixture version 22.1.2' ;;
  "$FIXTURE_ROOT/stable SDK/"*) echo 'LLVM fixture version 23.1.1' ;;
  *) exit 85 ;;
esac
"#;

    const RECIPE_CARGO: &str = r#"#!/bin/bash
set -e
case "$0" in
  "$FIXTURE_ROOT/pinned SDK/bin/cargo")
    channel=1.95.0; sdk="$FIXTURE_ROOT/pinned SDK"; release=1.95.0; raw=10; layout=llvm_raw64_le_v10 ;;
  "$FIXTURE_ROOT/stable SDK/bin/cargo")
    channel=stable; sdk="$FIXTURE_ROOT/stable SDK"; release=1.99.1; raw=11; layout=llvm_raw64_le_v11 ;;
  *) exit 85 ;;
esac
[[ "$RUSTC" == "$sdk/bin/rustc" && "$RUSTUP_TOOLCHAIN" == "$channel" ]] || exit 86
[[ "$(command -v rustc)" == "$RUSTC" && "$(command -v cargo)" == "$0" ]] || exit 86
[[ "$LLVM_COV" == "$sdk/lib/rustlib/fixture-host/bin/llvm-cov" ]] || exit 86
[[ "$LLVM_PROFDATA" == "$sdk/lib/rustlib/fixture-host/bin/llvm-profdata" ]] || exit 86
explicit=$("$RUSTC" -vV)
literal=$(rustc -vV)
nested=$(cd "$FIXTURE_ROOT/nested fixture"; rustc -vV)
[[ "$explicit" == "rustc $release fixture"$'\n'* && "$literal" == "$explicit" && "$nested" == "$explicit" ]] || exit 86
printf '%s' "$channel" >> "$FIXTURE_ROOT/cargo-events"
printf '|%s' "$@" >> "$FIXTURE_ROOT/cargo-events"
printf '\n' >> "$FIXTURE_ROOT/cargo-events"
case "$*" in
  'test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture')
    phase="$channel"
    if [[ "$SCAN_CASE" != no-profile ]]; then
      [[ "$SCAN_CASE" != wrong-profile ]] || raw=9
      [[ "$SCAN_CASE" != profile-prefix ]] || printf 'test fixture::real_profile ... '
      printf 'real LLVM association: raw_version=%s, layout=%s, build-id=abc123, retained=184, mapping="matched", original=failure, replay=1\n' "$raw" "$layout"
      if [[ "$SCAN_CASE" == duplicate-profile ]]; then
        printf 'real LLVM association: raw_version=%s, layout=%s, build-id=abc123, retained=184, mapping="matched", original=failure, replay=1\n' "$raw" "$layout"
      fi
    fi
    ;;
  'test --locked -p repo-policy --test frontdoor') phase=frontdoor ;;
  'clippy --locked -p repo-policy --all-targets -- -D warnings')
    phase=clippy
    [[ "$(command -v cargo-clippy)" == "$sdk/bin/cargo-clippy" ]] || exit 86
    if [[ "$SCAN_CASE" != silent-clippy ]]; then
      printf '\033[1;32m    Finished\033[0m `dev` profile [unoptimized + debuginfo] target(s) in 0.01s\n'
    fi
    ;;
  *) exit 87 ;;
esac
if [[ "$phase" != clippy ]]; then
  printf '    Finished `test` profile [unoptimized + debuginfo] target(s)\n'
  if [[ "$SCAN_CASE" != no-summary ]]; then
    passed=42; failed=0
    [[ "$SCAN_CASE" != zero-tests ]] || passed=0
    [[ "$SCAN_CASE" != failed-summary ]] || failed=10
    printf 'test result: ok. %s passed; %s failed; 0 ignored; 0 measured; 63 filtered out; finished in 0.01s\n' "$passed" "$failed"
    if [[ "$SCAN_CASE" == duplicate-summary ]]; then
      echo 'test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 63 filtered out; finished in 0.01s'
    fi
  fi
fi
case "$SCAN_CASE:$phase" in
  cargo-pin:1.95.0) exit 77 ;;
  cargo-stable:stable) exit 78 ;;
  cargo-frontdoor:frontdoor) exit 79 ;;
  cargo-clippy:clippy) exit 80 ;;
esac
"#;

    const EXPECTED_EVENTS: [&str; 4] = [
        "1.95.0|test|--locked|-p|repo-policy|--lib|coverage_diagnostics|--|--nocapture",
        "stable|test|--locked|-p|repo-policy|--lib|coverage_diagnostics|--|--nocapture",
        "stable|test|--locked|-p|repo-policy|--test|frontdoor",
        "stable|clippy|--locked|-p|repo-policy|--all-targets|--|-D|warnings",
    ];

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(case: &str) -> Result<Self, String> {
            let root = super::test_support::scratch(&format!("future-clippy-{case}"))?;
            fs::create_dir_all(root.join("bin")).map_err(|e| e.to_string())?;
            let fixture = Self(root);
            fixture.executable("cargo", CARGO)?;
            Ok(fixture)
        }

        fn executable(&self, name: &str, source: &str) -> Result<(), String> {
            self.executable_at(&format!("bin/{name}"), source)
        }

        fn executable_at(&self, relative: &str, source: &str) -> Result<(), String> {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().ok_or("missing parent")?)
                .map_err(|e| e.to_string())?;
            fs::write(&path, source).map_err(|e| e.to_string())?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
        }

        fn recipe_fixture(case: &str, document: &str) -> Result<Self, String> {
            let fixture = Self::new(case)?;
            for directory in ["docs/ci", "runner temp", "nested fixture"] {
                fs::create_dir_all(fixture.0.join(directory)).map_err(|e| e.to_string())?;
            }
            fs::write(fixture.0.join("docs/ci/coverage.md"), document)
                .map_err(|e| e.to_string())?;
            fs::write(
                fixture.0.join("nested fixture/rust-toolchain.toml"),
                "[toolchain]\nchannel = \"unselected-fixture-toolchain\"\n",
            )
            .map_err(|e| e.to_string())?;
            fixture.executable("rustup", RECIPE_RUSTUP)?;
            fixture.executable("rustc", "#!/bin/bash\nexit 88\n")?;
            fixture.executable("cargo", "#!/bin/bash\nexit 89\n")?;
            fixture.executable("llvm-profdata", "#!/bin/bash\nexit 90\n")?;
            for sdk in ["pinned SDK", "stable SDK"] {
                fixture.executable_at(&format!("{sdk}/bin/rustc"), RECIPE_RUSTC)?;
                fixture.executable_at(&format!("{sdk}/bin/cargo"), RECIPE_CARGO)?;
                fixture
                    .executable_at(&format!("{sdk}/bin/cargo-clippy"), "#!/bin/bash\nexit 0\n")?;
                for tool in ["llvm-cov", "llvm-profdata"] {
                    fixture.executable_at(
                        &format!("{sdk}/lib/rustlib/fixture-host/bin/{tool}"),
                        RECIPE_LLVM,
                    )?;
                }
            }
            if case == "missing-llvm" {
                fs::remove_file(
                    fixture
                        .0
                        .join("pinned SDK/lib/rustlib/fixture-host/bin/llvm-profdata"),
                )
                .map_err(|e| e.to_string())?;
            }
            let clippy = fixture.0.join("stable SDK/bin/cargo-clippy");
            if case == "missing-clippy" {
                fs::remove_file(&clippy).map_err(|e| e.to_string())?;
            } else if case == "nonexecutable-clippy" {
                fs::set_permissions(clippy, fs::Permissions::from_mode(0o644))
                    .map_err(|e| e.to_string())?;
            }
            Ok(fixture)
        }

        fn events(&self) -> Result<Vec<String>, String> {
            let path = self.0.join("cargo-events");
            if path.exists() {
                Ok(self
                    .read("cargo-events")?
                    .lines()
                    .map(str::to_owned)
                    .collect())
            } else {
                Ok(Vec::new())
            }
        }

        fn read(&self, path: &str) -> Result<String, String> {
            fs::read_to_string(self.0.join(path)).map_err(|e| e.to_string())
        }

        fn run(&self, name: &str, case: &str, outcome: &str) -> Result<Output, String> {
            self.run_script(&step(name)?, case, outcome)
        }

        fn run_script(&self, source: &str, case: &str, outcome: &str) -> Result<Output, String> {
            let script = self.0.join("step.sh");
            fs::write(&script, source).map_err(|e| e.to_string())?;
            let mut paths = vec![self.0.join("bin")];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").ok_or("missing PATH")?,
            ));
            Command::new("timeout")
                .args(["20s", "bash", "--noprofile", "--norc", "-eo", "pipefail"])
                .arg(script)
                .current_dir(&self.0)
                .env(
                    "PATH",
                    std::env::join_paths(paths).map_err(|e| e.to_string())?,
                )
                .env("ARG_LOG", self.0.join("args"))
                .env("SCAN_CASE", case)
                .env("SCAN_OUTCOME", outcome)
                .env("FIXTURE_ROOT", &self.0)
                .env("RUNNER_TEMP", self.0.join("runner temp"))
                .env("RUSTUP_TOOLCHAIN", "stable")
                .env_remove("RUSTC")
                .env_remove("LLVM_COV")
                .env_remove("LLVM_PROFDATA")
                .env_remove("BASH_ENV")
                .env("GITHUB_SHA", "candidate-sha")
                .env("GITHUB_RUN_ID", "7307")
                .env("GITHUB_RUN_ATTEMPT", "2")
                .env("GITHUB_STEP_SUMMARY", self.0.join("summary"))
                .output()
                .map_err(|e| e.to_string())
        }

        fn recipe(&self, case: &str) -> Result<Output, String> {
            for name in [EXTRACT, NATIVE] {
                let output = self.run(name, case, "unused")?;
                if !output.status.success() {
                    return Ok(output);
                }
            }
            self.run(VERIFY, case, "unused")
        }

        fn summary(&self, outcome: &str) -> Result<String, String> {
            let output = self.run("Write step summary", "empty", outcome)?;
            assert!(output.status.success(), "{output:?}");
            self.read("summary")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(e) = fs::remove_dir_all(&self.0) {
                eprintln!("retain fixture {}: {e}", self.0.display());
            }
        }
    }

    #[test]
    fn stable_identity_uses_matching_llvm_and_preserves_tool_failures() -> Result<(), String> {
        let document = coverage_document()?;
        for case in ["success", "profile-prefix"] {
            let fixture = Fixture::recipe_fixture(case, &document)?;
            let extracted = fixture.run(EXTRACT, case, "unused")?;
            assert!(extracted.status.success(), "{extracted:?}");
            let native = fixture.run(NATIVE, case, "unused")?;
            assert!(native.status.success(), "{native:?}");
            let stdout = String::from_utf8_lossy(&native.stdout);
            assert!(stdout.contains(&format!(
                "initial_sdk_rustc={}/stable SDK/bin/rustc\n",
                fixture.0.display()
            )));
            let verified = fixture.run(VERIFY, case, "unused")?;
            assert!(verified.status.success(), "{verified:?}");
            let events = fixture.events()?;
            assert_eq!(events.len(), EXPECTED_EVENTS.len());
            for (observed, expected) in events.iter().zip(EXPECTED_EVENTS) {
                assert_eq!(observed.as_str(), expected);
            }
            let log = fixture.read("runner temp/ripr-coverage-upgrade.log")?;
            for identity in [
                "rustc 1.95.0 fixture",
                "rustc 1.99.1 fixture",
                "LLVM fixture version 22.1.2",
                "LLVM fixture version 23.1.1",
                "raw_version=10, layout=llvm_raw64_le_v10",
                "raw_version=11, layout=llvm_raw64_le_v11",
            ] {
                assert!(log.contains(identity), "missing {identity}: {log}");
            }
            assert_eq!(
                fixture.read("provision-events")?,
                "toolchain install 1.95.0 --profile minimal --component clippy --component llvm-tools-preview\nupdate stable\ncomponent add --toolchain stable clippy llvm-tools-preview\n"
            );
        }
        for (case, status, count) in [
            ("provision", 66, 0),
            ("update", 71, 0),
            ("components", 72, 0),
            ("which", 74, 0),
            ("compiler", 65, 0),
            ("sysroot", 67, 0),
            ("host", 1, 0),
            ("cov", 69, 0),
            ("llvm", 70, 0),
            ("missing-llvm", 127, 0),
            ("cargo-pin", 77, 1),
            ("cargo-stable", 78, 2),
            ("cargo-frontdoor", 79, 3),
            ("cargo-clippy", 80, 4),
            ("missing-clippy", 73, 3),
            ("nonexecutable-clippy", 1, 3),
            ("foreign-clippy", 1, 3),
            ("tee", 74, 4),
        ] {
            let fixture = Fixture::recipe_fixture(case, &document)?;
            if case == "tee" {
                fixture.executable("tee", "#!/bin/bash\n/bin/cat\nexit 74\n")?;
            }
            let output = fixture.recipe(case)?;
            assert_eq!(output.status.code(), Some(status), "{case}: {output:?}");
            assert_eq!(fixture.events()?.len(), count, "{case}");
            let log = fixture.0.join("runner temp/ripr-coverage-upgrade.log");
            if log.exists() {
                assert!(
                    !fixture
                        .read("runner temp/ripr-coverage-upgrade.log")?
                        .contains("recipe_completed "),
                    "{case} must not retain a successful native completion"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn documented_recipe_boundaries_refuse_ambiguous_or_stale_execution() -> Result<(), String> {
        let document = coverage_document()?;
        let start = "<!-- ripr:coverage-upgrade:start -->";
        let end = "<!-- ripr:coverage-upgrade:end -->";
        let (before, tail) = document.split_once(start).ok_or("missing recipe start")?;
        let (_, after) = tail.split_once(end).ok_or("missing recipe end")?;
        let body = |script: &str| format!("{before}{start}\n```bash\n{script}```\n{end}{after}");
        for (case, changed) in [
            ("missing-start", document.replace(start, "")),
            ("missing-end", document.replace(end, "")),
            (
                "duplicate-start",
                document.replace(start, &format!("{start}\n{start}")),
            ),
            (
                "duplicate-end",
                document.replace(end, &format!("{end}\n{end}")),
            ),
            (
                "missing-heading",
                document.replace("### Validate locally", "### Other"),
            ),
            (
                "duplicate-heading",
                document.replace(
                    "### Validate locally",
                    "### Validate locally\n### Validate locally",
                ),
            ),
            (
                "wrong-language",
                document.replace(&format!("{start}\n```bash"), &format!("{start}\n```sh")),
            ),
            (
                "unterminated-fence",
                document.replace(&format!("```\n{end}"), end),
            ),
            ("extra-fence", body("```\n")),
            ("empty", body("")),
            ("invalid-bash", body("(\n")),
        ] {
            assert_ne!(changed, document, "{case}");
            let fixture = Fixture::recipe_fixture(case, &changed)?;
            let extracted = fixture.0.join("runner temp/ripr-coverage-upgrade.sh");
            fs::write(
                &extracted,
                "echo stale > \"$FIXTURE_ROOT/stale-executed\"\n",
            )
            .map_err(|e| e.to_string())?;
            let output = fixture.recipe(case)?;
            assert!(!output.status.success(), "{case} was green: {output:?}");
            assert!(fixture.events()?.is_empty(), "{case} ran Cargo");
            assert!(
                !fixture.0.join("stale-executed").exists(),
                "{case} ran stale script"
            );
            if case.starts_with("missing-") || case.starts_with("duplicate-") {
                assert!(!extracted.exists(), "{case} retained the old script");
            }
        }
        Ok(())
    }

    #[test]
    fn documented_recipe_requires_all_four_native_checks() -> Result<(), String> {
        let document = coverage_document()?;
        for (index, expected) in EXPECTED_EVENTS.iter().enumerate() {
            let command = format!("ripr_cargo_for {}", expected.replace('|', " "));
            for (case, replacement) in [
                ("omitted", ":".to_string()),
                ("comment-only", format!("# {command}")),
                ("dead", format!("if false; then {command}; fi")),
            ] {
                let changed = document.replace(&command, &replacement);
                assert_ne!(changed, document, "{case} {index}");
                let fixture = Fixture::recipe_fixture(&format!("{case}-{index}"), &changed)?;
                let output = fixture.recipe("success")?;
                assert!(!output.status.success(), "{case} {index}: {output:?}");
                assert_eq!(fixture.events()?.len(), 3);
            }
        }
        for (case, from, to, status) in [
            ("wrong-explicit-compiler", " RUSTC=\"$ripr_rustc\"", "", 86),
            (
                "wrong-PATH-compiler",
                "${ripr_rustc%/*}:$ripr_llvm_bin:",
                "",
                86,
            ),
            (
                "wrong-nested-toolchain",
                "RUSTUP_TOOLCHAIN=\"$ripr_channel\"",
                "RUSTUP_TOOLCHAIN=stable",
                86,
            ),
            (
                "missing-explicit-pin-install",
                "rustup toolchain install 1.95.0 --profile minimal --component clippy --component llvm-tools-preview",
                ":",
                73,
            ),
        ] {
            let changed = document.replace(from, to);
            assert_ne!(changed, document, "{case}");
            let fixture = Fixture::recipe_fixture(case, &changed)?;
            let output = fixture.recipe("success")?;
            assert_eq!(output.status.code(), Some(status), "{case}: {output:?}");
            assert!(
                fixture.events()?.is_empty(),
                "{case} must reject before Cargo evidence"
            );
        }
        Ok(())
    }

    #[test]
    fn documented_recipe_evidence_refuses_zero_subjects_and_stale_runs() -> Result<(), String> {
        let document = coverage_document()?;
        for case in [
            "zero-tests",
            "failed-summary",
            "no-summary",
            "duplicate-summary",
            "no-profile",
            "wrong-profile",
            "duplicate-profile",
            "silent-clippy",
        ] {
            let fixture = Fixture::recipe_fixture(case, &document)?;
            let output = fixture.recipe(case)?;
            assert!(!output.status.success(), "{case} was green: {output:?}");
            assert_eq!(fixture.events()?.len(), 4, "{case} must reach the verifier");
        }
        let fixture = Fixture::recipe_fixture("current-evidence", &document)?;
        let output = fixture.recipe("success")?;
        assert!(output.status.success(), "{output:?}");
        let relative = "runner temp/ripr-coverage-upgrade.log";
        let original = fixture.read(relative)?;
        let completion = "recipe_completed source_sha=candidate-sha run_id=7307 run_attempt=2";
        let first = "Completed: 1.95.0 cargo test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture";
        let summary = "test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 63 filtered out; finished in 0.01s\n";
        let last_summary = original
            .rfind(summary)
            .ok_or("missing final test summary")?;
        let mut missing_final_summary = original.clone();
        missing_final_summary.replace_range(last_summary..last_summary + summary.len(), "");
        for (case, changed) in [
            (
                "stale-source",
                original.replace("source_sha=candidate-sha", "source_sha=old-sha"),
            ),
            (
                "stale-run",
                original.replace("run_id=7307", "run_id=old-run"),
            ),
            (
                "stale-attempt",
                original.replace("run_attempt=2", "run_attempt=1"),
            ),
            ("missing-completion", original.replace(completion, "")),
            (
                "duplicate-completion",
                original.replace(completion, &format!("{completion}\n{completion}")),
            ),
            (
                "early-completion",
                format!("{completion}\n{}", original.replace(completion, "")),
            ),
            ("missing-first-command", original.replace(first, "")),
            (
                "duplicate-first-command",
                original.replace(first, &format!("{first}\n{first}")),
            ),
            (
                "wrong-order",
                original.replace("Completed: 1.95.0 cargo", "Completed: stable cargo"),
            ),
            ("previous-phase-summary", missing_final_summary),
        ] {
            assert_ne!(changed, original, "{case}");
            fs::write(fixture.0.join(relative), &changed).map_err(|e| e.to_string())?;
            let rejected = fixture.run(VERIFY, "success", "unused")?;
            assert!(!rejected.status.success(), "{case} was green: {rejected:?}");
        }
        // A fresh partial execution must truncate even a fully successful older log.
        fs::write(fixture.0.join(relative), original).map_err(|e| e.to_string())?;
        fs::write(
            fixture.0.join("runner temp/ripr-coverage-upgrade.sh"),
            ":\n",
        )
        .map_err(|e| e.to_string())?;
        let partial = fixture.run(NATIVE, "success", "unused")?;
        assert!(partial.status.success(), "{partial:?}");
        let rejected = fixture.run(VERIFY, "success", "unused")?;
        assert!(
            !rejected.status.success(),
            "stale evidence was reused: {rejected:?}"
        );
        Ok(())
    }

    #[test]
    fn missing_component_is_not_a_success() -> Result<(), String> {
        let fixture = Fixture::new("missing")?;
        let output = fixture.run("Run future Clippy (advisory)", "missing", "failure")?;
        assert!(
            !output.status.success(),
            "missing Clippy was green: {output:?}"
        );
        assert!(
            fixture
                .read("target/future-clippy.log")?
                .contains("not installed")
        );
        let summary = fixture.summary("failure")?;
        assert!(summary.contains("cargo_exit=1"));
        assert!(!summary.contains("scan_completed=true"));
        Ok(())
    }

    #[test]
    fn warnings_and_cached_artifacts_remain_advisory() -> Result<(), String> {
        let fixture = Fixture::new("warning")?;
        let output = fixture.run("Run future Clippy (advisory)", "warning", "success")?;
        assert!(output.status.success(), "{output:?}");
        let args = fixture.read("args")?;
        assert!(args.starts_with("+1.95.0\nclippy\n--workspace\n--all-targets\n"));
        assert!(args.contains("--message-format=json-render-diagnostics\n"));
        assert_eq!(args.lines().filter(|arg| *arg == "-W").count(), 10);
        assert!(args.contains("clippy::same_length_and_capacity\n"));
        assert!(
            fixture
                .read("target/future-clippy.log")?
                .contains("warning:")
        );
        let summary = fixture.summary("success")?;
        for evidence in [
            "source_sha=candidate-sha",
            "run_id=7307",
            "run_attempt=2",
            "toolchain=1.95.0",
            "cargo_exit=0",
            "tee_exit=0",
            "scan_completed=true",
        ] {
            assert!(summary.contains(evidence), "missing {evidence}: {summary}");
        }
        Ok(())
    }

    #[test]
    fn failed_compile_or_incomplete_current_scan_cannot_reuse_old_success() -> Result<(), String> {
        for case in [
            "compile",
            "empty",
            "truncated",
            "completion-only",
            "late-failed",
        ] {
            let fixture = Fixture::new(case)?;
            fs::create_dir_all(fixture.0.join("target")).map_err(|e| e.to_string())?;
            fs::write(fixture.0.join("target/future-clippy.log"), "{\"reason\":\"compiler-artifact\"}\n{\"reason\":\"build-finished\",\"success\":true}\n")
                .map_err(|e| e.to_string())?;
            fs::write(
                fixture.0.join("target/future-clippy-status.txt"),
                "scan_completed=true\nsource_sha=old-sha\n",
            )
            .map_err(|e| e.to_string())?;
            let output = fixture.run("Run future Clippy (advisory)", case, "failure")?;
            assert!(!output.status.success(), "{case} was green: {output:?}");
            let summary = fixture.summary("failure")?;
            assert!(
                !summary.contains("scan_completed=true"),
                "{case}: {summary}"
            );
            assert!(!summary.contains("old-sha"), "{case}: {summary}");
            if case == "compile" {
                assert!(summary.contains("cargo_exit=101"), "{summary}");
            }
        }
        Ok(())
    }

    #[test]
    fn failed_log_capture_and_absent_scan_are_visible() -> Result<(), String> {
        let fixture = Fixture::new("tee")?;
        fixture.executable("tee", "#!/bin/bash\n/bin/cat\nexit 74\n")?;
        let output = fixture.run("Run future Clippy (advisory)", "warning", "failure")?;
        assert!(!output.status.success(), "failed tee was green: {output:?}");
        let summary = fixture.summary("failure")?;
        assert!(summary.contains("cargo_exit=0"), "{summary}");
        assert!(summary.contains("tee_exit=74"), "{summary}");
        assert!(!summary.contains("scan_completed=true"), "{summary}");
        let absent = Fixture::new("not-run")?;
        assert!(
            absent
                .summary("skipped")?
                .contains("Scan evidence unavailable")
        );
        fs::create_dir_all(absent.0.join("target")).map_err(|e| e.to_string())?;
        for identity in [
            "source_sha=old-sha\nrun_id=7307\nrun_attempt=2",
            "source_sha=candidate-sha\nrun_id=old-run\nrun_attempt=2",
            "source_sha=candidate-sha\nrun_id=7307\nrun_attempt=1",
        ] {
            fs::write(absent.0.join("summary"), "").map_err(|e| e.to_string())?;
            fs::write(
                absent.0.join("target/future-clippy-status.txt"),
                format!("{identity}\nscan_completed=true\n"),
            )
            .map_err(|e| e.to_string())?;
            let summary = absent.summary("skipped")?;
            assert!(summary.contains("Scan evidence unavailable"), "{summary}");
            assert!(!summary.contains("scan_completed=true"), "{summary}");
        }
        Ok(())
    }
}
