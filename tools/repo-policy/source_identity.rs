use sha2::{Digest, Sha256};
use std::path::Path;

pub fn source_identity(root: &Path) -> Result<String, String> {
    let mut paths = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("rust-toolchain.toml"),
    ];
    collect(&root.join("tools/repo-policy"), &mut paths)?;
    paths.sort();
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
