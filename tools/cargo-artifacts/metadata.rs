use super::*;

pub(super) fn repository_root(start: &Path) -> Result<PathBuf> {
    let start = start.canonicalize().map_err(|error| {
        ArtifactError::new(format!("resolve {}: {error}", start.display()))
    })?;
    for candidate in std::iter::once(start.as_path()).chain(start.ancestors().skip(1)) {
        if candidate.join("Cargo.toml").is_file()
            && candidate.join(".cargo/config.toml").is_file()
            && is_git_checkout(candidate)?
        {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(ArtifactError::new(format!(
        "run from the RIPR Git checkout; no checkout root with Cargo.toml and .cargo/config.toml found above {}",
        start.display()
    )))
}

pub(super) fn is_git_checkout(candidate: &Path) -> Result<bool> {
    let marker = candidate.join(".git");
    match fs::symlink_metadata(&marker) {
        Ok(metadata) => {
            require_plain_path(&marker, &metadata)?;
            Ok(metadata.is_file() || metadata.is_dir())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ArtifactError::new(format!(
            "inspect Git checkout marker {}: {error}",
            marker.display()
        ))),
    }
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
    let bytes = json.as_bytes();
    let first = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .ok_or_else(|| ArtifactError::new("cargo metadata JSON is empty"))?;
    if bytes[first] != b'{' {
        return Err(ArtifactError::new("cargo metadata root is not a JSON object"));
    }

    let mut depth = 0usize;
    let mut index = first;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let end = json_string_end(json, index)?;
                if depth == 1 {
                    let candidate = parse_json_string(&json[index..end])?;
                    let mut after = end;
                    while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                        after += 1;
                    }
                    if after < bytes.len() && bytes[after] == b':' && candidate == key {
                        after += 1;
                        while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                            after += 1;
                        }
                        return parse_json_string(&json[after..]).map_err(|error| {
                            ArtifactError::new(format!(
                                "invalid top-level cargo metadata field `{key}`: {error}"
                            ))
                        });
                    }
                }
                index = end;
            }
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                index += 1;
            }
            b'}' | b']' => {
                if depth == 0 {
                    return Err(ArtifactError::new("unbalanced cargo metadata JSON"));
                }
                depth -= 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    Err(ArtifactError::new(format!(
        "cargo metadata lacks top-level `{key}`"
    )))
}

pub(super) fn json_string_end(json: &str, start: usize) -> Result<usize> {
    let bytes = json.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return Err(ArtifactError::new("expected a JSON string"));
    }
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Ok(index + 1),
            b'\\' => {
                index += 2;
                if index > bytes.len() {
                    return Err(ArtifactError::new("unterminated JSON escape"));
                }
            }
            _ => index += 1,
        }
    }
    Err(ArtifactError::new("unterminated JSON string"))
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
    windows_path_key(left) == windows_path_key(right)
}

#[cfg(windows)]
fn windows_path_key(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");
    if let Some(rest) = value.strip_prefix("\\\\?\\UNC\\") {
        value = format!("\\\\{rest}");
    } else if let Some(rest) = value.strip_prefix("\\\\?\\") {
        value = rest.to_string();
    }
    while value.len() > 3 && value.ends_with('\\') {
        value.pop();
    }
    value.to_ascii_lowercase()
}

#[cfg(not(windows))]
pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    left == right
}
