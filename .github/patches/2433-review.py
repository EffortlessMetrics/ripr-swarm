from pathlib import Path

TARGET = Path("crates/ripr/src/agent/loop_commands.rs")
text = TARGET.read_text(encoding="utf-8")
old = r'''    #[cfg(unix)]
    #[test]
    fn redirect_shaped_gap_id_cannot_open_a_second_redirect() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("system clock before UNIX_EPOCH: {err}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-shell-arg-redirect-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|err| format!("create redirect proof root failed: {err}"))?;
        let value = "gap:pr:amount>=threshold";
        let script = format!("printf '%s' {} > result.txt", shell_arg(value));
        let output = run_bash(&script, Some(&root))?;
        if !output.status.success() {
            return Err(format!(
                "Bash rejected redirect proof: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let actual = std::fs::read(root.join("result.txt"))
            .map_err(|err| format!("read redirect proof result failed: {err}"))?;
        if actual != value.as_bytes() {
            return Err(format!(
                "redirect proof changed the gap id: {:?}",
                String::from_utf8_lossy(&actual)
            ));
        }
        if root.join("threshold").exists() {
            return Err(
                "redirect-shaped gap id created an unintended `threshold` file".to_string(),
            );
        }
        std::fs::remove_dir_all(&root)
            .map_err(|err| format!("remove redirect proof root failed: {err}"))?;
        Ok(())
    }
'''
new = r'''    #[cfg(unix)]
    #[test]
    fn redirect_shaped_gap_id_cannot_open_a_second_redirect() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("system clock before UNIX_EPOCH: {err}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-shell-arg-redirect-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|err| format!("create redirect proof root failed: {err}"))?;

        let proof = (|| {
            let value = "gap:pr:amount>=threshold";
            let script = format!("printf '%s' {} > result.txt", shell_arg(value));
            let output = run_bash(&script, Some(&root))?;
            if !output.status.success() {
                return Err(format!(
                    "Bash rejected redirect proof: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            let actual = std::fs::read(root.join("result.txt"))
                .map_err(|err| format!("read redirect proof result failed: {err}"))?;
            if actual != value.as_bytes() {
                return Err(format!(
                    "redirect proof changed the gap id: {:?}",
                    String::from_utf8_lossy(&actual)
                ));
            }
            let mut entries = std::fs::read_dir(&root)
                .map_err(|err| format!("read redirect proof root failed: {err}"))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .map_err(|err| format!("read redirect proof entry failed: {err}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            if entries != ["result.txt"] {
                return Err(format!(
                    "redirect-shaped gap id created unexpected files: {entries:?}"
                ));
            }
            Ok(())
        })();

        let cleanup = std::fs::remove_dir_all(&root)
            .map_err(|err| format!("remove redirect proof root failed: {err}"));
        match (proof, cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(cleanup_error)) => Err(cleanup_error),
            (Err(error), Err(cleanup_error)) => {
                Err(format!("{error}; cleanup also failed: {cleanup_error}"))
            }
        }
    }
'''
count = text.count(old)
if count != 1:
    raise SystemExit(f"redirect proof: expected exactly one match, found {count}")
TARGET.write_text(text.replace(old, new, 1), encoding="utf-8")
