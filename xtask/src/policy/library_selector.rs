//! Source resolution of library unit-test `covered_by` selectors (#13705).
//!
//! Listing a `cargo test -p <package> --lib <module path>` selector needs the
//! package's library test target. For `ripr` that one build holds about
//! 13.8 GB, which does not fit a 15 GB container. Local `precommit` therefore
//! resolves such a selector against source: the named module must be declared
//! along the path from the library root and must hold at least one `#[test]`
//! function. That is weaker than a listing (it does not compile anything), so
//! its report row says so, and hosted qualification still lists the selector.
//!
//! Every shape this module does not understand returns `None`, and the caller
//! falls back to building and listing. A `None` is never a failure.

use std::path::{Path, PathBuf};

use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use ra_ap_syntax::{AstNode, Edition, SourceFile};

/// `cargo test -p <package> --lib <module path>`, with optional lock flags and
/// nothing else.
#[derive(Debug, PartialEq, Eq)]
struct LibrarySelector {
    package: String,
    module_path: Vec<String>,
}

fn parse_library_selector(command: &str) -> Option<LibrarySelector> {
    let mut words = command.split_whitespace();
    if words.next() != Some("cargo") || words.next() != Some("test") {
        return None;
    }
    let mut package = None;
    let mut filter = None;
    let mut lib = false;
    while let Some(word) = words.next() {
        match word {
            "-p" | "--package" if package.is_none() => {
                package = Some(words.next().filter(|value| !value.starts_with('-'))?);
            }
            "--lib" if !lib => lib = true,
            "--locked" | "--offline" => {}
            positional if !positional.starts_with('-') && filter.is_none() => {
                filter = Some(positional);
            }
            _ => return None,
        }
    }
    if !lib {
        return None;
    }
    let module_path = filter?.split("::").map(str::to_string).collect::<Vec<_>>();
    let is_identifier = |segment: &String| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    };
    if !module_path.iter().all(is_identifier) {
        return None;
    }
    Some(LibrarySelector {
        package: package?.to_string(),
        module_path,
    })
}

/// The library root source file of workspace member `package`, read from the
/// manifests. Globbed members, a missing member or an unreadable manifest
/// give `None`.
fn library_root(root: &Path, package: &str) -> Option<PathBuf> {
    let workspace: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()?;
    let members = workspace.get("workspace")?.get("members")?.as_array()?;
    for member in members {
        let member = member.as_str()?;
        if member.contains(['*', '?', '[']) {
            return None;
        }
        let directory = root.join(member);
        let manifest: toml::Table = std::fs::read_to_string(directory.join("Cargo.toml"))
            .ok()?
            .parse()
            .ok()?;
        if manifest.get("package")?.get("name")?.as_str()? != package {
            continue;
        }
        let relative = match manifest.get("lib").and_then(|lib| lib.get("path")) {
            Some(path) => path.as_str()?,
            None => "src/lib.rs",
        };
        let path = directory.join(relative);
        return path.is_file().then_some(path);
    }
    None
}

fn attribute_text(attr: &ast::Attr) -> String {
    attr.syntax()
        .text()
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

/// The value of one whitespace-free `cfg` predicate in a test build on this
/// host, or `None` when it is not understood here. Only `test` and the host
/// family and operating system are understood, alone or under `not(..)`:
/// selectors are resolved for the host that runs the local build.
fn host_cfg_value(predicate: &str) -> Option<bool> {
    if let Some(inner) = predicate
        .strip_prefix("not(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return host_cfg_value(inner).map(|value| !value);
    }
    match predicate {
        "test" => Some(true),
        "unix" => Some(cfg!(unix)),
        "windows" => Some(cfg!(windows)),
        _ => {
            let os = predicate.strip_prefix("target_os=\"")?.strip_suffix('"')?;
            Some(os == std::env::consts::OS)
        }
    }
}

/// Whether an item is present in a test build on this host: `Some(true)` or
/// `Some(false)` when every `cfg` on it is understood (see
/// [`host_cfg_value`]), `None` when any `cfg` is not, or when it carries a
/// `cfg_attr` or a `#[path]`.
fn presence_in_test_build<T: HasAttrs>(item: &T) -> Option<bool> {
    let mut present = true;
    for text in item.attrs().map(|attr| attribute_text(&attr)) {
        if text.starts_with("#[cfg_attr") || text.starts_with("#[path") {
            return None;
        }
        if let Some(predicate) = text
            .strip_prefix("#[cfg(")
            .and_then(|rest| rest.strip_suffix(")]"))
        {
            present &= host_cfg_value(predicate)?;
        } else if text.starts_with("#[cfg") {
            return None;
        }
    }
    Some(present)
}

fn is_test_function(function: &ast::Fn) -> bool {
    function
        .attrs()
        .any(|attr| attribute_text(&attr) == "#[test]")
}

fn parse_items(path: &Path) -> Option<Vec<ast::Item>> {
    let text = std::fs::read_to_string(path).ok()?;
    let parse = SourceFile::parse(&text, Edition::Edition2024);
    if !parse.errors().is_empty() {
        return None;
    }
    Some(parse.tree().items().collect())
}

/// Whether an attribute could mark a function as a test in a way this module
/// does not count: anything naming `test` other than `#[test]` and
/// `#[cfg(test)]`, such as `#[tokio::test]` or `#[test_case(..)]`.
fn is_other_test_attribute(text: &str) -> bool {
    text.contains("test") && text != "#[test]" && text != "#[cfg(test)]"
}

/// Test names under `items`, relative to `prefix`, or `None` when the list
/// could be incomplete. Nested inline modules are followed. A test or module
/// whose `cfg` is false on this host is left out, as the build leaves it out.
/// Each of these refuses the whole selector, which then goes to the
/// build-and-list path: a `cfg` not understood by [`host_cfg_value`], a
/// `cfg_attr` or `#[path]`, a nested file module (`mod deeper;`, not read
/// here), a macro invocation or definition (it can expand to tests), and a
/// function carrying another test attribute.
fn collect_tests(items: &[ast::Item], prefix: &str, tests: &mut Vec<String>) -> Option<()> {
    for item in items {
        match item {
            ast::Item::Fn(function) => {
                if function
                    .attrs()
                    .any(|attr| is_other_test_attribute(&attribute_text(&attr)))
                {
                    return None;
                }
                if is_test_function(function) && presence_in_test_build(function)? {
                    tests.push(format!("{prefix}::{}", function.name()?.text()));
                }
            }
            ast::Item::Module(module) => {
                if !presence_in_test_build(module)? {
                    continue;
                }
                let name = module.name()?;
                let nested = module.item_list()?.items().collect::<Vec<_>>();
                collect_tests(&nested, &format!("{prefix}::{}", name.text()), tests)?;
            }
            ast::Item::MacroCall(_) | ast::Item::MacroRules(_) | ast::Item::MacroDef(_) => {
                return None;
            }
            _ => {}
        }
    }
    Some(())
}

/// The tests a library unit-test selector names, read from source under
/// `root`. `None` means "not resolved here": the caller builds and lists.
pub(crate) fn resolve_in_source(root: &Path, command: &str) -> Option<Vec<String>> {
    let selector = parse_library_selector(command)?;
    let library = library_root(root, &selector.package)?;
    // Children of `lib.rs` live beside it.
    let mut child_directory = library.parent()?.to_path_buf();
    let mut items = parse_items(&library)?;
    for segment in &selector.module_path {
        let mut declared = items.iter().filter_map(|item| match item {
            ast::Item::Module(module)
                if module
                    .name()
                    .is_some_and(|name| name.text() == segment.as_str()) =>
            {
                Some(module.clone())
            }
            _ => None,
        });
        let module = declared.next()?;
        // Two declarations of one name are cfg alternatives; not understood.
        if declared.next().is_some() || presence_in_test_build(&module) != Some(true) {
            return None;
        }
        child_directory.push(segment);
        items = match module.item_list() {
            Some(list) => list.items().collect(),
            None => {
                let file = child_directory.with_extension("rs");
                let directory_module = child_directory.join("mod.rs");
                match (file.is_file(), directory_module.is_file()) {
                    (true, false) => parse_items(&file)?,
                    (false, true) => parse_items(&directory_module)?,
                    _ => return None,
                }
            }
        };
    }
    let mut tests = Vec::new();
    collect_tests(&items, &selector.module_path.join("::"), &mut tests)?;
    (!tests.is_empty()).then_some(tests)
}

#[cfg(test)]
mod tests {
    use super::{LibrarySelector, parse_library_selector, resolve_in_source};
    use std::path::{Path, PathBuf};

    struct Workspace(PathBuf);

    impl Workspace {
        fn new(label: &str) -> Result<Self, String> {
            let path = std::env::temp_dir().join(format!(
                "ripr-library-selector-{label}-{}",
                std::process::id()
            ));
            if path.exists() {
                std::fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
            }
            let workspace = Self(path);
            workspace.write(
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/a\"]\nresolver = \"2\"\n",
            )?;
            workspace.write("crates/a/Cargo.toml", "[package]\nname = \"a\"\n")?;
            Ok(workspace)
        }

        fn write(&self, relative: &str, contents: &str) -> Result<(), String> {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(path, contents).map_err(|error| error.to_string())
        }

        fn root(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0) {
                eprintln!("temporary workspace not removed: {error}");
            }
        }
    }

    const SELECTOR: &str = "cargo test -p a --lib outer::inner::tests";
    const INNER: &str = "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn first() {}\n    fn helper() {}\n    mod nested {\n        #[test]\n        fn second() {}\n    }\n}\n";

    fn layout(label: &str, inner: &str) -> Result<Workspace, String> {
        let workspace = Workspace::new(label)?;
        workspace.write("crates/a/src/lib.rs", "pub mod outer;\n")?;
        workspace.write("crates/a/src/outer/mod.rs", "pub(crate) mod inner;\n")?;
        workspace.write("crates/a/src/outer/inner.rs", inner)?;
        Ok(workspace)
    }

    fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(message.into())
        }
    }

    #[test]
    fn selector_grammar_accepts_only_one_package_lib_and_module_path() -> Result<(), String> {
        let expected = Some(LibrarySelector {
            package: "a".to_string(),
            module_path: vec!["outer".to_string(), "tests".to_string()],
        });
        for accepted in [
            "cargo test -p a --lib outer::tests",
            "cargo test --package a --locked --offline --lib outer::tests",
        ] {
            require(
                parse_library_selector(accepted) == expected,
                format!("not accepted: {accepted}"),
            )?;
        }
        for refused in [
            "cargo test -p a outer::tests",
            "cargo test -p a --lib",
            "cargo test --lib outer::tests",
            "cargo test -p a --lib outer::tests other",
            "cargo test -p a -p b --lib outer::tests",
            "cargo test -p a --lib --test t outer::tests",
            "cargo test -p a --lib --features x outer::tests",
            "cargo test -p a --lib outer::tests -- --ignored",
            "cargo test -p a --lib outer::",
            "cargo test -p a --lib outer::te-sts",
            "cargo nextest run -p a --lib outer::tests",
        ] {
            require(
                parse_library_selector(refused).is_none(),
                format!("accepted: {refused}"),
            )?;
        }
        Ok(())
    }

    #[test]
    fn a_declared_module_with_tests_resolves_to_their_names() -> Result<(), String> {
        let workspace = layout("resolves", INNER)?;
        let tests = resolve_in_source(workspace.root(), SELECTOR);
        require(
            tests
                == Some(vec![
                    "outer::inner::tests::first".to_string(),
                    "outer::inner::tests::nested::second".to_string(),
                ]),
            format!("wrong resolution: {tests:?}"),
        )
    }

    /// Each of these is a stale or not-understood pointer. None may resolve:
    /// the caller then builds and lists, which reports the real state.
    #[test]
    fn stale_or_unsupported_shapes_do_not_resolve() -> Result<(), String> {
        let no_tests = "#[cfg(test)]\nmod tests {\n    fn helper() {}\n}\n";
        let renamed = "#[cfg(test)]\nmod checks {\n    #[test]\n    fn first() {}\n}\n";
        let feature = "#[cfg(feature = \"x\")]\nmod tests {\n    #[test]\n    fn first() {}\n}\n";
        let pathed = "#[path = \"elsewhere.rs\"]\nmod tests;\n";
        let gated_test = "#[cfg(test)]\nmod tests {\n    #[cfg(feature = \"x\")]\n    #[test]\n    fn first() {}\n}\n";
        let broken = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn first( {}\n";
        // Inline tests plus a nested file module: its tests would be left out
        // of the list, so the selector is not resolved at all.
        let nested_file =
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn first() {}\n    mod deeper;\n}\n";
        // An unconditional test beside a test or module under a condition this
        // walk does not evaluate: the build may list all of them, so a list of
        // the unconditional one alone could be partial.
        let mixed_test = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn always() {}\n    #[cfg(feature = \"x\")]\n    #[test]\n    fn unix_only() {}\n}\n";
        let mixed_module = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn always() {}\n    #[cfg(any(unix, windows))]\n    mod nested {\n        #[test]\n        fn nested_test() {}\n    }\n}\n";
        let mixed_nested = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn always() {}\n    mod nested {\n        #[test]\n        fn plain() {}\n        #[cfg_attr(unix, ignore)]\n        #[cfg(not(feature = \"x\"))]\n        #[test]\n        fn unix_only() {}\n    }\n}\n";
        let other_attribute = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn always() {}\n    #[tokio::test]\n    async fn other() {}\n}\n";
        let macro_tests = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn always() {}\n    make_tests!(a, b);\n}\n";
        for (label, inner) in [
            ("no-tests", no_tests),
            ("renamed", renamed),
            ("feature", feature),
            ("pathed", pathed),
            ("gated-test", gated_test),
            ("broken", broken),
            ("nested-file", nested_file),
            ("mixed-test", mixed_test),
            ("mixed-module", mixed_module),
            ("mixed-nested", mixed_nested),
            ("other-attribute", other_attribute),
            ("macro-tests", macro_tests),
        ] {
            let workspace = layout(label, inner)?;
            let tests = resolve_in_source(workspace.root(), SELECTOR);
            require(tests.is_none(), format!("{label} resolved: {tests:?}"))?;
        }

        // The module file exists but the parent no longer declares it.
        let undeclared = layout("undeclared", INNER)?;
        undeclared.write("crates/a/src/outer/mod.rs", "// inner was removed\n")?;
        require(
            resolve_in_source(undeclared.root(), SELECTOR).is_none(),
            "an undeclared module resolved",
        )?;

        // Both `inner.rs` and `inner/mod.rs`: rustc refuses this, so do we.
        let ambiguous = layout("ambiguous", INNER)?;
        ambiguous.write("crates/a/src/outer/inner/mod.rs", INNER)?;
        require(
            resolve_in_source(ambiguous.root(), SELECTOR).is_none(),
            "an ambiguous module file resolved",
        )?;

        let other_package = layout("other-package", INNER)?;
        require(
            resolve_in_source(
                other_package.root(),
                "cargo test -p b --lib outer::inner::tests",
            )
            .is_none(),
            "an unknown package resolved",
        )
    }

    /// Host conditions are evaluated as the build on this host would: a test
    /// or module whose `cfg` is false is left out, and the rest are listed,
    /// including under `not(..)` and `target_os`.
    #[test]
    fn host_conditions_list_exactly_what_this_host_builds() -> Result<(), String> {
        let inner = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn always() {}\n    #[cfg(unix)]\n    #[test]\n    fn unix_only() {}\n    #[cfg(windows)]\n    #[test]\n    fn windows_only() {}\n    #[cfg(not(unix))]\n    #[test]\n    fn not_unix() {}\n    #[cfg(target_os = \"linux\")]\n    #[test]\n    fn linux_only() {}\n    #[cfg(unix)]\n    mod nested {\n        #[test]\n        fn nested_unix() {}\n    }\n}\n";
        let workspace = layout("host-conditions", inner)?;
        let mut expected = vec!["outer::inner::tests::always"];
        if cfg!(unix) {
            expected.push("outer::inner::tests::unix_only");
        }
        if cfg!(windows) {
            expected.push("outer::inner::tests::windows_only");
        }
        if !cfg!(unix) {
            expected.push("outer::inner::tests::not_unix");
        }
        if cfg!(target_os = "linux") {
            expected.push("outer::inner::tests::linux_only");
        }
        if cfg!(unix) {
            expected.push("outer::inner::tests::nested::nested_unix");
        }
        let tests = resolve_in_source(workspace.root(), SELECTOR);
        require(
            tests.as_deref()
                == Some(
                    &expected
                        .iter()
                        .map(|name| name.to_string())
                        .collect::<Vec<_>>()[..],
                ),
            format!("expected {expected:?}, got {tests:?}"),
        )
    }

    /// The shipped selector this exists for resolves in the real tree, so the
    /// local run does not build the `ripr` library test target for it.
    #[test]
    fn the_shipped_ripr_library_selector_resolves_in_this_repository() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("xtask has no parent directory")?;
        let policy = std::fs::read_to_string(root.join("policy/non-rust-allowlist.toml"))
            .map_err(|error| error.to_string())?;
        let selectors = policy
            .split('"')
            .filter(|text| text.starts_with("cargo test -p ripr ") && text.contains(" --lib "))
            .collect::<std::collections::BTreeSet<_>>();
        require(
            !selectors.is_empty(),
            "no ripr library selector is declared",
        )?;
        for selector in selectors {
            let tests = resolve_in_source(root, selector);
            require(
                tests.as_ref().is_some_and(|tests| !tests.is_empty()),
                format!("`{selector}` does not resolve in source: {tests:?}"),
            )?;
        }
        // Its module holds a `#[cfg(unix)]` test, which the native listing on
        // a Unix host includes; a resolved list must include it too.
        let admission = resolve_in_source(
            root,
            "cargo test -p ripr --lib analysis::scip::admission::tests",
        )
        .ok_or("the admission selector does not resolve")?;
        let gated =
            "analysis::scip::admission::tests::rejects_symlink_project_root_that_escapes_workspace";
        require(
            admission.iter().any(|name| name == gated) == cfg!(unix),
            format!("host-gated test listed wrongly: {admission:?}"),
        )?;
        Ok(())
    }
}
