//! #8547: skip only guidance lookups that cannot return a predicate pointer.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str, before: &str, after: &str) -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-inventory-{}-{stamp}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        fixture.write("Cargo.toml", "[package]\nname = \"stub_inventory\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n")?;
        fixture.write("src/lib.rs", source)?;
        fixture.write(
            "change.patch",
            &format!("diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2 +2 @@\n-{before}\n+{after}\n"),
        )?;
        Ok(fixture)
    }

    fn write(&self, relative: &str, source: &str) -> Result<(), String> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, source).map_err(|error| error.to_string())
    }

    fn finding(&self, family: &str) -> Result<Finding, String> {
        let output = crate::app::check_workspace(crate::app::CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("change.patch")),
            ..Default::default()
        })?;
        output
            .findings
            .into_iter()
            .find(|finding| {
                finding.probe.family.as_str() == family && finding.class != ExposureClass::Exposed
            })
            .ok_or_else(|| format!("fixture produced no non-exposed {family} finding"))
    }

    fn explicit(&self, family: &str) -> Result<TestStubResolution, String> {
        let selector = TestStubSelector::parse_at("src/lib.rs:2")?.with_kind(family)?;
        resolve_test_stub(&self.0, &RiprConfig::default(), &selector)
            .map_err(|error| format!("{error:?}"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct LookupCount(usize);

impl LookupCount {
    fn reset() -> Self {
        Self(INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.replace(0)))
    }

    fn get(&self) -> usize {
        INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.get())
    }
}

impl Drop for LookupCount {
    fn drop(&mut self) {
        INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.set(self.0));
    }
}

#[test]
fn nonboundary_route_skips_inventory_but_explicit_command_keeps_disclosure() -> Result<(), String> {
    for (source, before, after, family) in [
        (
            "pub fn amount(value: i32) -> i32 {\n    value + 1\n}\n",
            "    value + 2",
            "    value + 1",
            "return_value",
        ),
        (
            "pub fn rejected() -> Result<u32, E> {\n    Err(E::New)\n}\n#[derive(Debug, PartialEq)]\npub enum E { Old, New }\n",
            "    Err(E::Old)",
            "    Err(E::New)",
            "error_path",
        ),
    ] {
        let fixture = Fixture::new(source, before, after)?;
        let finding = fixture.finding(family)?;
        let calls = LookupCount::reset();
        let route = stub_route_for_finding(&fixture.0, &RiprConfig::default(), &finding, &mut None)
            .ok_or("real non-predicate finding did not resolve")?;
        assert_eq!(
            calls.get(),
            0,
            "non-predicate guidance unexpectedly loaded the full inventory"
        );
        assert_eq!(route.finding_id, finding.id);
        assert!(
            matches!(route.decision, StubRouteDecision::Stub { ref file, line: 2, kind }
            if file == "src/lib.rs" && kind == family),
            "{route:?}"
        );
        let explicit = fixture.explicit(family)?;
        assert_eq!(
            calls.get(),
            1,
            "explicit stub must still obtain its class disclosure"
        );
        assert!(explicit.outcome.is_ok(), "{:?}", explicit.outcome);
        assert!(
            explicit.grip_class.is_some(),
            "explicit class disclosure was lost"
        );
        assert!(explicit.weak_oracle_guidance.is_none());
        assert_eq!(explicit.seam_line, 2);
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("src/lib.rs"))
                .map_err(|error| error.to_string())?,
            source
        );
    }
    Ok(())
}

const BOUNDARY: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty > 10 { price * 2 } else { price }\n}\n";
const WEAK_TEST: &str =
    "#[test]\nfn weak_at_boundary() {\n    assert!(stub_inventory::discount(10, 10) > 0);\n}\n";

#[test]
fn predicate_route_retains_existing_weak_test_guidance() -> Result<(), String> {
    let fixture = Fixture::new(
        BOUNDARY,
        "    if qty >= 10 { price * 2 } else { price }",
        "    if qty > 10 { price * 2 } else { price }",
    )?;
    fixture.write("tests/weak.rs", WEAK_TEST)?;
    let finding = fixture.finding("predicate")?;
    let calls = LookupCount::reset();
    let route = stub_route_for_finding(&fixture.0, &RiprConfig::default(), &finding, &mut None)
        .ok_or("predicate finding did not resolve")?;
    assert_eq!(calls.get(), 1, "predicate lost its full evidence lookup");
    let StubRouteDecision::WeakOracle { guidance } = route.decision else {
        return Err(format!(
            "expected existing-test guidance, got {:?}",
            route.decision
        ));
    };
    assert!(guidance.contains(crate::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP));
    assert!(guidance.contains("weak_at_boundary"));
    assert!(guidance.contains("tests/weak.rs"));
    let explicit = fixture.explicit("predicate")?;
    assert_eq!(
        explicit.weak_oracle_guidance.as_deref(),
        Some(guidance.as_str())
    );
    Ok(())
}

#[test]
fn predicate_inventory_still_sees_a_second_stronger_test() -> Result<(), String> {
    let fixture = Fixture::new(
        BOUNDARY,
        "    if qty >= 10 { price * 2 } else { price }",
        "    if qty > 10 { price * 2 } else { price }",
    )?;
    fixture.write("tests/weak.rs", WEAK_TEST)?;
    // Keep the original diff finding while the explicit resolver sees another
    // test. Replacing the full lookup with that finding's facts would wrongly
    // retain the single-weak-observer guidance.
    let finding = fixture.finding("predicate")?;
    fixture.write("tests/strong.rs", "#[test]\nfn exact_at_boundary() {\n    assert_eq!(stub_inventory::discount(10, 10), 10);\n}\n")?;
    let calls = LookupCount::reset();
    let route = stub_route_for_finding(&fixture.0, &RiprConfig::default(), &finding, &mut None)
        .ok_or("predicate finding did not resolve")?;
    assert_eq!(
        calls.get(),
        1,
        "predicate evidence must not be replaced by diff facts"
    );
    assert!(
        matches!(
            route.decision,
            StubRouteDecision::Stub {
                kind: "predicate",
                ..
            }
        ),
        "{route:?}"
    );
    let explicit = fixture.explicit("predicate")?;
    assert!(explicit.outcome.is_ok(), "{:?}", explicit.outcome);
    assert!(
        explicit.weak_oracle_guidance.is_none(),
        "second observer was omitted"
    );
    assert!(explicit.grip_class.is_some());
    Ok(())
}
