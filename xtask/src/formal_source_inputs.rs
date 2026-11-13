//! Shared passive source prerequisites for pre-registration and native capture.
//!
//! Extends a caller-supplied ordinary source inventory; does not independently
//! establish complete compiler inputs, atomic source custody, or assurance.
use crate::formal_evidence::native::{read_file_bounded as read_bounded, relative_path};
use std::{collections::BTreeSet, fs, path::Path};
const FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FILES: usize = 20_000;

/// Remove only syntax trivia, never bytes inside a literal token. Raw
/// identifiers have the same spelling as their ordinary identifier form.
fn token_text(node: &ra_ap_syntax::SyntaxNode) -> String {
    use ra_ap_syntax::SyntaxKind;
    let mut text = String::new();
    for token in node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
    {
        let spelling = token.text();
        text.push_str(if token.kind() == SyntaxKind::IDENT {
            spelling.strip_prefix("r#").unwrap_or(spelling)
        } else {
            spelling
        });
    }
    text
}

fn contains_input_macro(node: &ra_ap_syntax::SyntaxNode) -> bool {
    let tokens: Vec<_> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens.windows(2).any(|pair| {
        matches!(
            pair[0].text().strip_prefix("r#").unwrap_or(pair[0].text()),
            "include" | "include_str" | "include_bytes"
        ) && pair[1].text() == "!"
    })
}

/// Admit literal Rust include/path inputs, including ignored/untracked files.
/// Dynamic generated includes and out-of-repository inputs remain unsupported;
/// a hash over tracked files alone cannot establish this closure.
///
/// The caller supplies the ordinary module/dependency roots and admits the
/// compiler configuration. On error, discard the possibly extended `paths`;
/// partially collected inputs must never be used as a complete manifest.
pub fn extend_source_inputs(
    root: &Path,
    paths: &mut BTreeSet<String>,
    packages: &[String],
) -> Result<BTreeSet<String>, String> {
    use ra_ap_syntax::{AstNode, Edition, SourceFile, ast};
    if paths.len() > MAX_FILES {
        return Err("source closure exceeds 20,000 files".to_string());
    }
    let canonical_root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let root = canonical_root.as_path();
    for path in paths.iter() {
        relative_path(path)?;
    }
    let mut inspected = BTreeSet::new();
    let mut explicit_inputs = BTreeSet::new();
    let mut build_roots = BTreeSet::from(["xtask/build.rs".to_string()]);
    for package in packages {
        if !matches!(package.as_str(), "ripr" | "ripr-contract") {
            return Err("unsupported source-input package".to_string());
        }
        build_roots.insert(format!("crates/{package}/build.rs"));
        if package == "ripr" {
            build_roots.insert("crates/ripr-contract/build.rs".to_string());
        }
    }
    for build in &build_roots {
        match fs::symlink_metadata(root.join(build)) {
            Ok(_) => {
                paths.insert(build.clone());
                explicit_inputs.insert(build.clone());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    if paths.len() > MAX_FILES {
        return Err("source closure exceeds 20,000 files".to_string());
    }
    let mut compiled_inputs: BTreeSet<String> = paths
        .iter()
        .filter(|path| {
            path.ends_with(".rs")
                && (packages
                    .iter()
                    .any(|package| path.starts_with(&format!("crates/{package}/src/")))
                    || (packages.iter().any(|p| p == "ripr")
                        && path.starts_with("crates/ripr-contract/src/"))
                    || path.starts_with("xtask/src/")
                    || build_roots.contains(path.as_str()))
        })
        .cloned()
        .collect();
    loop {
        let excluded = test_only_source_files(root, &compiled_inputs)?;
        let pending: Vec<_> = compiled_inputs
            .iter()
            .filter(|path| !inspected.contains(*path) && !excluded.contains(*path))
            .cloned()
            .collect();
        if pending.is_empty() {
            break;
        }
        for name in pending {
            inspected.insert(name.clone());
            let bytes = read_bounded(&source_path(root, &name)?, FILE_BYTES)?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|error| format!("source file is not UTF-8: {error}"))?;
            if !text.contains("include") && !text.contains("path") {
                continue;
            }
            let parsed = SourceFile::parse(text, Edition::Edition2024);
            if !parsed.errors().is_empty() {
                return Err(format!("cannot determine Rust input closure: {name}"));
            }
            let mut inputs = Vec::new();
            // Macro definitions are not expanded by this profile. A literal
            // in such a body is relative to its eventual expansion site and
            // cannot be silently omitted or guessed from the definition file.
            for definition in parsed
                .syntax_node()
                .descendants()
                .filter_map(ast::MacroRules::cast)
            {
                if is_test_only(definition.syntax()) {
                    continue;
                }
                if contains_input_macro(definition.syntax()) {
                    return Err(format!(
                        "macro-generated includes require an explicit supported closure rule: {name}"
                    ));
                }
            }
            for call in parsed
                .syntax_node()
                .descendants()
                .filter_map(ast::MacroCall::cast)
            {
                if is_test_only(call.syntax()) {
                    continue;
                }
                let Some(path) = call.path().map(|path| token_text(path.syntax())) else {
                    continue;
                };
                let name_tail = path.rsplit("::").next().unwrap_or(&path).trim();
                if matches!(name_tail, "include" | "include_str" | "include_bytes") {
                    let tree = call.token_tree().ok_or("include has no argument")?;
                    let tokens = token_text(tree.syntax());
                    let argument = tokens
                        .get(1..tokens.len().saturating_sub(1))
                        .ok_or("malformed include")?
                        .trim()
                        .trim_end_matches(',')
                        .trim();
                    let input = serde_json::from_str::<String>(argument).map_err(|error| {
                        format!(
                            "dynamic/nonliteral include is outside captured input profile: {name}: {error}"
                        )
                    })?;
                    inputs.push((input, name_tail == "include"));
                } else if call
                    .token_tree()
                    .is_some_and(|tree| contains_input_macro(tree.syntax()))
                {
                    return Err(format!(
                        "nested macro includes are outside captured input profile: {name}"
                    ));
                }
            }
            for attr in parsed
                .syntax_node()
                .descendants()
                .filter_map(ast::Attr::cast)
            {
                if is_test_only(attr.syntax()) {
                    continue;
                }
                let tokens = token_text(attr.syntax());
                let compact = tokens.as_str();
                if compact.starts_with("#[path=") {
                    // Inline module #[path] bases depend on module context,
                    // including enclosing path attributes. This first profile
                    // refuses them rather than interpreting a physical-file
                    // relative path as the Rust module resolution rule.
                    if attr
                        .syntax()
                        .ancestors()
                        .filter_map(ast::Module::cast)
                        .any(|module| module.item_list().is_some())
                    {
                        return Err(format!(
                            "inline module path is outside captured input profile: {name}"
                        ));
                    }
                    let argument = tokens
                        .split_once('=')
                        .ok_or("malformed path attribute")?
                        .1
                        .trim()
                        .strip_suffix(']')
                        .ok_or("malformed path attribute")?
                        .trim();
                    inputs.push((
                        serde_json::from_str::<String>(argument).map_err(|error| {
                            format!("nonliteral Rust module path: {name}: {error}")
                        })?,
                        true,
                    ));
                } else if compact.starts_with("#[cfg_attr(") && compact.contains("path=") {
                    return Err(format!(
                        "conditional module path is outside captured input profile: {name}"
                    ));
                }
            }
            for (input, compiled) in inputs {
                let path = root
                    .join(&name)
                    .parent()
                    .ok_or("source parent absent")?
                    .join(input);
                let canonical = fs::canonicalize(&path).map_err(|error| {
                    format!("included source is unavailable from {name}: {error}")
                })?;
                let relative = canonical
                    .strip_prefix(root)
                    .map_err(|error| {
                        format!("included source escapes repository: {name}: {error}")
                    })?
                    .to_str()
                    .ok_or("included source path is not UTF-8")?
                    .replace('\\', "/");
                // Reject the original lexical symlink even when its target is local.
                let mut parent = Some(path.as_path());
                while let Some(part) = parent {
                    if fs::symlink_metadata(part)
                        .map_err(|e| e.to_string())?
                        .file_type()
                        .is_symlink()
                    {
                        return Err("included source traverses a symlink".to_string());
                    }
                    if part == root {
                        break;
                    }
                    parent = part.parent();
                }
                if compiled {
                    compiled_inputs.insert(relative.clone());
                }
                explicit_inputs.insert(relative.clone());
                paths.insert(relative);
                if paths.len() > MAX_FILES {
                    return Err("source closure exceeds 20,000 files".to_string());
                }
            }
        }
    }
    Ok(explicit_inputs)
}

fn source_path(root: &Path, name: &str) -> Result<std::path::PathBuf, String> {
    relative_path(name)?;
    let mut path = root.to_path_buf();
    for component in Path::new(name).components() {
        path.push(component);
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("source input traverses a symlink".to_string());
        }
    }
    let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if !canonical.starts_with(root) {
        return Err("source input escapes repository".to_string());
    }
    Ok(canonical)
}

fn is_test_only(node: &ra_ap_syntax::SyntaxNode) -> bool {
    use ra_ap_syntax::{AstNode, ast};
    node.ancestors().any(|ancestor| {
        ancestor
            .children()
            .filter_map(ast::Attr::cast)
            .any(|attribute| {
                let text = token_text(attribute.syntax());
                matches!(text.as_str(), "#[cfg(test)]" | "#![cfg(test)]" | "#[test]")
            })
    })
}

/// Carry explicit cfg(test) ancestry through external module declarations.
/// Names never imply exclusion: a file with any production incoming edge, or
/// no known declaration at all, remains in the conservative compiled closure.
fn test_only_source_files(
    root: &Path,
    files: &BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
    use ra_ap_syntax::{
        AstNode, Edition, SourceFile,
        ast::{self, HasAttrs, HasName},
    };
    use std::collections::BTreeMap;
    let mut incoming = BTreeMap::<String, Vec<(String, bool)>>::new();
    let mut intrinsic = BTreeSet::new();
    let mut bytes_read = 0u64;
    for file in files {
        let bytes = read_bounded(&source_path(root, file)?, FILE_BYTES)?;
        bytes_read = bytes_read
            .checked_add(bytes.len() as u64)
            .ok_or("source-input size overflow")?;
        if bytes_read > 256 * 1024 * 1024 {
            return Err("source-input scan exceeds 256 MiB".to_string());
        }
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        // Whitespace/comment spelling of `mod` must not hide a production
        // edge and leave its target classified solely through a test edge.
        let parsed = SourceFile::parse(text, Edition::Edition2024);
        if !parsed.errors().is_empty() {
            return Err(format!("cannot determine Rust module ancestry: {file}"));
        }
        if is_test_only(&parsed.syntax_node()) {
            intrinsic.insert(file.clone());
        }
        let source = root.join(file);
        let parent = source.parent().ok_or("module source parent absent")?;
        let stem = source
            .file_stem()
            .and_then(|v| v.to_str())
            .ok_or("module file stem absent")?;
        let default_directory = if matches!(stem, "lib" | "main" | "mod") {
            parent.to_path_buf()
        } else {
            parent.join(stem)
        };
        let mut edge = |target: std::path::PathBuf, test_only: bool| -> Result<(), String> {
            if !target.is_file() {
                return Ok(());
            }
            let canonical = fs::canonicalize(target).map_err(|e| e.to_string())?;
            if let Ok(relative) = canonical.strip_prefix(root) {
                let relative = relative
                    .to_str()
                    .ok_or("module path is not UTF-8")?
                    .replace('\\', "/");
                incoming
                    .entry(relative)
                    .or_default()
                    .push((file.clone(), test_only));
            }
            Ok(())
        };
        for module in parsed
            .syntax_node()
            .descendants()
            .filter_map(ast::Module::cast)
        {
            if module.item_list().is_some() {
                continue;
            }
            let Some(name) = module.name().map(|name| token_text(name.syntax())) else {
                continue;
            };
            let mut inline: Vec<_> = module
                .syntax()
                .ancestors()
                .skip(1)
                .filter_map(ast::Module::cast)
                .filter(|m| m.item_list().is_some())
                .filter_map(|module| module.name().map(|name| token_text(name.syntax())))
                .collect();
            inline.reverse();
            let mut directory = default_directory.clone();
            for name in &inline {
                directory.push(name);
            }
            let mut explicit = None;
            for attribute in module.attrs() {
                let text = token_text(attribute.syntax());
                if text.starts_with("#[path=") {
                    explicit = text
                        .split_once('=')
                        .and_then(|(_, v)| v.trim().strip_suffix(']'))
                        .and_then(|v| serde_json::from_str::<String>(v.trim()).ok());
                }
            }
            let test_only = is_test_only(module.syntax());
            if let Some(path) = explicit {
                // The active-source pass refuses inline #[path] contexts.
                // Do not use guessed edges here to exclude another source.
                if inline.is_empty() {
                    edge(parent.join(path), test_only)?;
                }
            } else {
                edge(directory.join(format!("{name}.rs")), test_only)?;
                edge(directory.join(name).join("mod.rs"), test_only)?;
            }
        }
        // An include! is another compilation edge. A production include of a
        // nominal test module defeats exclusion just like a production mod.
        for call in parsed
            .syntax_node()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            if call.path().is_none_or(|path| {
                token_text(path.syntax())
                    .rsplit("::")
                    .next()
                    .is_none_or(|name| name.trim() != "include")
            }) {
                continue;
            }
            if let Some(tree) = call.token_tree() {
                let text = token_text(tree.syntax());
                if let Some(argument) = text.get(1..text.len().saturating_sub(1))
                    && let Ok(path) =
                        serde_json::from_str::<String>(argument.trim().trim_end_matches(',').trim())
                {
                    edge(parent.join(path), is_test_only(call.syntax()))?;
                }
            }
        }
    }
    let mut excluded = intrinsic.clone();
    loop {
        let next: BTreeSet<_> = incoming
            .iter()
            .filter(|(_, edges)| {
                !edges.is_empty()
                    && edges
                        .iter()
                        .all(|(source, test_only)| *test_only || excluded.contains(source))
            })
            .map(|(path, _)| path.clone())
            .chain(intrinsic.iter().cloned())
            .collect();
        if next == excluded {
            return Ok(next);
        }
        excluded = next;
    }
}

/// The registered ordinary-harness profile has one proof entry and exactly
/// the declared local unwind override. Transforming/panic/stub attributes need
/// a separate reviewed profile and cannot borrow this ordinary proof contract.
pub fn validate_harness_configuration(
    source: &str,
    bare_name: &str,
    expected_unwind: Option<u64>,
) -> Result<(), String> {
    use ra_ap_syntax::{
        AstNode, Edition, SourceFile,
        ast::{self, HasAttrs, HasName},
    };
    let parsed = SourceFile::parse(source, Edition::Edition2024);
    if !parsed.errors().is_empty() {
        return Err("registered harness source cannot be parsed".to_string());
    }
    let functions: Vec<_> = parsed
        .syntax_node()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.name().is_some_and(|name| name.text() == bare_name))
        .collect();
    let [function] = functions.as_slice() else {
        return Err(
            "registered harness function is absent or ambiguous in its source file".to_string(),
        );
    };
    let mut proof = 0;
    let mut unwind = None;
    for attribute in function.attrs() {
        let text = token_text(attribute.syntax());
        if text == "#[kani::proof]" {
            proof += 1;
        } else if let Some(number) = text
            .strip_prefix("#[kani::unwind(")
            .and_then(|v| v.strip_suffix(")]"))
        {
            if unwind.is_some() {
                return Err("duplicate registered harness unwind".to_string());
            }
            let number = number.parse::<u64>().map_err(|e| e.to_string())?;
            if number == 0 {
                return Err("registered harness unwind must be positive".to_string());
            }
            unwind = Some(number);
        } else if ![
            "#[doc=",
            "#[doc(",
            "#[allow(",
            "#[expect(",
            "#[deny(",
            "#[forbid(",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        {
            return Err(
                "registered ordinary harness has an unsupported transforming or verifier attribute"
                    .to_string(),
            );
        }
    }
    if proof != 1 || unwind != expected_unwind {
        return Err(
            "registered harness attributes disagree with the declared proof/bounds profile"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
