use sha2::{Digest, Sha256};
use std::path::Path;

pub fn source_identity(root: &Path) -> Result<String, String> {
    identity_for_paths(root, source_inputs(root)?)
}

/// Production identity is the complete production package, without exclusions.
pub fn implementation_identity(root: &Path) -> Result<String, String> {
    identity_for_paths(root, implementation_inputs(root)?)
}

fn implementation_inputs(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut paths = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("rust-toolchain.toml"),
    ];
    collect(&root.join("tools/repo-policy"), &mut paths)?;
    paths.sort();
    Ok(paths)
}

pub fn source_inputs(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut paths = implementation_inputs(root)?;
    // Minimal standalone custody fixtures need only the production package.
    for resource in [
        "tools/repo-policy-contracts",
        "tools/repo-policy-test-identity",
    ] {
        let path = root.join(resource);
        if path.exists() {
            collect(&path, &mut paths)?;
        }
    }
    paths.sort();
    Ok(paths)
}

fn identity_for_paths(root: &Path, paths: Vec<std::path::PathBuf>) -> Result<String, String> {
    let mut hash = Sha256::new();
    for path in paths {
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let name = relative.to_string_lossy().replace('\\', "/");
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("policy source identity: {}: {e}", path.display()))?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn collect(path: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    if path.is_file() {
        files.push(path.to_path_buf());
    } else {
        for entry in std::fs::read_dir(path)
            .map_err(|e| format!("policy source identity: {}: {e}", path.display()))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            collect(&entry.path(), files)?;
        }
    }
    Ok(())
}
