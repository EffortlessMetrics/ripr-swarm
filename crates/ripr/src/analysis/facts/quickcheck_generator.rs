//! Bounded identity authority for property-test input generators (#11545).
//!
//! A generator is a method of a hand-written `impl` of a provider's
//! `Arbitrary` trait: QuickCheck (`quickcheck::Arbitrary`), proptest
//! (`proptest::arbitrary::Arbitrary` or its `prelude` re-export) or the
//! `arbitrary` crate (`arbitrary::Arbitrary<'a>`). The provider is the sole
//! mandatory, manifest-verified registry dependency of a conventional
//! single-package crate; its `src/lib.rs` / `src/main.rs` root and the files
//! reached from it through plain out-of-line `mod name;` declarations are
//! eligible. The trait is spelled either by its qualified provider path or
//! by a name an explicit provider import binds in the impl's own module.
//!
//! A trait name, nearest manifest, glob, unexpanded macro, or derive alone is
//! not identity. `#[derive(Arbitrary)]` generates no hand-written generator
//! and is not a recognized attribute: a file carrying it keeps its ordinary
//! analysis, and a change to a derived type's fields stays product data
//! shape. Unsupported layouts retain their ordinary analysis; no role is
//! inferred from a function named `arbitrary`.

use super::{FunctionContainer, FunctionFact, FunctionSourceRole, RustIndex};
use ra_ap_syntax::ast::{HasAttrs, HasModuleItem, HasName, HasVisibility};
use ra_ap_syntax::{AstNode, SyntaxNode, ast};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Admitted generator `impl` blocks, by root-relative file.
pub(super) type GeneratorImpls = BTreeMap<PathBuf, Vec<AdmittedImpl>>;

/// One admitted generator `impl` block: its inclusive 1-based line range and
/// its trait spelling, compacted as the parser's `FunctionContainer` records
/// it. A method qualifies only when both agree, so another impl sharing a
/// boundary line never borrows the role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AdmittedImpl {
    start_line: usize,
    end_line: usize,
    trait_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Provider {
    QuickCheck,
    Proptest,
    Arbitrary,
}

impl Provider {
    const ALL: [Self; 3] = [Self::QuickCheck, Self::Proptest, Self::Arbitrary];

    fn krate(self) -> &'static str {
        match self {
            Self::QuickCheck => "quickcheck",
            Self::Proptest => "proptest",
            Self::Arbitrary => "arbitrary",
        }
    }

    /// Published paths of the provider's generator trait.
    fn trait_paths(self) -> &'static [&'static [&'static str]] {
        match self {
            Self::QuickCheck => &[&["quickcheck", "Arbitrary"]],
            Self::Proptest => &[
                &["proptest", "arbitrary", "Arbitrary"],
                &["proptest", "prelude", "Arbitrary"],
            ],
            Self::Arbitrary => &[&["arbitrary", "Arbitrary"]],
        }
    }

    /// `arbitrary::Arbitrary<'a>` carries one lifetime parameter; the other
    /// providers' traits carry none.
    fn takes_lifetime(self) -> bool {
        self == Self::Arbitrary
    }
}

/// Standard derives that need no import and cannot bind a provider name.
const BUILTIN_DERIVES: &[&str] = &[
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "Eq",
    "Hash",
    "Ord",
    "PartialEq",
    "PartialOrd",
];

/// Names a provider-, crate- or super-rooted import must not rebind: the
/// derives and attributes the namespace check trusts.
fn rebinds_trusted_name(name: &str) -> bool {
    BUILTIN_DERIVES.contains(&name)
        || matches!(name, "cfg" | "derive" | "test")
        || Provider::ALL
            .iter()
            .any(|provider| provider.krate() == name)
}

/// Whether `function` is a method directly inside an admitted generator impl:
/// within its lines and declared in an impl of the same trait spelling.
pub(super) fn is_generator(function: &FunctionFact, impls: &[AdmittedImpl]) -> bool {
    let FunctionContainer::TraitImpl { trait_path, .. } = &function.item.container else {
        return false;
    };
    impls.iter().any(|admitted| {
        admitted.start_line <= function.start_line
            && function.end_line <= admitted.end_line
            && admitted.trait_path == *trait_path
    })
}

/// Line coordinates cannot distinguish a generator from a production subject
/// sharing that line. Never grant the generator interpretation in that case.
pub(crate) fn is_generator_subject(index: &RustIndex, path: &std::path::Path, line: usize) -> bool {
    let Some(facts) = index.files.get(path) else {
        return false;
    };
    let mut found = false;
    for id in &facts.functions {
        let function = &index.function_facts[*id];
        if function.start_line <= line && line <= function.end_line {
            // Other item kinds have no FunctionFact. Code outside the method
            // can share either boundary line, so only its interior is known.
            if function.source_role != FunctionSourceRole::TestInputGenerator
                || line == function.start_line
                || line == function.end_line
            {
                return false;
            }
            found = true;
        }
    }
    found
}

fn sole_provider_dependency(manifest: &toml::Value) -> Option<Provider> {
    // A differently keyed dependency can export a library named like the
    // provider. This route does not resolve other edges, so it cannot rule
    // that out.
    if [
        "target",
        "dev-dependencies",
        "dev_dependencies",
        "build-dependencies",
        "build_dependencies",
    ]
    .iter()
    .any(|key| manifest.get(*key).is_some())
    {
        return None;
    }
    let dependencies = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)?;
    if dependencies.len() != 1 {
        return None;
    }
    Provider::ALL.into_iter().find(|provider| {
        dependencies.get(provider.krate()).is_some_and(|spec| {
            spec.get("optional")
                .is_none_or(|optional| optional.as_bool() == Some(false))
        })
    })
}

pub(super) fn verified_impls(index: &RustIndex) -> GeneratorImpls {
    let mut verified = GeneratorImpls::new();
    let Some(root) = index.drop_in_manifests.root() else {
        return verified;
    };
    // Do not add manifest/configuration work to indexes without a candidate.
    let candidate = index.files.values().any(|facts| {
        facts.source.contains("Arbitrary")
            && facts.functions.iter().any(|id| {
                matches!(
                    index.function_facts[*id].item.container,
                    FunctionContainer::TraitImpl { .. }
                )
            })
    });
    if !candidate {
        return verified;
    }
    let Some(manifest) = std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
    else {
        return verified;
    };
    let Some(provider) = sole_provider_dependency(&manifest) else {
        return verified;
    };
    let krate = provider.krate();
    let package = manifest.get("package");
    let edition = package
        .and_then(|package| package.get("edition"))
        .and_then(toml::Value::as_str);
    if !matches!(edition, Some("2018" | "2021" | "2024"))
        || manifest
            .get("workspace")
            .is_some_and(|workspace| !workspace.as_table().is_some_and(toml::Table::is_empty))
        || package
            .and_then(|package| package.get("workspace"))
            .is_some()
        || package
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .is_some_and(|name| name.replace('-', "_") == krate)
        || manifest.get("lib").is_some_and(|lib| {
            lib.get("path").is_some()
                || lib
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|name| name.replace('-', "_") == krate)
        })
        || ["bin", "test", "bench", "example"]
            .iter()
            .any(|key| manifest.get(*key).is_some())
        || crate::analysis::workspace::config_snapshot_identity(root).is_none()
        || !plain_compiler_configuration(root)
    {
        return verified;
    }
    let mut walk = ModuleWalk {
        index,
        provider,
        visited: BTreeSet::new(),
        admitted: GeneratorImpls::new(),
        tainted: BTreeSet::new(),
        taint_all: false,
    };
    for path in CRATE_ROOTS.map(PathBuf::from) {
        if index.files.contains_key(&path) && index.member_crates.is_default_target_root(&path) {
            walk.admit(&path, Path::new("src"), true);
        }
    }
    if walk.taint_all {
        return verified;
    }
    // A file is admitted only when no root's walk refused it or an ancestor:
    // `src/lib.rs` and `src/main.rs` can declare the same module file, and a
    // refused parent's opaque content can redirect what that child names.
    for (path, impls) in walk.admitted {
        if !walk.tainted.contains(&path) {
            verified.insert(path, impls);
        }
    }
    verified
}

const CRATE_ROOTS: [&str; 2] = ["src/lib.rs", "src/main.rs"];

struct ModuleWalk<'a> {
    index: &'a RustIndex,
    provider: Provider,
    visited: BTreeSet<PathBuf>,
    admitted: GeneratorImpls,
    /// Files a refused file may load or redirect; never admitted.
    tainted: BTreeSet<PathBuf>,
    /// A refused file whose module targets cannot be bounded (no syntax, or
    /// an attributed module item such as `#[path]`): admit nothing.
    taint_all: bool,
}

impl ModuleWalk<'_> {
    /// Admit `path` and, recursively, the files its plain top-level
    /// `mod name;` declarations load (`dir/name.rs` or `dir/name/mod.rs`). A
    /// file that is unparsed, lexically indexed, unverified or
    /// namespace-ambiguous is refused and taints its whole module subtree.
    fn admit(&mut self, path: &Path, module_dir: &Path, crate_root: bool) {
        if !self.visited.insert(path.to_path_buf()) {
            return;
        }
        let index = self.index;
        let Some(facts) = index.files.get(path) else {
            return;
        };
        let tree = index
            .source_syntax
            .get(index, path)
            .and_then(|snapshot| snapshot.tree());
        let Some(tree) = tree.filter(|tree| {
            !facts.used_lexical_fallback
                && index
                    .drop_in_manifests
                    .verified(path, self.provider.krate())
                && namespace_is_unambiguous(tree, self.provider, crate_root)
        }) else {
            self.taint(path, module_dir);
            return;
        };
        let impls = generator_impls(&tree, self.provider);
        if !impls.is_empty() {
            self.admitted.insert(path.to_path_buf(), impls);
        }
        for item in tree.items() {
            let ast::Item::Module(module) = item else {
                continue;
            };
            if module.item_list().is_some() {
                // Out-of-line children of inline modules use nested
                // directories; this route does not follow them.
                continue;
            }
            let Some(name) = module.name().map(|name| name.text().to_string()) else {
                continue;
            };
            let flat = module_dir.join(format!("{name}.rs"));
            let nested = module_dir.join(&name).join("mod.rs");
            let child = match (
                index.files.contains_key(&flat),
                index.files.contains_key(&nested),
            ) {
                (true, false) => flat,
                (false, true) => nested,
                _ => continue,
            };
            self.admit(&child, &module_dir.join(&name), false);
        }
    }

    /// Taint a refused file and every file its modules can load. Without
    /// attributes on module items, every descendant lives under `module_dir`
    /// (other crate roots are separate crates). An unreadable file or an
    /// attributed module item (`#[path]`, `cfg_attr`) is unbounded.
    fn taint(&mut self, path: &Path, module_dir: &Path) {
        self.tainted.insert(path.to_path_buf());
        let index = self.index;
        let bounded = index
            .source_syntax
            .get(index, path)
            .and_then(|snapshot| snapshot.tree())
            .is_some_and(|tree| {
                tree.syntax()
                    .descendants()
                    .filter_map(ast::Module::cast)
                    .all(|module| {
                        module.attrs().all(|attribute| {
                            attribute
                                .syntax()
                                .text()
                                .to_string()
                                .split_whitespace()
                                .collect::<String>()
                                == "#[cfg(test)]"
                        })
                    })
            });
        if !bounded {
            self.taint_all = true;
            return;
        }
        for file in index.files.keys() {
            if file.starts_with(module_dir)
                && !CRATE_ROOTS.iter().any(|root| file == Path::new(root))
            {
                self.tainted.insert(file.clone());
            }
        }
    }
}

/// Line ranges of module-level `impl <provider trait> for T` blocks whose
/// trait spelling resolves to the provider: a qualified provider path (the
/// namespace check refuses every type-namespace binding of the provider
/// name), or a single name bound by an explicit provider import in the
/// impl's own module.
fn generator_impls(tree: &ast::SourceFile, provider: Provider) -> Vec<AdmittedImpl> {
    let text = tree.syntax().text().to_string();
    let line_of = |offset: usize| {
        text.get(..offset)
            .map_or(0, |prefix| prefix.matches('\n').count())
            + 1
    };
    tree.syntax()
        .descendants()
        .filter_map(ast::Impl::cast)
        .filter(|item| {
            // Only module-level impls: a function body may hold local items
            // and macro expansions that this file-wide check does not read.
            let Some(scope) = item.syntax().parent().filter(|scope| {
                ast::SourceFile::can_cast(scope.kind()) || ast::ItemList::can_cast(scope.kind())
            }) else {
                return false;
            };
            let Some(trait_ty) = item.trait_() else {
                return false;
            };
            let Some(segments) = trait_segments(&trait_ty.syntax().text().to_string(), provider)
            else {
                return false;
            };
            if provider
                .trait_paths()
                .iter()
                .any(|path| path.iter().copied().eq(segments.iter().map(String::as_str)))
            {
                return true;
            }
            let [name] = segments.as_slice() else {
                return false;
            };
            scope_imports_trait(&scope, name, provider)
        })
        .filter_map(|item| {
            let range = item.syntax().text_range();
            let trait_path = item.trait_()?.syntax().text().to_string();
            Some(AdmittedImpl {
                start_line: line_of(usize::from(range.start())),
                end_line: line_of(usize::from(range.end())),
                // The parser's compaction (`compact_syntax_text`).
                trait_path: trait_path.split_whitespace().collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

/// Segments of a trait path written as plain identifiers, an optional leading
/// `::`, and, for a lifetime-carrying provider trait, only lifetime arguments
/// on the last segment. Anything else (qualified `<T as _>` paths, type
/// arguments, `self`/`super`/`crate` roots) is not a provider spelling.
fn trait_segments(text: &str, provider: Provider) -> Option<Vec<String>> {
    let compact = text.split_whitespace().collect::<String>();
    let path = compact.strip_prefix("::").unwrap_or(&compact);
    let path = match path.split_once('<') {
        None => path,
        Some((head, arguments)) => {
            let arguments = arguments.strip_suffix('>')?;
            if !provider.takes_lifetime()
                || arguments.contains(['<', '>'])
                || !arguments.split(',').all(|argument| {
                    argument
                        .strip_prefix('\'')
                        .is_some_and(|name| !name.is_empty() && name.chars().all(is_ident_char))
                })
            {
                return None;
            }
            head
        }
    };
    let segments = path
        .split("::")
        .map(|segment| segment.strip_prefix("r#").unwrap_or(segment).to_string())
        .collect::<Vec<_>>();
    segments
        .iter()
        .all(|segment| {
            !segment.is_empty()
                && segment.chars().all(is_ident_char)
                && !matches!(segment.as_str(), "self" | "super" | "crate" | "Self")
        })
        .then_some(segments)
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether a `use` item directly in `scope` (the impl's own module: a file
/// or an inline module's item list) binds `name` to a provider trait path.
/// A same-module item of that name would collide with the import (E0255),
/// and an explicit import outranks globs.
fn scope_imports_trait(scope: &SyntaxNode, name: &str, provider: Provider) -> bool {
    scope.children().filter_map(ast::Use::cast).any(|import| {
        flattened_use(&import).is_some_and(|bindings| {
            bindings.iter().any(|binding| {
                binding.name.as_deref() == Some(name)
                    && provider.trait_paths().iter().any(|path| {
                        path.iter()
                            .copied()
                            .eq(binding.path.iter().map(String::as_str))
                    })
            })
        })
    })
}

struct UseBinding {
    path: Vec<String>,
    /// The bound name; `None` for `as _`.
    name: Option<String>,
}

/// Flatten a use tree into its imported paths. `None` for a glob, a `self`
/// leaf, or a shape this route does not read.
fn flattened_use(import: &ast::Use) -> Option<Vec<UseBinding>> {
    fn walk(tree: &ast::UseTree, prefix: &[String], out: &mut Vec<UseBinding>) -> Option<()> {
        if tree.star_token().is_some() {
            return None;
        }
        let mut path = prefix.to_vec();
        if let Some(own) = tree.path() {
            for segment in own
                .syntax()
                .text()
                .to_string()
                .split_whitespace()
                .collect::<String>()
                .split("::")
            {
                if segment.is_empty() {
                    // A leading `::` names the extern prelude root.
                    continue;
                }
                if segment == "self" || !segment.chars().all(is_ident_char) {
                    return None;
                }
                path.push(segment.strip_prefix("r#").unwrap_or(segment).to_string());
            }
        }
        if let Some(list) = tree.use_tree_list() {
            if tree.rename().is_some() {
                return None;
            }
            for child in list.use_trees() {
                walk(&child, &path, out)?;
            }
            return Some(());
        }
        let name = match tree.rename() {
            Some(rename) if rename.underscore_token().is_some() => None,
            Some(rename) => Some(rename.name()?.text().to_string()),
            None => Some(path.last()?.clone()),
        };
        out.push(UseBinding { path, name });
        Some(())
    }
    let mut out = Vec::new();
    walk(&import.use_tree()?, &[], &mut out)?;
    Some(out)
}

fn namespace_is_unambiguous(tree: &ast::SourceFile, provider: Provider, crate_root: bool) -> bool {
    let krate = provider.krate();
    // Refusal is deliberately file-wide. Macro expansions and imported
    // namespaces cannot be used as absence certificates for shadowing.
    !tree.syntax().descendants().any(|node| {
        if let Some(name) = ast::Name::cast(node.clone()) {
            // A function lives in the value namespace and cannot shadow the
            // provider's module path (`fn arbitrary` of the `arbitrary`
            // crate's trait). Every other binding of the name is refused.
            let text = name.text();
            let bare = text.strip_prefix("r#").unwrap_or(text);
            return bare == krate
                && !node
                    .parent()
                    .is_some_and(|parent| ast::Fn::can_cast(parent.kind()));
        }
        if let Some(import) = ast::Use::cast(node.clone()) {
            return !import_is_admitted(&import, krate, crate_root);
        }
        if let Some(attribute) = ast::Attr::cast(node.clone()) {
            return !attribute_is_trusted(&attribute);
        }
        if ast::MacroCall::can_cast(node.kind()) {
            // A macro invoked inside a function body expands into that block
            // only; generator impls are admitted at module level only.
            return !node
                .ancestors()
                .any(|ancestor| ast::BlockExpr::can_cast(ancestor.kind()));
        }
        ast::ExternCrate::can_cast(node.kind())
            || ast::MacroRules::can_cast(node.kind())
            || ast::MacroDef::can_cast(node.kind())
    })
}

/// Admitted imports cannot rebind the provider name or a trusted built-in:
/// non-glob paths rooted at the provider, `crate` or `super`, plus
/// `use super::*;` inside an inline module (its parent's names are in this
/// file and checked here).
fn import_is_admitted(import: &ast::Use, krate: &str, crate_root: bool) -> bool {
    if import.attrs().next().is_some() || import.visibility().is_some() {
        return false;
    }
    let compact = import
        .syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect::<String>();
    let inline_module = import
        .syntax()
        .parent()
        .is_some_and(|parent| ast::ItemList::can_cast(parent.kind()));
    if compact == "usesuper::*;" {
        return inline_module;
    }
    let Some(bindings) = flattened_use(import) else {
        return false;
    };
    !bindings.is_empty()
        && bindings.iter().all(|binding| {
            let root_ok = match binding.path.first().map(String::as_str) {
                Some(root) if root == krate => true,
                Some("crate") => true,
                Some("super") => !crate_root || inline_module,
                _ => false,
            };
            root_ok
                && binding
                    .name
                    .as_deref()
                    .is_none_or(|name| !rebinds_trusted_name(name))
        })
}

/// Built-in attributes that cannot introduce a provider binding: derives of
/// standard traits, `#[cfg(test)]` and `#[test]`. Every other attribute,
/// including `#[derive(Arbitrary)]`, refuses the file.
fn attribute_is_trusted(attribute: &ast::Attr) -> bool {
    let text = attribute
        .syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect::<String>();
    if matches!(text.as_str(), "#[cfg(test)]" | "#[test]") {
        return true;
    }
    text.strip_prefix("#[derive(")
        .and_then(|rest| rest.strip_suffix(")]"))
        .is_some_and(|list| {
            !list.is_empty() && list.split(',').all(|name| BUILTIN_DERIVES.contains(&name))
        })
}

fn plain_compiler_configuration(root: &std::path::Path) -> bool {
    if std::env::vars_os().any(|(key, value)| {
        let key = key.to_string_lossy().to_ascii_uppercase();
        !value.is_empty()
            && (matches!(
                key.as_str(),
                "RUSTFLAGS"
                    | "CARGO_ENCODED_RUSTFLAGS"
                    | "RUSTC"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
            ) || key.starts_with("CARGO_")
                && (key.ends_with("RUSTFLAGS") || key.contains("RUSTC")))
    }) {
        return false;
    }
    let Ok(home) = home::env::cargo_home_with_cwd_env(&home::env::OS_ENV, root) else {
        return false;
    };
    let home = if home.is_absolute() {
        home
    } else {
        root.join(home)
    };
    let mut paths = vec![home.join("config"), home.join("config.toml")];
    for directory in root.ancestors() {
        paths.extend([
            directory.join(".cargo/config"),
            directory.join(".cargo/config.toml"),
        ]);
    }
    paths
        .iter()
        .all(|path| match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str::<toml::Value>(&text)
                .ok()
                .is_some_and(|config| {
                    let compiler_keys = [
                        "rustflags",
                        "rustc",
                        "rustc-wrapper",
                        "rustc-workspace-wrapper",
                    ];
                    let plain = |table: &toml::Value| {
                        compiler_keys.iter().all(|key| table.get(*key).is_none())
                    };
                    config.get("include").is_none()
                        && config.get("build").is_none_or(plain)
                        && config.get("target").is_none_or(|targets| {
                            targets
                                .as_table()
                                .is_some_and(|targets| targets.values().all(plain))
                        })
                        && config.get("env").is_none_or(|env| {
                            env.as_table().is_some_and(|env| {
                                env.keys()
                                    .all(|key| !key.to_ascii_uppercase().starts_with("RUST"))
                            })
                        })
                }),
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        })
}
