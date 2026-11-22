//! Packet schema and profile-matrix controls. The builders are shared with the
//! record tests so both exercise the same production parser.

use serde_json::{Value as Json, json};

use super::*;

pub(crate) const ISSUER_ID: u64 = 15_812_269;
pub(crate) const ISSUER_LOGIN: &str = "EffortlessSteven";
pub(crate) const REPOSITORY_ID: u64 = 1_226_356_933;
pub(crate) const REPOSITORY: &str = "EffortlessMetrics/ripr";
pub(crate) const RECORD_ISSUE: u64 = 1470;
pub(crate) const NONCE: &str = "0123456789abcdef0123456789abcdef";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Probe,
    PublicRc,
    StableGithub,
    StableCrates,
    VsMarketplace,
    OpenVsx,
}

pub(crate) const KINDS: [Kind; 6] = [
    Kind::Probe,
    Kind::PublicRc,
    Kind::StableGithub,
    Kind::StableCrates,
    Kind::VsMarketplace,
    Kind::OpenVsx,
];

pub(crate) fn sha(fill: char) -> String {
    format!("sha256:{}", fill.to_string().repeat(64))
}

pub(crate) fn oid(fill: char) -> String {
    fill.to_string().repeat(40)
}

/// Sets `allowed_operations` and derives the complementary denied list.
pub(crate) fn set_operations(draft: &mut Json, allowed: &[&str]) {
    let mut allowed = allowed.to_vec();
    allowed.sort_unstable();
    let mut denied = Operation::ALL
        .iter()
        .map(|operation| operation.as_str())
        .filter(|name| !allowed.contains(name))
        .collect::<Vec<_>>();
    denied.sort_unstable();
    draft["allowed_operations"] = json!(allowed);
    draft["denied_operations"] = json!(denied);
}

fn evidence(present: &[&str]) -> Json {
    let digest_for = |kind: &str| present.contains(&kind).then(|| sha('e'));
    let mut map = serde_json::Map::new();
    for kind in EvidenceKind::ALL {
        map.insert(kind.as_str().to_string(), json!(digest_for(kind.as_str())));
    }
    map.insert("policy_ruling".to_string(), Json::Null);
    Json::Object(map)
}

fn channels(authorized: &[&str]) -> Json {
    Channel::ALL
        .iter()
        .map(|channel| {
            let state = if authorized.contains(&channel.as_str()) {
                "authorized"
            } else {
                "not_authorized"
            };
            (channel.as_str().to_string(), json!(state))
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

const STABLE_EVIDENCE: [&str; 6] = [
    "ship_packet",
    "readiness",
    "qualification",
    "public_rc_verification",
    "cohort",
    "stable_decision",
];

/// A valid draft (no `authorization_id`) for each closed boundary.
pub(crate) fn draft(kind: Kind) -> Json {
    let mut draft = json!({
        "schema": PACKET_SCHEMA,
        "tool": PACKET_TOOL,
        "repository": {"id": REPOSITORY_ID, "name": REPOSITORY},
        "programme_issue": 1463,
        "record": {"issue": RECORD_ISSUE, "purpose": RECORD_PURPOSE},
        "issuer": {"user_id": ISSUER_ID, "login": ISSUER_LOGIN},
        "sequence": 1,
        "nonce": NONCE,
        "validity": {
            "created_at": "2026-11-21T00:00:00Z",
            "not_before": "2026-11-21T00:00:00Z",
            "not_after": "2026-11-28T00:00:00Z",
        },
        "supersedes": null,
        "version": "0.11.0",
        "source": {"kind": "source_release", "commit": oid('1'), "tree": oid('2')},
        "workflow": {
            "path": ".github/workflows/release.yml",
            "blob": oid('3'),
            "commit": oid('4'),
            "plan_schema": "ripr-release-plan/v1",
            "plan_digest": sha('5'),
        },
        "executors": [{"kind": "user", "id": ISSUER_ID}],
        "consumption": {
            "mode": "one_use",
            "idempotency": "verify_only_when_exact",
            "terminal_receipt_schema": CONSUMPTION_RECEIPT_SCHEMA,
        },
        "invalidation": INVALIDATIONS,
        "stop_conditions": STOP_CONDITIONS,
        "non_claims": NON_CLAIMS,
    });
    let github_subjects = json!([
        {"class": "checksums", "name": "SHA256SUMS", "sha256": sha('6')},
        {"class": "server_manifest", "name": "ripr-server-manifest.json", "sha256": sha('8')},
        {"class": "server_archive", "name": "ripr-x86_64-unknown-linux-gnu.tar.gz", "sha256": sha('7')},
    ]);
    let vsix = json!([{"class": "vsix", "name": "ripr-0.11.0.vsix", "sha256": sha('9')}]);
    let transaction = |kind: &str| {
        json!({
            "kind": kind,
            "schema": format!("ripr-{kind}-transaction/v1"),
            "tool": "xtask-release/1",
            "request_id": format!("{kind}-request-1"),
        })
    };
    let tag_event = |version: &str| json!({"kind": "annotated_tag_push", "ref": format!("refs/tags/v{version}")});
    let continuation = json!({"kind": "exact_channel_continuation", "ref": null});
    let environment = |channel: &str, credential: &str| json!([{"channel": channel, "name": format!("release-{channel}"), "credentials": [credential]}]);
    match kind {
        Kind::Probe => {
            let version = format!("0.0.0-ripr-trigger-probe.{NONCE}");
            draft["profile"] = json!("reserved_probe");
            draft["source"]["kind"] = json!("probe_source");
            draft["validity"]["not_after"] = json!("2026-11-21T12:00:00Z");
            draft["evidence"] = evidence(&[]);
            draft["transaction"] = transaction("probe");
            draft["event"] = tag_event(&version);
            draft["version"] = json!(version);
            draft["channels"] = channels(&[]);
            draft["subjects"] = json!([]);
            draft["environments"] = json!([]);
            draft["consumption"]["idempotency"] = json!("none");
            set_operations(
                &mut draft,
                &[
                    "create_annotated_tag",
                    "trigger_no_publish_plan",
                    "read_event_artifact_attestation",
                    "record_consumption_receipt",
                    "delete_exact_probe_tag",
                ],
            );
        }
        Kind::PublicRc => {
            draft["profile"] = json!("public_rc");
            draft["version"] = json!("0.11.0-rc.1");
            draft["evidence"] = evidence(&STABLE_EVIDENCE[..3]);
            draft["evidence"]["policy_ruling"] =
                json!({"issue": 1634, "comment_id": 4242, "body_sha256": sha('c')});
            draft["transaction"] = transaction("github");
            draft["event"] = tag_event("0.11.0-rc.1");
            draft["channels"] = channels(&["git_tag", "github_prerelease"]);
            draft["subjects"] = github_subjects;
            draft["environments"] = environment("github_prerelease", "RELEASE_GITHUB_APP_KEY");
            set_operations(
                &mut draft,
                &[
                    "create_annotated_tag",
                    "github_prerelease_create",
                    "github_asset_upload",
                    "provenance_attest",
                    "request_named_environment",
                    "read_event_artifact_attestation",
                    "record_consumption_receipt",
                ],
            );
        }
        Kind::StableGithub => {
            draft["profile"] = json!("stable_core");
            draft["evidence"] = evidence(&STABLE_EVIDENCE);
            draft["transaction"] = transaction("github");
            draft["event"] = tag_event("0.11.0");
            draft["channels"] = channels(&["git_tag", "github_release"]);
            draft["subjects"] = github_subjects;
            draft["environments"] = environment("github_release", "RELEASE_GITHUB_APP_KEY");
            set_operations(
                &mut draft,
                &[
                    "create_annotated_tag",
                    "github_release_create",
                    "github_asset_upload",
                    "provenance_attest",
                    "request_named_environment",
                    "read_event_artifact_attestation",
                    "record_consumption_receipt",
                ],
            );
        }
        Kind::StableCrates => {
            draft["profile"] = json!("stable_core");
            draft["evidence"] = evidence(&STABLE_EVIDENCE);
            draft["transaction"] = transaction("crates_io");
            draft["event"] = continuation;
            draft["channels"] = channels(&["crates_io"]);
            draft["subjects"] =
                json!([{"class": "crate", "name": "ripr-0.11.0.crate", "sha256": sha('a')}]);
            draft["environments"] = environment("crates_io", "CARGO_REGISTRY_TOKEN");
            set_operations(
                &mut draft,
                &[
                    "crates_io_publish",
                    "request_named_environment",
                    "read_event_artifact_attestation",
                    "record_consumption_receipt",
                ],
            );
        }
        Kind::VsMarketplace | Kind::OpenVsx => {
            let (channel, credential) = if kind == Kind::VsMarketplace {
                ("vs_marketplace", "VSCE_PAT")
            } else {
                ("open_vsx", "OVSX_PAT")
            };
            let mut all = STABLE_EVIDENCE.to_vec();
            all.push("predecessor_receipt");
            draft["profile"] = json!("stable_channel_continuation");
            draft["evidence"] = evidence(&all);
            draft["transaction"] = transaction(channel);
            draft["event"] = continuation;
            draft["channels"] = channels(&[channel]);
            draft["subjects"] = vsix;
            draft["environments"] = environment(channel, credential);
            let publish = format!("{channel}_publish");
            set_operations(
                &mut draft,
                &[
                    publish.as_str(),
                    "request_named_environment",
                    "read_event_artifact_attestation",
                    "record_consumption_receipt",
                ],
            );
        }
    }
    draft
}

/// Canonical packet bytes for a draft, through the production sealer.
pub(crate) fn seal(draft: &Json) -> Result<Vec<u8>, String> {
    seal_draft(draft.to_string().as_bytes(), "authorization_id").map(|(_, bytes)| bytes)
}

pub(crate) fn parsed(draft: &Json) -> Result<Packet, String> {
    Packet::parse(&seal(draft)?)
}

fn expect_rejected(label: &str, draft: &Json, needle: &str) -> Result<(), String> {
    match parsed(draft) {
        Ok(_) => Err(format!("{label}: accepted")),
        Err(error) if error.contains(needle) => Ok(()),
        Err(error) => Err(format!(
            "{label}: rejected for `{error}`, expected `{needle}`"
        )),
    }
}

fn add_allowed(draft: &mut Json, operation: &str) {
    let mut allowed = draft["allowed_operations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Json::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    allowed.push(operation.to_string());
    let allowed = allowed.iter().map(String::as_str).collect::<Vec<_>>();
    set_operations(draft, &allowed);
}

#[test]
fn release_authorization_packet_every_boundary_has_one_valid_canonical_form() -> Result<(), String>
{
    for kind in KINDS {
        let draft = draft(kind);
        let canonical = seal(&draft)?;
        let packet = Packet::parse(&canonical).map_err(|e| format!("{kind:?}: {e}"))?;
        // Key order and whitespace in the draft are not part of the identity.
        let pretty = serde_json::to_vec_pretty(&draft).map_err(|e| e.to_string())?;
        let (id, again) = seal_draft(&pretty, "authorization_id")?;
        if again != canonical || id != packet.authorization_id {
            return Err(format!(
                "{kind:?}: canonical bytes depend on draft formatting"
            ));
        }
        let rendered = render_from_draft(draft.to_string().as_bytes())?;
        if !rendered.comment.starts_with(RECORD_MARKER) || rendered.canonical != canonical {
            return Err(format!(
                "{kind:?}: rendered comment does not carry the packet"
            ));
        }
        let explicit =
            packet.allowed.len() + draft["denied_operations"].as_array().map_or(0, Vec::len);
        if explicit != Operation::ALL.len() {
            return Err(format!(
                "{kind:?}: an operation is neither allowed nor denied"
            ));
        }
    }
    Ok(())
}

#[test]
fn release_authorization_packet_identity_covers_every_field() -> Result<(), String> {
    let canonical =
        String::from_utf8(seal(&draft(Kind::StableCrates))?).map_err(|e| e.to_string())?;
    let tampered = canonical.replacen(&oid('1'), &oid('f'), 1);
    if tampered == canonical {
        return Err("the control edit did not change the packet".to_string());
    }
    match Packet::parse(tampered.as_bytes()) {
        Err(error) if error.contains("authorization_id does not match") => {}
        other => return Err(format!("a retargeted packet kept its id: {other:?}")),
    }
    let mut first = draft(Kind::StableCrates);
    let one = parsed(&first)?.authorization_id;
    first["nonce"] = json!("fedcba9876543210fedcba9876543210");
    if parsed(&first)?.authorization_id == one {
        return Err("a new nonce did not produce a new authorization".to_string());
    }
    if render_from_draft(&seal(&draft(Kind::Probe))?).is_ok() {
        return Err("a draft that already carries an id was re-sealed".to_string());
    }
    Ok(())
}

#[test]
fn release_authorization_packet_closed_fields_reject_ambiguity() -> Result<(), String> {
    type Edit = fn(&mut Json);
    let edits: [(&str, Edit, &str); 14] = [
        ("unknown field", |d| d["note"] = json!("x"), "unknown field"),
        (
            "nested unknown field",
            |d| d["issuer"]["email"] = json!("a@b"),
            "unknown field",
        ),
        (
            "missing field",
            |d| {
                if let Some(map) = d.as_object_mut() {
                    map.remove("supersedes");
                }
            },
            "required field is missing",
        ),
        (
            "unknown schema",
            |d| d["schema"] = json!("ripr-release-authorization-packet/v2"),
            "unsupported",
        ),
        (
            "unknown profile",
            |d| d["profile"] = json!("stable"),
            "not a known value",
        ),
        (
            "unknown operation",
            |d| add_allowed(d, "publish_everything"),
            "unknown operation",
        ),
        (
            "omitted operation",
            |d| {
                if let Some(list) = d["denied_operations"].as_array_mut() {
                    list.pop();
                }
            },
            "neither allowed nor denied",
        ),
        (
            "contradictory operation",
            |d| {
                let first = d["denied_operations"][0].clone();
                d["allowed_operations"] = json!([first]);
            },
            "both allowed and denied",
        ),
        (
            "unsorted operations",
            |d| {
                if let Some(list) = d["denied_operations"].as_array_mut() {
                    list.reverse();
                }
            },
            "ascending",
        ),
        (
            "ambiguous channel",
            |d| d["channels"]["npm"] = json!(false),
            "expected a nonempty string",
        ),
        (
            "defaulted evidence",
            |d| {
                if let Some(map) = d["evidence"].as_object_mut() {
                    map.remove("cohort");
                }
            },
            "required field is missing",
        ),
        (
            "short commit",
            |d| d["source"]["commit"] = json!("abc1234"),
            "Git object id",
        ),
        (
            "uppercase digest",
            |d| d["workflow"]["plan_digest"] = json!(format!("sha256:{}", "A".repeat(64))),
            "sha256",
        ),
        (
            "float sequence",
            |d| d["sequence"] = json!(1.5),
            "plain unsigned integer",
        ),
    ];
    for (label, edit, needle) in edits {
        let mut draft = draft(Kind::StableCrates);
        edit(&mut draft);
        expect_rejected(label, &draft, needle)?;
    }
    Ok(())
}

#[test]
fn release_authorization_packet_probe_cannot_carry_public_authority() -> Result<(), String> {
    type Edit = fn(&mut Json);
    let edits: [(&str, Edit, &str); 8] = [
        (
            "crates channel",
            |d| d["channels"]["crates_io"] = json!("authorized"),
            "cannot authorize channel",
        ),
        (
            "git_tag channel",
            |d| d["channels"]["git_tag"] = json!("authorized"),
            "cannot authorize channel",
        ),
        (
            "publish operation",
            |d| add_allowed(d, "crates_io_publish"),
            "cannot allow operation",
        ),
        (
            "release operation",
            |d| add_allowed(d, "github_release_create"),
            "cannot allow operation",
        ),
        (
            "environment request",
            |d| add_allowed(d, "request_named_environment"),
            "cannot allow operation",
        ),
        (
            "credential name",
            |d| d["environments"] = json!([{"channel": "crates_io", "name": "release", "credentials": ["CARGO_REGISTRY_TOKEN"]}]),
            "not authorized",
        ),
        (
            "release evidence",
            |d| d["evidence"]["ship_packet"] = json!(sha('a')),
            "forbids",
        ),
        (
            "subject",
            |d| {
                d["subjects"] =
                    json!([{"class": "crate", "name": "ripr.crate", "sha256": sha('a')}])
            },
            "forbids a subject catalog",
        ),
    ];
    for (label, edit, needle) in edits {
        let mut draft = draft(Kind::Probe);
        edit(&mut draft);
        expect_rejected(label, &draft, needle)?;
    }
    let mut long = draft(Kind::Probe);
    long["validity"]["not_after"] = json!("2026-11-23T00:00:00Z");
    expect_rejected("long probe window", &long, "at most 24 hours")?;
    let mut release_tag = draft(Kind::Probe);
    release_tag["event"]["ref"] = json!("refs/tags/v0.11.0");
    expect_rejected("probe on a release tag", &release_tag, "event.ref")?;
    let mut other_nonce = draft(Kind::Probe);
    other_nonce["nonce"] = json!("fedcba9876543210fedcba9876543210");
    expect_rejected("probe tag for another nonce", &other_nonce, "nonce")?;
    Ok(())
}

#[test]
fn release_authorization_packet_rc_cannot_carry_stable_or_registry_authority() -> Result<(), String>
{
    type Edit = fn(&mut Json);
    let edits: [(&str, Edit, &str); 9] = [
        (
            "crate subject",
            |d| {
                d["subjects"] =
                    json!([{"class": "crate", "name": "ripr-0.11.0.crate", "sha256": sha('a')}])
            },
            "cannot carry",
        ),
        (
            "crates channel",
            |d| d["channels"]["crates_io"] = json!("authorized"),
            "cannot authorize channel",
        ),
        (
            "crates operation",
            |d| add_allowed(d, "crates_io_publish"),
            "cannot allow operation",
        ),
        (
            "stable release channel",
            |d| d["channels"]["github_release"] = json!("authorized"),
            "cannot authorize channel",
        ),
        (
            "stable release operation",
            |d| add_allowed(d, "github_release_create"),
            "cannot allow operation",
        ),
        (
            "marketplace operation",
            |d| add_allowed(d, "vs_marketplace_publish"),
            "cannot allow operation",
        ),
        (
            "npm channel",
            |d| d["channels"]["npm"] = json!("authorized"),
            "cannot authorize channel",
        ),
        (
            "signing operation",
            |d| add_allowed(d, "signing_key_operation"),
            "cannot allow operation",
        ),
        (
            "stable version",
            |d| d["version"] = json!("0.11.0"),
            "not a public_rc version",
        ),
    ];
    for (label, edit, needle) in edits {
        let mut draft = draft(Kind::PublicRc);
        edit(&mut draft);
        expect_rejected(label, &draft, needle)?;
    }
    let mut unruled = draft(Kind::PublicRc);
    unruled["evidence"]["policy_ruling"] = Json::Null;
    expect_rejected("rc without a cited ruling", &unruled, "policy_ruling")?;
    Ok(())
}

#[test]
fn release_authorization_packet_stable_requires_exact_decision_evidence() -> Result<(), String> {
    for kind in [Kind::StableGithub, Kind::StableCrates, Kind::VsMarketplace] {
        for missing in ["stable_decision", "public_rc_verification", "ship_packet"] {
            let mut draft = draft(kind);
            draft["evidence"][missing] = Json::Null;
            expect_rejected(
                &format!("{kind:?} without {missing}"),
                &draft,
                "requires it",
            )?;
        }
        let mut candidate = draft(kind);
        candidate["version"] = json!("0.11.0-rc.1");
        expect_rejected(&format!("{kind:?} rc version"), &candidate, "version")?;
    }
    let mut unpreceded = draft(Kind::OpenVsx);
    unpreceded["evidence"]["predecessor_receipt"] = Json::Null;
    expect_rejected(
        "continuation without predecessor",
        &unpreceded,
        "requires it",
    )?;
    Ok(())
}

#[test]
fn release_authorization_packet_channels_never_imply_or_share() -> Result<(), String> {
    // Crates authority does not reach the tag or GitHub, and the reverse.
    let mut crates = draft(Kind::StableCrates);
    crates["channels"]["git_tag"] = json!("authorized");
    expect_rejected(
        "crates packet authorizing a tag",
        &crates,
        "cannot authorize channel",
    )?;
    let mut github = draft(Kind::StableGithub);
    add_allowed(&mut github, "crates_io_publish");
    expect_rejected(
        "github packet publishing a crate",
        &github,
        "cannot allow operation",
    )?;

    // A channel without its operation, or the operation without its channel.
    let mut silent = draft(Kind::StableCrates);
    silent["channels"]["crates_io"] = json!("not_authorized");
    expect_rejected(
        "operation without channel",
        &silent,
        "authorizes no channel",
    )?;
    let mut released = draft(Kind::StableGithub);
    released["channels"]["github_release"] = json!("not_authorized");
    expect_rejected(
        "release operation without channel",
        &released,
        "authorized together",
    )?;

    // One continuation carries one channel, one environment, one credential set.
    let mut both = draft(Kind::VsMarketplace);
    both["channels"]["open_vsx"] = json!("authorized");
    expect_rejected("two marketplaces", &both, "cannot authorize channel")?;
    let mut borrowed = draft(Kind::VsMarketplace);
    borrowed["environments"][0]["channel"] = json!("open_vsx");
    expect_rejected("another channel's environment", &borrowed, "not authorized")?;
    let mut npm = draft(Kind::VsMarketplace);
    npm["transaction"]["kind"] = json!("npm");
    expect_rejected("npm continuation", &npm, "npm continuation")?;

    let mut shared = draft(Kind::StableGithub);
    shared["environments"] = json!([
        {"channel": "git_tag", "name": "release", "credentials": ["TAG_KEY"]},
        {"channel": "github_release", "name": "release", "credentials": ["APP_KEY"]},
    ]);
    expect_rejected("shared environment", &shared, "shared between channels")?;
    shared["environments"][1]["name"] = json!("release-github");
    shared["environments"][1]["credentials"] = json!(["TAG_KEY"]);
    expect_rejected("shared credential", &shared, "shared between environments")?;
    shared["environments"][1]["credentials"] = json!(["APP_KEY"]);
    parsed(&shared).map(|_| ())
}

#[test]
fn release_authorization_packet_subjects_and_events_are_exact() -> Result<(), String> {
    let mut wrong_class = draft(Kind::VsMarketplace);
    wrong_class["subjects"][0]["class"] = json!("server_archive");
    expect_rejected(
        "archive in a marketplace packet",
        &wrong_class,
        "cannot carry",
    )?;
    let mut two = draft(Kind::VsMarketplace);
    two["subjects"] = json!([
        {"class": "vsix", "name": "a.vsix", "sha256": sha('1')},
        {"class": "vsix", "name": "b.vsix", "sha256": sha('2')},
    ]);
    expect_rejected("two VSIX subjects", &two, "exactly one VSIX")?;
    let mut truncated = draft(Kind::StableCrates);
    truncated["subjects"][0]["sha256"] = json!("sha256:abc");
    expect_rejected("truncated subject digest", &truncated, "sha256")?;
    let mut repeated = draft(Kind::StableGithub);
    let first = repeated["subjects"][0].clone();
    repeated["subjects"] = json!([first.clone(), first]);
    expect_rejected("repeated subject", &repeated, "unique")?;

    let mut moved = draft(Kind::StableGithub);
    moved["event"]["ref"] = json!("refs/tags/v0.11.1");
    expect_rejected("tag for another version", &moved, "event.ref")?;
    let mut branch = draft(Kind::StableGithub);
    branch["event"]["ref"] = json!("refs/heads/main");
    expect_rejected("branch ref", &branch, "event.ref")?;
    let mut tagged = draft(Kind::StableCrates);
    tagged["event"] = json!({"kind": "annotated_tag_push", "ref": "refs/tags/v0.11.0"});
    expect_rejected("crates packet as a tag push", &tagged, "event.kind")?;

    let mut inverted = draft(Kind::StableCrates);
    inverted["validity"]["not_after"] = json!("2026-11-20T00:00:00Z");
    expect_rejected("inverted window", &inverted, "validity")?;
    let mut unbounded = draft(Kind::StableCrates);
    unbounded["validity"]["not_after"] = json!("2027-11-21T00:00:00Z");
    expect_rejected("unbounded window", &unbounded, "window of at most")?;
    Ok(())
}
