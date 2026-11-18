//! Reconstructed controls for #13452, not recovered experiment test bytes.
use super::*;

fn gap_intervening_writer(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    for existing in [false, true] {
        let (_guard, root, decoy) = materialize(
            "blind_journey_installed_typescript",
            "head",
            "ownership-13452",
            Some("[languages]\nenabled = [\"typescript\"]\n"),
        )?;
        let root_arg = root.to_string_lossy().into_owned();
        let (ok, check) = ripr(&decoy, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
        expect_exit(ok, true, "gap source check")?;
        let gap = selected_typescript_gap(&check)?;
        let workflow = root.join("target/ripr/workflow");
        std::fs::create_dir_all(&workflow)?;
        let names = [
            "before.check-findings.json",
            "gap-repair-packet.json",
            "attempt-baseline.json",
        ];
        if existing {
            for name in names {
                std::fs::write(workflow.join(name), format!("prior {name}"))?;
            }
        }
        let shim_dir = root.join(".git/ownership-shim");
        std::fs::create_dir_all(&shim_dir)?;
        let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|path| path.join("git"))
            .find(|path| path.is_file())
            .ok_or("git executable unavailable")?;
        let shim = shim_dir.join("git");
        std::fs::write(
            &shim,
            r#"#!/bin/sh
if ! test -f "$RIPR_WITNESS/injected"; then
  for candidate in "$RIPR_TEST_ROOT"/target/ripr/repair-preparations/* "$RIPR_TEST_ROOT/target/ripr/workflow"; do
    if test -f "$candidate/before.check-findings.json" && test -f "$candidate/gap-repair-packet.json"; then
      case "$(head -c 1 "$candidate/before.check-findings.json")" in
        '{')
          printf '%s' "$candidate" > "$RIPR_WITNESS/selected"
          printf 'foreign snapshot' > "$candidate/before.check-findings.json"
          printf 'foreign packet' > "$candidate/gap-repair-packet.json"
          case "$RIPR_WRITER_MODE" in
            baseline) printf 'foreign baseline' > "$candidate/attempt-baseline.json" ;;
            rollback)
              rm -f "$candidate/attempt-baseline.json"
              mkdir "$candidate/attempt-baseline.json"
              printf 'foreign child' > "$candidate/attempt-baseline.json/foreign.txt" ;;
          esac
          : > "$RIPR_WITNESS/injected"
          break ;;
      esac
    fi
  done
fi
exec "$RIPR_REAL_GIT" "$@"
"#,
        )?;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))?;
        let mut paths = vec![shim_dir.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let output = ripr_output_env(
            &decoy,
            &[
                "agent", "repair", "--root", &root_arg, "--gap-id", &gap, "--base", "HEAD~1",
                "--phase", "before",
            ],
            &[
                ("PATH", std::env::join_paths(paths)?),
                ("RIPR_REAL_GIT", real_git.into_os_string()),
                ("RIPR_TEST_ROOT", root.clone().into_os_string()),
                ("RIPR_WITNESS", shim_dir.clone().into_os_string()),
                ("RIPR_WRITER_MODE", mode.into()),
            ],
        )?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !shim_dir.join("injected").exists()
            || output.status.code() != Some(2)
            || !output.stdout.is_empty()
            || attempt_manifests(&root)? != 0
        {
            return Err(
                format!("gap post-preparation failure not reached ({mode}): {output:?}").into(),
            );
        }
        if mode == "parse" {
            if !stderr.contains("parse") || !stderr.contains("before.check-findings.json") {
                return Err(format!("expected later snapshot parse failure: {stderr}").into());
            }
        } else if !(stderr.contains("attempt-baseline.json")
            || mode == "baseline"
                && stderr.contains("parse")
                && stderr.contains("before.check-findings.json"))
        {
            return Err(format!("expected baseline failure or later parse error: {stderr}").into());
        }
        let selected = PathBuf::from(std::fs::read_to_string(shim_dir.join("selected"))?);
        // These assertions follow the witnessed error and injection assertions.
        for (name, bytes) in [
            ("before.check-findings.json", "foreign snapshot"),
            ("gap-repair-packet.json", "foreign packet"),
        ] {
            if std::fs::read(selected.join(name)).map_err(|error| {
                format!("intervening {name} lost after witnessed {mode} failure: {error}")
            })? != bytes.as_bytes()
            {
                return Err(format!(
                    "gap failure replaced intervening {name}, mode={mode}, existing={existing}"
                )
                .into());
            }
        }
        if mode == "baseline"
            && std::fs::read(selected.join("attempt-baseline.json"))? != b"foreign baseline"
        {
            return Err("baseline producer replaced intervening baseline".into());
        }
        if mode == "rollback"
            && std::fs::read(selected.join("attempt-baseline.json/foreign.txt"))?
                != b"foreign child"
        {
            return Err("partial failure removed intervening directory content".into());
        }
        if selected != workflow {
            for name in names {
                let bytes = std::fs::read(workflow.join(name));
                let unchanged = if existing {
                    bytes.ok() == Some(format!("prior {name}").into_bytes())
                } else {
                    matches!(bytes, Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                };
                if !unchanged {
                    return Err(format!("shared gap {name} changed").into());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn gap_failure_preserves_intervening_snapshot_and_packet() -> Result<(), Box<dyn std::error::Error>>
{
    gap_intervening_writer("rollback")
}

#[test]
fn gap_baseline_producer_preserves_intervening_baseline() -> Result<(), Box<dyn std::error::Error>>
{
    gap_intervening_writer("baseline")
}

#[test]
fn gap_later_parse_failure_preserves_intervening_inputs() -> Result<(), Box<dyn std::error::Error>>
{
    gap_intervening_writer("parse")
}
