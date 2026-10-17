//! Collect one coherent advisory snapshot, without treating it as merge authority.
use serde_json::Value;

pub(crate) struct Observation {
    pub(crate) pr: Value,
    pub(crate) contexts: Vec<String>,
    pub(crate) contexts_available: bool,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Subject {
    head: String,
    base: String,
    target: String,
}

impl Subject {
    fn read(value: &Value, number: u64) -> Result<Self, String> {
        if value["number"].as_u64() != Some(number) {
            return Err(format!("PR identity mismatch: expected #{number}"));
        }
        let oid = |key: &str| -> Result<String, String> {
            let text = value[key]
                .as_str()
                .ok_or_else(|| format!("missing {key}"))?;
            if text.len() != 40 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("invalid full commit identity: {key}"));
            }
            Ok(text.to_ascii_lowercase())
        };
        let target = value["baseRefName"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .ok_or("missing baseRefName")?
            .to_string();
        Ok(Self {
            head: oid("headRefOid")?,
            base: oid("baseRefOid")?,
            target,
        })
    }
}

// Keep full-fact collection on gh's existing shape/pagination path. gh 2.46 supports
// headRefOid here, but baseRefOid is a GraphQL field, not a pr-view JSON field.
pub(crate) const VIEW_FIELDS: &str = "number,title,isDraft,mergeStateStatus,headRefName,headRefOid,baseRefName,reviewDecision,latestReviews,reviews,comments,statusCheckRollup";
const IDENTITY_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){number headRefOid baseRefOid baseRefName}}}";

fn collect_identity(
    number: u64,
    run: &mut impl FnMut(&str, &[String]) -> Result<String, String>,
) -> Result<Value, String> {
    let output = run(
        "gh",
        &[
            "api".to_string(),
            "graphql".to_string(),
            "-F".to_string(),
            "owner={owner}".to_string(),
            "-F".to_string(),
            "name={repo}".to_string(),
            "-F".to_string(),
            format!("number={number}"),
            "-f".to_string(),
            format!("query={IDENTITY_QUERY}"),
        ],
    )?;
    let response: Value = serde_json::from_str(&output)
        .map_err(|error| format!("failed to parse PR identity GraphQL JSON: {error}"))?;
    if response
        .get("errors")
        .is_some_and(|errors| !errors.as_array().is_some_and(|errors| errors.is_empty()))
    {
        return Err(
            "PR identity GraphQL response contains errors; refusing partial data".to_string(),
        );
    }
    let identity = response
        .pointer("/data/repository/pullRequest")
        .ok_or("missing PR identity GraphQL data")?
        .clone();
    Subject::read(&identity, number)?;
    Ok(identity)
}

pub(crate) fn collect_snapshot(
    number: u64,
    mut run: impl FnMut(&str, &[String]) -> Result<String, String>,
) -> Result<Value, String> {
    let first = collect_identity(number, &mut run)?;
    let before = Subject::read(&first, number)?;
    let output = run(
        "gh",
        &[
            "pr".to_string(),
            "view".to_string(),
            number.to_string(),
            "--json".to_string(),
            VIEW_FIELDS.to_string(),
        ],
    )?;
    let mut pr: Value = serde_json::from_str(&output)
        .map_err(|error| format!("failed to parse gh PR JSON: {error}"))?;
    let last = collect_identity(number, &mut run)?;
    let after = Subject::read(&last, number)?;
    unchanged(number, &before, &after)?;
    // Do not associate a newer base with old merge/check facts. Both identity
    // reads must agree, and the intervening full response must name that PR/head/target.
    if pr
        .get("baseRefOid")
        .is_some_and(|base| base.as_str() != Some(before.base.as_str()))
    {
        return Err("full PR response base disagrees with identity bracket".to_string());
    }
    pr.as_object_mut()
        .ok_or("full PR response must be an object")?
        .insert("baseRefOid".to_string(), Value::String(before.base.clone()));
    let full_subject = Subject::read(&pr, number)?;
    unchanged(number, &before, &full_subject)?;
    Ok(pr)
}

fn unchanged(number: u64, before: &Subject, after: &Subject) -> Result<(), String> {
    if before == after {
        return Ok(());
    }
    let affected = if before.head != after.head {
        "refresh affected head-bound proof/review and integration judgment"
    } else if before.target != after.target {
        "refresh target protection and integration judgment; retain unchanged-head proof/review"
    } else {
        "refresh integration judgment; retain unchanged-head proof/review"
    };
    Err(format!(
        "PR #{number} subject changed during collection: head {} -> {}, base {} -> {}, target {} -> {}; no current readiness published; {affected}",
        before.head, after.head, before.base, after.base, before.target, after.target
    ))
}

pub(crate) fn collect_current(
    number: u64,
    mut collect: impl FnMut() -> Result<Value, String>,
    mut contexts: impl FnMut(&str) -> Result<Vec<String>, String>,
) -> Result<Observation, String> {
    let first = collect()?;
    let before = Subject::read(&first, number)?;
    let (contexts, contexts_available, mut warnings) = match contexts(&before.target) {
        Ok(contexts) => (contexts, true, Vec::new()),
        Err(error) => (
            Vec::new(),
            false,
            vec![format!(
                "required status context lookup failed; using status rollup only: {error}"
            )],
        ),
    };
    // Refresh the full response: unchanged identities do not imply unchanged checks/review.
    let pr = collect().map_err(|error| format!("final PR snapshot unavailable: {error}"))?;
    let after = Subject::read(&pr, number)
        .map_err(|error| format!("final PR snapshot identity unavailable: {error}"))?;
    unchanged(number, &before, &after)?;
    warnings.push(format!(
        "Observed PR #{number} head {} and base {} ({}) in the final snapshot; this read is not atomic with a later merge and does not authorize one",
        after.head, after.base, after.target
    ));
    Ok(Observation {
        pr,
        contexts,
        contexts_available,
        warnings,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;

    const H1: &str = "1111111111111111111111111111111111111111";
    const H2: &str = "2222222222222222222222222222222222222222";
    const B1: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B2: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn snapshot() -> Value {
        json!({"number":42,"headRefOid":H1,"baseRefOid":B1,"baseRefName":"main",
            "headRefName":"candidate","mergeStateStatus":"CLEAN","reviewDecision":"APPROVED",
            "statusCheckRollup":[{"name":"rust","status":"COMPLETED","conclusion":"SUCCESS"}]})
    }

    fn run(
        first: Value,
        last: Result<Value, String>,
    ) -> (Result<Observation, String>, Vec<String>) {
        let calls = RefCell::new(Vec::new());
        let mut responses = vec![Ok(first), last].into_iter();
        let result = collect_current(
            42,
            || {
                calls.borrow_mut().push("pr".to_string());
                responses.next().ok_or("unexpected third read")?
            },
            |target| {
                calls.borrow_mut().push(format!("contexts:{target}"));
                Ok(vec!["rust".to_string()])
            },
        );
        (result, calls.into_inner())
    }

    #[test]
    fn unchanged_subject_uses_final_facts_after_context_lookup() -> Result<(), String> {
        let mut last = snapshot();
        last["statusCheckRollup"][0]["status"] = json!("IN_PROGRESS");
        last["reviewDecision"] = json!("CHANGES_REQUESTED");
        let (result, calls) = run(snapshot(), Ok(last.clone()));
        let observation = result?;
        assert_eq!(calls, ["pr", "contexts:main", "pr"]);
        assert_eq!(observation.pr, last);
        assert_eq!(observation.contexts, ["rust"]);
        assert!(observation.contexts_available);
        assert_eq!(observation.warnings.len(), 1);
        assert!(observation.warnings[0].contains(H1) && observation.warnings[0].contains(B1));
        assert!(observation.warnings[0].contains("not atomic"));
        Ok(())
    }

    #[test]
    fn moved_head_refuses_old_evidence_even_with_same_branch_names() -> Result<(), String> {
        let mut last = snapshot();
        last["headRefOid"] = json!(H2);
        let (result, _) = run(snapshot(), Ok(last));
        let error = result
            .err()
            .ok_or("moved head accepted old green evidence")?;
        assert!(error.contains(H1) && error.contains(H2), "{error}");
        assert!(error.contains("head-bound"), "{error}");
        Ok(())
    }

    #[test]
    fn moved_base_preserves_head_bound_proof_but_refuses_old_integration() -> Result<(), String> {
        let mut last = snapshot();
        last["baseRefOid"] = json!(B2);
        let (result, _) = run(snapshot(), Ok(last));
        let error = result
            .err()
            .ok_or("moved base accepted old integration evidence")?;
        assert!(error.contains(B1) && error.contains(B2), "{error}");
        assert!(
            error.contains("retain unchanged-head proof/review"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn retarget_refuses_former_protection_contexts() -> Result<(), String> {
        let mut last = snapshot();
        last["baseRefName"] = json!("release");
        let (result, _) = run(snapshot(), Ok(last));
        let error = result.err().ok_or("retarget accepted former contexts")?;
        assert!(
            error.contains("main") && error.contains("release"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn missing_malformed_or_failed_final_identity_refuses() -> Result<(), String> {
        for key in ["headRefOid", "baseRefOid", "baseRefName", "number"] {
            for value in [Value::Null, json!("short"), json!("g".repeat(40))] {
                let mut last = snapshot();
                last[key] = value;
                assert!(
                    run(snapshot(), Ok(last)).0.is_err(),
                    "accepted invalid {key}"
                );
            }
        }
        let error = run(snapshot(), Err("transport lost".to_string()))
            .0
            .err()
            .ok_or("accepted failed read")?;
        assert!(error.contains("transport lost"), "{error}");
        Ok(())
    }

    #[test]
    fn invalid_initial_identity_never_reads_contexts() {
        let mut first = snapshot();
        first["headRefOid"] = Value::Null;
        let (result, calls) = run(first, Ok(snapshot()));
        assert!(result.is_err());
        assert_eq!(calls, ["pr"]);
    }

    #[test]
    fn unavailable_protection_still_refreshes_final_snapshot() -> Result<(), String> {
        let mut reads = 0;
        let observation = collect_current(
            42,
            || {
                reads += 1;
                Ok(snapshot())
            },
            |_| Err("denied".to_string()),
        )?;
        assert_eq!(reads, 2);
        assert!(!observation.contexts_available);
        assert!(observation.contexts.is_empty());
        assert_eq!(observation.warnings.len(), 2);
        assert!(observation.warnings[0].contains("denied"));
        Ok(())
    }

    fn graphql(identity: Value) -> Value {
        json!({"data":{"repository":{"pullRequest":identity}}})
    }

    fn bracket(responses: Vec<Value>) -> Result<Value, String> {
        let mut responses = responses.into_iter();
        let mut calls = 0;
        let result = collect_snapshot(42, |program, args| {
            assert_eq!(program, "gh");
            calls += 1;
            if calls == 2 {
                assert_eq!(&args[..4], ["pr", "view", "42", "--json"]);
                assert!(!args[4].split(',').any(|field| field == "baseRefOid"));
            } else {
                assert_eq!(&args[..2], ["api", "graphql"]);
                assert!(args.iter().any(|arg| arg == "owner={owner}"));
                assert!(args.iter().any(|arg| arg == "name={repo}"));
                assert!(args.iter().any(|arg| arg.contains("baseRefOid")));
            }
            responses
                .next()
                .map(|value| value.to_string())
                .ok_or("unexpected command".to_string())
        });
        assert!(
            responses.next().is_none(),
            "collection did not consume the expected bracket"
        );
        result
    }

    #[test]
    fn gh_pr_status_bracket_preserves_full_facts_without_view_base_field() -> Result<(), String> {
        let identity = snapshot();
        let mut full = identity.clone();
        full.as_object_mut()
            .ok_or("fixture must be an object")?
            .remove("baseRefOid");
        full["comments"] = json!([{"body":"raw provider notice","url":"fixture-url"}]);
        full["statusCheckRollup"][0]["status"] = json!("IN_PROGRESS");
        let observed = bracket(vec![
            graphql(identity.clone()),
            full.clone(),
            graphql(identity),
        ])?;
        full["baseRefOid"] = json!(B1);
        assert_eq!(observed, full);
        Ok(())
    }

    #[test]
    fn gh_pr_status_bracket_refuses_base_head_and_target_movement() -> Result<(), String> {
        for (field, value) in [
            ("headRefOid", H2),
            ("baseRefOid", B2),
            ("baseRefName", "release"),
        ] {
            let first = snapshot();
            let mut last = first.clone();
            last[field] = json!(value);
            let Err(error) = bracket(vec![graphql(first.clone()), first, graphql(last)]) else {
                return Err(format!("moved {field} identity accepted"));
            };
            assert!(
                error.contains("subject changed during collection"),
                "{field}: {error}"
            );
            if field != "headRefOid" {
                assert!(error.contains("retain unchanged-head proof/review"));
            }
        }
        Ok(())
    }

    #[test]
    fn gh_pr_status_bracket_refuses_full_response_identity_mismatch() -> Result<(), String> {
        for field in ["number", "headRefOid", "baseRefName", "baseRefOid"] {
            let identity = snapshot();
            let mut full = identity.clone();
            full[field] = match field {
                "number" => json!(43),
                "baseRefName" => json!("release"),
                _ => json!(H2),
            };
            let Err(_) = bracket(vec![graphql(identity.clone()), full, graphql(identity)]) else {
                return Err(format!("mismatched full-response {field} accepted"));
            };
        }
        Ok(())
    }

    #[test]
    fn gh_pr_status_bracket_refuses_graphql_partial_errors_and_missing_subject()
    -> Result<(), String> {
        for response in [
            json!({"errors":[{"message":"denied"}],"data":{"repository":{"pullRequest":snapshot()}}}),
            json!({"errors":"malformed", "data":{"repository":{"pullRequest":snapshot()}}}),
            json!({"data":{"repository":null}}),
            graphql(Value::Null),
        ] {
            let Err(_) = bracket(vec![response.clone()]) else {
                return Err("invalid first identity accepted".to_string());
            };
            let Err(_) = bracket(vec![graphql(snapshot()), snapshot(), response]) else {
                return Err("invalid final bracket identity accepted".to_string());
            };
        }
        Ok(())
    }
}
