use super::*;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::domain::{ExposureClass, ProbeFamily};

#[test]
fn quickcheck_generator_closing_line_cannot_absorb_a_production_initializer() -> Result<(), String>
{
    let source = "#[derive(Clone, Debug)]\nstruct Pair(u8, u8);\nimpl quickcheck::Arbitrary for Pair {\n    fn arbitrary(_: &mut quickcheck::Gen) -> Self {\n        Pair(0, 0)\n    } } static PRODUCT: Pair = { let a = 1u8; Pair(a, a.wrapping_add(2)) };\n";
    for finding in findings(MANIFEST, source)? {
        assert!(
            !finding
                .evidence
                .iter()
                .any(|text| text.contains("test-input generator")),
            "{finding:?}"
        );
    }
    Ok(())
}

#[test]
fn quickcheck_generator_dependency_library_collision_is_not_provider_identity() -> Result<(), String>
{
    let manifest = MANIFEST.replace(
        "quickcheck='1'",
        "quickcheck={version='1',optional=true}\nproduction={path='production'}",
    );
    let other = "[package]\nname='production'\nversion='0.1.0'\n[lib]\nname='quickcheck'\n";
    for finding in findings_at(
        &manifest,
        SOURCE,
        "src/lib.rs",
        &[
            ("production/Cargo.toml", other),
            ("production/src/lib.rs", "pub trait Arbitrary {}"),
        ],
    )? {
        assert_eq!(finding.class, ExposureClass::NoStaticPath);
        assert_eq!(finding.probe.family, ProbeFamily::ReturnValue);
    }
    Ok(())
}

#[test]
fn quickcheck_generator_sharing_a_production_line_cannot_hide_its_gap() -> Result<(), String> {
    let source = "#[derive(Clone, Debug)]\nstruct Pair(u8, u8);\nfn production(a: u8) -> Pair { Pair(a, a.wrapping_add(2)) } impl quickcheck::Arbitrary for Pair { fn arbitrary(_: &mut quickcheck::Gen) -> Self { Pair(0, 0) } }\n";
    let findings = findings(MANIFEST, source)?;
    // Existing line-granular ownership can select the adjacent method. The
    // changed production expression must nevertheless retain its gap.
    assert!(
        findings.iter().any(
            |finding| finding.probe.expression == "Pair(a, a.wrapping_add(2))"
                && finding.probe.family == ProbeFamily::ReturnValue
                && finding.class == ExposureClass::NoStaticPath
        ),
        "{findings:?}"
    );
    assert!(findings.iter().all(|finding| {
        !finding
            .evidence
            .iter()
            .any(|text| text.contains("test-input generator"))
    }));
    Ok(())
}

const MANIFEST: &str = "[workspace]\n[package]\nname='stimulus'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nquickcheck='1'\n";
const SOURCE: &str = "use quickcheck::Arbitrary;\n#[derive(Clone, Debug)]\nstruct Pair(u8, u8);\nimpl quickcheck::Arbitrary for Pair {\n    fn arbitrary(g: &mut quickcheck::Gen) -> Self {\n        let a = u8::arbitrary(g);\n        Pair(a, a.wrapping_add(2))\n    }\n}\n";

fn findings(manifest: &str, source: &str) -> Result<Vec<crate::domain::Finding>, String> {
    findings_at(manifest, source, "src/lib.rs", &[])
}

fn findings_at(
    manifest: &str,
    source: &str,
    path: &str,
    extra: &[(&str, &str)],
) -> Result<Vec<crate::domain::Finding>, String> {
    findings_at_with_provider_admission(manifest, source, path, extra, true)
}

fn findings_at_with_provider_admission(
    manifest: &str,
    source: &str,
    path: &str,
    extra: &[(&str, &str)],
    expected_provider_admission: bool,
) -> Result<Vec<crate::domain::Finding>, String> {
    let mut files = vec![("Cargo.toml", manifest), (path, source)];
    files.extend_from_slice(extra);
    let root = drop_in::temp_workspace("quickcheck-generator", &files)?;
    let result = (|| {
        let path = PathBuf::from(path);
        let index = build_index(&root, std::slice::from_ref(&path))?;
        if manifest == MANIFEST
            && source == SOURCE
            && path.as_path() == std::path::Path::new("src/lib.rs")
            && extra.is_empty()
        {
            assert!(
                index.drop_in_manifests.verified(&path, "quickcheck"),
                "fixture dependency identity"
            );
            assert!(
                index.member_crates.is_default_target_root(&path),
                "fixture target identity"
            );
            assert!(
                crate::analysis::workspace::config_snapshot_identity(&root).is_some(),
                "fixture ambient configuration"
            );
            assert_eq!(
                quickcheck_generator::verified_files(&index).contains(&path),
                expected_provider_admission,
                "provider admission; functions: {:?}",
                index
                    .functions()
                    .iter()
                    .map(|f| (&f.item.container, f.source_role))
                    .collect::<Vec<_>>()
            );
        }
        let line = source
            .lines()
            .position(|line| line.contains("Pair(a, a.wrapping_add(2))"))
            .ok_or("missing changed stimulus")?
            + 1;
        let changed = ChangedFile {
            path,
            added_lines: vec![ChangedLine {
                line,
                new_side_line: line,
                text: source.lines().nth(line - 1).ok_or("changed line")?.into(),
            }],
            removed_lines: vec![ChangedLine {
                line,
                new_side_line: line,
                text: source
                    .lines()
                    .nth(line - 1)
                    .ok_or("changed line")?
                    .replace("wrapping_add(2)", "wrapping_add(1)"),
            }],
        };
        let probes =
            crate::analysis::probes::probes_for_file_with_relations(&root, &changed, &index, &[]);
        assert!(!probes.is_empty(), "changed generator must remain visible");
        assert!(
            index.tests().is_empty(),
            "generator is not an executable test"
        );
        Ok(probes
            .iter()
            .map(|seed| {
                crate::analysis::classifier::classify_probe(&seed.probe, &index, true, None)
            })
            .collect())
    })();
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    result
}

#[test]
fn quickcheck_generator_change_is_stimulus_unknown() -> Result<(), String> {
    crate::testing::compiler_environment::plain(
        module_path!(),
        "quickcheck_generator_change_is_stimulus_unknown",
        || {
            for source in [
                SOURCE.to_string(),
                SOURCE.replace("impl quickcheck::", "impl ::quickcheck::"),
            ] {
                let findings = findings(MANIFEST, &source)?;
                assert_eq!(findings.len(), 1, "one changed generator return");
                for finding in findings {
                    assert!(
                        finding
                            .probe
                            .owner
                            .as_ref()
                            .is_some_and(|owner| owner.0.ends_with("::arbitrary"))
                    );
                    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
                    assert_eq!(finding.probe.family, ProbeFamily::StaticUnknown);
                    assert!(
                        finding
                            .evidence
                            .iter()
                            .any(|text| {
                                text.contains("test-input generator") && text.contains("stimulus")
                            })
                    );
                    assert!(finding.canonical_gap.is_none());
                }
            }
            Ok(())
        },
    )
}

#[test]
fn quickcheck_generator_nonempty_compiler_wrapper_refuses_role() -> Result<(), String> {
    crate::testing::compiler_environment::untrusted_wrapper(
        module_path!(),
        "quickcheck_generator_nonempty_compiler_wrapper_refuses_role",
        "quickcheck_generator_change_is_stimulus_unknown",
        || {
            let findings =
                findings_at_with_provider_admission(MANIFEST, SOURCE, "src/lib.rs", &[], false)?;
            assert_eq!(findings.len(), 1, "one changed return remains a product subject");
            for finding in findings {
                assert_eq!(finding.class, ExposureClass::NoStaticPath);
                assert_eq!(finding.probe.family, ProbeFamily::ReturnValue);
                assert!(
                    !finding
                        .evidence
                        .iter()
                        .any(|text| text.contains("test-input generator")),
                    "an untrusted compiler wrapper must not establish provider identity"
                );
            }
            Ok(())
        },
    )
}

#[test]
fn quickcheck_generator_lookalikes_remain_production() -> Result<(), String> {
    let sources = [
        SOURCE.replace("quickcheck::Arbitrary", "production::Arbitrary"),
        SOURCE.replace("quickcheck::Arbitrary", "Arbitrary"),
        SOURCE.replace("impl quickcheck::Arbitrary for Pair", "impl Pair"),
        format!("mod quickcheck {{ pub trait Arbitrary {{}} }}\n{SOURCE}"),
        format!("use production as quickcheck;\n{SOURCE}"),
        format!("use production::*;\n{SOURCE}"),
        format!("extern crate production as quickcheck;\n{SOURCE}"),
    ];
    for source in sources {
        for finding in findings(MANIFEST, &source)? {
            assert_eq!(
                finding.class,
                ExposureClass::NoStaticPath,
                "{source}: {finding:?}"
            );
            assert_eq!(finding.probe.family, ProbeFamily::ReturnValue);
        }
    }
    Ok(())
}

#[test]
fn quickcheck_generator_unverified_dependency_is_not_reclassified() -> Result<(), String> {
    for dependency in [
        "quickcheck={package='production',version='1'}",
        "quickcheck={path='../production'}",
        "",
        "quickcheck='1'\n[patch.crates-io]\nquickcheck={path='../production'}",
    ] {
        let manifest = MANIFEST.replace("quickcheck='1'", dependency);
        for finding in findings(&manifest, SOURCE)? {
            assert_eq!(
                finding.class,
                ExposureClass::NoStaticPath,
                "{dependency}: {finding:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn quickcheck_generator_shrink_change_is_stimulus_unknown() -> Result<(), String> {
    crate::testing::compiler_environment::plain(
        module_path!(),
        "quickcheck_generator_shrink_change_is_stimulus_unknown",
        || {
            let source = SOURCE
                .replace(
                    "fn arbitrary(g: &mut quickcheck::Gen) -> Self",
                    "fn shrink(&self) -> Box<dyn Iterator<Item = Self>>",
                )
                .replace("let a = u8::arbitrary(g);", "let a = self.0;")
                .replace(
                    "Pair(a, a.wrapping_add(2))",
                    "Box::new(std::iter::once(Pair(a, a.wrapping_add(2))))",
                );
            let source = source.replace("impl quickcheck::Arbitrary for Pair {", "impl quickcheck::Arbitrary for Pair {\n    fn arbitrary(_: &mut quickcheck::Gen) -> Self { Pair(0, 0) }");
            for finding in findings(MANIFEST, &source)? {
                assert_eq!(finding.class, ExposureClass::StaticUnknown);
                assert_eq!(finding.probe.family, ProbeFamily::StaticUnknown);
            }
            Ok(())
        },
    )
}

#[test]
fn quickcheck_generator_nested_helper_remains_production() -> Result<(), String> {
    let source = SOURCE.replace("        Pair(a, a.wrapping_add(2))", "        fn arbitrary(a: u8) -> Pair {\n            Pair(a, a.wrapping_add(2))\n        }\n        arbitrary(a)");
    for finding in findings(MANIFEST, &source)? {
        assert_ne!(finding.probe.family, ProbeFamily::StaticUnknown);
        assert!(
            !finding
                .evidence
                .iter()
                .any(|text| text.contains("test-input generator"))
        );
    }
    Ok(())
}

#[test]
fn quickcheck_generator_unsupported_target_identity_is_not_reclassified() -> Result<(), String> {
    for manifest in [
        MANIFEST.replace("name='stimulus'", "name='quickcheck'"),
        format!("{MANIFEST}\n[lib]\nname='quickcheck'\n"),
        format!("{MANIFEST}\n[lib]\npath='src/lib.rs'\n"),
        MANIFEST.replace("edition='2024'", "edition='2015'"),
        MANIFEST.replace("[workspace]", "[workspace]\nmembers=['other']"),
        format!("{MANIFEST}\n[[bin]]\nname='other'\npath='src/lib.rs'\n"),
    ] {
        for finding in findings(&manifest, SOURCE)? {
            assert_ne!(
                finding.probe.family,
                ProbeFamily::StaticUnknown,
                "{manifest}"
            );
        }
    }
    for path in ["src/helper.rs", "other/src/lib.rs"] {
        for finding in findings_at(MANIFEST, SOURCE, path, &[])? {
            assert_ne!(finding.probe.family, ProbeFamily::StaticUnknown, "{path}");
        }
    }
    Ok(())
}

#[test]
fn quickcheck_generator_opaque_expansions_and_compiler_overrides_refuse_role() -> Result<(), String>
{
    crate::testing::compiler_environment::plain(
        module_path!(),
        "quickcheck_generator_opaque_expansions_and_compiler_overrides_refuse_role",
        || {
            for source in [
                format!("make_provider!();\n{SOURCE}"),
                format!("#[provider]\n{SOURCE}"),
                format!("macro_rules! provider {{ () => {{}} }}\n{SOURCE}"),
                format!("use other::Clone;\n{SOURCE}"),
                format!("use other::quickcheck;\n{SOURCE}"),
            ] {
                for finding in findings(MANIFEST, &source)? {
                    assert_ne!(finding.probe.family, ProbeFamily::StaticUnknown);
                }
            }
            for config in [
                "[build]\nrustflags=['--extern=quickcheck=other.rlib']\n",
                "[source.crates-io]\nreplace-with='local'\n[source.local]\ndirectory='other'\n",
            ] {
                for finding in findings_at(
                    MANIFEST,
                    SOURCE,
                    "src/lib.rs",
                    &[(".cargo/config.toml", config)],
                )? {
                    assert_ne!(finding.probe.family, ProbeFamily::StaticUnknown);
                }
            }
            Ok(())
        },
    )
}
