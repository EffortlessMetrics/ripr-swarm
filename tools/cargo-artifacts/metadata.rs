use super::*;

pub(super) fn repository_root(start: &Path) -> Result<PathBuf> {
    let start = start.canonicalize().map_err(|error| {
        ArtifactError::new(format!("resolve {}: {error}", start.display()))
    })?;
    for candidate in std::iter::once(start.as_path()).chain(start.ancestors().skip(1)) {
        if candidate.join("Cargo.toml").is_file()
            && candidate.join(".cargo/config.toml").is_file()
        {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(ArtifactError::new(format!(
        "run from the RIPR repository; no Cargo.toml plus .cargo/config.toml found above {}",
        start.display()
    )))
}

pub(super) fn cargo_metadata(repository: &Path) -> Result<CargoPaths> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--offline",
        ])
        .current_dir(repository)
        .output()
        .map_err(|error| ArtifactError::new(format!("execute cargo metadata: {error}")))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ArtifactError::new(format!(
            "cargo metadata failed with {}: {}",
            output.status,
            if detail.is_empty() {
                "no diagnostics"
            } else {
                &detail
            }
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| ArtifactError::new(format!("cargo metadata emitted non-UTF-8 JSON: {error}")))?;
    parse_cargo_paths(&stdout)
}

pub(super) fn parse_cargo_paths(json: &str) -> Result<CargoPaths> {
    Ok(CargoPaths {
        workspace_root: PathBuf::from(json_string_field(json, "workspace_root")?),
        target_directory: PathBuf::from(json_string_field(json, "target_directory")?),
        build_directory: PathBuf::from(json_string_field(json, "build_directory")?),
    })
}

pub(super) fn json_string_field(json: &str, key: &str) -> Result<String> {
    let needle = format!("\"{key}\"");
    let start = json
        .find(&needle)
        .ok_or_else(|| ArtifactError::new(format!("cargo metadata lacks `{key}`")))?;
    let after_key = &json[start + needle.len()..];
    let colon = after_key
        .find(':')
        .ok_or_else(|| ArtifactError::new(format!("cargo metadata field `{key}` lacks a colon")))?;
    let value = after_key[colon + 1..].trim_start();
    parse_json_string(value).map_err(|error| {
        ArtifactError::new(format!("invalid cargo metadata field `{key}`: {error}"))
    })
}

pub(super) fn parse_json_string(value: &str) -> Result<String> {
    let mut chars = value.chars();
    if chars.next() != Some('"') {
        return Err(ArtifactError::new("expected a JSON string"));
    }
    let mut result = String::new();
    while let Some(character) = chars.next() {
        match character {
            '"' => return Ok(result),
            '\\' => {
                let escaped = chars
                    .next()
                    .ok_or_else(|| ArtifactError::new("unterminated JSON escape"))?;
                match escaped {
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    '/' => result.push('/'),
                    'b' => result.push('\u{0008}'),
                    'f' => result.push('\u{000c}'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    'u' => {
                        let code = read_hex_quad(&mut chars)?;
                        let decoded = char::from_u32(code as u32).ok_or_else(|| {
                            ArtifactError::new(format!("unsupported JSON unicode escape {code:04x}"))
                        })?;
                        result.push(decoded);
                    }
                    other => {
                        return Err(ArtifactError::new(format!(
                            "unsupported JSON escape `\\{other}`"
                        )))
                    }
                }
            }
            control if control <= '\u{001f}' => {
                return Err(ArtifactError::new("unescaped control character in JSON string"))
            }
            other => result.push(other),
        }
    }
    Err(ArtifactError::new("unterminated JSON string"))
}

pub(super) fn read_hex_quad(chars: &mut impl Iterator<Item = char>) -> Result<u16> {
    let mut value = 0u16;
    for _ in 0..4 {
        let character = chars
            .next()
            .ok_or_else(|| ArtifactError::new("short JSON unicode escape"))?;
        let digit = character
            .to_digit(16)
            .ok_or_else(|| ArtifactError::new("non-hex JSON unicode escape"))?;
        value = value * 16 + digit as u16;
    }
    Ok(value)
}

pub(super) fn validate_cargo_paths(repository: &Path, paths: &CargoPaths) -> Result<PathBuf> {
    let repository = normalize_absolute(repository)?;
    let workspace_root = normalize_absolute(&paths.workspace_root)?;
    if !same_path(&repository, &workspace_root) {
        return Err(ArtifactError::new(format!(
            "cargo metadata resolved workspace {} instead of current repository {}",
            workspace_root.display(),
            repository.display()
        )));
    }

    let expected_target = normalize_absolute(&repository.join("target"))?;
    let target_directory = normalize_absolute(&paths.target_directory)?;
    if !same_path(&expected_target, &target_directory) {
        return Err(ArtifactError::new(format!(
            "clean-current owns only {}; cargo resolved target_directory {}",
            expected_target.display(),
            target_directory.display()
        )));
    }

    let build_directory = normalize_absolute(&paths.build_directory)?;
    if !same_path(&target_directory, &build_directory) {
        return Err(ArtifactError::new(format!(
            "clean-current requires Cargo's exact build_directory to equal target_directory; got {} and {}",
            build_directory.display(),
            target_directory.display()
        )));
    }
    Ok(target_directory)
}

pub(super) fn normalize_absolute(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(ArtifactError::new(format!(
            "expected an absolute path, got {}",
            path.display()
        )));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(ArtifactError::new(format!(
                        "path escapes its root: {}",
                        path.display()
                    )));
                }
            }
        }
    }
    Ok(normalized)
}

#[cfg(windows)]
pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .replace('/', "\\")
        .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
}

#[cfg(not(windows))]
pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    left == right
}
