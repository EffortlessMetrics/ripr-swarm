//! Owned scratch for rendered-help contracts and their cleanup controls.

struct HelpWorkspaceCleanup(Option<std::path::PathBuf>);

impl HelpWorkspaceCleanup {
    fn finish(mut self) -> Result<(), String> {
        let Some(root) = self.0.take() else {
            return Ok(());
        };
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }
}

impl Drop for HelpWorkspaceCleanup {
    fn drop(&mut self) {
        if let Some(root) = &self.0 {
            // Preserve the original setup error or panic during unwinding.
            // Normal returns use finish() so cleanup errors still fail the test.
            if let Ok(()) = std::fs::remove_dir_all(root) {}
        }
    }
}

pub(super) fn in_help_workspace(
    test: impl FnOnce(&std::path::Path) -> Result<(), String>,
) -> Result<(), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let root = parent.join(format!("ripr-group-help-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    // Arm only after exclusive creation and before any fallible population.
    let cleanup = HelpWorkspaceCleanup(Some(root.clone()));
    // Help must neither load configuration nor write reports, even in a
    // workspace where an executing command would reject the configuration.
    let config = "deliberately invalid TOML [";
    std::fs::write(root.join("ripr.toml"), config).map_err(|error| error.to_string())?;
    let result = test(&root).and_then(|()| {
        let entries = std::fs::read_dir(&root).map_err(|error| error.to_string())?;
        let entries = entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let actual =
            std::fs::read_to_string(root.join("ripr.toml")).map_err(|error| error.to_string())?;
        if entries.len() != 1 || actual != config {
            return Err(format!("group help changed workspace contents: {root:?}"));
        }
        Ok(())
    });
    let cleanup = cleanup.finish();
    result.and(cleanup)
}
