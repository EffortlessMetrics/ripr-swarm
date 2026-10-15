//! Inactive, offline prototype. Never executes tests or changes workflow policy.
//! See docs/ci/mandatory-partitions.md for the input trust boundary.
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::{env, fs};

type Subject = (String, String);
type Inventory = BTreeMap<Subject, bool>; // selected; ignored entries remain visible

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    active: bool,
    report: String,
    profile: String,
    candidates: Vec<usize>,
    wall_budget_seconds: u64,
}

fn plan(text: &str) -> Result<Plan, String> {
    let plan: Plan = toml::from_str(text).map_err(|e| e.to_string())?;
    if plan.active
        || plan.profile != "ci"
        || plan.report.len() != 40
        || plan.candidates != [2, 3, 4]
        || plan.wall_budget_seconds != 1200
    {
        return Err("prototype contract changed; review the plan before using it".into());
    }
    Ok(plan)
}

fn inventory(value: &Value, partition: bool) -> Result<Inventory, String> {
    let suites = value["rust-suites"]
        .as_object()
        .ok_or("missing rust-suites")?;
    let mut out = BTreeMap::new();
    for (id, suite) in suites {
        if suite["binary-id"].as_str() != Some(id) || suite["status"] != "listed" {
            return Err(format!("unlisted or mismatched binary {id}"));
        }
        let tests = suite["testcases"].as_object().ok_or("missing testcases")?;
        for (name, test) in tests {
            let ignored = test["ignored"].as_bool().ok_or("missing ignored state")?;
            let status = test["filter-match"]["status"].as_str();
            let reason = test["filter-match"]["reason"].as_str();
            let selected = match (ignored, status, reason) {
                (false, Some("matches"), None) => true,
                (true, Some("mismatch"), Some("ignored")) => false,
                (false, Some("mismatch"), Some("partition")) if partition => false,
                _ => return Err(format!("unexpected filter for {id}::{name}: {test}")),
            };
            out.insert((id.clone(), name.clone()), selected);
        }
    }
    if out.is_empty() || value["test-count"].as_u64() != Some(out.len() as u64) {
        return Err("empty or inconsistent native inventory".into());
    }
    Ok(out)
}

fn coverage(full: &Inventory, shards: &[Inventory]) -> Result<Vec<BTreeSet<Subject>>, String> {
    if !(2..=4).contains(&shards.len()) {
        return Err("expected 2..4 shards".into());
    }
    let expected: BTreeSet<_> = full
        .iter()
        .filter(|(_, selected)| **selected)
        .map(|(id, _)| id.clone())
        .collect();
    if expected.is_empty() {
        return Err("no runnable subjects".into());
    }
    let mut seen = BTreeSet::new();
    let mut selections = Vec::new();
    for shard in shards {
        if !full.keys().eq(shard.keys()) {
            return Err("different shard universe".into());
        }
        let selected: BTreeSet<_> = shard
            .iter()
            .filter(|(_, selected)| **selected)
            .map(|(id, _)| id.clone())
            .collect();
        if selected.is_empty() {
            return Err("empty shard".into());
        }
        for id in &selected {
            if !expected.contains(id) || !seen.insert(id.clone()) {
                return Err(format!("extra or repeated subject: {id:?}"));
            }
        }
        selections.push(selected);
    }
    if seen != expected {
        return Err("missing runnable subject".into());
    }
    Ok(selections)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    shard: usize,    // 1-based, same as native hash:m/n
    context: String, // caller-bound head/tree/toolchain/features/config/runner/attempt identity
    exit: i32,
    complete: bool,
    passed: BTreeSet<Subject>,
}

fn aggregate(
    expected: &[BTreeSet<Subject>],
    context: &str,
    receipts: &[Receipt],
) -> Result<(), String> {
    if context.is_empty() || receipts.len() != expected.len() {
        return Err("missing context/shard".into());
    }
    let mut seen = BTreeSet::new();
    for receipt in receipts {
        let Some(subjects) = receipt.shard.checked_sub(1).and_then(|i| expected.get(i)) else {
            return Err("unexpected shard".into());
        };
        if !seen.insert(receipt.shard)
            || receipt.context != context
            || receipt.exit != 0
            || !receipt.complete
            || receipt.passed != *subjects
        {
            return Err(format!(
                "failed, stale, repeated or incomplete shard {}",
                receipt.shard
            ));
        }
    }
    Ok(())
}

fn read_json(path: &str) -> Result<Value, String> {
    serde_json::from_str(&fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?)
        .map_err(|e| format!("{path}: {e}"))
}

fn receipt_offset(args: &[String]) -> usize {
    args.iter()
        .enumerate()
        .skip(3)
        .find_map(|(i, arg)| (arg == "--receipts").then_some(i))
        .unwrap_or(args.len())
}

fn run(args: &[String]) -> Result<(), String> {
    if args.len() < 5 {
        return Err("usage: mandatory_partitions PLAN FULL SHARD1 SHARD2 [SHARD3 [SHARD4]]; or ... PLAN FULL SHARDS... --receipts CONTEXT RECEIPTS_JSON".into());
    }
    let plan = plan(&fs::read_to_string(&args[1]).map_err(|e| e.to_string())?)?;
    let receipt_offset = receipt_offset(args);
    let full = inventory(&read_json(&args[2])?, false)?;
    let shards = args[3..receipt_offset]
        .iter()
        .map(|p| inventory(&read_json(p)?, true))
        .collect::<Result<Vec<_>, _>>()?;
    if !plan.candidates.contains(&shards.len()) {
        return Err("unconfigured partition count".into());
    }
    let selections = coverage(&full, &shards)?;
    if receipt_offset < args.len() {
        if args.len() != receipt_offset + 3 {
            return Err("expected context and receipts file".into());
        }
        let receipts: Vec<Receipt> = serde_json::from_value(read_json(&args[receipt_offset + 2])?)
            .map_err(|e| e.to_string())?;
        aggregate(&selections, &args[receipt_offset + 1], &receipts)?;
        println!("compiled-test aggregation complete; doctests remain a separate obligation");
    } else {
        println!(
            "coverage complete; runnable per shard: {:?}; ignored: {}",
            selections.iter().map(BTreeSet::len).collect::<Vec<_>>(),
            full.values().filter(|s| !**s).count()
        );
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run(&env::args().collect::<Vec<_>>()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("partition prototype refused: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn required_input_named_receipts_is_a_path() {
        let mut args = [
            "program",
            "plan.toml",
            "--receipts",
            "part1.json",
            "part2.json",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(receipt_offset(&args), 5);
        args.extend(["--receipts", "context", "receipts.json"].map(str::to_owned));
        assert_eq!(receipt_offset(&args), 5);
    }

    fn full() -> Inventory {
        [
            ("lib", "same", true),
            ("integration", "same", true),
            ("lib", "ignored", false),
        ]
        .into_iter()
        .map(|(b, t, s)| ((b.into(), t.into()), s))
        .collect()
    }
    fn shards() -> Vec<Inventory> {
        (0..2)
            .map(|i| {
                full()
                    .into_iter()
                    .map(|(id, selected)| {
                        let take = selected && (id.0 == "lib") == (i == 0);
                        (id, take)
                    })
                    .collect()
            })
            .collect()
    }
    fn receipts() -> Result<Vec<Receipt>, String> {
        Ok(coverage(&full(), &shards())?
            .into_iter()
            .enumerate()
            .map(|(i, passed)| Receipt {
                shard: i + 1,
                context: "candidate/config/attempt".into(),
                exit: 0,
                complete: true,
                passed,
            })
            .collect())
    }
    #[test]
    fn offline_configuration_is_inactive_and_bounded() {
        let config = include_str!("../../docs/ci/mandatory-partitions.toml");
        assert!(plan(config).is_ok());
        for bad in [
            config.replace("false", "true"),
            config.replace("1200", "1800"),
            config.replace("ci", "filtered"),
        ] {
            assert!(plan(&bad).is_err());
        }
    }
    #[test]
    fn binary_identity_ignored_and_exact_disjoint_union() -> Result<(), String> {
        assert_eq!(
            coverage(&full(), &shards())?
                .iter()
                .map(BTreeSet::len)
                .collect::<Vec<_>>(),
            [1, 1]
        );
        let mut bad = shards();
        bad[1] = bad[0].clone();
        assert!(coverage(&full(), &bad).is_err());
        let mut bad = shards();
        bad[0].insert(("foreign".into(), "same".into()), true);
        assert!(coverage(&full(), &bad).is_err());
        let mut bad = shards();
        bad[0].insert(("lib".into(), "ignored".into()), true);
        assert!(coverage(&full(), &bad).is_err());
        assert!(coverage(&Inventory::new(), &shards()).is_err());
        Ok(())
    }
    #[test]
    fn missing_and_empty_shards_do_not_pass() {
        assert!(coverage(&full(), &shards()[..1]).is_err());
        let mut bad = shards();
        for value in bad[0].values_mut() {
            *value = false;
        }
        assert!(coverage(&full(), &bad).is_err());
    }
    #[test]
    fn native_parser_refuses_filters_unknown_states_and_count_drift() -> Result<(), String> {
        let good = json!({"test-count":1,"rust-suites":{"lib":{"binary-id":"lib","status":"listed","testcases":{"case":{"ignored":false,"filter-match":{"status":"matches"}}}}}});
        assert_eq!(inventory(&good, false)?.len(), 1);
        let mut bad = good.clone();
        bad["test-count"] = json!(0);
        assert!(inventory(&bad, false).is_err());
        let mut bad = good.clone();
        bad["rust-suites"]["lib"]["status"] = json!("skipped");
        assert!(inventory(&bad, false).is_err());
        let mut bad = good;
        bad["rust-suites"]["lib"]["testcases"]["case"]["filter-match"] =
            json!({"status":"mismatch","reason":"expression"});
        assert!(inventory(&bad, false).is_err());
        assert!(inventory(&bad, true).is_err());
        Ok(())
    }
    #[test]
    fn aggregation_requires_every_exact_successful_terminal_receipt() -> Result<(), String> {
        let expected = coverage(&full(), &shards())?;
        let context = "candidate/config/attempt";
        assert!(aggregate(&expected, context, &receipts()?).is_ok());
        for exit in [1, 100, 124, 130] {
            let mut bad = receipts()?;
            bad[1].exit = exit;
            assert!(aggregate(&expected, context, &bad).is_err());
        }
        assert!(aggregate(&expected, context, &receipts()?[..1]).is_err());
        let mut bad = receipts()?;
        bad[1].shard = 1;
        assert!(aggregate(&expected, context, &bad).is_err());
        let mut bad = receipts()?;
        bad[1].complete = false;
        assert!(aggregate(&expected, context, &bad).is_err());
        let mut bad = receipts()?;
        bad[1].context = "old-head".into();
        assert!(aggregate(&expected, context, &bad).is_err());
        let mut bad = receipts()?;
        bad[1].passed.clear();
        assert!(aggregate(&expected, context, &bad).is_err());
        let mut bad = receipts()?;
        bad[1].passed = bad[0].passed.clone();
        assert!(aggregate(&expected, context, &bad).is_err());
        Ok(())
    }
}
