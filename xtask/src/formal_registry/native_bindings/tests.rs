use super::*;
use crate::formal_registry::{ClaimDocument, OutputFormat, decision_digest, project};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SUBJECT_FILE: &str = "crates/ripr-contract/src/proof_a.rs";
const SUBJECT_SOURCE: &str = "#[kani::proof]\nfn proof_a() {\n    kani::cover!(true);\n    kani::cover!(false);\n}\nconst DATA: &str = include_str!(\"../../../data/input.txt\");\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> RegistryResult<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "formal-native-bindings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        for directory in [
            ".cargo",
            "xtask/src",
            "tools/repo-policy",
            "policy/formal-claims.d",
            "data",
        ] {
            fs::create_dir_all(fixture.0.join(directory)).map_err(|error| error.to_string())?;
        }
        for file in COMMON_FILES {
            fixture.write(
                file,
                if file.ends_with(".rs") {
                    "// fixture source\n"
                } else {
                    "# fixture configuration\n"
                },
            )?;
        }
        fixture.write(
            "crates/ripr-contract/Cargo.toml",
            "[package]\nname = 'ripr-contract'\nversion = '0.1.0'\n",
        )?;
        fixture.write(
            "crates/ripr-contract/src/lib.rs",
            "mod proof_a;\nmod proof_b;\n",
        )?;
        fixture.write(SUBJECT_FILE, SUBJECT_SOURCE)?;
        fixture.write(
            "crates/ripr-contract/src/proof_b.rs",
            "#[kani::proof]\nfn proof_b() { kani::cover!(true); kani::cover!(false); }\n",
        )?;
        fixture.write(
            "data/input.txt",
            "a literal input outside ordinary Rust roots\n",
        )?;
        fixture.write(
            "docs/specs/RIPR-SPEC-0371-fixture.md",
            "# Synthetic fixture specification\n",
        )?;
        fixture.write("docs/authority.md", "# Synthetic authority\n")?;
        fixture.write_registry()?;
        Ok(fixture)
    }

    fn write(&self, path: &str, contents: &str) -> RegistryResult<()> {
        let path = self.0.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, contents).map_err(|error| error.to_string())
    }

    fn write_registry(&self) -> RegistryResult<()> {
        let mut entries = Vec::new();
        for name in ["a", "b"] {
            let claim: ClaimDocument = load::parse_unique(json!({
                "schema": "ripr.formal-claim.v1", "claim_id": format!("example.{name}"),
                "claim_generation": 1, "family": "bounded_model_check", "provider": "kani",
                "criticality": "critical", "phase": "admitted_merge_group",
                "proposition": "A synthetic predicate preserves its two cases.",
                "non_claims": ["No production assurance."], "owners": ["fixture::predicate"],
                "source_closure": [format!("crates/ripr-contract/src/proof_{name}.rs")],
                "domain": "two synthetic values", "assumptions": [],
                "trust_dependencies": ["synthetic source correspondence"], "observable": "boolean",
                "bounds": {"states": 2}, "subjects": [format!("proof_{name}")],
                "required_covers": ["left", "right"], "negative_controls": ["invert the predicate"],
                "result_satisfaction": "one current verified row", "currentness_inputs": ["source and configuration"],
                "transition_policy": "base_and_head", "owner_spec": "RIPR-SPEC-0371", "owner_issue": 14473
            }).to_string().as_bytes())?;
            let document = format!("policy/formal-claims.d/{}.json", claim.claim_id);
            self.write(
                &document,
                &serde_json::to_string_pretty(&claim).map_err(|error| error.to_string())?,
            )?;
            let authorities = vec!["docs/specs/RIPR-SPEC-0371-fixture.md".to_string()];
            entries.push(json!({
                "document": document, "claim_id": claim.claim_id, "claim_generation": 1,
                "route": "native", "authority_paths": authorities,
                "expected_decision_digest": decision_digest(&claim, &document, ProviderRoute::Native, &authorities)?
            }));
        }
        self.write(
            "policy/formal-claims.json",
            &json!({
                "schema": "ripr.formal-claim-registry.v1", "repository": "example/repository",
                "registry_generation": 1, "global_authority_paths": ["docs/authority.md"],
                "claims": entries, "retirements": []
            })
            .to_string(),
        )
    }

    fn view(&self) -> RegistryResult<ValidatedRegistry> {
        ValidatedRegistry::load(&self.0)
    }

    fn bindings(&self, view: &ValidatedRegistry) -> RegistryResult<BindingInput> {
        let claim = view
            .claims()
            .find(|claim| claim.id() == "example.a")
            .ok_or("fixture canonical claim absent")?;
        Ok(BindingInput {
            schema: "ripr.native-bindings.v1".to_string(),
            registry_generation: view.generation().to_string(),
            bindings: vec![ClaimBinding {
                claim_id: claim.id().to_string(),
                claim_generation: claim.generation(),
                decision_digest: claim.decision_digest().to_string(),
                profile: native::DeclaredKaniProfile::pinned(),
                subjects: vec![SubjectBinding {
                    canonical_subject: "proof_a".to_string(),
                    package: "ripr-contract".to_string(),
                    file: SUBJECT_FILE.to_string(),
                    qualified_name: "proof_a::proof_a".to_string(),
                }],
                covers: [("left", "true", 3u64, 1u64), ("right", "false", 4, 2)]
                    .into_iter()
                    .map(|(obligation, condition, line, number)| CoverBinding {
                        obligation: obligation.to_string(),
                        canonical_subject: "proof_a".to_string(),
                        file: SUBJECT_FILE.to_string(),
                        anchor: format!("kani::cover!({condition});"),
                        presentation: Some(NativePresentation {
                            property: format!("proof_a::proof_a.cover.{number}"),
                            description: format!("\"{condition}\""),
                            file: SUBJECT_FILE.to_string(),
                            function: "proof_a::proof_a".to_string(),
                            line,
                            column: 5,
                        }),
                    })
                    .collect(),
                correspondence: "Synthetic source correspondence only".to_string(),
                limitations: vec!["Supplied metadata is not native execution".to_string()],
            }],
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn refuses<T>(outcome: RegistryResult<T>, fragment: &str) -> RegistryResult<()> {
    match outcome {
        Ok(_) => Err(format!(
            "expected native binding refusal containing {fragment}"
        )),
        Err(error) => {
            assert!(
                !fragment.is_empty() && error.contains(fragment),
                "unexpected refusal: {error}"
            );
            Ok(())
        }
    }
}

fn document(prepared: &PreparedNativeInputs) -> RegistryResult<Value> {
    serde_json::from_str(&prepared.to_json()?).map_err(|error| error.to_string())
}

#[test]
fn canonical_snapshot_and_missing_bindings_keep_the_full_denominator() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    assert_eq!(view.claims().count(), 2);
    let claim = view.claims().next().ok_or("fixture claims empty")?;
    assert_eq!(claim.document()?.subjects, ["proof_a"]);
    assert_eq!(claim.route(), "native");
    assert_eq!(
        view.index_json()?,
        project::render_index(&view.loaded, OutputFormat::Json)?
    );
    let prepared = view.prepare_native(None)?;
    let report = document(&prepared)?;
    assert_eq!(
        report["claims"]
            .as_array()
            .ok_or("claim rows absent")?
            .len(),
        2
    );
    for row in report["claims"].as_array().ok_or("claim rows absent")? {
        assert_eq!(row["binding_state"], "missing");
        assert_eq!(row["required_covers"], json!(["left", "right"]));
        assert_eq!(row["negative_controls"], json!(["invert the predicate"]));
        assert_eq!(
            row["missing_obligations"]
                .as_array()
                .ok_or("missing obligations absent")?
                .len(),
            4
        );
    }
    assert_eq!(
        report["canonical_scoreboard"]["summary"]["registered_claims"],
        2
    );
    assert_eq!(
        report["canonical_scoreboard"]["summary"]["not_established"],
        2
    );
    assert_eq!(report["native_origin"], "not_established");
    assert_eq!(report["execution_authority"], "not_granted");
    assert_eq!(view.input_digests().len(), 3);
    Ok(())
}

#[test]
fn prepared_binding_uses_real_source_bytes_and_keeps_canonical_subjects() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    let input = fixture.bindings(&view)?;
    let prepared = view.prepare_native(Some(&input))?;
    let row = &prepared.claims[0];
    assert_eq!(row.binding_state, "prepared");
    assert_eq!(row.canonical_subjects, ["proof_a"]);
    assert_eq!(
        row.subjects[0].native.qualified_name.as_deref(),
        Some("proof_a::proof_a")
    );
    assert_eq!(row.subjects[0].effective_unwind, 32);
    assert_eq!(
        row.source_manifest.get(SUBJECT_FILE),
        Some(&native::sha256(SUBJECT_SOURCE.as_bytes()))
    );
    assert!(row.explicit_inputs.contains(&"data/input.txt".to_string()));
    assert!(row.source_manifest.contains_key("data/input.txt"));
    assert!(
        row.source_manifest
            .contains_key("policy/formal-claims.json")
    );
    assert!(
        row.source_manifest
            .contains_key("policy/formal-claims.d/example.b.json")
    );
    assert!(
        row.source_manifest
            .contains_key("crates/ripr-contract/src/proof_b.rs")
    );
    let report = document(&prepared)?;
    assert_eq!(report["canonical_scoreboard"]["summary"]["established"], 0);
    assert_eq!(
        report["canonical_scoreboard"]["summary"]["not_established"],
        2
    );
    assert_eq!(prepared.registry().generation(), view.generation());
    assert_eq!(
        prepared.to_json()?,
        view.prepare_native(Some(&input))?.to_json()?
    );
    Ok(())
}

#[test]
fn exact_canonical_keys_refuse_aliases_stale_generations_and_duplicate_bindings()
-> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    for mutation in 0..5 {
        let mut input = fixture.bindings(&view)?;
        let expected = match mutation {
            0 => {
                input.registry_generation.push('x');
                "registry generation mismatch"
            }
            1 => {
                input.bindings[0].claim_id = "aggregate.old-claim".to_string();
                "unknown canonical claim"
            }
            2 => {
                input.bindings[0].claim_generation += 1;
                "claim generation or decision digest mismatch"
            }
            3 => {
                input.bindings[0].decision_digest = native::sha256(b"a different decision");
                "claim generation or decision digest mismatch"
            }
            _ => {
                input.bindings.push(input.bindings[0].clone());
                "duplicate canonical claim binding"
            }
        };
        refuses(view.prepare_native(Some(&input)), expected)?;
    }
    Ok(())
}

#[test]
fn provider_profile_and_exact_canonical_subject_inventory_are_required() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    for mutation in 0..6 {
        let mut input = fixture.bindings(&view)?;
        let binding = &mut input.bindings[0];
        let expected = match mutation {
            0 => {
                binding.profile.kani_version = "next".to_string();
                "pinned ordinary Kani profile"
            }
            1 => {
                binding.profile.default_unwind = 16;
                "pinned ordinary Kani profile"
            }
            2 => {
                binding.subjects.clear();
                "native subject inventory differs"
            }
            3 => {
                binding.subjects[0].file = "crates/ripr-contract/src/proof_b.rs".to_string();
                "outside canonical source closure"
            }
            4 => {
                binding.subjects[0].qualified_name = "other::proof_b".to_string();
                "differs from canonical subject name"
            }
            _ => {
                binding.subjects.push(binding.subjects[0].clone());
                "duplicate canonical subject binding"
            }
        };
        refuses(view.prepare_native(Some(&input)), expected)?;
    }
    Ok(())
}

#[test]
fn rust_only_profile_is_distinct_and_does_not_borrow_default_assurance() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    let mut input = fixture.bindings(&view)?;
    let ordinary = view.prepare_native(Some(&input))?;
    input.bindings[0].profile = native::DeclaredKaniProfile::rust_only();
    let rust_only = view.prepare_native(Some(&input))?;
    assert_ne!(ordinary.bindings_digest, rust_only.bindings_digest);
    assert_eq!(
        document(&rust_only)?["canonical_scoreboard"]["summary"]["established"],
        0
    );
    assert_eq!(document(&rust_only)?["native_origin"], "not_established");
    for mutation in 0..5 {
        let mut profile = native::DeclaredKaniProfile::rust_only();
        match mutation {
            0 => profile.configuration_policy = "kani-0.68-defaults-v1".to_string(),
            1 => profile.default_features = true,
            2 => profile.all_features = true,
            3 => profile.lib = false,
            _ => profile.features.push("lang-python".to_string()),
        }
        refuses(profile.validate(), "pinned ordinary Kani profile")?;
    }
    Ok(())
}

#[test]
fn duplicate_or_foreign_cover_mappings_cannot_reuse_a_witness() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    for mutation in 0..4 {
        let mut input = fixture.bindings(&view)?;
        let covers = &mut input.bindings[0].covers;
        let expected = match mutation {
            0 => {
                covers[1].obligation = "left".to_string();
                "duplicate canonical cover binding"
            }
            1 => {
                covers[1].obligation = "constructor-not-declared-by-this-generation".to_string();
                "unknown canonical cover obligation"
            }
            2 => {
                covers[1]
                    .presentation
                    .as_mut()
                    .ok_or("fixture presentation absent")?
                    .property = "proof_a::proof_a.cover.1".to_string();
                "one native property cannot satisfy two"
            }
            _ => {
                let mut presentation = covers[0]
                    .presentation
                    .clone()
                    .ok_or("fixture presentation absent")?;
                presentation.property = "proof_a::proof_a.cover.2".to_string();
                covers[1].presentation = Some(presentation);
                "duplicate native cover presentation"
            }
        };
        refuses(view.prepare_native(Some(&input)), expected)?;
    }
    Ok(())
}

#[test]
fn missing_presentations_remain_explicit_without_shrinking_required_covers() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    for omitted_mapping in [false, true] {
        let mut input = fixture.bindings(&view)?;
        if omitted_mapping {
            input.bindings[0].covers.pop();
        } else {
            input.bindings[0].covers[1].presentation = None;
        }
        let prepared = view.prepare_native(Some(&input))?;
        assert_eq!(prepared.claims[0].binding_state, "incomplete");
        assert_eq!(prepared.claims[0].required_covers, ["left", "right"]);
        assert_eq!(
            prepared.claims[0].missing_obligations,
            ["native cover presentation: right"]
        );
        assert_eq!(
            document(&prepared)?["canonical_scoreboard"]["summary"]["established"],
            0
        );
    }
    Ok(())
}

#[test]
fn post_validation_registry_byte_changes_are_refused_even_if_semantically_equal()
-> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    let path = fixture.0.join("policy/formal-claims.json");
    let mut bytes = fs::read(&path).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| error.to_string())?;
    refuses(
        view.prepare_native(None),
        "canonical registry bytes changed after validation",
    )?;
    assert_eq!(fixture.view()?.generation(), view.generation());
    Ok(())
}

#[test]
fn post_validation_unlisted_claim_cannot_evade_canonical_membership() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    fixture.write("policy/formal-claims.d/unlisted.json", "{}")?;
    refuses(view.prepare_native(None), "registry membership differs")?;
    refuses(fixture.view(), "registry membership differs")?;
    Ok(())
}

#[test]
fn source_refusals_use_real_harness_configuration_and_include_inputs() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    let input = fixture.bindings(&view)?;
    fixture.write(
        SUBJECT_FILE,
        &SUBJECT_SOURCE.replace("#[kani::proof]", "#[kani::proof_for_contract(predicate)]"),
    )?;
    refuses(
        view.prepare_native(Some(&input)),
        "unsupported transforming or verifier attribute",
    )?;
    fixture.write(
        SUBJECT_FILE,
        &SUBJECT_SOURCE.replace(
            "include_str!(\"../../../data/input.txt\")",
            "include_str!(concat!(\"../../../data/\", \"input.txt\"))",
        ),
    )?;
    refuses(view.prepare_native(Some(&input)), "literal")?;
    fixture.write(SUBJECT_FILE, SUBJECT_SOURCE)?;
    fs::remove_file(fixture.0.join("data/input.txt")).map_err(|error| error.to_string())?;
    refuses(
        view.prepare_native(Some(&input)),
        "native source input closure",
    )?;
    Ok(())
}

#[test]
fn cover_source_anchors_and_native_locations_must_exist() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    for mutation in 0..3 {
        let mut input = fixture.bindings(&view)?;
        let cover = &mut input.bindings[0].covers[0];
        let expected = match mutation {
            0 => {
                cover.anchor = "kani::cover!(absent);".to_string();
                "anchor is missing or ambiguous"
            }
            1 => {
                cover
                    .presentation
                    .as_mut()
                    .ok_or("fixture presentation absent")?
                    .line = 999;
                "line is outside captured source"
            }
            _ => {
                cover
                    .presentation
                    .as_mut()
                    .ok_or("fixture presentation absent")?
                    .column = 999;
                "column is outside captured source"
            }
        };
        refuses(view.prepare_native(Some(&input)), expected)?;
    }
    Ok(())
}

#[test]
fn strict_binding_json_rejects_duplicate_keys_and_native_origin_assertions() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    let input = fixture.bindings(&view)?;
    let bytes = serde_json::to_vec(&input).map_err(|error| error.to_string())?;
    let decoded = BindingInput::parse(&bytes)?;
    assert_eq!(decoded.bindings.len(), 1);
    let mut value = serde_json::to_value(input).map_err(|error| error.to_string())?;
    value["native_captured"] = json!(true);
    refuses(
        BindingInput::parse(value.to_string().as_bytes()),
        "unknown field",
    )?;
    refuses(
        BindingInput::parse(br#"{"schema":"one","schema":"two"}"#),
        "duplicate JSON key",
    )?;
    Ok(())
}

#[test]
fn cli_projects_all_canonical_claims_and_refuses_unknown_arguments() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let args = vec![
        "prepare-native".to_string(),
        "--root".to_string(),
        fixture.0.to_string_lossy().to_string(),
    ];
    let report: Value = serde_json::from_str(&crate::formal_registry::execute(&args)?)
        .map_err(|error| error.to_string())?;
    assert_eq!(
        report["canonical_scoreboard"]["summary"]["registered_claims"],
        2
    );
    refuses(execute(&["--root".to_string()]), "requires a value")?;
    refuses(
        execute(&["--native-captured".to_string(), "true".to_string()]),
        "unknown native preparation argument",
    )?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn source_symlink_is_refused_before_hashing() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let view = fixture.view()?;
    let input = fixture.bindings(&view)?;
    let path = fixture.0.join(SUBJECT_FILE);
    fs::remove_file(&path).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink("proof_b.rs", path).map_err(|error| error.to_string())?;
    refuses(
        view.prepare_native(Some(&input)),
        "regular non-symlink file",
    )?;
    Ok(())
}
