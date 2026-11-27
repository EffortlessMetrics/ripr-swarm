use std::path::Path;

/// Resolve one private owner for both Cargo's final and intermediate output.
/// Callers bind this value to `CARGO_TARGET_DIR` and `CARGO_BUILD_BUILD_DIR`:
/// the former alone does not override an inherited or configured build-dir.
pub(crate) fn private_cargo_output_dir(target: &Path) -> Result<String, String> {
    let target = std::path::absolute(target)
        .map_err(|error| format!("resolve private Cargo output directory: {error}"))?;
    let target = target
        .to_str()
        .ok_or("private Cargo output directory must be UTF-8; choose a UTF-8 output path")?;
    // Cargo expands build-dir templates even in an environment value. The
    // selected owner must remain a literal path, like the judged-panel owner.
    if target.contains(['{', '}']) {
        return Err(
            "private Cargo output directory cannot contain Cargo build-dir template braces; choose an output path without braces"
                .into(),
        );
    }
    Ok(target.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_output_is_absolute_and_preserves_literal_path_characters() -> Result<(), String> {
        let _cwd_guard = crate::acquire_test_cwd_read_guard();
        let path = Path::new("private output 'quoted' & unicode-λ");
        let actual = private_cargo_output_dir(path)?;
        assert_eq!(
            Path::new(&actual),
            std::path::absolute(path).map_err(|e| e.to_string())?
        );
        Ok(())
    }

    #[test]
    fn private_output_refuses_cargo_template_paths() {
        let _cwd_guard = crate::acquire_test_cwd_read_guard();
        for path in ["private/{workspace-path-hash}", "private/}", "private/{"] {
            assert!(
                private_cargo_output_dir(Path::new(path))
                    .is_err_and(|error| error.contains("template braces"))
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn private_output_refuses_non_utf8_paths() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let _cwd_guard = crate::acquire_test_cwd_read_guard();
        assert!(
            private_cargo_output_dir(Path::new(OsStr::from_bytes(b"private-\xff")))
                .is_err_and(|error| error.contains("must be UTF-8"))
        );
    }
}
