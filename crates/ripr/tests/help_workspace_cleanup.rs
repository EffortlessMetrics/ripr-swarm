//! Lightweight cleanup controls; separate from the portable help inventory.

#[path = "common/help_workspace.rs"]
mod help_workspace;
use help_workspace::in_help_workspace;

#[test]
fn help_workspace_is_removed_after_a_test_panics() -> Result<(), String> {
    let mut observed_root = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        in_help_workspace(|root| {
            observed_root = Some(root.to_path_buf());
            // Include nested output so removing only the config or an empty
            // directory cannot satisfy the cleanup contract.
            std::fs::create_dir(root.join("partial-output")).map_err(|error| error.to_string())?;
            std::fs::write(root.join("partial-output/report.json"), "unfinished")
                .map_err(|error| error.to_string())?;
            std::panic::resume_unwind(Box::new("injected help assertion failure"))
        })
    }));
    let root = observed_root.ok_or("help callback did not run")?;
    let leaked = root.try_exists().map_err(|error| error.to_string())?;
    // Dispose of the control's residue even when the old helper leaks it.
    if leaked {
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    if !matches!(result, Err(payload) if payload.downcast_ref::<&str>() == Some(&"injected help assertion failure"))
    {
        return Err("helper did not propagate the injected panic".to_string());
    }
    if leaked {
        return Err("help workspace leaked after callback panic".to_string());
    }
    Ok(())
}

#[test]
fn help_workspace_removes_success_and_error_results() -> Result<(), String> {
    for fail in [false, true] {
        let mut observed_root = None;
        let result = in_help_workspace(|root| {
            observed_root = Some(root.to_path_buf());
            if fail {
                Err("injected help error".to_string())
            } else {
                Ok(())
            }
        });
        let expected = if fail {
            Err("injected help error".to_string())
        } else {
            Ok(())
        };
        assert_eq!(result, expected);
        let root = observed_root.ok_or("help callback did not run")?;
        assert!(
            !root.try_exists().map_err(|error| error.to_string())?,
            "help workspace leaked after {expected:?}"
        );
    }
    Ok(())
}

#[test]
fn help_workspace_still_rejects_changed_contents() -> Result<(), String> {
    let mut observed_root = None;
    let result = in_help_workspace(|root| {
        observed_root = Some(root.to_path_buf());
        std::fs::write(root.join("unexpected-report.json"), "{}").map_err(|error| error.to_string())
    });
    let root = observed_root.ok_or("help callback did not run")?;
    assert!(
        matches!(result, Err(error) if error.contains("group help changed workspace contents")),
        "workspace side effects must still fail the help contract"
    );
    assert!(
        !root.try_exists().map_err(|error| error.to_string())?,
        "help workspace leaked after the side-effect check"
    );
    Ok(())
}
