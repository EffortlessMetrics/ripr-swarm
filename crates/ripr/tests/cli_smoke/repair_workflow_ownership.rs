use super::*;

/// Reconstructed #13452 control; the unpublished original test is unavailable.
fn exercise_before_boundary(intervention: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let artifacts = [
        "workflow.json",
        "commands.md",
        "agent-brief.json",
        "before.repo-exposure.json",
        "agent-packet.json",
        "attempt-baseline.json",
    ];
    for facade in [false, true] {
        for existing in [false, true] {
            let fixture =
                RepairBuildIgnoreFixture::new(unique_temp_workspace("late-ignore-13452"))?;
            let root = fixture.0.as_path();
            init_producer_fixture_repo(root)?;
            let workflow = root.join("target/ripr/workflow");
            if existing {
                std::fs::create_dir_all(&workflow)?;
                for name in artifacts {
                    std::fs::write(workflow.join(name), format!("prior {name}"))?;
                }
            }
            let shim_dir = root.join(".git/ignore-shim");
            std::fs::create_dir_all(&shim_dir)?;
            let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|path| path.join("git"))
                .find(|path| path.is_file())
                .ok_or("git executable unavailable")?;
            let shim = shim_dir.join("git");
            std::fs::write(
                &shim,
                r#"#!/bin/sh
if test "$RIPR_INTERVENTION" = publication; then
  for candidate in "$RIPR_TEST_ROOT"/target/ripr/repair-attempts/*/artifacts; do
    if test -d "$candidate"; then
      printf 'foreign publication' > "$candidate/foreign.txt"
      printf '%s' "$candidate/foreign.txt" > "$RIPR_IGNORE_WITNESS/publication"
      exit 128
    fi
  done
fi
case " $* " in
  *" check-ignore --no-index --quiet -- target "*)
    if test -f "$RIPR_IGNORE_WITNESS/initial" && ! test -f "$RIPR_IGNORE_WITNESS/prewrite"; then
      : > "$RIPR_IGNORE_WITNESS/prewrite"
      exec "$RIPR_REAL_GIT" "$@"
    fi
    if test -f "$RIPR_IGNORE_WITNESS/prewrite"; then
      : > "$RIPR_IGNORE_WITNESS/final"
      for candidate in "$RIPR_TEST_ROOT"/target/ripr/repair-preparations/*; do
        if test -d "$candidate"; then
          printf '%s' "$candidate" > "$RIPR_IGNORE_WITNESS/preparation"
          case "$RIPR_INTERVENTION" in
            preparation) printf 'foreign preparation' > "$candidate/commands.md" ;;
            baseline) printf 'foreign baseline' > "$candidate/attempt-baseline.json" ;;
            blocked) mkdir "$candidate/attempt-baseline.json"
                     printf 'foreign child' > "$candidate/attempt-baseline.json/foreign.txt" ;;
          esac
        fi
      done
      if test "$RIPR_INTERVENTION" = shared; then
        mkdir -p "$RIPR_TEST_ROOT/target/ripr/workflow"
        printf 'foreign commands' > "$RIPR_TEST_ROOT/target/ripr/workflow/commands.md"
      fi
      case "$RIPR_INTERVENTION" in baseline|blocked|publication) exit 0 ;; esac
      exit 1
    fi
    : > "$RIPR_IGNORE_WITNESS/initial"
    ;;
esac
exec "$RIPR_REAL_GIT" "$@"
"#,
            )?;
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))?;
            let mut paths = vec![shim_dir.clone()];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            let path = std::env::join_paths(paths)?.to_string_lossy().into_owned();
            let root_arg = root.to_string_lossy();
            let args = if facade {
                vec!["repair", BOUNDARY_GAP_SEAM_ID, "--root", &root_arg]
            } else {
                vec![
                    "agent",
                    "repair",
                    "--root",
                    &root_arg,
                    "--seam-id",
                    BOUNDARY_GAP_SEAM_ID,
                    "--phase",
                    "before",
                ]
            };
            let binary = std::env::var("RIPR_13452_SUBJECT_BINARY")
                .unwrap_or_else(|_| env!("CARGO_BIN_EXE_ripr").to_string());
            let output = run_command_with_env(
                &binary,
                root,
                &args,
                &[
                    ("PATH", &path),
                    ("RIPR_REAL_GIT", &real_git.to_string_lossy()),
                    ("RIPR_IGNORE_WITNESS", &shim_dir.to_string_lossy()),
                    ("RIPR_INTERVENTION", intervention),
                    ("RIPR_TEST_ROOT", &root_arg),
                ],
            )?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            let operational = matches!(intervention, "baseline" | "blocked" | "publication");
            if !shim_dir.join("initial").exists()
                || !shim_dir.join("final").exists()
                || output.status.code() != Some(2)
                || !output.stdout.is_empty()
                || stderr.contains("disposition=not_started")
                || (!operational && !stderr.contains("preparation was retained"))
                || (operational
                    && (stderr.contains("disposition=not_started")
                        || (intervention != "publication" && !stderr.contains("attempt-baseline"))
                        || (intervention == "publication"
                            && !stderr.contains("finalize head verification failed"))))
            {
                return Err(format!("late refusal boundary not reached: {output:?}").into());
            }
            if intervention == "publication" {
                let path = PathBuf::from(std::fs::read_to_string(shim_dir.join("publication"))?);
                if std::fs::read(&path).map_err(|error| {
                    format!(
                        "intervening publication lost at {}: {error}",
                        path.display()
                    )
                })? != b"foreign publication"
                {
                    return Err("intervening publication bytes changed".into());
                }
            }
            if matches!(intervention, "preparation" | "baseline" | "blocked") {
                let preparation =
                    PathBuf::from(std::fs::read_to_string(shim_dir.join("preparation"))?);
                let (path, expected) = match intervention {
                    "preparation" => (
                        preparation.join("commands.md"),
                        b"foreign preparation".as_slice(),
                    ),
                    "baseline" => (
                        preparation.join("attempt-baseline.json"),
                        b"foreign baseline".as_slice(),
                    ),
                    _ => (
                        preparation.join("attempt-baseline.json/foreign.txt"),
                        b"foreign child".as_slice(),
                    ),
                };
                if std::fs::read(&path)? != expected {
                    return Err(format!(
                        "intervening {intervention} bytes were changed at {}",
                        path.display()
                    )
                    .into());
                }
                if !stderr.contains("preparation retained") {
                    return Err(format!("failure omitted retained preparation: {stderr}").into());
                }
            }
            for entry in std::fs::read_dir(root.join("target/ripr/repair-attempts"))? {
                if entry?.path().join("attempt.json").exists() {
                    return Err("late refusal published a durable attempt".into());
                }
            }
            for name in artifacts {
                let actual = std::fs::read(workflow.join(name));
                let preserved = if intervention == "shared" && name == "commands.md" {
                    actual.ok() == Some(b"foreign commands".to_vec())
                } else if existing {
                    actual.ok() == Some(format!("prior {name}").into_bytes())
                } else {
                    matches!(actual, Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                };
                if !preserved {
                    return Err(format!(
                        "late refusal changed shared {name}; facade={facade}, existing={existing}"
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn late_build_ignore_refusal_preserves_shared_workflow() -> Result<(), Box<dyn std::error::Error>> {
    exercise_before_boundary("none")
}

#[test]
fn late_refusal_preserves_intervening_shared_commands() -> Result<(), Box<dyn std::error::Error>> {
    exercise_before_boundary("shared")
}

#[test]
fn late_refusal_retains_intervening_preparation_commands() -> Result<(), Box<dyn std::error::Error>>
{
    exercise_before_boundary("preparation")
}

#[test]
fn baseline_writer_refuses_intervening_baseline() -> Result<(), Box<dyn std::error::Error>> {
    exercise_before_boundary("baseline")
}

#[test]
fn partial_failure_retains_intervening_directory_content() -> Result<(), Box<dyn std::error::Error>>
{
    exercise_before_boundary("blocked")
}

#[test]
fn failed_attempt_publication_preserves_intervening_artifact()
-> Result<(), Box<dyn std::error::Error>> {
    exercise_before_boundary("publication")
}
