use super::observe::{RegistryRead, Response, index_path, observe, split_status};
use super::plan::{
    CratesIoTransactionRequest, Decision, PUBLISH_OPERATION, PackageNode, REQUEST_SCHEMA,
    RegistryInventory, RegistryObservation, Step, StopKind, VerifiedAuthorization, decide,
    package_set_sha256,
};
use crate::blind_journey::sha256_hex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SUPPORT: &[u8] = b"support crate archive";
const FACADE: &[u8] = b"facade crate archive";
const SUPPORT_API: &str = "https://crates.io/api/v1/crates/ripr-contract/0.11.0";
const SUPPORT_INDEX: &str = "https://index.crates.io/ri/pr/ripr-contract";
const SUPPORT_ARCHIVE: &str =
    "https://static.crates.io/crates/ripr-contract/ripr-contract-0.11.0.crate";
const FACADE_API: &str = "https://crates.io/api/v1/crates/ripr/0.11.0";
const FACADE_INDEX: &str = "https://index.crates.io/ri/pr/ripr";
const FACADE_ARCHIVE: &str = "https://static.crates.io/crates/ripr/ripr-0.11.0.crate";

/// Answers only the scripted URLs; anything else gets no HTTP answer.
#[derive(Default)]
struct Scripted {
    answers: BTreeMap<String, (u16, Vec<u8>)>,
    asked: Vec<String>,
}

impl Scripted {
    fn text(&mut self, url: &str, status: u16, body: &str) {
        self.bytes(url, status, body.as_bytes());
    }
    fn bytes(&mut self, url: &str, status: u16, body: &[u8]) {
        self.answers
            .insert(url.to_string(), (status, body.to_vec()));
    }
}

impl RegistryRead for Scripted {
    fn get(&mut self, url: &str) -> Result<Response, String> {
        self.asked.push(url.to_string());
        match self.answers.get(url) {
            Some((status, body)) => Ok(Response {
                status: *status,
                body: body.clone(),
            }),
            None => Err(format!("no route to {url}")),
        }
    }
}

fn request() -> Result<CratesIoTransactionRequest, String> {
    let node = |name: &str, bytes: &[u8], requires: &[&str]| PackageNode {
        name: name.to_string(),
        version: "0.11.0".to_string(),
        crate_sha256: sha256_hex(bytes),
        size: bytes.len() as u64,
        requires: requires.iter().map(|name| name.to_string()).collect(),
    };
    let packages = vec![
        node("ripr-contract", SUPPORT, &[]),
        node("ripr", FACADE, &["ripr-contract"]),
    ];
    let seed = |seed: char, len: usize| std::iter::repeat_n(seed, len).collect::<String>();
    let request = CratesIoTransactionRequest {
        schema: REQUEST_SCHEMA.to_string(),
        registry: "crates.io".to_string(),
        candidate_sha: seed('1', 40),
        candidate_tree: seed('2', 40),
        ship_packet_sha256: seed('c', 64),
        stable_decision_sha256: seed('d', 64),
        package_set_sha256: package_set_sha256(&packages)?,
        packages,
    };
    match request.violations().as_slice() {
        [] => Ok(request),
        violations => Err(format!("fixture request rejected: {violations:?}")),
    }
}

fn authorization(request: &CratesIoTransactionRequest) -> Result<VerifiedAuthorization, String> {
    Ok(VerifiedAuthorization {
        authorization_id: "sha256:authorization".to_string(),
        transaction_request_id: request.request_id()?,
        allowed_operations: BTreeSet::from([PUBLISH_OPERATION.to_string()]),
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

fn not_found() -> String {
    json!({"errors": [{"detail": "crate does not have that version"}]}).to_string()
}

fn api_version(name: &str, bytes: &[u8], yanked: bool) -> String {
    json!({"version": {"crate": name, "num": "0.11.0", "yanked": yanked,
                       "checksum": sha256_hex(bytes)}})
    .to_string()
}

fn index_line(name: &str, version: &str, bytes: &[u8], yanked: bool) -> String {
    let line: Value =
        json!({"name": name, "vers": version, "cksum": sha256_hex(bytes), "yanked": yanked});
    format!("{line}\n")
}

/// Neither package published; the facade has older versions in the index.
fn nothing_published() -> Scripted {
    let mut registry = Scripted::default();
    registry.text(SUPPORT_API, 404, &not_found());
    registry.text(SUPPORT_INDEX, 404, "");
    registry.text(FACADE_API, 404, &not_found());
    registry.text(
        FACADE_INDEX,
        200,
        &index_line("ripr", "0.10.0", b"older", false),
    );
    registry
}

/// Both packages published with the exact bytes.
fn both_published() -> Scripted {
    let mut registry = Scripted::default();
    registry.text(
        SUPPORT_API,
        200,
        &api_version("ripr-contract", SUPPORT, false),
    );
    registry.text(
        SUPPORT_INDEX,
        200,
        &index_line("ripr-contract", "0.11.0", SUPPORT, false),
    );
    registry.bytes(SUPPORT_ARCHIVE, 200, SUPPORT);
    registry.text(FACADE_API, 200, &api_version("ripr", FACADE, false));
    let older = index_line("ripr", "0.10.0", b"older", false);
    let current = index_line("ripr", "0.11.0", FACADE, false);
    registry.text(FACADE_INDEX, 200, &format!("{older}{current}"));
    registry.bytes(FACADE_ARCHIVE, 200, FACADE);
    registry
}

fn states(inventory: &RegistryInventory) -> Vec<&RegistryObservation> {
    inventory
        .packages
        .iter()
        .map(|package| &package.registry)
        .collect()
}

fn exact(bytes: &[u8]) -> RegistryObservation {
    RegistryObservation::Present {
        yanked: false,
        downloaded_sha256: Some(sha256_hex(bytes)),
    }
}

fn step(decision: &Decision) -> Option<&Step> {
    match decision {
        Decision::Proceed { step } => Some(step),
        Decision::Stop { .. } => None,
    }
}

fn stop_kind(decision: &Decision) -> Option<StopKind> {
    match decision {
        Decision::Stop { stop } => Some(stop.kind),
        Decision::Proceed { .. } => None,
    }
}

#[test]
fn transfer_output_splits_into_body_and_trailing_status() {
    let parsed = split_status(b"{\"a\":1}404").map(|r| (r.status, r.body));
    assert_eq!(parsed, Some((404, b"{\"a\":1}".to_vec())));
    // A binary body that itself ends in digits keeps them.
    let parsed = split_status(b"\x1f\x8b123200").map(|r| (r.status, r.body));
    assert_eq!(parsed, Some((200, b"\x1f\x8b123".to_vec())));
    assert_eq!(split_status(b"200").map(|r| r.body), Some(Vec::new()));
    for refused in [&b""[..], b"20", b"body", b"body000", b"body999", b"body2x0"] {
        assert!(split_status(refused).is_none(), "{refused:?}");
    }
}

#[test]
fn index_paths_follow_the_sparse_layout() {
    assert_eq!(index_path("a"), "1/a");
    assert_eq!(index_path("ab"), "2/ab");
    assert_eq!(index_path("abc"), "3/a/abc");
    assert_eq!(index_path("ripr"), "ri/pr/ripr");
    assert_eq!(index_path("Ripr-Contract"), "ri/pr/ripr-contract");
}

#[test]
fn absent_and_exact_need_the_api_and_the_index_to_agree() -> Result<(), String> {
    let request = request()?;
    let auth = authorization(&request)?;

    let mut registry = nothing_published();
    let inventory = observe(&mut registry, &request);
    assert_eq!(
        states(&inventory),
        vec![&RegistryObservation::Absent, &RegistryObservation::Absent]
    );
    assert!(inventory.attempted.is_empty());
    // Nothing is downloaded for a version neither source lists.
    assert!(!registry.asked.iter().any(|url| url.ends_with(".crate")));
    let planned = decide(&request, &auth, &inventory);
    let expected = Step::Publish {
        name: "ripr-contract".to_string(),
    };
    assert_eq!(step(&planned.decision), Some(&expected));

    let inventory = observe(&mut both_published(), &request);
    assert_eq!(states(&inventory), vec![&exact(SUPPORT), &exact(FACADE)]);
    let planned = decide(&request, &auth, &inventory);
    assert_eq!(step(&planned.decision), Some(&Step::Complete));
    Ok(())
}

#[test]
fn a_disagreement_or_a_non_answer_is_unavailable_never_absent() -> Result<(), String> {
    let request = request()?;
    let auth = authorization(&request)?;
    let mut cases: Vec<(&str, Scripted)> = Vec::new();

    // Published, index not caught up: a dependent could not resolve it yet.
    let mut index_behind = both_published();
    index_behind.text(SUPPORT_INDEX, 404, "");
    cases.push(("index behind the API", index_behind));
    let mut api_behind = nothing_published();
    api_behind.text(
        SUPPORT_INDEX,
        200,
        &index_line("ripr-contract", "0.11.0", SUPPORT, false),
    );
    cases.push(("API behind the index", api_behind));
    let mut other_bytes = both_published();
    other_bytes.text(
        SUPPORT_INDEX,
        200,
        &index_line("ripr-contract", "0.11.0", b"other", false),
    );
    cases.push(("sources disagree on bytes", other_bytes));
    let mut rate_limited = nothing_published();
    rate_limited.text(SUPPORT_API, 429, &not_found());
    cases.push(("rate limited", rate_limited));
    let mut plain_404 = nothing_published();
    plain_404.text(SUPPORT_API, 404, "<html>not found</html>");
    cases.push(("a 404 that is not the registry's", plain_404));
    let mut empty_errors = nothing_published();
    empty_errors.text(SUPPORT_API, 404, "{\"errors\":[]}");
    cases.push(("a 404 naming no error", empty_errors));
    let mut no_api = nothing_published();
    no_api.answers.remove(SUPPORT_API);
    cases.push(("no API answer", no_api));
    let mut no_index = nothing_published();
    no_index.answers.remove(SUPPORT_INDEX);
    cases.push(("no index answer", no_index));
    let mut index_error = nothing_published();
    index_error.text(SUPPORT_INDEX, 503, "");
    cases.push(("index server error", index_error));
    let mut bad_line = nothing_published();
    bad_line.text(SUPPORT_INDEX, 200, "{\"name\":\"ripr-contract\"\n");
    cases.push(("unparseable index line", bad_line));
    let mut wrong_crate = nothing_published();
    wrong_crate.text(
        SUPPORT_INDEX,
        200,
        &index_line("ripr", "0.10.0", b"older", false),
    );
    cases.push(("index file of another crate", wrong_crate));
    let mut folded_name = both_published();
    folded_name.text(
        SUPPORT_API,
        200,
        &api_version("ripr_contract", SUPPORT, false),
    );
    cases.push(("API answers another spelling", folded_name));
    let mut listed_twice = both_published();
    let line = index_line("ripr-contract", "0.11.0", SUPPORT, false);
    listed_twice.text(SUPPORT_INDEX, 200, &format!("{line}{line}"));
    cases.push(("version listed twice", listed_twice));

    for (name, mut registry) in cases {
        let inventory = observe(&mut registry, &request);
        let support = &inventory.packages[0].registry;
        assert!(
            matches!(support, RegistryObservation::Unavailable { .. }),
            "{name}: {support:?}"
        );
        let planned = decide(&request, &auth, &inventory);
        assert_eq!(
            stop_kind(&planned.decision),
            Some(StopKind::RegistryUnavailable),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn a_present_version_is_exact_only_by_its_downloaded_archive() -> Result<(), String> {
    let request = request()?;
    let auth = authorization(&request)?;
    let unread = RegistryObservation::Present {
        yanked: false,
        downloaded_sha256: None,
    };
    let mut cases: Vec<(&str, Scripted)> = Vec::new();
    let mut no_archive = both_published();
    no_archive.answers.remove(SUPPORT_ARCHIVE);
    cases.push(("archive not answered", no_archive));
    let mut forbidden = both_published();
    forbidden.bytes(SUPPORT_ARCHIVE, 403, SUPPORT);
    cases.push(("archive forbidden", forbidden));
    // Bytes that differ from the registry's own checksum were misread.
    let mut corrupted = both_published();
    corrupted.bytes(SUPPORT_ARCHIVE, 200, b"support crate archivE");
    cases.push(("archive differs from the listed checksum", corrupted));
    for (name, mut registry) in cases {
        let inventory = observe(&mut registry, &request);
        assert_eq!(states(&inventory), vec![&unread, &exact(FACADE)], "{name}");
        let planned = decide(&request, &auth, &inventory);
        assert_eq!(
            stop_kind(&planned.decision),
            Some(StopKind::RegistryUnavailable),
            "{name}"
        );
    }

    // The registry consistently holds other bytes: an immutable conflict.
    let mut other = both_published();
    other.text(
        SUPPORT_API,
        200,
        &api_version("ripr-contract", b"other", false),
    );
    other.text(
        SUPPORT_INDEX,
        200,
        &index_line("ripr-contract", "0.11.0", b"other", false),
    );
    other.bytes(SUPPORT_ARCHIVE, 200, b"other");
    let planned = decide(&request, &auth, &observe(&mut other, &request));
    assert_eq!(
        stop_kind(&planned.decision),
        Some(StopKind::ImmutableConflict)
    );

    // Yanked in either source is yanked.
    for yanked_in_api in [true, false] {
        let mut yanked = both_published();
        yanked.text(
            SUPPORT_API,
            200,
            &api_version("ripr-contract", SUPPORT, yanked_in_api),
        );
        yanked.text(
            SUPPORT_INDEX,
            200,
            &index_line("ripr-contract", "0.11.0", SUPPORT, !yanked_in_api),
        );
        let inventory = observe(&mut yanked, &request);
        assert_eq!(
            inventory.packages[0].registry,
            RegistryObservation::Present {
                yanked: true,
                downloaded_sha256: Some(sha256_hex(SUPPORT)),
            }
        );
        let planned = decide(&request, &auth, &inventory);
        assert_eq!(
            stop_kind(&planned.decision),
            Some(StopKind::ImmutableConflict)
        );
    }
    Ok(())
}
