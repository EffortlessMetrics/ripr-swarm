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
    if before != after {
        let affected = if before.head != after.head {
            "refresh affected head-bound proof/review and integration judgment"
        } else if before.target != after.target {
            "refresh target protection and integration judgment; retain unchanged-head proof/review"
        } else {
            "refresh integration judgment; retain unchanged-head proof/review"
        };
        return Err(format!(
            "PR #{number} subject changed during collection: head {} -> {}, base {} -> {}, target {} -> {}; no current readiness published; {affected}",
            before.head, after.head, before.base, after.base, before.target, after.target
        ));
    }
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
}
