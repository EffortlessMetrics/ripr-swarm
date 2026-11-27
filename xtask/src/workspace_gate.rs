//! Root-only maintenance gates read repository-relative paths and may write them.
use std::path::PathBuf;

#[derive(serde::Deserialize)]
struct Workspace {
    workspace_root: PathBuf,
    packages: Vec<Package>,
}

#[derive(serde::Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
}

pub(super) fn from_root(gate: fn() -> Result<(), String>) -> Result<(), String> {
    // --no-deps returns only workspace members. Ask Cargo about the caller's
    // workspace even when this executable was built in another checkout.
    let args = [
        "metadata",
        "--no-deps",
        "--format-version=1",
        "--offline",
        "--locked",
    ]
    .map(str::to_owned);
    let output = crate::run::capture_process_output("cargo", &args, &[])
        .map_err(|error| format!("cannot resolve the RIPR workspace root: {}", error.message))?;
    let workspace: Workspace = serde_json::from_slice(&output)
        .map_err(|error| format!("cannot read Cargo workspace metadata: {error}"))?;
    let root = workspace.workspace_root;
    for (name, manifest) in [
        ("ripr", "crates/ripr/Cargo.toml"),
        ("xtask", "xtask/Cargo.toml"),
    ] {
        let path = root.join(manifest);
        if !workspace
            .packages
            .iter()
            .any(|p| p.name == name && p.manifest_path == path)
        {
            return Err(format!(
                "{} is not the RIPR workspace: missing workspace member {name} at {manifest}",
                root.display()
            ));
        }
    }

    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve workspace root {}: {error}", root.display()))?;
    let cwd = std::fs::canonicalize(".")
        .map_err(|error| format!("cannot resolve current directory: {error}"))?;
    if cwd != root {
        return Err(format!(
            "run from the RIPR workspace root {} (current directory: {})",
            root.display(),
            cwd.display()
        ));
    }
    gate()
}
