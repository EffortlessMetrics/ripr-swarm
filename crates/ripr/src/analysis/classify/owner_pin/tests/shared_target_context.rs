//! Shared roles do not establish one execution or binding context.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    index: RustIndex,
    root: PathBuf,
}
impl Fixture {
    fn try_at(
        root: PathBuf,
        build: impl FnOnce(&Path) -> Result<RustIndex, String>,
    ) -> Result<Self, String> {
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let mut fixture = Self {
            index: RustIndex::default(),
            root,
        };
        fixture.index = build(&fixture.root)?;
        Ok(fixture)
    }
}

fn fixture_root(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "ripr-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn fixture_from_files(label: &str, files: &[(&str, &str)]) -> Result<Fixture, String> {
    Fixture::try_at(fixture_root(label), |root| {
        let mut paths = Vec::new();
        for &(name, source) in files {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&path, source).map_err(|error| error.to_string())?;
            if name.ends_with(".rs") {
                paths.push(PathBuf::from(name));
            }
        }
        crate::analysis::facts::build_index(root, &paths)
    })
}
impl std::ops::Deref for Fixture {
    type Target = RustIndex;
    fn deref(&self) -> &RustIndex {
        &self.index
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture(
    lib_prefix: &str,
    main_prefix: &str,
    nested: bool,
    qualified: bool,
) -> Result<Fixture, String> {
    fixture_with_shared_prefix(lib_prefix, main_prefix, nested, qualified, "")
}

fn fixture_with_shared_prefix(
    lib_prefix: &str,
    main_prefix: &str,
    nested: bool,
    qualified: bool,
    prefix: &str,
) -> Result<Fixture, String> {
    let target = if nested {
        "src/shared/nested.rs"
    } else {
        "src/shared.rs"
    };
    let assertion = if qualified {
        "std::assert_eq!(weight(4), 12);"
    } else {
        "assert_eq!(weight(4), 12);"
    };
    let source = format!(
        "{prefix}\npub fn weight(x: u32) -> u32 {{ 3 * x }}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn pin() {{\n{assertion}\n}}\n}}\n"
    );
    let mut files = vec![
        (
            "Cargo.toml",
            "[package]\nname=\"demo\"\nversion=\"0.0.0\"\nedition=\"2024\"\n".to_string(),
        ),
        ("src/lib.rs", format!("{lib_prefix}\nmod shared;\n")),
        (
            "src/main.rs",
            format!("{main_prefix}\nmod shared;\nfn main() {{}}\n"),
        ),
        (
            "src/bin/other.rs",
            "macro_rules! assert_eq { ($($t:tt)*) => {} }\nfn main() {}\n".to_string(),
        ),
        (target, source),
    ];
    if nested {
        files.push(("src/shared.rs", "mod nested;\n".to_string()));
    }
    let files: Vec<_> = files
        .iter()
        .map(|(name, source)| (*name, source.as_str()))
        .collect();
    fixture_from_files("shared-context", &files)
}

fn admission(index: &RustIndex) -> Result<bool, String> {
    let test = index
        .tests()
        .iter()
        .find(|t| t.name.ends_with("pin"))
        .ok_or("missing fixture test")?;
    let oracle = test.assertions.first().ok_or("missing exact assertion")?;
    assert_eq!(test.assertions.len(), 1);
    Ok(OwnerPinSyntax::default().admits(test, oracle, index))
}

const SHADOW: &str = "macro_rules! assert_eq { ($($t:tt)*) => {} }";

#[test]
fn binary_macro_boundaries_route_withheld_sources() -> Result<(), String> {
    let source = "fuzz_target!(|bytes: &[u8]| { assert_eq!(bytes, bytes); });";
    let fixture = fixture_from_files(
        "withheld-fuzz",
        &[
            ("Cargo.toml", "[package]\nname='demo'\nversion='0.0.0'\n"),
            ("src/lib.rs", WEIGHT_LIB),
            (
                "tests/pin.rs",
                "use demo::weight;\n#[test]\nfn pin() { assert_eq!(weight(4), 12); }\n",
            ),
            (
                "fuzz/Cargo.toml",
                "[package]\nname='fuzz'\nversion='0.0.0'\n[workspace]\n[[bin]]\nname='parse'\npath='parse.rs'\n",
            ),
            ("fuzz/parse.rs", source),
        ],
    )?;
    let mut index = fixture.index.clone();
    assert!(index.remove_file(Path::new("fuzz/parse.rs")));
    // Membership changes invalidate execution authority. Recompose the
    // retained source set before exercising narrowed-index admission.
    crate::analysis::facts::compose_fixture_source_roles(&mut index, &fixture.root);
    assert!(index.execution_context(Path::new("tests/pin.rs")).is_some());
    let mut withheld = WithheldMacroBindings::default();
    assert!(!withheld.absorb_with_members(
        Path::new("fuzz/parse.rs"),
        source,
        index.macro_scope_crates(),
        &index.drop_in_manifests,
        &index.member_crates,
    ));
    assert!(!withheld.trusted.contains("assert_eq"));
    assert!(
        withheld
            .by_root
            .get(Path::new("fuzz/parse.rs"))
            .is_some_and(|names| names.contains("assert_eq"))
    );
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "pin")
        .ok_or("missing test")?;
    assert_eq!(test.assertions.len(), 1);
    let syntax = OwnerPinSyntax::default().with_withheld(withheld);
    assert_eq!(syntax.refusal(test, &test.assertions[0], &index), None);
    Ok(())
}

#[test]
fn composed_binary_macro_sites_return_to_workspace_scope() {
    let source = "fuzz_target!(|bytes: &[u8]| { assert_eq!(bytes, bytes); });";
    let binary_path = Path::new("fuzz/parse.rs");
    let confined = || {
        let mut withheld = WithheldMacroBindings::default();
        withheld.confined_binaries.insert(binary_path.to_path_buf());
        withheld
            .by_root
            .entry(binary_path.to_path_buf())
            .or_default()
            .insert("assert_eq".to_string());
        let site = macro_binding_scan(
            "macro_rules! assert_eq { ($($t:tt)*) => {} }",
            &BTreeSet::new(),
            &assertion_context_macros(),
            &|_: &str| false,
        )
        .into_iter()
        .next()
        .map(|(_, site)| site);
        (withheld, site, source)
    };
    for (composed, widened) in [
        (&["other.rs"][..], false),
        (&["parse.rs"][..], true),
        (&["*"][..], true),
    ] {
        let (mut withheld, site, _) = confined();
        if let Some(site) = site {
            withheld.root_sites.insert(
                ("assert_eq".to_string(), binary_path.to_path_buf()),
                (binary_path.to_path_buf(), site),
            );
        }
        let names = composed.iter().map(|name| name.to_string()).collect();
        withheld.widen_composed(&names);
        assert_eq!(
            withheld.trusted.contains("assert_eq"),
            widened,
            "{composed:?}"
        );
        assert_eq!(withheld.by_root.contains_key(binary_path), !widened);
    }
}

#[test]
fn shared_assertion_requires_every_target_context() -> Result<(), String> {
    for (lib, main, admitted) in [("", "", true), (SHADOW, "", false), ("", SHADOW, false)] {
        let index = fixture(lib, main, false, false)?;
        let facts = index
            .files()
            .get(Path::new("src/shared.rs"))
            .ok_or("missing shared file")?;
        assert!(facts.role_provenance.earliest_unresolved_reason.is_none());
        assert_eq!(
            facts.role_provenance.edges.len(),
            1,
            "accepted role representation remains deterministic"
        );
        assert_eq!(admission(&index)?, admitted, "lib={lib}; main={main}");
    }
    Ok(())
}

#[test]
fn rejected_target_owners_never_grant_execution_context() -> Result<(), String> {
    let shared = Path::new("src/shared.rs");
    let approved = fixture("", "", false, false)?;
    assert_eq!(
        approved
            .execution_context(shared)
            .ok_or("missing approved target contexts")?
            .roots
            .len(),
        2,
        "standard distinct targets still establish both contexts"
    );
    for (prefix, has_include_limit) in [
        (
            "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));",
            true,
        ),
        ("#[path = \"main.rs\"] mod mounted_main;", false),
    ] {
        let index = fixture(prefix, "", false, false)?;
        assert_eq!(!index.include_limitations.is_empty(), has_include_limit);
        assert!(
            index
                .files()
                .get(shared)
                .ok_or("missing shared file")?
                .role_provenance
                .earliest_unresolved_reason
                .is_some(),
            "the existing target-root guard must refuse this placement"
        );
        assert!(
            !index
                .execution_module_parents
                .as_ref()
                .ok_or("missing composed context map")?
                .contains_key(shared),
            "rejected owners must be filtered before context capture"
        );
        assert!(index.execution_context(shared).is_none());
        assert!(!admission(&index)?);
    }
    Ok(())
}

#[test]
fn shared_descendants_keep_all_relevant_roots_and_ignore_other_targets() -> Result<(), String> {
    assert!(
        admission(&*fixture("", "", true, false)?)?,
        "unrelated target's private macro stays isolated"
    );
    assert!(
        !admission(&*fixture("", SHADOW, true, false)?)?,
        "the second root reaches a shared descendant"
    );
    let index = fixture("", "", false, false)?;
    let roots = TargetRoots::new(&index);
    assert!(
        roots.root(Path::new("src/shared.rs"), &index).is_none(),
        "multiple roots do not prove unique owner identity"
    );
    assert_eq!(
        roots.root(Path::new("src/main.rs"), &index),
        Some(PathBuf::from("src/main.rs"))
    );
    Ok(())
}

#[test]
fn qualified_shared_assertions_need_all_standard_root_contexts() -> Result<(), String> {
    assert!(
        admission(&*fixture("", SHADOW, false, true)?)?,
        "a bare macro shadow does not replace std::assert_eq"
    );
    assert!(
        !admission(&*fixture("", "mod std {}", false, true)?)?,
        "a root shadow in either context refuses the qualified invocation"
    );
    Ok(())
}

#[test]
fn deserialized_representative_provenance_cannot_grant_complete_context() -> Result<(), String> {
    let index = fixture("", "", false, false)?;
    assert!(admission(&index)?);
    assert!(admission(&index.clone())?);
    let json = serde_json::to_string(&*index).map_err(|e| e.to_string())?;
    let restored: RustIndex = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    assert!(
        !admission(&restored)?,
        "wire facts lack alternative execution-context authority until recomposed"
    );
    Ok(())
}

#[test]
fn possible_shadows_are_not_lost_when_unique_attribution_is_refused() -> Result<(), String> {
    let index = fixture_with_shared_prefix(
        "",
        "",
        false,
        false,
        "macro_rules! emit { ($name:ident) => {} } emit!(weight); mod std {}",
    )?;
    let syntax = OwnerPinSyntax::default();
    let roots = syntax.target_roots(&index);
    assert!(roots.root(Path::new("src/shared.rs"), &index).is_none());
    for root in ["src/lib.rs", "src/main.rs"] {
        assert!(syntax.crate_may_hide_name(&index, Path::new(root), "weight", roots));
        assert!(test_crate_may_bind(Path::new(root), "std", &index, roots));
    }
    assert!(!syntax.crate_may_hide_name(&index, Path::new("src/bin/other.rs"), "weight", roots));
    assert!(!test_crate_may_bind(
        Path::new("src/bin/other.rs"),
        "std",
        &index,
        roots
    ));
    Ok(())
}

#[test]
fn incomplete_execution_graph_cannot_become_a_partial_context() -> Result<(), String> {
    let fixture = fixture("", "", false, false)?;
    let mut index = fixture.index.clone();
    let context = index
        .execution_context(Path::new("src/shared.rs"))
        .ok_or("expected complete graph")?;
    assert_eq!(
        context.roots,
        BTreeSet::from([PathBuf::from("src/lib.rs"), PathBuf::from("src/main.rs")])
    );
    let parents = index
        .execution_module_parents
        .as_mut()
        .ok_or("missing graph")?;
    let edge = parents
        .get_mut(Path::new("src/shared.rs"))
        .and_then(|v| v.last_mut())
        .ok_or("missing alternate parent")?;
    edge.parent = PathBuf::from("src/not_indexed.rs");
    assert!(
        index
            .execution_context(Path::new("src/shared.rs"))
            .is_none()
    );
    assert!(!admission(&index)?);
    let roots = TargetRoots::new(&index);
    assert!(roots.may_belong(Path::new("src/shared.rs"), Path::new("src/main.rs"), &index));
    let mut index = fixture.index.clone();
    index.execution_module_parents = None;
    assert!(!admission(&index)?);
    assert!(
        TargetRoots::new(&index)
            .root(Path::new("src/lib.rs"), &index)
            .is_none()
    );
    Ok(())
}

#[test]
fn incomplete_placement_cannot_certify_only_the_recorded_roots() -> Result<(), String> {
    let unresolved = "#[cfg_attr(feature = \"alternate\", path = \"other.rs\")] mod unplaced;";
    let plain = fixture("", unresolved, false, false)?;
    assert!(
        OwnerPinSyntax::default()
            .resolved_module_declarations(&plain)
            .contains(&(PathBuf::from("src/lib.rs"), 2, "mod shared;".to_string())),
        "known declarations remain available even when unrelated placement is unknown"
    );
    let qualified = fixture("", unresolved, false, true)?;
    assert!(
        qualified
            .execution_context(Path::new("src/shared.rs"))
            .is_some(),
        "recorded ancestry is available"
    );
    assert!(
        TargetRoots::new(&qualified).disabled,
        "unread alternatives prevent complete placement"
    );
    assert!(
        !admission(&qualified)?,
        "qualified trust cannot certify a partial root set"
    );
    Ok(())
}

#[test]
fn shared_drop_in_imports_require_all_manifests_and_match_retained_sites() -> Result<(), String> {
    for (dependency, unknown, admitted) in [
        ("pretty_assertions = \"1\"", false, true),
        (
            "pretty_assertions = { package = \"fake-assertions\", version = \"1\" }",
            false,
            false,
        ),
        ("pretty_assertions = { path = \"../fake\" }", false, false),
        ("", false, false),
        ("pretty_assertions = \"1\"", true, false),
    ] {
        let a_manifest = "[package]\nname=\"a\"\nversion=\"0.0.0\"\n[dev-dependencies]\npretty_assertions=\"1\"\n";
        let b_manifest =
            format!("[package]\nname=\"b\"\nversion=\"0.0.0\"\n[dev-dependencies]\n{dependency}\n");
        let b_source = format!(
            "{}#[path=\"../../a/src/shared.rs\"] mod shared;\n",
            if unknown {
                "#[cfg_attr(feature=\"alternate\", path=\"other.rs\")] mod unplaced;\n"
            } else {
                ""
            }
        );
        let fixture = fixture_from_files(
            "shared-context-manifests",
            &[
                (
                    "Cargo.toml",
                    "[workspace]\nmembers=[\"a\",\"b\"]\nresolver=\"3\"\n",
                ),
                ("a/Cargo.toml", a_manifest),
                ("b/Cargo.toml", &b_manifest),
                ("a/src/lib.rs", "mod shared;\n"),
                ("b/src/lib.rs", &b_source),
                (
                    "a/src/shared.rs",
                    "use pretty_assertions::assert_eq;\npub fn weight(x: u32) -> u32 {\n3 * x\n}\n#[cfg(test)] mod tests { use super::*; #[test] fn pin() { assert_eq!(weight(4), 12); } }\n",
                ),
            ],
        )?;
        let index = &fixture.index;
        let shared = Path::new("a/src/shared.rs");
        assert!(
            index
                .drop_in_manifests
                .verified_root(shared, "pretty_assertions"),
            "the physical file alone always verifies"
        );
        let context = index
            .execution_context(shared)
            .ok_or("missing approved shared contexts")?;
        assert_eq!(
            context.roots,
            BTreeSet::from([PathBuf::from("a/src/lib.rs"), PathBuf::from("b/src/lib.rs")])
        );
        assert_eq!(
            admission(index)?,
            admitted,
            "dependency={dependency}; unknown={unknown}"
        );
        let syntax = OwnerPinSyntax::default();
        let test = index
            .tests()
            .iter()
            .find(|test| test.name.ends_with("pin"))
            .ok_or("missing pin")?;
        let function = index
            .functions()
            .iter()
            .find(|function| function.file == shared && function.name == "weight")
            .ok_or("missing shared owner")?;
        let probe = return_probe(function, "3 * x");
        let refusal = syntax.equality_assertion_refusal(&probe, test, &test.assertions[0], index);
        if admitted {
            assert_eq!(refusal, None);
        } else {
            assert!(
                matches!(&refusal, Some(AssertionRefusal::MacroBinding { site: Some((path, site)), .. })
                if path == shared && site.line == 1 && site.kind == MacroBindingKind::UnverifiedDropIn("pretty_assertions".into())),
                "{refusal:?}"
            );
        }
        let roots = syntax.target_roots(index);
        assert_eq!(roots.disabled, unknown);
        let modules = syntax.resolved_module_declarations(index);
        let resolved = |path: &Path, line: usize, declaration: &str| {
            modules.contains(&(path.to_path_buf(), line, declaration.to_string()))
        };
        for name in assertion_context_macros() {
            for root in [
                None,
                Some(Path::new("a/src/lib.rs")),
                Some(Path::new("b/src/lib.rs")),
            ] {
                assert_eq!(
                    syntax.workspace_macro_site(name, root, index),
                    workspace_macro_binding_site(name, root, index, roots, &resolved),
                    "{name}; {root:?}; {dependency}; {unknown}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn alternative_execution_ancestry_obeys_depth_and_cycle_bounds() -> Result<(), String> {
    for (depth, cycle, valid) in [(32, false, true), (33, false, false), (8, true, false)] {
        let fixture = fixture("", "", false, false)?;
        let mut index = fixture.index.clone();
        let shared = PathBuf::from("src/shared.rs");
        let terminal = PathBuf::from("src/lib.rs");
        let nodes: Vec<_> = (0..depth - 1)
            .map(|n| PathBuf::from(format!("src/chain{n}.rs")))
            .collect();
        for node in &nodes {
            index.insert_file_only(node.clone(), summarize_file(node.clone(), "".to_string()));
        }
        let edge = |child: &Path, parent: &Path| SourceRoleProvenanceEdge {
            kind: SourceRoleProvenanceEdgeKind::Module,
            parent: parent.to_path_buf(),
            child: child.to_path_buf(),
            declaration: "mod child;".to_string(),
            line: 1,
            requires_test: false,
        };
        // A valid shallow route runs first; a bad alternative must still fail.
        let mut parents = BTreeMap::from([(
            shared.clone(),
            vec![edge(&shared, &terminal), edge(&shared, &nodes[0])],
        )]);
        for (at, child) in nodes.iter().enumerate() {
            let parent = nodes
                .get(at + 1)
                .unwrap_or(if cycle { &shared } else { &terminal });
            parents.insert(child.clone(), vec![edge(child, parent)]);
        }
        index.execution_module_parents = Some(parents);
        assert_eq!(
            index.execution_context(&shared).is_some(),
            valid,
            "depth={depth}; cycle={cycle}"
        );
    }
    Ok(())
}

mod fixture_ownership {
    use super::*;

    fn parent() -> Result<Fixture, String> {
        let root = fixture_root("shared-context-ownership");
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Fixture {
            index: RustIndex::default(),
            root,
        })
    }

    #[test]
    fn fixture_root_collision_preserves_the_existing_owner() -> Result<(), String> {
        let parent = parent()?;
        let root = parent.root.join("occupied");
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let sentinel = root.join("sentinel");
        std::fs::write(&sentinel, b"existing owner").map_err(|error| error.to_string())?;
        let called = std::cell::Cell::new(false);
        let result = Fixture::try_at(root, |_| {
            called.set(true);
            Ok(RustIndex::default())
        });
        assert!(result.is_err(), "an occupied root must not be acquired");
        assert!(!called.get(), "setup must not run for a refused root");
        assert_eq!(
            std::fs::read(sentinel).map_err(|error| error.to_string())?,
            b"existing owner"
        );
        Ok(())
    }

    #[test]
    fn fixture_root_setup_failure_removes_only_the_acquired_child() -> Result<(), String> {
        let parent = parent()?;
        let root = parent.root.join("partial");
        let neighbor = parent.root.join("neighbor");
        std::fs::write(&neighbor, b"keep").map_err(|error| error.to_string())?;
        let result = Fixture::try_at(root.clone(), |root| {
            std::fs::write(root.join("partial"), b"written").map_err(|error| error.to_string())?;
            Err("injected setup failure".to_string())
        });
        assert_eq!(
            result.err().ok_or("setup unexpectedly succeeded")?,
            "injected setup failure"
        );
        assert!(!root.exists(), "partial setup must release its owned root");
        assert_eq!(
            std::fs::read(neighbor).map_err(|error| error.to_string())?,
            b"keep"
        );
        Ok(())
    }

    #[test]
    fn fixture_root_success_keeps_ownership_until_drop() -> Result<(), String> {
        let parent = parent()?;
        let root = parent.root.join("complete");
        let neighbor = parent.root.join("neighbor");
        std::fs::write(&neighbor, b"keep").map_err(|error| error.to_string())?;
        let fixture = Fixture::try_at(root.clone(), |root| {
            std::fs::write(root.join("ready"), b"complete").map_err(|error| error.to_string())?;
            Ok(RustIndex::default())
        })?;
        assert_eq!(
            std::fs::read(root.join("ready")).map_err(|error| error.to_string())?,
            b"complete"
        );
        drop(fixture);
        assert!(!root.exists(), "dropping the fixture must release its root");
        assert_eq!(
            std::fs::read(neighbor).map_err(|error| error.to_string())?,
            b"keep"
        );
        Ok(())
    }
}
