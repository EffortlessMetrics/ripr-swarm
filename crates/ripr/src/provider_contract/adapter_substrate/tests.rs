use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use super::*;

const HASH: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn packaging() -> SubstratePackagingImplicationsV1 {
    SubstratePackagingImplicationsV1 {
        license_spdx: "MIT".into(),
        msrv: "1.95".into(),
        unsafe_surface: "forbidden".into(),
        build_script: "none".into(),
        generated_code: "none".into(),
        native_runtime: "none".into(),
        platform_limits: vec!["all-tier1".into()],
        compile_size_class: "small".into(),
        binary_size_class: "small".into(),
        distribution_notes: "statically linked parser".into(),
    }
}

fn input_behavior() -> SubstrateInputBehaviorProfileV1 {
    SubstrateInputBehaviorProfileV1 {
        exact_span: "retain_exact_spans".into(),
        malformed_partial_input: "typed_limit".into(),
        source_map: "preserve_when_present".into(),
        generated_code: "mark_generated_nodes".into(),
        unavailable_tool: "typed_unavailable".into(),
    }
}

fn adr_ref() -> AdapterSubstrateAuthorityRefV1 {
    AdapterSubstrateAuthorityRefV1 {
        ref_kind: AdapterSubstrateAuthorityRefKindV1::AdrDocument,
        ref_id: "adr/0009-python-parser-substrate".into(),
        ref_generation: "accepted.v3".into(),
        currentness: AuthorityCurrentnessV1::Current,
    }
}

fn safety_ref() -> AdapterSubstrateSafetyRefV1 {
    AdapterSubstrateSafetyRefV1 {
        receipt_id: "safety/no-exec.v1".into(),
        harness_generation: "6512.v1".into(),
        no_execution: true,
        no_network: true,
        root_containment: true,
        process_tree_bounded: true,
        currentness: AuthorityCurrentnessV1::Current,
    }
}

fn syntax_layer(layer_id: &str) -> AdapterSubstrateLayerV1 {
    AdapterSubstrateLayerV1 {
        layer_id: layer_id.into(),
        layer_class: AdapterSubstrateLayerClassV1::SyntaxOnly,
        claim_ceiling: SubstrateClaimCeilingV1 {
            ceiling: SemanticClaimCeilingV1::SyntaxFactsOnly,
            message: "syntax facts only".into(),
        },
        dependency_ids: vec!["dep.primary".into()],
    }
}

fn dependency(id: &str, version: &str, grammar: &str) -> AdapterSubstrateDependencyRefV1 {
    AdapterSubstrateDependencyRefV1 {
        dependency_id: id.into(),
        crate_or_tool: "example-parser".into(),
        exact_version: version.into(),
        grammar_or_schema_generation: grammar.into(),
    }
}

fn base_qualification(
    qualification_id: &str,
    language: &str,
    substrate_class: AdapterSubstrateClassV1,
) -> AdapterSubstrateQualificationV1 {
    AdapterSubstrateQualificationV1 {
        schema_version: RIPR_ADAPTER_SUBSTRATE_QUALIFICATION_SCHEMA_VERSION.into(),
        qualification_id: qualification_id.into(),
        controller: "ripr".into(),
        language: language.into(),
        dialect: None,
        framework: None,
        platform: "host".into(),
        analysis_modes: vec!["diff".into()],
        substrate_class,
        shared_adapter_family_id: None,
        layers: vec![syntax_layer("syntax")],
        dependencies: vec![dependency("dep.primary", "1.0.0", "grammar.v1")],
        adr_ref: adr_ref(),
        producer_owner: "analysis::language::python".into(),
        source_input_identity: HASH.into(),
        project_configuration_identity: HASH.into(),
        platform_identity: HASH.into(),
        currentness_generation: "currentness.v1".into(),
        input_behavior: input_behavior(),
        packaging: packaging(),
        safety_refs: vec![safety_ref()],
        package_inclusion_disposition: PackageInclusionDispositionV1::OptionalFeature,
        claim_ceiling: SubstrateClaimCeilingV1 {
            ceiling: SemanticClaimCeilingV1::SyntaxFactsOnly,
            message: "syntax-only substrate".into(),
        },
        required_fallback: None,
        limitations: vec![],
        non_claims: vec![SubstrateNonClaimV1 {
            claim: "analyzer_correctness".into(),
            reason: "substrate qualification does not establish analyzer semantics".into(),
        }],
        semantic_digest: None,
        supersession_history: vec![],
    }
}

#[derive(Deserialize)]
struct CorpusFile {
    cases: Vec<CorpusCase>,
}

#[derive(Deserialize)]
struct CorpusCase {
    id: String,
    expect: String,
    #[serde(default)]
    error_code: Option<String>,
    qualification: AdapterSubstrateQualificationV1,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/adapter-substrate-qualification-corpus/corpus.json")
}

#[test]
fn corpus_cases_cover_substrate_acceptance_matrix() -> Result<(), String> {
    let text =
        std::fs::read_to_string(corpus_path()).map_err(|error| format!("read corpus: {error}"))?;
    let corpus: CorpusFile =
        serde_json::from_str(&text).map_err(|error| format!("parse corpus: {error}"))?;
    assert!(
        corpus.cases.len() >= 10,
        "expected substrate acceptance/rejection matrix cases"
    );
    let matrix = corpus
        .cases
        .iter()
        .find(|case| case.id == "matrix_all_substrate_classes")
        .ok_or_else(|| "missing matrix_all_substrate_classes corpus case".to_string())?;
    // Count alone passes when one class layer id is swapped for another unique id.
    let expected_layers: std::collections::BTreeSet<String> = AdapterSubstrateClassV1::ALL
        .iter()
        .map(|class| format!("class.{}", class.as_str()))
        .collect();
    let actual_layers: std::collections::BTreeSet<String> = matrix
        .qualification
        .layers
        .iter()
        .map(|layer| layer.layer_id.clone())
        .collect();
    assert_eq!(
        actual_layers, expected_layers,
        "matrix case must carry exactly one layer per substrate class"
    );
    assert_eq!(
        matrix.qualification.layers.len(),
        AdapterSubstrateClassV1::ALL.len(),
        "matrix case must not repeat a class layer"
    );
    for case in corpus.cases {
        let result = case.qualification.validate();
        match case.expect.as_str() {
            "accept" => result.map_err(|error| format!("{}: {error}", case.id))?,
            "reject" => {
                let error = result
                    .err()
                    .ok_or_else(|| format!("{} expected rejection but validated", case.id))?;
                if let Some(expected) = &case.error_code {
                    let actual = format!("{:?}", error.code);
                    if actual != *expected {
                        return Err(format!(
                            "{} expected error {:?}, got {:?}",
                            case.id, expected, error
                        ));
                    }
                }
            }
            other => return Err(format!("{} has unknown expect `{other}`", case.id)),
        }
    }
    Ok(())
}

fn require_ok<T, E: std::fmt::Display>(result: Result<T, E>, label: &str) -> Result<T, String> {
    result.map_err(|error| format!("{label}: {error}"))
}

fn require_err<T, E: std::fmt::Display>(result: Result<T, E>, label: &str) -> Result<E, String> {
    match result {
        Err(error) => Ok(error),
        Ok(_) => Err(format!("{label}: expected error")),
    }
}

#[test]
fn every_substrate_class_has_a_constructible_shape() -> Result<(), String> {
    for class in AdapterSubstrateClassV1::ALL {
        let mut qualification = base_qualification(class.as_str(), "python", class);
        match class {
            AdapterSubstrateClassV1::GeneratedParser => {
                qualification.packaging.build_script = "none".into();
                qualification.packaging.generated_code = "checked-in parser tables".into();
            }
            AdapterSubstrateClassV1::ExternalHelper => {
                qualification.required_fallback = Some("syntax_only_in_process".into());
            }
            AdapterSubstrateClassV1::PacketProducerConsumer => {
                qualification.claim_ceiling.ceiling =
                    SemanticClaimCeilingV1::NoLocalSemanticAuthority;
                qualification.claim_ceiling.message = "packet-only".into();
                qualification.layers[0].claim_ceiling = SubstrateClaimCeilingV1 {
                    ceiling: SemanticClaimCeilingV1::NoLocalSemanticAuthority,
                    message: "packet-only layer".into(),
                };
            }
            AdapterSubstrateClassV1::NativeLibrary => {
                qualification.packaging.platform_limits = vec!["linux-x86_64".into()];
            }
            AdapterSubstrateClassV1::Hybrid => {
                qualification.limitations.push(SubstrateLimitationV1 {
                    limitation_id: format!("{}.limit", class.as_str()),
                    category: "family_scope".into(),
                    message: "does not promote every member language".into(),
                });
            }
            _ => {}
        }
        require_ok(qualification.validate(), class.as_str())?;
    }
    Ok(())
}

#[test]
fn typescript_and_javascript_share_family_but_keep_language_cells() -> Result<(), String> {
    let mut typescript = base_qualification(
        "typescript.oxc",
        "typescript",
        AdapterSubstrateClassV1::InProcessParser,
    );
    typescript.shared_adapter_family_id = Some("oxc.shared".into());
    let mut javascript = base_qualification(
        "javascript.oxc",
        "javascript",
        AdapterSubstrateClassV1::InProcessParser,
    );
    javascript.shared_adapter_family_id = Some("oxc.shared".into());
    require_ok(typescript.validate(), "typescript")?;
    require_ok(javascript.validate(), "javascript")?;
    assert!(substrate_classes_compatible(&typescript, &javascript));
    assert!(!substrate_classes_compatible(&typescript, &typescript));
    Ok(())
}

#[test]
fn same_dependency_version_different_grammar_generations_validate() -> Result<(), String> {
    let mut qualification = base_qualification(
        "grammar.generations",
        "python",
        AdapterSubstrateClassV1::InProcessParser,
    );
    qualification.dependencies = vec![
        dependency("dep.v1", "2.0.0", "grammar.v1"),
        dependency("dep.v2", "2.0.0", "grammar.v2"),
    ];
    qualification.layers[0].dependency_ids = vec!["dep.v1".into(), "dep.v2".into()];
    require_ok(qualification.validate(), "two grammars")?;
    Ok(())
}

#[test]
fn receipt_compiler_ceiling_requires_compiler_capable_layer() -> Result<(), String> {
    let mut qualification = base_qualification(
        "receipt.compiler",
        "python",
        AdapterSubstrateClassV1::InProcessParser,
    );
    qualification.claim_ceiling = SubstrateClaimCeilingV1 {
        ceiling: SemanticClaimCeilingV1::CompilerFactsWithFallback,
        message: "compiler facts".into(),
    };
    let error = require_err(qualification.validate(), "receipt compiler")?;
    if error.code != AdapterSubstrateQualificationErrorCodeV1::ImpossibleClaimCeiling {
        return Err(format!("receipt compiler: got {:?}", error.code));
    }
    Ok(())
}

#[test]
fn shared_family_requires_matching_implementation() {
    let mut typescript = base_qualification(
        "typescript.oxc",
        "typescript",
        AdapterSubstrateClassV1::InProcessParser,
    );
    typescript.shared_adapter_family_id = Some("oxc.shared".into());
    let mut javascript = base_qualification(
        "javascript.helper",
        "javascript",
        AdapterSubstrateClassV1::ExternalHelper,
    );
    javascript.shared_adapter_family_id = Some("oxc.shared".into());
    javascript.required_fallback = Some("syntax_only_in_process".into());
    assert!(!substrate_classes_compatible(&typescript, &javascript));
}

#[test]
fn native_syntax_layer_may_declare_platform_limited_ceiling() -> Result<(), String> {
    let mut qualification = base_qualification(
        "native.platform",
        "python",
        AdapterSubstrateClassV1::NativeLibrary,
    );
    qualification.packaging.platform_limits = vec!["linux-x86_64".into()];
    qualification.layers[0].claim_ceiling = SubstrateClaimCeilingV1 {
        ceiling: SemanticClaimCeilingV1::NativePlatformLimited,
        message: "platform limited syntax".into(),
    };
    require_ok(qualification.validate(), "native platform layer")?;
    Ok(())
}

#[test]
fn generated_parser_allows_checked_in_tables_without_build_script() -> Result<(), String> {
    let mut qualification = base_qualification(
        "generated.checked_in",
        "python",
        AdapterSubstrateClassV1::GeneratedParser,
    );
    qualification.packaging.build_script = "none".into();
    qualification.packaging.generated_code = "checked-in parser tables".into();
    require_ok(qualification.validate(), "checked-in generated parser")?;
    Ok(())
}

#[test]
fn syntax_layer_cannot_inherit_compiler_ceiling() -> Result<(), String> {
    let mut qualification = base_qualification(
        "syntax.inherit",
        "python",
        AdapterSubstrateClassV1::InProcessParser,
    );
    qualification.layers[0].claim_ceiling = SubstrateClaimCeilingV1 {
        ceiling: SemanticClaimCeilingV1::CompilerFactsWithFallback,
        message: "compiler resolved".into(),
    };
    let error = require_err(qualification.validate(), "syntax inherit")?;
    if error.code != AdapterSubstrateQualificationErrorCodeV1::ImpossibleClaimCeiling {
        return Err(format!("syntax inherit: got {:?}", error.code));
    }
    Ok(())
}

/// #13034: a safety reference that does not establish containment is refused,
/// one flag at a time, and the refusal names exactly the flags that are false.
#[test]
fn safety_ref_must_establish_every_containment_flag() -> Result<(), String> {
    type Clear = fn(&mut AdapterSubstrateSafetyRefV1);
    let flags: [(&str, Clear); 4] = [
        ("no_execution", |safety| safety.no_execution = false),
        ("no_network", |safety| safety.no_network = false),
        ("root_containment", |safety| safety.root_containment = false),
        ("process_tree_bounded", |safety| {
            safety.process_tree_bounded = false
        }),
    ];
    let base = || {
        base_qualification(
            "safety.flags",
            "python",
            AdapterSubstrateClassV1::InProcessParser,
        )
    };
    // The control: the same qualification with every flag established passes,
    // so each refusal below is attributable to the flag it clears.
    require_ok(base().validate(), "all flags established")?;

    for (name, clear) in flags {
        let mut qualification = base();
        clear(&mut qualification.safety_refs[0]);
        let error = require_err(qualification.validate(), name)?;
        if error.code != AdapterSubstrateQualificationErrorCodeV1::AuthorityViolation {
            return Err(format!("{name}: got {:?}", error.code));
        }
        let message = error.to_string();
        if !message.contains(name) {
            return Err(format!("{name}: message does not name it: {message}"));
        }
        for (other, _) in flags {
            if other != name && message.contains(other) {
                return Err(format!("{name}: message also names {other}: {message}"));
            }
        }
    }

    let mut all_false = base();
    for (_, clear) in flags {
        clear(&mut all_false.safety_refs[0]);
    }
    let error = require_err(all_false.validate(), "all flags false")?;
    let message = error.to_string();
    for (name, _) in flags {
        if !message.contains(name) {
            return Err(format!("all flags false: message omits {name}: {message}"));
        }
    }
    Ok(())
}

/// #13034: a stale reference keeps its own error code even when its flags are
/// also false, so the existing staleness contract does not move.
#[test]
fn stale_safety_ref_keeps_its_error_over_unestablished_flags() -> Result<(), String> {
    let mut qualification = base_qualification(
        "safety.stale",
        "python",
        AdapterSubstrateClassV1::InProcessParser,
    );
    qualification.safety_refs[0].currentness = AuthorityCurrentnessV1::Stale;
    qualification.safety_refs[0].root_containment = false;
    let error = require_err(qualification.validate(), "stale and uncontained")?;
    if error.code != AdapterSubstrateQualificationErrorCodeV1::StaleAuthorityReference {
        return Err(format!("stale and uncontained: got {:?}", error.code));
    }
    Ok(())
}

#[test]
fn unknown_fields_are_rejected_at_deserialize() -> Result<(), String> {
    let encoded = require_ok(
        serde_json::to_value(base_qualification(
            "unknown",
            "python",
            AdapterSubstrateClassV1::InProcessParser,
        )),
        "encode qualification",
    )?;
    let mut value = encoded;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "qualification did not serialize as object".to_string())?;
    object.insert("unexpected".into(), json!(true));
    match serde_json::from_value::<AdapterSubstrateQualificationV1>(value) {
        Err(error) if error.to_string().contains("unknown field") => Ok(()),
        Err(error) => Err(format!("unknown field failed for other reason: {error}")),
        Ok(_) => Err("unknown field accepted".into()),
    }
}

#[test]
fn semantic_digest_is_order_and_relocation_invariant() -> Result<(), String> {
    let mut qualification = base_qualification(
        "digest.order",
        "python",
        AdapterSubstrateClassV1::InProcessParser,
    );
    qualification.layers = vec![
        syntax_layer("b"),
        syntax_layer("a"),
        AdapterSubstrateLayerV1 {
            layer_id: "compiler".into(),
            layer_class: AdapterSubstrateLayerClassV1::CompilerResolved,
            claim_ceiling: SubstrateClaimCeilingV1 {
                ceiling: SemanticClaimCeilingV1::CompilerFactsWithFallback,
                message: "compiler facts with fallback".into(),
            },
            dependency_ids: vec!["dep.primary".into()],
        },
    ];
    qualification.substrate_class = AdapterSubstrateClassV1::CompilerIndexService;
    qualification.claim_ceiling = SubstrateClaimCeilingV1 {
        ceiling: SemanticClaimCeilingV1::CompilerFactsWithFallback,
        message: "compiler service".into(),
    };
    qualification.limitations.push(SubstrateLimitationV1 {
        limitation_id: "locator.case".into(),
        category: "checkout".into(),
        message: "portable|locator:/tmp/checkout/src/lib.rs".into(),
    });
    let digest = adapter_substrate_qualification_semantic_digest(&qualification)
        .map_err(|error| error.to_string())?;
    qualification.layers.reverse();
    qualification.semantic_digest = Some(digest.clone());
    require_ok(qualification.validate(), "digest bound")?;
    let mut relocated = qualification.clone();
    relocated.semantic_digest = None;
    relocated.limitations[0].message = "portable|locator:/other/checkout/src/lib.rs".into();
    let relocated_digest = adapter_substrate_qualification_semantic_digest(&relocated)
        .map_err(|error| error.to_string())?;
    assert_eq!(digest, relocated_digest);
    Ok(())
}

#[test]
fn digest_sorted_lists_reject_duplicate_keys() -> Result<(), String> {
    type Corrupt = fn(&mut AdapterSubstrateQualificationV1);
    let cases: [(&str, Corrupt); 4] = [
        ("limitations", |q| {
            for message in ["a", "b"] {
                q.limitations.push(SubstrateLimitationV1 {
                    limitation_id: "dup".into(),
                    category: "scope".into(),
                    message: message.into(),
                });
            }
        }),
        ("safety_refs", |q| {
            let mut second = q.safety_refs[0].clone();
            second.harness_generation = "other.v1".into();
            q.safety_refs.push(second);
        }),
        ("non_claims", |q| {
            q.non_claims.push(SubstrateNonClaimV1 {
                claim: q.non_claims[0].claim.clone(),
                reason: "different reason".into(),
            });
        }),
        ("supersession_history", |q| {
            for reason in ["a", "b"] {
                q.supersession_history.push(SubstrateSupersessionRecordV1 {
                    superseded_qualification_id: "old.q".into(),
                    reason: reason.into(),
                });
            }
        }),
    ];
    for (field, corrupt) in cases {
        let mut qualification = base_qualification(
            "dup.keys",
            "python",
            AdapterSubstrateClassV1::InProcessParser,
        );
        require_ok(qualification.validate(), field)?;
        corrupt(&mut qualification);
        let error = require_err(qualification.validate(), field)?;
        assert_eq!(
            error.code,
            AdapterSubstrateQualificationErrorCodeV1::DuplicateIdentity,
            "{field}: {error}"
        );
        assert!(
            error
                .message
                .contains(&format!("{field} contains duplicate identity values")),
            "{field}: {error}"
        );
    }
    Ok(())
}

#[test]
fn dependency_order_does_not_change_substrate_compatibility() {
    let with_dependencies = |language: &str, order: [usize; 3]| {
        let mut qualification = base_qualification(
            "dep.order",
            language,
            AdapterSubstrateClassV1::InProcessParser,
        );
        qualification.shared_adapter_family_id = Some("family.shared".into());
        let canonical = [
            dependency("dep.primary", "1.0.0", "grammar.v1"),
            dependency("dep.secondary", "2.0.0", "grammar.v2"),
            dependency("dep.tertiary", "3.0.0", "grammar.v3"),
        ];
        qualification.dependencies = order
            .iter()
            .map(|&index| canonical[index].clone())
            .collect();
        qualification
    };
    let left = with_dependencies("typescript", [0, 1, 2]);
    // Every ordering of the same three dependencies is compatible.
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        assert!(
            substrate_classes_compatible(&left, &with_dependencies("javascript", order)),
            "order {order:?} must stay compatible"
        );
    }
    // A changed version is still incompatible whichever slot it sits in.
    for slot in 0..3 {
        let mut different = with_dependencies("javascript", [2, 0, 1]);
        different.dependencies[slot].exact_version = "9.9.9".into();
        assert!(
            !substrate_classes_compatible(&left, &different),
            "a changed version in slot {slot} must break compatibility"
        );
    }
    // A missing dependency is incompatible too.
    let mut shorter = with_dependencies("javascript", [1, 2, 0]);
    shorter.dependencies.pop();
    assert!(!substrate_classes_compatible(&left, &shorter));
}
