//! Routing-contract suites grouped without changing their platform boundaries.

#[path = "routing_contracts/ci_leaf_contract.rs"]
mod ci_leaf_contract;
#[path = "routing_contracts/docs_only_routing.rs"]
mod docs_only_routing;

#[cfg(target_os = "linux")]
mod conservation {
    use std::collections::BTreeSet;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn expected() -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("routing_contracts/inventory.json"))?;
        let mut old = BTreeSet::new();
        let mut new = BTreeSet::new();
        for row in rows {
            let binary = row["old_binary"].as_str().ok_or("missing old binary")?;
            let case = row["old_case"].as_str().ok_or("missing old case")?;
            let mapped = row["new_case"].as_str().ok_or("missing new case")?;
            assert!(
                old.insert((binary.to_owned(), case.to_owned())),
                "duplicate old case"
            );
            assert_eq!(mapped, format!("{binary}::{case}"));
            assert!(new.insert(mapped.to_owned()), "duplicate mapped case");
        }
        assert_eq!(new.len(), 35);
        Ok(new)
    }

    fn listed(text: &str) -> BTreeSet<String> {
        text.lines()
            .filter_map(|line| line.strip_suffix(": test"))
            .filter(|name| !name.starts_with("conservation::"))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn native_inventory_conserves_every_original_case_and_ignored_state() -> TestResult {
        let executable = std::env::current_exe()?;
        for ignored in [false, true] {
            let mut command = std::process::Command::new(&executable);
            command.args(["--list", "--format", "terse"]);
            if ignored {
                command.arg("--ignored");
            }
            let output = command.output()?;
            assert!(output.status.success());
            let actual = listed(std::str::from_utf8(&output.stdout)?);
            assert_eq!(
                actual,
                if ignored {
                    BTreeSet::new()
                } else {
                    expected()?
                }
            );
        }
        Ok(())
    }

    #[test]
    fn omitted_case_is_not_a_complete_inventory() -> TestResult {
        let expected = expected()?;
        let mut missing = expected.clone();
        missing.pop_first();
        assert_ne!(missing, expected);
        Ok(())
    }
}
