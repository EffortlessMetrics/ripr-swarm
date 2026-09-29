use super::{RULE_VERSION, WORKSPACE_MANIFEST_PATH, rule};

pub(super) fn parse_workspace_version(text: &str, violations: &mut Vec<String>) -> Option<String> {
    let value: toml::Value = match toml::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            violations.push(rule(
                RULE_VERSION,
                WORKSPACE_MANIFEST_PATH,
                &format!("cannot parse workspace manifest: {error}"),
            ));
            return None;
        }
    };
    let version = value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str);
    match version {
        Some(version) => Some(version.to_string()),
        None => {
            violations.push(rule(
                RULE_VERSION,
                WORKSPACE_MANIFEST_PATH,
                "missing string `[workspace.package] version` authority",
            ));
            None
        }
    }
}

pub(super) fn map_pep440_version(version: &str) -> Result<String, String> {
    if version.contains('+') {
        return Err(format!(
            "workspace version `{version}` contains build metadata, which has no approved registry mapping"
        ));
    }
    let (core, prerelease) = match version.split_once('-') {
        Some((core, prerelease)) => (core, Some(prerelease)),
        None => (version, None),
    };
    validate_numeric_components(core, 3, "release")?;
    let Some(prerelease) = prerelease else {
        return Ok(core.to_string());
    };
    let mut parts = prerelease.split('.');
    let Some(kind) = parts.next() else {
        return Err(format!(
            "workspace version `{version}` has an empty prerelease"
        ));
    };
    let Some(number) = parts.next() else {
        return Err(format!(
            "workspace prerelease `{prerelease}` must be `<rc|alpha|beta|dev>.<number>`"
        ));
    };
    if parts.next().is_some() {
        return Err(format!(
            "workspace prerelease `{prerelease}` has unsupported extra components"
        ));
    }
    validate_numeric_components(number, 1, "prerelease")?;
    let mapped = match kind {
        "rc" => format!("{core}rc{number}"),
        "alpha" => format!("{core}a{number}"),
        "beta" => format!("{core}b{number}"),
        "dev" => format!("{core}.dev{number}"),
        _ => {
            return Err(format!(
                "workspace prerelease kind `{kind}` has no approved PEP 440 mapping"
            ));
        }
    };
    Ok(mapped)
}

fn validate_numeric_components(value: &str, count: usize, label: &str) -> Result<(), String> {
    let components = value.split('.').collect::<Vec<_>>();
    if components.len() != count {
        return Err(format!(
            "{label} version `{value}` must contain exactly {count} numeric component(s)"
        ));
    }
    for component in components {
        if component.is_empty()
            || !component
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            return Err(format!(
                "{label} version component `{component}` is not an unsigned decimal integer"
            ));
        }
        if component.len() > 1 && component.starts_with('0') {
            return Err(format!(
                "{label} version component `{component}` has a leading zero"
            ));
        }
    }
    Ok(())
}
