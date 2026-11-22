use super::inventory::{
    AssetState, AttestationObservation, AttestationState, BytesObservation, ReleaseState,
    ReleasesObservation, RemoteAsset, RemoteInventory, RemoteRelease, TagObservation, TagState,
    classify,
};
use super::model::{
    ExpectedSubject, GitHubReleaseTransactionRequest, Mode, Provenance, REQUEST_SCHEMA,
    WithheldChannels, subject_inventory_sha256,
};
use super::plan::{AuthorizedSubject, Decision, Step, StopKind, VerifiedAuthorization, decide};
use std::collections::BTreeSet;

const CANDIDATE: &str = "1111111111111111111111111111111111111111";
const OTHER_SHA: &str = "2222222222222222222222222222222222222222";
/// The annotated tag object the authorization event pushes.
const TAG_OBJECT: &str = "3333333333333333333333333333333333333333";
/// Another annotated tag object (other tagger or message) at the candidate.
const FOREIGN_TAG_OBJECT: &str = "4444444444444444444444444444444444444444";

fn hex64(seed: char) -> String {
    std::iter::repeat_n(seed, 64).collect()
}

fn subjects() -> Vec<ExpectedSubject> {
    vec![
        ExpectedSubject {
            name: "ripr-0.11.0.vsix".to_string(),
            kind: "vsix".to_string(),
            size: 10,
            sha256: hex64('a'),
        },
        ExpectedSubject {
            name: "ripr-server-manifest-v0.11.0.json".to_string(),
            kind: "server_manifest".to_string(),
            size: 20,
            sha256: hex64('b'),
        },
    ]
}

fn rc_request() -> Result<GitHubReleaseTransactionRequest, String> {
    let subjects = subjects();
    Ok(GitHubReleaseTransactionRequest {
        schema: REQUEST_SCHEMA.to_string(),
        repository: "EffortlessMetrics/ripr".to_string(),
        mode: Mode::PublicRc,
        product_version: "0.11.0".to_string(),
        candidate_sha: CANDIDATE.to_string(),
        candidate_tree: OTHER_SHA.to_string(),
        tag: "v0.11.0-rc.1".to_string(),
        title: "ripr 0.11.0-rc.1".to_string(),
        body_sha256: hex64('c'),
        prerelease: true,
        ship_packet_sha256: hex64('d'),
        stable_decision_sha256: None,
        subject_inventory_sha256: subject_inventory_sha256(&subjects)?,
        subjects,
        provenance: Provenance {
            signer_workflow: "EffortlessMetrics/ripr/.github/workflows/release.yml".to_string(),
            source_ref: "refs/heads/main".to_string(),
        },
        withheld_channels: WithheldChannels::default(),
    })
}

fn stable_request() -> Result<GitHubReleaseTransactionRequest, String> {
    let mut request = rc_request()?;
    request.mode = Mode::Stable;
    request.tag = "v0.11.0".to_string();
    request.title = "ripr 0.11.0".to_string();
    request.prerelease = false;
    request.stable_decision_sha256 = Some(hex64('e'));
    Ok(request)
}

fn authorization(
    request: &GitHubReleaseTransactionRequest,
    operations: &[&str],
) -> Result<VerifiedAuthorization, String> {
    Ok(VerifiedAuthorization {
        authorization_id: format!("sha256:{}", hex64('f')),
        profile: match request.mode {
            Mode::PublicRc => "public_rc",
            Mode::Stable => "stable_core",
        }
        .to_string(),
        transaction_kind: "github".to_string(),
        transaction_request_id: request.request_id()?,
        source_commit: request.candidate_sha.clone(),
        event_ref: format!("refs/tags/{}", request.tag),
        expected_tag_object: TAG_OBJECT.to_string(),
        allowed_operations: operations.iter().map(|name| name.to_string()).collect(),
        subjects: request
            .subjects
            .iter()
            .map(|subject| AuthorizedSubject {
                name: subject.name.clone(),
                class: subject.kind.clone(),
                digest: format!("sha256:{}", subject.sha256),
            })
            .collect(),
    })
}

const RC_OPERATIONS: [&str; 3] = [
    "create_annotated_tag",
    "github_prerelease_create",
    "github_asset_upload",
];

fn empty_remote_for(request: &GitHubReleaseTransactionRequest) -> RemoteInventory {
    let mut remote = empty_remote();
    remote.repository = request.repository.clone();
    remote.tag_name = request.tag.clone();
    remote
}

/// An empty inventory observed for the public RC request's target.
fn empty_remote() -> RemoteInventory {
    RemoteInventory {
        repository: "EffortlessMetrics/ripr".to_string(),
        tag_name: "v0.11.0-rc.1".to_string(),
        tag: TagObservation::Absent,
        releases: ReleasesObservation::Listed {
            releases: Vec::new(),
        },
        attestations: Vec::new(),
    }
}

fn exact_tag() -> TagObservation {
    TagObservation::Present {
        object_sha: TAG_OBJECT.to_string(),
        peeled_sha: CANDIDATE.to_string(),
        annotated: true,
    }
}

fn exact_asset(id: u64, subject: &ExpectedSubject) -> RemoteAsset {
    RemoteAsset {
        id,
        name: subject.name.clone(),
        size: subject.size,
        bytes: BytesObservation::Downloaded {
            sha256: subject.sha256.clone(),
        },
    }
}

fn release(
    request: &GitHubReleaseTransactionRequest,
    draft: bool,
    assets: Vec<RemoteAsset>,
) -> RemoteRelease {
    RemoteRelease {
        id: 7,
        tag: request.tag.clone(),
        draft,
        prerelease: request.prerelease,
        title: request.title.clone(),
        body_sha256: request.body_sha256.clone(),
        assets,
    }
}

fn remote_with(
    request: &GitHubReleaseTransactionRequest,
    draft: bool,
    assets: Vec<RemoteAsset>,
    attested: bool,
) -> RemoteInventory {
    RemoteInventory {
        repository: request.repository.clone(),
        tag_name: request.tag.clone(),
        tag: exact_tag(),
        releases: ReleasesObservation::Listed {
            releases: vec![release(request, draft, assets)],
        },
        attestations: if attested {
            request
                .subjects
                .iter()
                .map(|subject| AttestationObservation {
                    subject: subject.name.clone(),
                    state: AttestationState::Verified,
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

fn all_exact(request: &GitHubReleaseTransactionRequest) -> Vec<RemoteAsset> {
    request
        .subjects
        .iter()
        .zip(1u64..)
        .map(|(subject, id)| exact_asset(id, subject))
        .collect()
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

#[test]
fn admissible_requests_have_no_violations_and_a_stable_identity() -> Result<(), String> {
    let rc = rc_request()?;
    let stable = stable_request()?;
    assert_eq!(rc.violations(), Vec::<String>::new());
    assert_eq!(stable.violations(), Vec::<String>::new());
    assert_eq!(rc.request_id()?, rc.clone().request_id()?);
    assert_ne!(rc.request_id()?, stable.request_id()?);
    let mut moved = rc.clone();
    moved.candidate_sha = OTHER_SHA.to_string();
    assert_ne!(rc.request_id()?, moved.request_id()?);
    assert_eq!(rc.request_id()?.len(), 64);
    Ok(())
}

#[test]
fn each_inadmissible_request_field_is_named() -> Result<(), String> {
    type Mutation = fn(&mut GitHubReleaseTransactionRequest);
    let cases: [(&str, bool, Mutation, &str); 18] = [
        ("schema", false, |r| r.schema.push('x'), "schema"),
        (
            "repository",
            false,
            |r| r.repository = "ripr".to_string(),
            "owner/name",
        ),
        (
            "version",
            false,
            |r| r.product_version = "0.11".to_string(),
            "MAJOR.MINOR.PATCH",
        ),
        (
            "branch ref as candidate",
            false,
            |r| r.candidate_sha = "main".to_string(),
            "40 lowercase hex",
        ),
        (
            "uppercase digest",
            false,
            |r| r.body_sha256 = r.body_sha256.to_uppercase(),
            "body_sha256",
        ),
        (
            "rc tag derived as stable",
            false,
            |r| r.tag = "v0.11.0".to_string(),
            "public_rc tag",
        ),
        (
            "rc tag for another version",
            false,
            |r| r.tag = "v0.12.0-rc.1".to_string(),
            "public_rc tag",
        ),
        (
            "rc not prerelease",
            false,
            |r| r.prerelease = false,
            "prerelease=true",
        ),
        (
            "rc with stable decision",
            false,
            |r| r.stable_decision_sha256 = Some(hex64('e')),
            "stable decision",
        ),
        (
            "stable with rc tag",
            true,
            |r| r.tag = "v0.11.0-rc.1".to_string(),
            "stable tag",
        ),
        (
            "stable as prerelease",
            true,
            |r| r.prerelease = true,
            "prerelease=false",
        ),
        (
            "stable without decision",
            true,
            |r| r.stable_decision_sha256 = None,
            "stable_decision_sha256",
        ),
        (
            "another channel",
            false,
            |r| r.withheld_channels.crates_io = true,
            "another channel",
        ),
        (
            "path in subject name",
            false,
            |r| r.subjects[0].name = "../x.vsix".to_string(),
            "plain file name",
        ),
        (
            "case-duplicate subject",
            false,
            |r| r.subjects[1].name = "RIPR-0.11.0.VSIX".to_string(),
            "duplicated",
        ),
        (
            "crate subject in a GitHub transaction",
            false,
            |r| r.subjects[0].kind = "crate".to_string(),
            "not a GitHub asset class",
        ),
        (
            "subjects out of order",
            false,
            |r| r.subjects.reverse(),
            "strict name order",
        ),
        (
            "no subjects",
            false,
            |r| r.subjects.clear(),
            "must not be empty",
        ),
    ];
    for (label, stable, mutate, expect) in cases {
        let mut request = if stable {
            stable_request()?
        } else {
            rc_request()?
        };
        mutate(&mut request);
        let violations = request.violations();
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(expect)),
            "{label} must be refused ({expect}); got {violations:?}"
        );
        let planned = decide(
            &request,
            &authorization(&request, &RC_OPERATIONS)?,
            &empty_remote(),
        );
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::RequestRejected,
            "{label}"
        );
    }
    // A changed subject without a recomputed inventory digest is refused.
    let mut request = rc_request()?;
    request.subjects[0].sha256 = hex64('9');
    assert!(
        request
            .violations()
            .iter()
            .any(|violation| violation.contains("does not match the listed subjects"))
    );
    Ok(())
}

#[test]
fn request_json_refuses_unknown_and_missing_fields() -> Result<(), String> {
    let mut value = serde_json::to_value(rc_request()?).map_err(|error| error.to_string())?;
    let parse = |value: &serde_json::Value| {
        serde_json::from_value::<GitHubReleaseTransactionRequest>(value.clone())
    };
    parse(&value).map_err(|error| error.to_string())?;
    value["force"] = serde_json::Value::Bool(true);
    assert!(
        parse(&value).is_err(),
        "an unknown field must not be ignored"
    );
    let mut value = serde_json::to_value(rc_request()?).map_err(|error| error.to_string())?;
    value
        .as_object_mut()
        .ok_or("request is not an object")?
        .remove("withheld_channels");
    assert!(parse(&value).is_err(), "a missing field has no default");
    // An optional value is still a required key.
    let mut value = serde_json::to_value(rc_request()?).map_err(|error| error.to_string())?;
    value
        .as_object_mut()
        .ok_or("request is not an object")?
        .remove("stable_decision_sha256");
    assert!(
        parse(&value).is_err(),
        "an omitted stable_decision_sha256 is not read as null"
    );
    Ok(())
}

/// A stand-in remote that applies exactly the step it is given.
fn apply(remote: &mut RemoteInventory, request: &GitHubReleaseTransactionRequest, step: &Step) {
    match step {
        Step::CreateTag => remote.tag = exact_tag(),
        Step::CreateDraftRelease => {
            remote.releases = ReleasesObservation::Listed {
                releases: vec![release(request, true, Vec::new())],
            };
        }
        Step::UploadSubject { name } => {
            if let ReleasesObservation::Listed { releases } = &mut remote.releases
                && let Some(release) = releases.first_mut()
                && let Some(subject) = request.subjects.iter().find(|s| s.name == *name)
            {
                let id = release.assets.len() as u64 + 1;
                release.assets.push(exact_asset(id, subject));
            }
        }
        Step::FinalizeRelease { .. } => {
            if let ReleasesObservation::Listed { releases } = &mut remote.releases
                && let Some(release) = releases.first_mut()
            {
                release.draft = false;
            }
        }
        Step::Complete => {}
    }
}

fn attest_all(remote: &mut RemoteInventory, request: &GitHubReleaseTransactionRequest) {
    remote.attestations = request
        .subjects
        .iter()
        .map(|subject| AttestationObservation {
            subject: subject.name.clone(),
            state: AttestationState::Verified,
        })
        .collect();
}

#[test]
fn a_fresh_transaction_takes_each_step_once_in_order_and_then_only_verifies() -> Result<(), String>
{
    for (request, release_operation) in [
        (rc_request()?, "github_prerelease_create"),
        (stable_request()?, "github_release_create"),
    ] {
        let authorization = authorization(
            &request,
            &[
                "create_annotated_tag",
                release_operation,
                "github_asset_upload",
            ],
        )?;
        let mut remote = empty_remote_for(&request);
        attest_all(&mut remote, &request);
        let mut taken = Vec::new();
        for _ in 0..16 {
            let planned = decide(&request, &authorization, &remote);
            let step = step_of(&planned.decision)?.clone();
            if step == Step::Complete {
                break;
            }
            apply(&mut remote, &request, &step);
            taken.push(step);
        }
        assert_eq!(
            taken,
            vec![
                Step::CreateTag,
                Step::CreateDraftRelease,
                Step::UploadSubject {
                    name: request.subjects[0].name.clone()
                },
                Step::UploadSubject {
                    name: request.subjects[1].name.clone()
                },
                Step::FinalizeRelease { release_id: 7 },
            ]
        );
        // Re-running against the finished public state writes nothing.
        let again = decide(&request, &authorization, &remote);
        assert_eq!(step_of(&again.decision)?, &Step::Complete);
        // Verify-only completion needs no mutation authority at all.
        let read_only = self::authorization(&request, &[])?;
        assert_eq!(
            step_of(&decide(&request, &read_only, &remote).decision)?,
            &Step::Complete
        );
    }
    Ok(())
}

#[test]
fn a_compatible_partial_transaction_resumes_at_the_first_missing_object() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let mut tag_only = empty_remote();
    tag_only.tag = exact_tag();
    assert_eq!(
        step_of(&decide(&request, &authorization, &tag_only).decision)?,
        &Step::CreateDraftRelease
    );
    let first_only = remote_with(
        &request,
        true,
        vec![exact_asset(1, &request.subjects[0])],
        true,
    );
    assert_eq!(
        step_of(&decide(&request, &authorization, &first_only).decision)?,
        &Step::UploadSubject {
            name: request.subjects[1].name.clone()
        }
    );
    Ok(())
}

#[test]
fn each_step_needs_its_own_authorized_operation() -> Result<(), String> {
    let request = rc_request()?;
    let stable = stable_request()?;
    let mut tag_only = empty_remote();
    tag_only.tag = exact_tag();
    let draft = remote_with(&request, true, Vec::new(), true);
    let full_draft = remote_with(&request, true, all_exact(&request), true);
    for (label, remote, missing) in [
        ("tag", empty_remote(), "create_annotated_tag"),
        ("draft", tag_only.clone(), "github_prerelease_create"),
        ("upload", draft, "github_asset_upload"),
        ("finalize", full_draft, "github_prerelease_create"),
    ] {
        let allowed: Vec<&str> = RC_OPERATIONS
            .iter()
            .copied()
            .filter(|operation| *operation != missing)
            .collect();
        let planned = decide(&request, &authorization(&request, &allowed)?, &remote);
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::NotAuthorized,
            "{label}"
        );
    }
    // Prerelease authority does not create a stable release.
    let mut stable_tag_only = empty_remote_for(&stable);
    stable_tag_only.tag = exact_tag();
    let planned = decide(
        &stable,
        &authorization(&stable, &RC_OPERATIONS)?,
        &stable_tag_only,
    );
    assert_eq!(stop_of(&planned.decision)?, StopKind::NotAuthorized);
    Ok(())
}

#[test]
fn an_authorization_for_another_request_or_subject_set_is_refused() -> Result<(), String> {
    let request = rc_request()?;
    let exact = authorization(&request, &RC_OPERATIONS)?;
    assert!(matches!(
        decide(&request, &exact, &empty_remote()).decision,
        Decision::Proceed { .. }
    ));
    type Change = fn(&mut VerifiedAuthorization) -> Result<(), String>;
    let variants: Vec<(&str, Change)> = vec![
        ("other request", |a| {
            a.transaction_request_id = stable_request()?.request_id()?;
            Ok(())
        }),
        ("other profile", |a| {
            a.profile = "stable_core".to_string();
            Ok(())
        }),
        ("other transaction kind", |a| {
            a.transaction_kind = "probe".to_string();
            Ok(())
        }),
        ("other source commit", |a| {
            a.source_commit = OTHER_SHA.to_string();
            Ok(())
        }),
        ("other tag ref", |a| {
            a.event_ref = "refs/tags/v0.11.0".to_string();
            Ok(())
        }),
        ("unusable tag object", |a| {
            a.expected_tag_object = "sha256:abc".to_string();
            Ok(())
        }),
        ("other bytes", |a| {
            a.subjects[0].digest = format!("sha256:{}", hex64('9'));
            Ok(())
        }),
        ("bare-hex digest", |a| {
            a.subjects[0].digest = hex64('a');
            Ok(())
        }),
        ("other class", |a| {
            a.subjects[0].class = "attestation_bundle".to_string();
            Ok(())
        }),
        ("fewer subjects", |a| {
            a.subjects.pop();
            Ok(())
        }),
    ];
    for (label, change) in variants {
        let mut authorization = exact.clone();
        change(&mut authorization)?;
        let planned = decide(&request, &authorization, &empty_remote());
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::AuthorizationMismatch,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn unreadable_remote_state_is_never_treated_as_absent() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let unavailable = || "HTTP 403".to_string();
    let mut tag = empty_remote();
    tag.tag = TagObservation::Unavailable {
        reason: unavailable(),
    };
    let mut releases = empty_remote();
    releases.tag = exact_tag();
    releases.releases = ReleasesObservation::Unavailable {
        reason: unavailable(),
    };
    let mut unread_asset = exact_asset(1, &request.subjects[0]);
    unread_asset.bytes = BytesObservation::Unavailable {
        reason: unavailable(),
    };
    let bytes = remote_with(&request, true, vec![unread_asset], true);
    for (label, remote) in [("tag", tag), ("releases", releases), ("asset bytes", bytes)] {
        let planned = decide(&request, &authorization, &remote);
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::RemoteUnavailable,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn conflicting_remote_objects_stop_and_are_never_replaced() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let mut moved_tag = empty_remote();
    moved_tag.tag = TagObservation::Present {
        object_sha: TAG_OBJECT.to_string(),
        peeled_sha: OTHER_SHA.to_string(),
        annotated: true,
    };
    let mut lightweight_tag = empty_remote();
    lightweight_tag.tag = TagObservation::Present {
        object_sha: CANDIDATE.to_string(),
        peeled_sha: CANDIDATE.to_string(),
        annotated: false,
    };
    // Annotated and at the candidate, but not the authorized tag object.
    let mut foreign_tag = empty_remote();
    foreign_tag.tag = TagObservation::Present {
        object_sha: FOREIGN_TAG_OBJECT.to_string(),
        peeled_sha: CANDIDATE.to_string(),
        annotated: true,
    };
    let mut other_bytes = exact_asset(1, &request.subjects[0]);
    other_bytes.bytes = BytesObservation::Downloaded { sha256: hex64('9') };
    // Same digest reported with another size is not exact either.
    let mut other_size = exact_asset(1, &request.subjects[0]);
    other_size.size += 1;
    let mut case_variant = exact_asset(1, &request.subjects[0]);
    case_variant.name = case_variant.name.to_uppercase();
    let mut unexpected = exact_asset(9, &request.subjects[0]);
    unexpected.name = "extra.bin".to_string();
    let twice = vec![
        exact_asset(1, &request.subjects[0]),
        exact_asset(2, &request.subjects[0]),
    ];
    let mut retitled = remote_with(&request, true, Vec::new(), true);
    let mut not_prerelease = retitled.clone();
    let mut other_tag = retitled.clone();
    let mut duplicated = retitled.clone();
    let mut release_without_tag = retitled.clone();
    release_without_tag.tag = TagObservation::Absent;
    for (remote, change) in [
        (&mut retitled, 0),
        (&mut not_prerelease, 1),
        (&mut other_tag, 2),
        (&mut duplicated, 3),
    ] {
        if let ReleasesObservation::Listed { releases } = &mut remote.releases {
            match change {
                0 => releases[0].title.push('!'),
                1 => releases[0].prerelease = false,
                2 => releases[0].tag = "v0.11.0".to_string(),
                _ => releases.push(releases[0].clone()),
            }
        }
    }
    let cases = [
        ("tag at another commit", moved_tag),
        ("lightweight tag", lightweight_tag),
        ("foreign annotated tag object at the candidate", foreign_tag),
        (
            "asset with other bytes",
            remote_with(&request, true, vec![other_bytes], true),
        ),
        (
            "asset with another size",
            remote_with(&request, true, vec![other_size], true),
        ),
        (
            "case-variant asset name",
            remote_with(&request, true, vec![case_variant], true),
        ),
        (
            "unexpected asset",
            remote_with(&request, true, vec![unexpected], true),
        ),
        (
            "asset attached twice",
            remote_with(&request, true, twice, true),
        ),
        ("retitled release", retitled),
        ("release flag differs", not_prerelease),
        ("release on another tag", other_tag),
        ("two releases for the tag", duplicated),
        ("release without its tag", release_without_tag),
    ];
    for (label, remote) in cases {
        let planned = decide(&request, &authorization, &remote);
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::RemoteConflict,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn finalization_waits_for_verified_provenance_of_every_subject() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let mut none = remote_with(&request, true, all_exact(&request), false);
    assert_eq!(
        stop_of(&decide(&request, &authorization, &none).decision)?,
        StopKind::AttestationNotVerified
    );
    attest_all(&mut none, &request);
    for state in [AttestationState::Missing, AttestationState::Unavailable] {
        let mut one_short = none.clone();
        one_short.attestations[1].state = state;
        assert_eq!(
            stop_of(&decide(&request, &authorization, &one_short).decision)?,
            StopKind::AttestationNotVerified
        );
    }
    assert_eq!(
        step_of(&decide(&request, &authorization, &none).decision)?,
        &Step::FinalizeRelease { release_id: 7 }
    );
    // An already-public release with unverified provenance is not complete.
    let public = remote_with(&request, false, all_exact(&request), false);
    assert_eq!(
        stop_of(&decide(&request, &authorization, &public).decision)?,
        StopKind::AttestationNotVerified
    );
    Ok(())
}

#[test]
fn a_public_release_missing_a_subject_is_an_incident_not_an_upload() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let public = remote_with(
        &request,
        false,
        vec![exact_asset(1, &request.subjects[0])],
        true,
    );
    let planned = decide(&request, &authorization, &public);
    assert_eq!(stop_of(&planned.decision)?, StopKind::PartialPublic);
    Ok(())
}

/// Independent invariant over every combination of tag, release and asset
/// state this model can observe: a mutation is proposed only when the object
/// it writes is absent and everything already present is exact.
#[test]
fn no_combination_of_remote_states_yields_a_replacing_or_premature_step() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let tags = [
        TagObservation::Absent,
        exact_tag(),
        TagObservation::Present {
            object_sha: TAG_OBJECT.to_string(),
            peeled_sha: OTHER_SHA.to_string(),
            annotated: true,
        },
        TagObservation::Present {
            object_sha: FOREIGN_TAG_OBJECT.to_string(),
            peeled_sha: CANDIDATE.to_string(),
            annotated: true,
        },
        TagObservation::Unavailable {
            reason: "timeout".to_string(),
        },
    ];
    // 0 absent, 1 exact, 2 other bytes, 3 unreadable, 4 attached twice.
    let asset_variant = |subject: &ExpectedSubject, variant: u8| -> Vec<RemoteAsset> {
        let mut asset = exact_asset(1, subject);
        match variant {
            0 => Vec::new(),
            1 => vec![asset],
            2 => {
                asset.bytes = BytesObservation::Downloaded { sha256: hex64('9') };
                vec![asset]
            }
            3 => {
                asset.bytes = BytesObservation::Unavailable {
                    reason: "timeout".to_string(),
                };
                vec![asset]
            }
            _ => vec![asset.clone(), asset],
        }
    };
    let mut combinations = 0usize;
    let mut proposed = BTreeSet::new();
    for tag in &tags {
        // 0 none, 1 draft, 2 public, 3 unavailable.
        for release_shape in 0u8..4 {
            for first in 0u8..5 {
                for second in 0u8..5 {
                    for attested in [false, true] {
                        let mut assets = asset_variant(&request.subjects[0], first);
                        assets.extend(asset_variant(&request.subjects[1], second));
                        let mut remote =
                            remote_with(&request, release_shape == 1, assets, attested);
                        remote.tag = tag.clone();
                        match release_shape {
                            0 => {
                                remote.releases = ReleasesObservation::Listed {
                                    releases: Vec::new(),
                                };
                            }
                            3 => {
                                remote.releases = ReleasesObservation::Unavailable {
                                    reason: "timeout".to_string(),
                                };
                            }
                            _ => {}
                        }
                        combinations += 1;
                        let classified =
                            classify(&request, &authorization.expected_tag_object, &remote);
                        let Decision::Proceed { step } =
                            decide(&request, &authorization, &remote).decision
                        else {
                            continue;
                        };
                        let clean = classified.assets.iter().all(|(_, state)| {
                            matches!(state, AssetState::Absent | AssetState::ExactPublicBytes)
                        });
                        let all_exact = classified
                            .assets
                            .iter()
                            .all(|(_, state)| *state == AssetState::ExactPublicBytes);
                        assert!(clean, "{step:?} proposed over {classified:?}");
                        match &step {
                            Step::CreateTag => {
                                assert_eq!(classified.tag, TagState::Absent);
                                assert_eq!(classified.release, ReleaseState::Absent);
                            }
                            Step::CreateDraftRelease => {
                                assert_eq!(classified.tag, TagState::ExactCandidate);
                                assert_eq!(classified.release, ReleaseState::Absent);
                            }
                            Step::UploadSubject { name } => {
                                assert_eq!(classified.tag, TagState::ExactCandidate);
                                assert_eq!(classified.release, ReleaseState::ExactDraft);
                                let state = classified
                                    .assets
                                    .iter()
                                    .find(|(asset, _)| asset == name)
                                    .map(|(_, state)| state.clone());
                                assert_eq!(state, Some(AssetState::Absent));
                            }
                            Step::FinalizeRelease { .. } => {
                                assert_eq!(classified.release, ReleaseState::ExactDraft);
                                assert!(all_exact && attested);
                            }
                            Step::Complete => {
                                assert_eq!(classified.tag, TagState::ExactCandidate);
                                assert_eq!(classified.release, ReleaseState::ExactPublic);
                                assert!(all_exact && attested);
                            }
                        }
                        proposed.insert(
                            format!("{step:?}")
                                .split([' ', '{'])
                                .next()
                                .map(str::to_string),
                        );
                    }
                }
            }
        }
    }
    assert_eq!(combinations, 5 * 4 * 5 * 5 * 2);
    // The sweep reached every kind of step, so the assertions above gripped.
    assert_eq!(proposed.len(), 5, "{proposed:?}");
    Ok(())
}

#[test]
fn plan_command_exits_nonzero_for_a_stop_and_refuses_loose_arguments() -> Result<(), String> {
    let request = rc_request()?;
    let dir = std::env::temp_dir().join(format!(
        "ripr-release-github-transaction-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let write = |name: &str, value: String| -> Result<String, String> {
        let path = dir.join(name);
        std::fs::write(&path, value).map_err(|error| error.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    };
    let json = |value: &dyn erased::Json| value.json();
    let request_path = write("request.json", json(&request)?)?;
    let authorization_path = write(
        "authorization.json",
        json(&authorization(&request, &RC_OPERATIONS)?)?,
    )?;
    let fresh = write("fresh.json", json(&empty_remote())?)?;
    let mut conflict = empty_remote();
    conflict.tag = TagObservation::Present {
        object_sha: TAG_OBJECT.to_string(),
        peeled_sha: OTHER_SHA.to_string(),
        annotated: true,
    };
    let conflicting = write("conflict.json", json(&conflict)?)?;
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
    let proceed = super::plan_command(&args(&fresh));
    let stopped = super::plan_command(&args(&conflicting));
    let mut twice = args(&fresh);
    twice.extend(["--inventory".to_string(), conflicting.clone()]);
    let duplicate = super::plan_command(&twice);
    let mut extra = args(&fresh);
    extra.push("--force".to_string());
    let loose = super::plan_command(&extra);
    let missing = super::plan_command(&args(&fresh)[..4]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(proceed, Ok(()));
    assert!(stopped.is_err_and(|error| error.contains("RemoteConflict")));
    assert!(duplicate.is_err_and(|error| error.contains("more than once")));
    assert!(loose.is_err_and(|error| error.contains("unexpected argument")));
    assert!(missing.is_err_and(|error| error.contains("missing --inventory")));
    Ok(())
}

mod erased {
    pub(super) trait Json {
        fn json(&self) -> Result<String, String>;
    }
    impl<T: serde::Serialize> Json for T {
        fn json(&self) -> Result<String, String> {
            serde_json::to_string(self).map_err(|error| error.to_string())
        }
    }
}

#[test]
fn an_inventory_for_another_repository_or_tag_is_refused() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let mut other_tag = empty_remote();
    other_tag.tag_name = "v0.11.0-rc.2".to_string();
    let mut other_repository = empty_remote();
    other_repository.repository = "EffortlessMetrics/ripr-swarm".to_string();
    for (label, remote) in [("tag", other_tag), ("repository", other_repository)] {
        let planned = decide(&request, &authorization, &remote);
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::InventoryMismatch,
            "{label}"
        );
        assert_eq!(planned.classified, None, "{label}");
    }
    // The same empty inventory for the right target proceeds.
    assert_eq!(
        step_of(&decide(&request, &authorization, &empty_remote()).decision)?,
        &Step::CreateTag
    );
    Ok(())
}

#[test]
fn conflicting_provenance_for_one_subject_stops_in_either_order() -> Result<(), String> {
    let request = rc_request()?;
    let authorization = authorization(&request, &RC_OPERATIONS)?;
    let base = remote_with(&request, true, all_exact(&request), true);
    assert_eq!(
        step_of(&decide(&request, &authorization, &base).decision)?,
        &Step::FinalizeRelease { release_id: 7 }
    );
    let duplicate = AttestationObservation {
        subject: request.subjects[0].name.clone(),
        state: AttestationState::Missing,
    };
    let mut verified_first = base.clone();
    verified_first.attestations.push(duplicate.clone());
    let mut missing_first = base.clone();
    missing_first.attestations.insert(0, duplicate);
    let mut verified_twice = base.clone();
    verified_twice
        .attestations
        .push(verified_twice.attestations[0].clone());
    for (label, remote) in [
        ("verified then missing", verified_first),
        ("missing then verified", missing_first),
        ("verified twice", verified_twice),
    ] {
        let planned = decide(&request, &authorization, &remote);
        assert_eq!(
            stop_of(&planned.decision)?,
            StopKind::AttestationNotVerified,
            "{label}"
        );
    }
    Ok(())
}
