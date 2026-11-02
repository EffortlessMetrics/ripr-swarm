//! Whether an owner file sits behind a `#[cfg]` on a parent `mod` declaration (#5616).
//!
//! `ripr agent stub` already refuses when the owner's own file gates the function
//! or an enclosing inline module. A gate on the out-of-line `mod child;` in a
//! parent file is invisible from the child file's AST, so this walks the default
//! module-declaration chain upward and reads only the declaring files along it.

use super::module_tree::enclosing_inline_modules;
use super::parse_clean_source_file;
use crate::analysis::facts::cfg_predicates::attribute_test_build_availability;
use crate::analysis::workspace::module_parent_candidates;
use ra_ap_syntax::AstNode;
use ra_ap_syntax::ast::{self, HasName};
use std::path::{Path, PathBuf};

/// `true` when a parent `mod` declaration along the chain carries a cfg a plain
/// `cargo test` build may not enable.
pub(crate) fn owner_gated_by_parent_mod_declarations(
    package_root: &Path,
    owner_file: &Path,
) -> bool {
    let mut current = owner_file
        .strip_prefix("./")
        .unwrap_or(owner_file)
        .to_path_buf();
    current = PathBuf::from(current.to_string_lossy().replace('\\', "/"));
    while let Some(child_name) = out_of_line_module_name(&current) {
        let candidates = module_parent_candidates(&current);
        if candidates.is_empty() {
            break;
        }
        let mut declared_in = None;
        for candidate in candidates {
            let path = package_root.join(&candidate);
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(module) = find_declaring_module(&source, &child_name, &candidate, &current)
            {
                if module_is_gated(&module) {
                    return true;
                }
                declared_in = Some(candidate);
                break;
            }
        }
        let Some(parent) = declared_in else {
            break;
        };
        if parent
            .file_name()
            .is_some_and(|name| matches!(name.to_str(), Some("lib.rs" | "main.rs")))
        {
            break;
        }
        current = parent;
    }
    false
}

fn out_of_line_module_name(file: &Path) -> Option<String> {
    let file_name = file.file_name()?.to_str()?;
    if file_name == "lib.rs" || file_name == "main.rs" {
        return None;
    }
    if file_name == "mod.rs" {
        return file
            .parent()?
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string);
    }
    file.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
}

fn find_declaring_module(
    source: &str,
    child_name: &str,
    declaring_file: &Path,
    child_file: &Path,
) -> Option<ast::Module> {
    let parse = parse_clean_source_file(source)?;
    for module in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
    {
        if module.item_list().is_some() {
            continue;
        }
        let Some(name) = module.name() else {
            continue;
        };
        if name.text().trim_start_matches("r#") != child_name {
            continue;
        }
        let inline = enclosing_inline_modules(&module)?;
        if default_resolution_matches(declaring_file, &inline, child_name, child_file) {
            return Some(module);
        }
    }
    None
}

fn default_resolution_matches(
    declaring_file: &Path,
    inline: &[String],
    name: &str,
    child_file: &Path,
) -> bool {
    let dir = module_directory_for(declaring_file, inline);
    let rs = dir.join(format!("{name}.rs"));
    let mod_rs = dir.join(name).join("mod.rs");
    child_file == rs || child_file == mod_rs
}

fn module_directory_for(declaring_file: &Path, inline: &[String]) -> PathBuf {
    let declaring = PathBuf::from(declaring_file.to_string_lossy().replace('\\', "/"));
    let parent = declaring
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or(Path::new(""));
    let file_name = declaring
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut directory = if file_name == "mod.rs" || file_name == "lib.rs" || file_name == "main.rs"
    {
        parent.to_path_buf()
    } else {
        parent.join(
            declaring
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    };
    for segment in inline {
        directory.push(segment);
    }
    directory
}

fn module_is_gated(module: &ast::Module) -> bool {
    module.syntax().ancestors().any(|node| {
        node.children().filter_map(ast::Attr::cast).any(|attr| {
            attribute_test_build_availability(&attr.syntax().text().to_string()) != Some(true)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_crate(name: &str, files: &[(&str, &str)]) -> Result<PathBuf, String> {
        let root = std::env::temp_dir().join(format!("ripr-5616-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"gate5616\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|err| err.to_string())?;
        for (path, text) in files {
            let file = root.join(path);
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            fs::write(file, text).map_err(|err| err.to_string())?;
        }
        Ok(root)
    }

    #[test]
    fn parent_feature_gate_on_mod_extra_refuses() -> Result<(), String> {
        let root = temp_crate(
            "extra",
            &[
                ("src/lib.rs", "#[cfg(feature = \"extra\")]\nmod extra;\n"),
                (
                    "src/extra.rs",
                    "pub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n",
                ),
            ],
        )?;
        assert!(owner_gated_by_parent_mod_declarations(
            &root,
            Path::new("src/extra.rs")
        ));
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn ungated_mod_extra_is_not_refused() -> Result<(), String> {
        let root = temp_crate(
            "plain",
            &[
                ("src/lib.rs", "mod extra;\n"),
                (
                    "src/extra.rs",
                    "pub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n",
                ),
            ],
        )?;
        assert!(!owner_gated_by_parent_mod_declarations(
            &root,
            Path::new("src/extra.rs")
        ));
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn cfg_test_mod_parent_is_not_a_gate() -> Result<(), String> {
        let root = temp_crate(
            "testmod",
            &[
                ("src/lib.rs", "#[cfg(test)]\nmod extra;\n"),
                (
                    "src/extra.rs",
                    "pub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n",
                ),
            ],
        )?;
        assert!(!owner_gated_by_parent_mod_declarations(
            &root,
            Path::new("src/extra.rs")
        ));
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn nested_chain_refuses_when_intermediate_mod_is_gated() -> Result<(), String> {
        let root = temp_crate(
            "nested",
            &[
                ("src/lib.rs", "#[cfg(feature = \"a\")]\nmod a;\n"),
                ("src/a.rs", "mod b;\n"),
                (
                    "src/a/b.rs",
                    "pub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n",
                ),
            ],
        )?;
        assert!(owner_gated_by_parent_mod_declarations(
            &root,
            Path::new("src/a/b.rs")
        ));
        let _ = fs::remove_dir_all(root);
        Ok(())
    }
}
