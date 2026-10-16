//! Deterministic executable-policy index, shared by both frontdoor fixtures.
use std::path::Path;

pub fn initialize(
    root: &Path,
    mut git: impl FnMut(&[&str]) -> Result<(), String>,
) -> Result<(), String> {
    git(&["init", "--quiet"])?;
    for name in ["allowed", "demoted", "ordinary", "unregistered"] {
        std::fs::write(root.join(name), format!("{name}\n")).map_err(|e| e.to_string())?;
    }
    // Do not stage copied policy inputs. Explicit paths keep the index and
    // filesystem-mode independence identical to the per-file setup.
    git(&[
        "-c",
        "core.filemode=false",
        "add",
        "--",
        "allowed",
        "demoted",
        "ordinary",
        "unregistered",
    ])?;
    git(&[
        "update-index",
        "--chmod=-x",
        "--",
        "allowed",
        "demoted",
        "ordinary",
        "unregistered",
    ])?;
    git(&[
        "update-index",
        "--chmod=+x",
        "--",
        "allowed",
        "demoted",
        "unregistered",
    ])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_stops_at_the_first_git_failure() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "executable-index-failures-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        for failure in 1..=4 {
            let mut calls = 0;
            let result = initialize(&root, |_| {
                calls += 1;
                if calls == failure {
                    Err(format!("git failure control {failure}"))
                } else {
                    Ok(())
                }
            });
            assert_eq!(result, Err(format!("git failure control {failure}")));
            assert_eq!(calls, failure, "no commands after the failed setup phase");
        }
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }
}
