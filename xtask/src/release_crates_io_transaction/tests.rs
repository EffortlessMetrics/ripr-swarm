use super::plan::{
    AggregateState, CratesIoTransactionRequest, Decision, PUBLISH_OPERATION, PackageNode,
    PackageObservation, PackageState, REQUEST_SCHEMA, RegistryInventory, RegistryObservation, Step,
    StopKind, VerifiedAuthorization, decide, package_set_sha256,
};
use std::collections::BTreeSet;

fn hex(seed: char, len: usize) -> String {
    std::iter::repeat_n(seed, len).collect()
}

fn node(name: &str, seed: char, requires: &[&str]) -> PackageNode {
    PackageNode {
        name: name.to_string(),
        version: "0.11.0".to_string(),
        crate_sha256: hex(seed, 64),
        size: 100,
        requires: requires.iter().map(|name| name.to_string()).collect(),
    }
}

fn request_for(packages: Vec<PackageNode>) -> Result<CratesIoTransactionRequest, String> {
    Ok(CratesIoTransactionRequest {
        schema: REQUEST_SCHEMA.to_string(),
        registry: "crates.io".to_string(),
        candidate_sha: hex('1', 40),
        candidate_tree: hex('2', 40),
        ship_packet_sha256: hex('c', 64),
        stable_decision_sha256: hex('d', 64),
        package_set_sha256: package_set_sha256(&packages)?,
        packages,
    })
}

/// The split candidate: the support crate, then the facade that requires it.
fn split() -> Result<CratesIoTransactionRequest, String> {
    request_for(vec![
        node("ripr-contract", 'a', &[]),
        node("ripr", 'b', &["ripr-contract"]),
    ])
}

fn singleton() -> Result<CratesIoTransactionRequest, String> {
    request_for(vec![node("ripr", 'b', &[])])
}

fn authorization(
    request: &CratesIoTransactionRequest,
    publish: bool,
) -> Result<VerifiedAuthorization, String> {
    Ok(VerifiedAuthorization {
        authorization_id: format!("sha256:{}", hex('f', 64)),
        transaction_request_id: request.request_id()?,
        allowed_operations: if publish {
            BTreeSet::from([PUBLISH_OPERATION.to_string()])
        } else {
            BTreeSet::new()
        },
        subjects: request
            .packages
            .iter()
            .map(|package| {
                (
                    format!("{}-{}.crate", package.name, package.version),
                    package.crate_sha256.clone(),
                )
            })
            .collect(),
    })
}

fn exact(package: &PackageNode) -> RegistryObservation {
    RegistryObservation::Present {
        yanked: false,
        downloaded_sha256: Some(package.crate_sha256.clone()),
    }
}

fn unavailable() -> RegistryObservation {
    RegistryObservation::Unavailable {
        reason: "index not yet updated".to_string(),
    }
}

fn inventory(
    request: &CratesIoTransactionRequest,
    states: &[RegistryObservation],
) -> RegistryInventory {
    RegistryInventory {
        packages: request
            .packages
            .iter()
            .zip(states)
            .map(|(package, registry)| PackageObservation {
                name: package.name.clone(),
                registry: registry.clone(),
            })
            .collect(),
        attempted: BTreeSet::new(),
    }
}

fn step_of(decision: &Decision) -> Result<&Step, String> {
    match decision {
        Decision::Proceed { step } => Ok(step),
        Decision::Stop { stop } => Err(format!("stopped: {:?} {}", stop.kind, stop.detail)),
    }
}

fn stop_of(decision: &Decision) -> Result<StopKind, String> {
    match decision {
        Decision::Stop { stop } => Ok(stop.kind),
        Decision::Proceed { step } => Err(format!("proceeded with {step:?}")),
    }
}

fn publish(name: &str) -> Step {
    Step::Publish {
        name: name.to_string(),
    }
}

#[test]
fn singleton_and_split_sets_publish_in_dependency_order_then_only_verify() -> Result<(), String> {
    let single = singleton()?;
    assert_eq!(single.violations(), Vec::<String>::new());
    let auth = authorization(&single, true)?;
    let absent = inventory(&single, &[RegistryObservation::Absent]);
    assert_eq!(
        step_of(&decide(&single, &auth, &absent).decision)?,
        &publish("ripr")
    );

    let request = split()?;
    assert_eq!(request.violations(), Vec::<String>::new());
    let auth = authorization(&request, true)?;
    let [support, facade] = [&request.packages[0], &request.packages[1]];
    let fresh = inventory(
        &request,
        &[RegistryObservation::Absent, RegistryObservation::Absent],
    );
    let planned = decide(&request, &auth, &fresh);
    assert_eq!(step_of(&planned.decision)?, &publish("ripr-contract"));
    assert_eq!(planned.aggregate, AggregateState::NothingPublished);

    let support_public = inventory(&request, &[exact(support), RegistryObservation::Absent]);
    let planned = decide(&request, &auth, &support_public);
    assert_eq!(step_of(&planned.decision)?, &publish("ripr"));
    // One of two packages is public: partial, even though the next upload is admitted.
    assert_eq!(planned.aggregate, AggregateState::PartialPublicationHold);

    let both = inventory(&request, &[exact(support), exact(facade)]);
    let planned = decide(&request, &auth, &both);
    assert_eq!(step_of(&planned.decision)?, &Step::Complete);
    assert_eq!(planned.aggregate, AggregateState::PackageSetPublishedExact);
    // Verify-only completion needs no publish authority.
    let read_only = authorization(&request, false)?;
    assert_eq!(
        step_of(&decide(&request, &read_only, &both).decision)?,
        &Step::Complete
    );
    Ok(())
}

#[test]
fn the_facade_waits_for_a_readable_exact_support_package() -> Result<(), String> {
    let request = split()?;
    let auth = authorization(&request, true)?;
    let support = &request.packages[0];
    let unread = RegistryObservation::Present {
        yanked: false,
        downloaded_sha256: None,
    };
    for (label, state) in [("unavailable", unavailable()), ("bytes unread", unread)] {
        let remote = inventory(&request, &[state, RegistryObservation::Absent]);
        let planned = decide(&request, &auth, &remote);
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::RegistryUnavailable,
            "{label}"
        );
    }
    // Support absent and the facade unknown: no support upload either, since
    // the unread facade could be a conflict that the set cannot get past.
    let unread_facade = RegistryObservation::Present {
        yanked: false,
        downloaded_sha256: None,
    };
    for (label, state) in [
        ("facade unavailable", unavailable()),
        ("facade bytes unread", unread_facade),
    ] {
        let remote = inventory(&request, &[RegistryObservation::Absent, state]);
        let planned = decide(&request, &auth, &remote);
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::RegistryUnavailable,
            "{label}"
        );
    }
    // An observation missing for a package, or given twice, is not absence.
    let mut missing = inventory(&request, &[exact(support), RegistryObservation::Absent]);
    missing.packages.pop();
    assert_eq!(
        stop_of(&decide(&request, &auth, &missing).decision)?,
        StopKind::RegistryUnavailable
    );
    let mut doubled = inventory(
        &request,
        &[RegistryObservation::Absent, RegistryObservation::Absent],
    );
    doubled.packages.push(PackageObservation {
        name: support.name.clone(),
        registry: exact(support),
    });
    assert_eq!(
        stop_of(&decide(&request, &auth, &doubled).decision)?,
        StopKind::RegistryUnavailable
    );
    Ok(())
}

#[test]
fn a_public_version_with_other_bytes_or_yanked_stops_before_any_upload() -> Result<(), String> {
    let request = split()?;
    let auth = authorization(&request, true)?;
    let support = &request.packages[0];
    let other = RegistryObservation::Present {
        yanked: false,
        downloaded_sha256: Some(hex('9', 64)),
    };
    let yanked = RegistryObservation::Present {
        yanked: true,
        downloaded_sha256: Some(support.crate_sha256.clone()),
    };
    for (label, states) in [
        (
            "support differs",
            [other.clone(), RegistryObservation::Absent],
        ),
        ("support yanked", [yanked, RegistryObservation::Absent]),
        // A conflicting facade stops the set before the support is uploaded.
        (
            "facade differs, support absent",
            [RegistryObservation::Absent, other.clone()],
        ),
    ] {
        let planned = decide(&request, &auth, &inventory(&request, &states));
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::ImmutableConflict,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn a_partial_publication_is_held_with_its_exact_prefix_retained() -> Result<(), String> {
    let request = split()?;
    let auth = authorization(&request, true)?;
    let support = &request.packages[0];
    // Support is public; the facade upload was attempted and is not visible.
    let mut held = inventory(&request, &[exact(support), RegistryObservation::Absent]);
    held.attempted.insert("ripr".to_string());
    let planned = decide(&request, &auth, &held);
    assert_eq!(stop_of(&planned.decision)?, StopKind::AttemptConsumed);
    assert_eq!(planned.aggregate, AggregateState::PartialPublicationHold);
    assert_eq!(
        planned.packages,
        vec![
            ("ripr-contract".to_string(), PackageState::ExactExisting),
            ("ripr".to_string(), PackageState::AbsentAdmitted),
        ]
    );
    // Support attempted with an unknown outcome: neither a support retry nor
    // a facade attempt follows until the registry shows it.
    let mut ambiguous = inventory(
        &request,
        &[RegistryObservation::Absent, RegistryObservation::Absent],
    );
    ambiguous.attempted.insert("ripr-contract".to_string());
    assert_eq!(
        stop_of(&decide(&request, &auth, &ambiguous).decision)?,
        StopKind::AttemptConsumed
    );
    // Once the attempted support reads back exact, the resume uploads only
    // the facade; the support is not uploaded again.
    let mut resumed = inventory(&request, &[exact(support), RegistryObservation::Absent]);
    resumed.attempted.insert("ripr-contract".to_string());
    assert_eq!(
        step_of(&decide(&request, &auth, &resumed).decision)?,
        &publish("ripr")
    );
    // Facade unreadable after the support published is a hold, not success.
    let unread = inventory(&request, &[exact(support), unavailable()]);
    let planned = decide(&request, &auth, &unread);
    assert_eq!(stop_of(&planned.decision)?, StopKind::RegistryUnavailable);
    assert_eq!(planned.aggregate, AggregateState::PartialPublicationHold);
    Ok(())
}

#[test]
fn the_attempt_record_is_matched_by_registry_name_and_bound_to_the_set() -> Result<(), String> {
    let request = split()?;
    let auth = authorization(&request, true)?;
    let support = &request.packages[0];
    // Another spelling of the same registry name still consumes the attempt.
    for spelling in ["ripr_contract", "Ripr-Contract", "RIPR_CONTRACT"] {
        let mut remote = inventory(
            &request,
            &[RegistryObservation::Absent, RegistryObservation::Absent],
        );
        remote.attempted.insert(spelling.to_string());
        assert_eq!(
            stop_of(&decide(&request, &auth, &remote).decision)?,
            StopKind::AttemptConsumed,
            "{spelling}"
        );
    }
    let mut facade = inventory(&request, &[exact(support), RegistryObservation::Absent]);
    facade.attempted.insert("RIPR".to_string());
    assert_eq!(
        stop_of(&decide(&request, &auth, &facade).decision)?,
        StopKind::AttemptConsumed
    );
    // An attempt for a package outside the set rejects the inventory rather
    // than being ignored.
    let mut foreign = inventory(
        &request,
        &[RegistryObservation::Absent, RegistryObservation::Absent],
    );
    foreign.attempted.insert("ripr-other".to_string());
    assert_eq!(
        stop_of(&decide(&request, &auth, &foreign).decision)?,
        StopKind::InventoryRejected
    );
    Ok(())
}

#[test]
fn an_upload_needs_publish_authority_for_this_exact_request_and_set() -> Result<(), String> {
    let request = split()?;
    let fresh = inventory(
        &request,
        &[RegistryObservation::Absent, RegistryObservation::Absent],
    );
    let denied = authorization(&request, false)?;
    assert_eq!(
        stop_of(&decide(&request, &denied, &fresh).decision)?,
        StopKind::NotAuthorized
    );
    // A grant that names only `ripr` does not authorize the support crate.
    let mut facade_only = authorization(&request, true)?;
    facade_only.subjects.remove(0);
    let mut other_bytes = authorization(&request, true)?;
    other_bytes.subjects[1].1 = hex('9', 64);
    let mut other_request = authorization(&request, true)?;
    other_request.transaction_request_id = singleton()?.request_id()?;
    for authorization in [facade_only, other_bytes, other_request] {
        assert_eq!(
            stop_of(&decide(&request, &authorization, &fresh).decision)?,
            StopKind::AuthorizationMismatch
        );
    }
    Ok(())
}

#[test]
fn each_inadmissible_request_is_named() -> Result<(), String> {
    type Mutation = fn(&mut CratesIoTransactionRequest);
    let cases: [(&str, Mutation, &str); 9] = [
        ("schema", |r| r.schema.push('x'), "schema"),
        (
            "another registry",
            |r| r.registry = "staging".to_string(),
            "crates.io",
        ),
        (
            "branch as candidate",
            |r| r.candidate_sha = "main".to_string(),
            "40 lowercase hex",
        ),
        (
            "prerelease version",
            |r| r.packages[1].version = "0.11.0-rc.1".to_string(),
            "MAJOR.MINOR.PATCH",
        ),
        (
            "facade before its support",
            |r| r.packages.reverse(),
            "not an earlier package",
        ),
        (
            "dependency outside the set",
            |r| r.packages[1].requires = vec!["serde".to_string()],
            "not an earlier package",
        ),
        (
            "same crate under another spelling",
            |r| r.packages[1].name = "Ripr_Contract".to_string(),
            "listed twice",
        ),
        (
            "unauthorized extra node without a new digest",
            |r| r.packages.push(node("ripr-extra", 'e', &[])),
            "does not match the listed packages",
        ),
        ("no packages", |r| r.packages.clear(), "must not be empty"),
    ];
    for (label, mutate, expect) in cases {
        let mut request = split()?;
        mutate(&mut request);
        let violations = request.violations();
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(expect)),
            "{label} must be refused ({expect}); got {violations:?}"
        );
        let auth = authorization(&request, true)?;
        let planned = decide(&request, &auth, &inventory(&request, &[]));
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::RequestRejected,
            "{label}"
        );
    }
    // A changed archive is a different request: an old authorization for the
    // original request no longer matches.
    let original = split()?;
    let auth = authorization(&original, true)?;
    let mut changed = split()?;
    changed.packages[1].crate_sha256 = hex('7', 64);
    changed.package_set_sha256 = package_set_sha256(&changed.packages)?;
    assert_eq!(changed.violations(), Vec::<String>::new());
    let fresh = inventory(
        &changed,
        &[RegistryObservation::Absent, RegistryObservation::Absent],
    );
    assert_eq!(
        stop_of(&decide(&changed, &auth, &fresh).decision)?,
        StopKind::AuthorizationMismatch
    );
    Ok(())
}

/// Independent invariant over every registry state of a two-package set, with
/// and without prior attempts: an upload is proposed only for an absent,
/// never-attempted package whose earlier packages are all exact and with no
/// conflict anywhere.
#[test]
fn no_registry_state_yields_a_repeat_out_of_order_or_conflicting_upload() -> Result<(), String> {
    let request = split()?;
    let auth = authorization(&request, true)?;
    let variants = |package: &PackageNode| {
        vec![
            RegistryObservation::Absent,
            exact(package),
            RegistryObservation::Present {
                yanked: false,
                downloaded_sha256: Some(hex('9', 64)),
            },
            RegistryObservation::Present {
                yanked: true,
                downloaded_sha256: Some(package.crate_sha256.clone()),
            },
            RegistryObservation::Present {
                yanked: false,
                downloaded_sha256: None,
            },
            unavailable(),
        ]
    };
    let (mut combinations, mut uploads, mut completes) = (0usize, 0usize, 0usize);
    for (first_index, first) in variants(&request.packages[0]).iter().enumerate() {
        for (second_index, second) in variants(&request.packages[1]).iter().enumerate() {
            for attempted in 0u8..4 {
                let mut remote = inventory(&request, &[first.clone(), second.clone()]);
                if attempted & 1 != 0 {
                    remote.attempted.insert("ripr-contract".to_string());
                }
                if attempted & 2 != 0 {
                    remote.attempted.insert("ripr".to_string());
                }
                combinations += 1;
                let conflict = [first_index, second_index]
                    .iter()
                    .any(|index| matches!(index, 2 | 3));
                match decide(&request, &auth, &remote).decision {
                    Decision::Stop { .. } => {}
                    Decision::Proceed {
                        step: Step::Complete,
                    } => {
                        completes += 1;
                        assert_eq!((first_index, second_index), (1, 1));
                    }
                    Decision::Proceed {
                        step: Step::Publish { name },
                    } => {
                        uploads += 1;
                        assert!(!conflict, "upload of {name} proposed beside a conflict");
                        assert!(!remote.attempted.contains(&name), "{name} uploaded twice");
                        if name == "ripr-contract" {
                            // The facade must be readable as absent or exact:
                            // an unknown facade can hide a conflict.
                            assert_eq!(first_index, 0);
                            assert!(matches!(second_index, 0 | 1), "facade state {second_index}");
                        } else {
                            assert_eq!((first_index, second_index), (1, 0));
                        }
                    }
                }
            }
        }
    }
    assert_eq!(combinations, 6 * 6 * 4);
    // The sweep reached uploads and completion, so the assertions gripped.
    assert!(
        uploads > 0 && completes == 4,
        "{uploads} uploads, {completes} completes"
    );
    Ok(())
}

#[test]
fn plan_command_exits_nonzero_for_a_stop() -> Result<(), String> {
    let request = split()?;
    let dir = std::env::temp_dir().join(format!(
        "ripr-release-crates-io-transaction-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let write = |name: &str, json: Result<String, serde_json::Error>| -> Result<String, String> {
        let path = dir.join(name);
        std::fs::write(&path, json.map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    };
    let request_path = write("request.json", serde_json::to_string(&request))?;
    let authorization_path = write(
        "authorization.json",
        serde_json::to_string(&authorization(&request, true)?),
    )?;
    let fresh = inventory(
        &request,
        &[RegistryObservation::Absent, RegistryObservation::Absent],
    );
    let held = inventory(&request, &[unavailable(), RegistryObservation::Absent]);
    let fresh_path = write("fresh.json", serde_json::to_string(&fresh))?;
    let held_path = write("held.json", serde_json::to_string(&held))?;
    let args = |inventory: &str| {
        vec![
            "--request".to_string(),
            request_path.clone(),
            "--authorization".to_string(),
            authorization_path.clone(),
            "--inventory".to_string(),
            inventory.to_string(),
        ]
    };
    let proceed = super::plan_command(&args(&fresh_path));
    let stopped = super::plan_command(&args(&held_path));
    let mut extra = args(&fresh_path);
    extra.push("--token".to_string());
    let loose = super::plan_command(&extra);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(proceed, Ok(()));
    assert!(stopped.is_err_and(|error| error.contains("RegistryUnavailable")));
    assert!(loose.is_err_and(|error| error.contains("unexpected argument")));
    Ok(())
}
