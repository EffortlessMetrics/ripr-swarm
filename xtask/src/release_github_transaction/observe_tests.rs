use super::inventory::{
    AssetState, AttestationState, ReleaseState, ReleasesObservation, RemoteInventory,
    TagObservation, TagState, classify,
};
use super::model::{
    ExpectedSubject, GitHubReleaseTransactionRequest, Mode, Provenance, REQUEST_SCHEMA,
    WithheldChannels, subject_inventory_sha256,
};
use super::observe::{
    GitHubRead, ProvenanceCheck, Response, names_subject, observe, parse_included,
};
use super::plan::{AuthorizedSubject, Decision, Step, StopKind, VerifiedAuthorization, decide};
use crate::blind_journey::sha256_hex;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const REPO: &str = "EffortlessMetrics/ripr";
const TAG: &str = "v0.11.0-rc.1";
const CANDIDATE: &str = "1111111111111111111111111111111111111111";
const TAG_OBJECT: &str = "3333333333333333333333333333333333333333";
const VSIX: &[u8] = b"vsix bytes";
const MANIFEST: &[u8] = b"server manifest bytes";

/// Answers only the scripted paths; anything else gets no HTTP answer.
#[derive(Default)]
struct Scripted {
    answers: BTreeMap<String, (u16, Vec<u8>)>,
    asked: Vec<String>,
}

impl Scripted {
    fn json(&mut self, path: &str, status: u16, value: Value) {
        self.answers
            .insert(path.to_string(), (status, value.to_string().into_bytes()));
    }
    fn bytes(&mut self, path: &str, status: u16, body: &[u8]) {
        self.answers
            .insert(path.to_string(), (status, body.to_vec()));
    }
}

impl GitHubRead for Scripted {
    fn get(&mut self, path: &str, _accept: &str) -> Result<Response, String> {
        self.asked.push(path.to_string());
        match self.answers.get(path) {
            Some((status, body)) => Ok(Response {
                status: *status,
                body: body.clone(),
            }),
            None => Err(format!("no route to {path}")),
        }
    }
}

/// Answers every check with one verdict and records which subjects were checked.
struct Attested {
    verdict: AttestationState,
    checked: Vec<(String, Vec<u8>)>,
}

impl Attested {
    fn all() -> Self {
        Self {
            verdict: AttestationState::Verified,
            checked: Vec::new(),
        }
    }
}

impl ProvenanceCheck for Attested {
    fn verify(
        &mut self,
        _request: &GitHubReleaseTransactionRequest,
        subject: &ExpectedSubject,
        bytes: &[u8],
    ) -> AttestationState {
        self.checked.push((subject.name.clone(), bytes.to_vec()));
        self.verdict
    }
}

fn hex64(seed: char) -> String {
    std::iter::repeat_n(seed, 64).collect()
}

fn request() -> Result<GitHubReleaseTransactionRequest, String> {
    let subject = |name: &str, kind: &str, bytes: &[u8]| ExpectedSubject {
        name: name.to_string(),
        kind: kind.to_string(),
        size: bytes.len() as u64,
        sha256: sha256_hex(bytes),
    };
    let subjects = vec![
        subject("ripr-0.11.0.vsix", "vsix", VSIX),
        subject("ripr-server-manifest.json", "server_manifest", MANIFEST),
    ];
    let request = GitHubReleaseTransactionRequest {
        schema: REQUEST_SCHEMA.to_string(),
        repository: REPO.to_string(),
        mode: Mode::PublicRc,
        product_version: "0.11.0".to_string(),
        candidate_sha: CANDIDATE.to_string(),
        candidate_tree: hex64('2')[..40].to_string(),
        tag: TAG.to_string(),
        title: "ripr 0.11.0-rc.1".to_string(),
        body_sha256: sha256_hex(b"notes"),
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
    };
    match request.violations().as_slice() {
        [] => Ok(request),
        violations => Err(format!("fixture request rejected: {violations:?}")),
    }
}

fn authorization(
    request: &GitHubReleaseTransactionRequest,
) -> Result<VerifiedAuthorization, String> {
    Ok(VerifiedAuthorization {
        authorization_id: format!("sha256:{}", hex64('f')),
        profile: "public_rc".to_string(),
        transaction_kind: "github".to_string(),
        transaction_request_id: request.request_id()?,
        source_commit: request.candidate_sha.clone(),
        event_ref: format!("refs/tags/{}", request.tag),
        expected_tag_object: TAG_OBJECT.to_string(),
        allowed_operations: [
            "create_annotated_tag",
            "github_prerelease_create",
            "github_asset_upload",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect(),
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

const REPO_PATH: &str = "repos/EffortlessMetrics/ripr";
const TAG_PATH: &str = "repos/EffortlessMetrics/ripr/git/ref/tags/v0.11.0-rc.1";
const TAG_OBJECT_PATH: &str =
    "repos/EffortlessMetrics/ripr/git/tags/3333333333333333333333333333333333333333";
const RELEASES_PAGE_1: &str = "repos/EffortlessMetrics/ripr/releases?per_page=100&page=1";
const RELEASES_PAGE_2: &str = "repos/EffortlessMetrics/ripr/releases?per_page=100&page=2";
const ASSETS_PAGE_1: &str = "repos/EffortlessMetrics/ripr/releases/7/assets?per_page=100&page=1";
const VSIX_BYTES_PATH: &str = "repos/EffortlessMetrics/ripr/releases/assets/71";
const MANIFEST_BYTES_PATH: &str = "repos/EffortlessMetrics/ripr/releases/assets/72";

/// A writable repository with no tag and no release.
fn empty_remote() -> Scripted {
    let mut api = Scripted::default();
    api.json(
        REPO_PATH,
        200,
        json!({"full_name": REPO, "permissions": {"push": true}}),
    );
    api.json(TAG_PATH, 404, json!({"message": "Not Found"}));
    api.json(RELEASES_PAGE_1, 200, json!([]));
    api
}

fn annotated_tag(api: &mut Scripted) {
    api.json(
        TAG_PATH,
        200,
        json!({"ref": format!("refs/tags/{TAG}"), "object": {"type": "tag", "sha": TAG_OBJECT}}),
    );
    api.json(
        TAG_OBJECT_PATH,
        200,
        json!({"object": {"type": "commit", "sha": CANDIDATE}}),
    );
}

fn release_json(tag: &str, id: u64) -> Value {
    json!({"id": id, "tag_name": tag, "draft": true, "prerelease": true,
           "name": "ripr 0.11.0-rc.1", "body": "notes"})
}

fn asset_json(id: u64, name: &str, size: usize) -> Value {
    json!({"id": id, "name": name, "size": size, "state": "uploaded"})
}

/// The exact tag and an exact draft release carrying both subjects.
fn complete_draft() -> Scripted {
    let mut api = empty_remote();
    annotated_tag(&mut api);
    api.json(RELEASES_PAGE_1, 200, json!([release_json(TAG, 7)]));
    api.json(
        ASSETS_PAGE_1,
        200,
        json!([
            asset_json(71, "ripr-0.11.0.vsix", VSIX.len()),
            asset_json(72, "ripr-server-manifest.json", MANIFEST.len()),
        ]),
    );
    api.bytes(VSIX_BYTES_PATH, 200, VSIX);
    api.bytes(MANIFEST_BYTES_PATH, 200, MANIFEST);
    api
}

fn tag_unavailable(inventory: &RemoteInventory) -> bool {
    matches!(inventory.tag, TagObservation::Unavailable { .. })
}

fn releases_unavailable(inventory: &RemoteInventory) -> bool {
    matches!(inventory.releases, ReleasesObservation::Unavailable { .. })
}

fn stop_kind(decision: &Decision) -> Option<StopKind> {
    match decision {
        Decision::Stop { stop } => Some(stop.kind),
        Decision::Proceed { .. } => None,
    }
}

#[test]
fn gh_include_output_splits_into_status_and_exact_body() {
    // The shape gh 2.89 prints: a bare-LF status line, CRLF headers.
    let parsed =
        parse_included(b"HTTP/1.1 404 Not Found\nX-A: 1\r\nX-B: 2\r\n\r\n{\"a\":1}\r\n\r\ntail");
    let parsed = parsed.map(|response| (response.status, response.body));
    assert_eq!(parsed, Some((404, b"{\"a\":1}\r\n\r\ntail".to_vec())));
    let empty_body = parse_included(b"HTTP/2.0 200 OK\nX-A: 1\r\n\r\n");
    assert_eq!(
        empty_body.map(|r| (r.status, r.body)),
        Some((200, Vec::new()))
    );
    for refused in [
        &b""[..],
        b"gh: Not Found (HTTP 404)\n",
        b"{\"message\":\"Not Found\"}\n\r\n\r\n",
        b"HTTP/1.1 200 OK\nX-A: 1\r\n",
        b"HTTP/1.1 OK\nX-A: 1\r\n\r\nbody",
    ] {
        assert!(parse_included(refused).is_none(), "{refused:?}");
    }
}

#[test]
fn a_writable_repository_with_nothing_published_reads_as_absent() -> Result<(), String> {
    let request = request()?;
    let mut api = empty_remote();
    let inventory = observe(&mut api, &mut Attested::all(), &request);
    assert_eq!(inventory.tag, TagObservation::Absent);
    assert_eq!(
        inventory.releases,
        ReleasesObservation::Listed {
            releases: Vec::new()
        }
    );
    let planned = decide(&request, &authorization(&request)?, &inventory);
    assert_eq!(
        planned.decision,
        Decision::Proceed {
            step: Step::CreateTag
        }
    );
    Ok(())
}

#[test]
fn not_found_is_absent_only_where_this_token_could_see_the_object() -> Result<(), String> {
    let request = request()?;
    let cases: [(&str, u16, Value); 5] = [
        (
            "read-only token",
            200,
            json!({"full_name": REPO, "permissions": {"push": false}}),
        ),
        ("no permissions field", 200, json!({"full_name": REPO})),
        ("repository hidden", 404, json!({"message": "Not Found"})),
        (
            "another repository",
            200,
            json!({"full_name": "x/y", "permissions": {"push": true}}),
        ),
        ("rate limited", 403, json!({"message": "rate limit"})),
    ];
    for (name, status, body) in cases {
        let mut api = empty_remote();
        api.json(REPO_PATH, status, body);
        let inventory = observe(&mut api, &mut Attested::all(), &request);
        assert!(tag_unavailable(&inventory), "{name}: tag");
        assert!(releases_unavailable(&inventory), "{name}: releases");
        // The tag and release 404s were never consulted.
        assert_eq!(api.asked, vec![REPO_PATH.to_string()], "{name}");
        let planned = decide(&request, &authorization(&request)?, &inventory);
        assert_eq!(
            stop_kind(&planned.decision),
            Some(StopKind::RemoteUnavailable)
        );
    }
    Ok(())
}

#[test]
fn a_tag_is_exact_only_as_an_annotated_tag_of_the_candidate() -> Result<(), String> {
    let request = request()?;
    let state = |api: &mut Scripted| {
        classify(
            &request,
            TAG_OBJECT,
            &observe(api, &mut Attested::all(), &request),
        )
        .tag
    };

    let mut api = empty_remote();
    annotated_tag(&mut api);
    assert_eq!(state(&mut api), TagState::ExactCandidate);

    let reference = |kind: &str, sha: &str| json!({"ref": format!("refs/tags/{TAG}"), "object": {"type": kind, "sha": sha}});
    let mut lightweight = empty_remote();
    lightweight.json(TAG_PATH, 200, reference("commit", CANDIDATE));
    assert_eq!(state(&mut lightweight), TagState::ConflictingTargetOrObject);

    let mut elsewhere = empty_remote();
    annotated_tag(&mut elsewhere);
    elsewhere.json(
        TAG_OBJECT_PATH,
        200,
        json!({"object": {"type": "commit", "sha": TAG_OBJECT}}),
    );
    assert_eq!(state(&mut elsewhere), TagState::ConflictingTargetOrObject);

    // A tag of a tag, even one whose inner object is named like the candidate.
    let mut nested = empty_remote();
    annotated_tag(&mut nested);
    nested.json(
        TAG_OBJECT_PATH,
        200,
        json!({"object": {"type": "tag", "sha": CANDIDATE}}),
    );
    assert_eq!(state(&mut nested), TagState::ConflictingTargetOrObject);

    // Another annotated tag object (another tagger, date or message) that
    // still peels to the candidate is not the authorized tag.
    const FOREIGN: &str = "4444444444444444444444444444444444444444";
    let mut foreign = empty_remote();
    foreign.json(TAG_PATH, 200, reference("tag", FOREIGN));
    foreign.json(
        &format!("{REPO_PATH}/git/tags/{FOREIGN}"),
        200,
        json!({"object": {"type": "commit", "sha": CANDIDATE}}),
    );
    let observed = observe(&mut foreign, &mut Attested::all(), &request).tag;
    assert_eq!(
        observed,
        TagObservation::Present {
            object_sha: FOREIGN.to_string(),
            peeled_sha: CANDIDATE.to_string(),
            annotated: true,
        }
    );
    assert_eq!(state(&mut foreign), TagState::ConflictingTargetOrObject);

    let mut unreadable: Vec<(&str, Scripted)> = Vec::new();
    let mut server_error = empty_remote();
    server_error.json(TAG_PATH, 500, json!({}));
    unreadable.push(("server error", server_error));
    let mut forbidden = empty_remote();
    forbidden.json(TAG_PATH, 403, json!({}));
    unreadable.push(("forbidden", forbidden));
    let mut no_answer = empty_remote();
    no_answer.answers.remove(TAG_PATH);
    unreadable.push(("no answer", no_answer));
    let mut not_json = empty_remote();
    not_json.bytes(TAG_PATH, 200, b"<html>");
    unreadable.push(("not json", not_json));
    let mut other_ref = empty_remote();
    other_ref.json(
        TAG_PATH,
        200,
        json!({"ref": "refs/tags/v0.11.0-rc.10", "object": {"type": "commit", "sha": CANDIDATE}}),
    );
    unreadable.push(("another ref", other_ref));
    let mut tag_object_missing = empty_remote();
    tag_object_missing.json(TAG_PATH, 200, reference("tag", TAG_OBJECT));
    tag_object_missing.json(TAG_OBJECT_PATH, 404, json!({}));
    unreadable.push(("tag object missing", tag_object_missing));
    for (name, mut api) in unreadable {
        assert_eq!(state(&mut api), TagState::UnknownOrUnavailable, "{name}");
    }
    Ok(())
}

#[test]
fn an_asset_is_exact_only_by_its_downloaded_bytes() -> Result<(), String> {
    let request = request()?;
    let assets = |api: &mut Scripted| {
        classify(
            &request,
            TAG_OBJECT,
            &observe(api, &mut Attested::all(), &request),
        )
        .assets
    };
    let vsix = |state: AssetState| ("ripr-0.11.0.vsix".to_string(), state);
    let manifest = (
        "ripr-server-manifest.json".to_string(),
        AssetState::ExactPublicBytes,
    );

    let mut exact = complete_draft();
    let inventory = observe(&mut exact, &mut Attested::all(), &request);
    let classified = classify(&request, TAG_OBJECT, &inventory);
    assert_eq!(classified.release, ReleaseState::ExactDraft);
    assert_eq!(
        classified.assets,
        vec![vsix(AssetState::ExactPublicBytes), manifest.clone()]
    );
    // Provenance was checked over the downloaded bytes of both subjects.
    let mut attested = Attested::all();
    let inventory = observe(&mut complete_draft(), &mut attested, &request);
    assert_eq!(
        attested.checked,
        vec![
            ("ripr-0.11.0.vsix".to_string(), VSIX.to_vec()),
            ("ripr-server-manifest.json".to_string(), MANIFEST.to_vec()),
        ]
    );
    let planned = decide(&request, &authorization(&request)?, &inventory);
    assert_eq!(
        planned.decision,
        Decision::Proceed {
            step: Step::FinalizeRelease { release_id: 7 }
        }
    );
    // A check that does not verify stops the plan before finalization.
    let mut unverified = Attested::all();
    unverified.verdict = AttestationState::Unavailable;
    let inventory = observe(&mut complete_draft(), &mut unverified, &request);
    let planned = decide(&request, &authorization(&request)?, &inventory);
    assert_eq!(
        stop_kind(&planned.decision),
        Some(StopKind::AttestationNotVerified)
    );

    // Bytes that are not the subject's are never offered for verification,
    // even to a check that would accept anything.
    let mut attested = Attested::all();
    let mut wrong = complete_draft();
    wrong.bytes(VSIX_BYTES_PATH, 200, b"VSIX BYTES");
    let inventory = observe(&mut wrong, &mut attested, &request);
    let checked: Vec<&str> = attested
        .checked
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(checked, vec!["ripr-server-manifest.json"]);
    assert_eq!(
        inventory.attestations[0].state,
        AttestationState::Unavailable
    );
    assert_eq!(inventory.attestations[1].state, AttestationState::Verified);

    // Same name and listed size, other bytes.
    let mut other_bytes = complete_draft();
    other_bytes.bytes(VSIX_BYTES_PATH, 200, b"VSIX BYTES");
    assert_eq!(
        assets(&mut other_bytes),
        vec![vsix(AssetState::ConflictingBytes), manifest.clone()]
    );

    let mut unreadable: Vec<(&str, Scripted)> = Vec::new();
    let mut short = complete_draft();
    short.bytes(VSIX_BYTES_PATH, 200, b"vsix");
    unreadable.push(("truncated download", short));
    let mut forbidden = complete_draft();
    forbidden.bytes(VSIX_BYTES_PATH, 403, VSIX);
    unreadable.push(("forbidden download", forbidden));
    let mut no_answer = complete_draft();
    no_answer.answers.remove(VSIX_BYTES_PATH);
    unreadable.push(("no answer", no_answer));
    let mut uploading = complete_draft();
    uploading.json(
        ASSETS_PAGE_1,
        200,
        json!([
            {"id": 71, "name": "ripr-0.11.0.vsix", "size": VSIX.len(), "state": "starter"},
            asset_json(72, "ripr-server-manifest.json", MANIFEST.len()),
        ]),
    );
    unreadable.push(("upload not complete", uploading));
    for (name, mut api) in unreadable {
        assert_eq!(
            assets(&mut api),
            vec![vsix(AssetState::Unavailable), manifest.clone()],
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn unexpected_and_case_variant_objects_are_reported_not_hidden() -> Result<(), String> {
    let request = request()?;

    let mut extra = complete_draft();
    extra.json(
        ASSETS_PAGE_1,
        200,
        json!([
            asset_json(71, "ripr-0.11.0.vsix", VSIX.len()),
            asset_json(72, "ripr-server-manifest.json", MANIFEST.len()),
            asset_json(73, "notes.txt", 5),
        ]),
    );
    let classified = classify(
        &request,
        TAG_OBJECT,
        &observe(&mut extra, &mut Attested::all(), &request),
    );
    assert_eq!(classified.unexpected_assets, vec!["notes.txt".to_string()]);
    assert!(!extra.asked.iter().any(|path| path.ends_with("/assets/73")));

    // A release under a case-variant tag name is kept and classifies as a conflict;
    // a release under an unrelated tag is not this request's.
    let mut variant = empty_remote();
    variant.json(
        RELEASES_PAGE_1,
        200,
        json!([release_json("v0.10.0", 5), release_json("V0.11.0-RC.1", 7)]),
    );
    variant.json(ASSETS_PAGE_1, 200, json!([]));
    let classified = classify(
        &request,
        TAG_OBJECT,
        &observe(&mut variant, &mut Attested::all(), &request),
    );
    assert_eq!(classified.release, ReleaseState::ConflictingTagTarget);
    assert!(
        !variant
            .asked
            .iter()
            .any(|path| path.contains("/releases/5/"))
    );

    let mut duplicate = empty_remote();
    duplicate.json(
        RELEASES_PAGE_1,
        200,
        json!([release_json(TAG, 7), release_json(TAG, 8)]),
    );
    duplicate.json(ASSETS_PAGE_1, 200, json!([]));
    duplicate.json(
        "repos/EffortlessMetrics/ripr/releases/8/assets?per_page=100&page=1",
        200,
        json!([]),
    );
    let classified = classify(
        &request,
        TAG_OBJECT,
        &observe(&mut duplicate, &mut Attested::all(), &request),
    );
    assert_eq!(classified.release, ReleaseState::DuplicateOrAmbiguous);
    Ok(())
}

#[test]
fn an_incomplete_release_listing_is_unavailable_never_a_shorter_list() -> Result<(), String> {
    let request = request()?;
    let full_page: Vec<Value> = (0..100)
        .map(|index| release_json("v0.10.0", 1000 + index))
        .collect();

    // The request's release is on the second page.
    let mut second_page = empty_remote();
    second_page.json(RELEASES_PAGE_1, 200, Value::Array(full_page.clone()));
    second_page.json(RELEASES_PAGE_2, 200, json!([release_json(TAG, 7)]));
    second_page.json(ASSETS_PAGE_1, 200, json!([]));
    let classified = classify(
        &request,
        TAG_OBJECT,
        &observe(&mut second_page, &mut Attested::all(), &request),
    );
    assert_eq!(classified.release, ReleaseState::ExactDraft);

    let mut unreadable: Vec<(&str, Scripted)> = Vec::new();
    let mut page_two_fails = empty_remote();
    page_two_fails.json(RELEASES_PAGE_1, 200, Value::Array(full_page.clone()));
    page_two_fails.json(RELEASES_PAGE_2, 502, json!({}));
    unreadable.push(("second page fails", page_two_fails));
    let mut page_two_missing = empty_remote();
    page_two_missing.json(RELEASES_PAGE_1, 200, Value::Array(full_page));
    page_two_missing.json(RELEASES_PAGE_2, 404, json!({}));
    unreadable.push(("second page 404", page_two_missing));
    let mut listing_404 = empty_remote();
    listing_404.json(RELEASES_PAGE_1, 404, json!({}));
    unreadable.push(("listing 404", listing_404));
    let mut not_a_list = empty_remote();
    not_a_list.json(RELEASES_PAGE_1, 200, json!({"message": "ok"}));
    unreadable.push(("not a list", not_a_list));
    let mut untagged = empty_remote();
    untagged.json(
        RELEASES_PAGE_1,
        200,
        json!([{"id": 7, "draft": true, "prerelease": true}]),
    );
    unreadable.push(("release without a tag name", untagged));
    let mut assets_fail = complete_draft();
    assets_fail.json(ASSETS_PAGE_1, 500, json!({}));
    unreadable.push(("asset listing fails", assets_fail));
    let mut asset_without_size = complete_draft();
    asset_without_size.json(
        ASSETS_PAGE_1,
        200,
        json!([{"id": 71, "name": "ripr-0.11.0.vsix"}]),
    );
    unreadable.push(("asset without size", asset_without_size));
    for (name, mut api) in unreadable {
        let inventory = observe(&mut api, &mut Attested::all(), &request);
        assert!(releases_unavailable(&inventory), "{name}");
        let planned = decide(&request, &authorization(&request)?, &inventory);
        assert_eq!(
            stop_kind(&planned.decision),
            Some(StopKind::RemoteUnavailable),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn a_verified_statement_must_name_the_subject_and_its_digest() -> Result<(), String> {
    let request = request()?;
    let subject = &request.subjects[0];
    let statement = |name: &str, sha256: &str| {
        json!([{"verificationResult": {"statement": {"subject": [
            {"name": "other.txt", "digest": {"sha256": hex64('9')}},
            {"name": name, "digest": {"sha256": sha256}},
        ]}}}])
        .to_string()
    };
    assert!(names_subject(
        statement(&subject.name, &subject.sha256).as_bytes(),
        subject
    ));
    let refused = [
        statement("RIPR-0.11.0.VSIX", &subject.sha256),
        statement(&subject.name, &hex64('9')),
        "[]".to_string(),
        "{}".to_string(),
        String::new(),
        json!([{"verificationResult": {"statement": {}}}]).to_string(),
    ];
    for output in refused {
        assert!(!names_subject(output.as_bytes(), subject), "{output}");
    }
    Ok(())
}

#[test]
fn the_request_names_one_workflow_in_its_repository_and_one_full_ref() -> Result<(), String> {
    let workflow = "provenance.signer_workflow must be <repository>/.github/workflows/<file>.yml";
    let source_ref = "provenance.source_ref must be refs/heads/<name> or refs/tags/<name>";
    let cases = [
        (
            "OtherOrg/ripr/.github/workflows/release.yml",
            "refs/heads/main",
            workflow,
        ),
        (
            "EffortlessMetrics/ripr/.github/workflows/release.yml@refs/heads/main",
            "refs/heads/main",
            workflow,
        ),
        (
            "EffortlessMetrics/ripr/.github/workflows/../release.yml",
            "refs/heads/main",
            workflow,
        ),
        (
            "EffortlessMetrics/ripr/.github/workflows/release.sh",
            "refs/heads/main",
            workflow,
        ),
        (
            "EffortlessMetrics/ripr/.github/workflows/release.yml",
            "main",
            source_ref,
        ),
        (
            "EffortlessMetrics/ripr/.github/workflows/release.yml",
            "refs/pull/1/merge",
            source_ref,
        ),
        (
            "EffortlessMetrics/ripr/.github/workflows/release.yml",
            "refs/heads/",
            source_ref,
        ),
        (
            "EffortlessMetrics/ripr/.github/workflows/release.yml",
            "refs/heads/a//b",
            source_ref,
        ),
    ];
    for (signer_workflow, named_ref, expected) in cases {
        let mut request = request()?;
        request.provenance.signer_workflow = signer_workflow.to_string();
        request.provenance.source_ref = named_ref.to_string();
        assert_eq!(
            request.violations(),
            vec![expected.to_string()],
            "{signer_workflow} {named_ref}"
        );
    }
    let mut tagged = request()?;
    tagged.provenance.source_ref = "refs/tags/v0.11.0-rc.1".to_string();
    tagged.provenance.signer_workflow =
        "EffortlessMetrics/ripr/.github/workflows/publish-server.yaml".to_string();
    assert_eq!(tagged.violations(), Vec::<String>::new());
    Ok(())
}
