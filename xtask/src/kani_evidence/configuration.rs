use super::*;
use formal::native::InstalledToolchain;

/// Only default checking with repository-declared ordinary harnesses is admitted.
/// A digest records identity; these independent predicates decide admissibility.
pub(super) fn configuration(
    root: &Path,
    sources: &BTreeMap<String, String>,
    packages: &[String],
) -> (String, Vec<String>, Vec<u8>) {
    let mut identity = BTreeMap::new();
    let mut reasons = Vec::new();
    let mut cargo_config_keys = BTreeMap::new();
    for (name, value) in std::env::vars_os() {
        let name = name.to_string_lossy();
        let unadmitted = (name.starts_with("KANI_") && name != "KANI_HOME")
            || matches!(
                name.as_ref(),
                "RUSTFLAGS"
                    | "CARGO_ALIAS_KANI"
                    | "CARGO_ENCODED_RUSTFLAGS"
                    | "RUSTC"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
                    | "RUSTDOCFLAGS"
                    | "RUSTC_BOOTSTRAP"
                    | "CARGO_ENCODED_RUSTDOCFLAGS"
            )
            || (name.starts_with("CARGO_")
                && (name.contains("RUSTFLAGS")
                    || name.contains("RUSTC")
                    || name == "CARGO_BUILD_TARGET"
                    || name.starts_with("CARGO_PROFILE_")
                    || name.starts_with("CARGO_TARGET_")
                    || name.starts_with("CARGO_UNSTABLE_")));
        if unadmitted && !value.is_empty() {
            reasons.push(format!("unadmitted verification environment: {name}"));
            identity.insert(
                format!("environment:{name}"),
                hash(value.as_encoded_bytes()),
            );
        }
    }
    for locator in [
        "KANI_HOME",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "PATH",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    ] {
        if let Some(value) = std::env::var_os(locator) {
            identity.insert(format!("locator:{locator}"), hash(value.as_encoded_bytes()));
        }
    }
    let mut configs = Vec::new();
    for ancestor in root.ancestors() {
        for name in [".cargo/config", ".cargo/config.toml"] {
            let path = ancestor.join(name);
            if path.exists() {
                configs.push(path);
            }
        }
    }
    if let Some(home) = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| home::cargo_home().ok())
    {
        for name in ["config", "config.toml"] {
            let path = home.join(name);
            if path.exists() {
                configs.push(path);
            }
        }
    }
    configs.sort();
    configs.dedup();
    for path in configs {
        match read_bounded(&path, SMALL_STREAM as u64).and_then(|bytes| {
            identity.insert(format!("cargo-config:{}", path.display()), hash(&bytes));
            let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
            // TOML's formatted parse error can include the original source
            // line. A user Cargo config can contain credentials; retain its
            // digest and safe parse category, never the source excerpt.
            let table: toml::Value = toml::from_str(text).map_err(|_source_error| {
                "Cargo configuration is not valid TOML; source contents are withheld".to_string()
            })?;
            let keys = configuration_keys(&table);
            cargo_config_keys.insert(format!("cargo-config:{}", path.display()), keys.clone());
            formal::native::validate_cargo_configuration_keys(&keys)
        }) {
            Ok(()) => {}
            Err(reason) => {
                reasons.push(format!("Cargo configuration {}: {reason}", path.display()))
            }
        }
    }
    for name in sources.keys() {
        if matches!(
            name.as_str(),
            "Cargo.toml"
                | "crates/ripr/Cargo.toml"
                | "crates/ripr-contract/Cargo.toml"
                | "xtask/Cargo.toml"
                | "tools/repo-policy/Cargo.toml"
        ) {
            match read_bounded(&root.join(name), FILE_BYTES).and_then(|bytes| {
                let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                let table: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
                reject_kani_metadata(&table)?;
                reject_external_paths(
                    &table,
                    root.join(name).parent().ok_or("manifest parent absent")?,
                    root,
                )
            }) {
                Ok(()) => {}
                Err(reason) => reasons.push(format!("{name}: {reason}")),
            }
        } else if name.ends_with(".rs")
            && (packages
                .iter()
                .any(|package| name.starts_with(&format!("crates/{package}/src/")))
                || (packages.iter().any(|p| p == "ripr")
                    && name.starts_with("crates/ripr-contract/src/")))
        {
            match read_bounded(&root.join(name), FILE_BYTES).and_then(|bytes| {
                let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                reject_replacements(text)
            }) {
                Ok(()) => {}
                Err(reason) => reasons.push(format!("{name}: {reason}")),
            }
        }
    }
    identity.extend(sources.iter().map(|(k, v)| (k.clone(), v.clone())));
    reasons.sort();
    reasons.dedup();
    let observed = formal::native::ConfigurationSnapshot {
        inputs: identity,
        cargo_config_keys,
    };
    let bytes = serde_json::to_vec_pretty(&observed).unwrap_or_else(|_| b"{}".to_vec());
    (hash(&bytes), reasons, bytes)
}

pub(super) fn reject_kani_metadata(value: &toml::Value) -> Result<(), String> {
    for keys in [
        &["workspace", "metadata", "kani"][..],
        &["package", "metadata", "kani"][..],
        &["kani"][..],
    ] {
        let found = keys.iter().try_fold(value, |value, key| value.get(key));
        if found.is_some() {
            return Err("custom Kani flags/unstable configuration is outside the admitted default-check profile".to_string());
        }
    }
    Ok(())
}
fn configuration_keys(value: &toml::Value) -> Vec<Vec<String>> {
    fn visit(value: &toml::Value, path: &mut Vec<String>, keys: &mut Vec<Vec<String>>) {
        if let Some(table) = value.as_table() {
            for (key, value) in table {
                path.push(key.clone());
                keys.push(path.clone());
                visit(value, path, keys);
                path.pop();
            }
        }
    }
    let mut keys = Vec::new();
    visit(value, &mut Vec::new(), &mut keys);
    keys
}
#[cfg(test)]
pub(super) fn reject_cargo_configuration(value: &toml::Value) -> Result<(), String> {
    formal::native::validate_cargo_configuration_keys(&configuration_keys(value))
}
pub(super) fn reject_replacements(text: &str) -> Result<(), String> {
    use ra_ap_syntax::{
        AstNode, Edition, SourceFile,
        ast::{self, HasAttrs},
    };
    if !(text.contains("kani") || text.contains("macro_rules") && text.contains("cfg")) {
        return Ok(());
    }
    let parsed = SourceFile::parse(text, Edition::Edition2024);
    if !parsed.errors().is_empty() {
        return Err("Rust verification configuration could not be parsed".to_string());
    }
    for definition in parsed
        .syntax_node()
        .descendants()
        .filter_map(ast::MacroRules::cast)
    {
        let compact: String = definition
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let conditional = compact
            .split("#[cfg(")
            .skip(1)
            .chain(compact.split("cfg!(").skip(1))
            .any(|tail| {
                tail.split(')')
                    .next()
                    .is_some_and(|condition| condition.contains("kani") || condition.contains('$'))
            });
        if conditional
            || compact.contains("#[cfg_attr(")
                && (compact.contains("kani") || compact.contains("#[cfg_attr($"))
            || compact.contains("#[kani::stub")
            || compact.contains("#[kani::proof_for_contract")
        {
            // This fixed reviewed generator only emits exhaustive-domain
            // constants/checks plus cover-only helper functions. A changed
            // generator or another verification-dependent macro needs review.
            if hash(compact.as_bytes())
                != "sha256:1ab37219d89165766a7d8fe682ac6b7c91578fe8142e73017649f92c660e6722"
            {
                return Err("verification-dependent macro expansion is outside the reviewed cover-generator profile".to_string());
            }
        }
    }
    for attr in parsed
        .syntax_node()
        .descendants()
        .filter_map(ast::Attr::cast)
    {
        let tokens: String = attr
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if tokens.starts_with("#[cfg(") && tokens.contains("kani") {
            let module = attr.syntax().parent().and_then(ast::Module::cast);
            if !matches!(
                tokens.as_str(),
                "#[cfg(kani)]" | "#[cfg(any(test,kani))]" | "#[cfg(any(kani,test))]"
            ) || (module.is_none()
                && !matches!(
                    tokens.as_str(),
                    "#[cfg(any(test,kani))]" | "#[cfg(any(kani,test))]"
                ))
            {
                return Err(
                    "verification-only non-module replacement requires a registered trust contract"
                        .to_string(),
                );
            }
        }
        if tokens.starts_with("#[kani::stub")
            || tokens.starts_with("#[kani::proof_for_contract")
            || (tokens.starts_with("#[cfg_attr(") && tokens.contains("kani"))
        {
            return Err(
                "unregistered Kani stub, contract replacement or conditional attribute".to_string(),
            );
        }
    }
    for call in parsed
        .syntax_node()
        .descendants()
        .filter_map(ast::MacroCall::cast)
    {
        if call.path().is_some_and(|p| {
            p.syntax()
                .text()
                .to_string()
                .rsplit("::")
                .next()
                .is_some_and(|name| name.trim() == "cfg")
        }) && call
            .token_tree()
            .is_some_and(|t| t.syntax().text().to_string().contains("kani"))
        {
            return Err(
                "verification-dependent expression requires registered correspondence".to_string(),
            );
        }
    }
    for function in parsed.syntax_node().descendants().filter_map(ast::Fn::cast) {
        if function.attrs().any(|a| {
            let tokens: String = a
                .syntax()
                .text()
                .to_string()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            tokens == "#[cfg(kani)]"
        }) {
            return Err(
                "verification-only replacement function requires a registered trust contract"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn reject_external_paths(
    value: &toml::Value,
    manifest_directory: &Path,
    root: &Path,
) -> Result<(), String> {
    match value {
        toml::Value::Table(table) => {
            if let Some(path) = table.get("path").and_then(toml::Value::as_str) {
                let path = manifest_directory.join(path);
                let resolved = fs::canonicalize(&path)
                    .map_err(|error| format!("manifest path input is not available: {error}"))?;
                if !resolved.starts_with(root) {
                    return Err(
                        "manifest path dependency or target escapes captured repository"
                            .to_string(),
                    );
                }
            }
            for item in table.values() {
                reject_external_paths(item, manifest_directory, root)?;
            }
        }
        toml::Value::Array(items) => {
            for item in items {
                reject_external_paths(item, manifest_directory, root)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Kani's bundled verifier compiler is distinct from the product toolchain.
/// These local installation commitments are evidence inputs, not signatures.
pub(super) fn installed_toolchain() -> Result<InstalledToolchain, String> {
    let home = std::env::var_os("KANI_HOME")
        .map(PathBuf::from)
        .or_else(|| home::home_dir().map(|p| p.join(".kani")))
        .ok_or("Kani installation directory is unavailable")?;
    let installation = fs::canonicalize(home.join("kani-0.68.0"))
        .map_err(|error| format!("Kani0.68.0 installation is unavailable: {error}"))?;
    let read_text = |name| -> Result<String, String> {
        let text = String::from_utf8(read_bounded(&installation.join(name), SMALL_STREAM as u64)?)
            .map_err(|error| format!("Kani installation metadata is not UTF-8: {error}"))?;
        let text = text.trim();
        if text.is_empty() || text.contains(['\n', '\r']) {
            return Err("ambiguous Kani compiler metadata".to_string());
        }
        Ok(text.to_string())
    };
    let rustc = read_text("rustc-version")?;
    let rustup_toolchain = read_text("rust-toolchain-version")?;
    let target = "x86_64-unknown-linux-gnu";
    if !rustup_toolchain.ends_with(target)
        || !cfg!(all(target_os = "linux", target_arch = "x86_64"))
    {
        return Err("this capture profile requires the Kani x86_64 Linux bundle".to_string());
    }
    let mut binaries = BTreeMap::new();
    for name in ["kani-compiler", "kani-driver", "cbmc"] {
        let bytes = read_bounded(&installation.join("bin").join(name), 64 * 1024 * 1024)?;
        binaries.insert(name.to_string(), hash(&bytes));
    }
    let observed = InstalledToolchain {
        installation: installation.display().to_string(),
        rustc,
        rustup_toolchain,
        target: target.to_string(),
        binaries,
    };
    formal::native::validate_installed_toolchain(&observed)?;
    Ok(observed)
}
