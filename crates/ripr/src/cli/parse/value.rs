use std::path::{Path, PathBuf};

pub(crate) fn expect_value<'a>(
    args: &'a [String],
    idx: usize,
    flag: &str,
) -> Result<&'a str, String> {
    args.get(idx)
        .map(|s| s.as_str())
        .ok_or_else(|| format!("missing value for {flag}; pass a value right after it"))
}

/// Reconcile a `--root VALUE` occurrence against the root a command parser
/// has already accepted (#9008, #10542).
///
/// #9008: a repeated `--root` with a *different* value is a usage error, not
/// a silent last-win that analyzes a root the user never meant. The error
/// names the command and both values, matching the tone of the agent
/// family's `refuse_repeated_flag` (`{command} takes {flag} once`).
///
/// #10542: `--root` deliberately stays friendly to scripted double-root
/// invocations, so an *identical* repeat is silently absorbed instead of
/// refused. The comparison is on the raw value against the already-set root;
/// parsers track whether their current root came from an explicit `--root`
/// (`current_explicit`) so the implicit default (usually `.`) is simply
/// upgraded by the first explicit occurrence, and a conflict between two
/// explicit values is what refuses.
///
/// Callers set `current` to the already-accepted root (the implicit default
/// when nothing was accepted yet) and pass `current_explicit = true` only
/// after an explicit `--root` was seen; on `Ok` they store the returned root
/// and mark it explicit.
pub(crate) fn reconcile_root(
    current: &Path,
    current_explicit: bool,
    value: &str,
    command: &str,
) -> Result<PathBuf, String> {
    if current_explicit && current != Path::new(value) {
        return Err(format!(
            "{command} takes --root once (got '{}' and '{value}')",
            current.display()
        ));
    }
    Ok(PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_root_sets_the_value_from_the_implicit_default() -> Result<(), String> {
        let reconciled = reconcile_root(Path::new("."), false, "workspace", "check")?;
        if reconciled.as_path() != Path::new("workspace") {
            return Err(format!("first --root was not stored: {reconciled:?}"));
        }
        Ok(())
    }

    #[test]
    fn identical_repeat_is_absorbed() -> Result<(), String> {
        let reconciled = reconcile_root(Path::new("workspace"), true, "workspace", "check")?;
        if reconciled.as_path() != Path::new("workspace") {
            return Err(format!("identical repeat changed the root: {reconciled:?}"));
        }
        Ok(())
    }

    #[test]
    fn conflicting_repeat_refuses_and_names_both_values() -> Result<(), String> {
        match reconcile_root(Path::new("a"), true, "b", "check") {
            Err(error) if error == "check takes --root once (got 'a' and 'b')" => Ok(()),
            other => Err(format!(
                "conflicting repeat was not refused as expected: {other:?}"
            )),
        }
    }

    #[test]
    fn upgrade_from_implicit_default_then_conflict_refuses() -> Result<(), String> {
        // `--root x` upgrades the implicit default `.`...
        let first = reconcile_root(Path::new("."), false, "x", "status")?;
        if first.as_path() != Path::new("x") {
            return Err(format!("first --root was not stored: {first:?}"));
        }
        // ...so a later `--root y` conflicts with x, not with the default.
        match reconcile_root(Path::new("x"), true, "y", "status") {
            Err(error) if error == "status takes --root once (got 'x' and 'y')" => Ok(()),
            other => Err(format!(
                "second explicit --root with a different value was not refused: {other:?}"
            )),
        }
    }

    #[test]
    fn implicit_default_never_conflicts_with_itself() -> Result<(), String> {
        // No explicit --root yet: any first value is accepted, including the
        // default spelled out (`--root .` on a `.`-defaulted parser).
        let reconciled = reconcile_root(Path::new("."), false, ".", "doctor")?;
        if reconciled.as_path() != Path::new(".") {
            return Err(format!(
                "--root . on the implicit default changed the root: {reconciled:?}"
            ));
        }
        Ok(())
    }
}
