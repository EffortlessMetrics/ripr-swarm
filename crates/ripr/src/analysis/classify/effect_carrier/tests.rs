use super::*;
use crate::analysis::rust_index::summarize_file;
use crate::domain::{DeltaKind, OracleStrength, ProbeId, SourceLocation, SymbolId};
use std::path::{Path, PathBuf};

const LIB: &str = "src/lib.rs";

/// The verdict-corpus `ledger-receive-refresh-low-stock` subject after its
/// diff: `receive` gained `self.refresh_low_stock(sku);`, which writes only
/// `self.low_stock`, read only by `is_low`.
const LEDGER: &str = r#"use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Received { sku: String, qty: u32 },
    Shipped { sku: String, qty: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub sku: String,
    pub qty: u32,
    pub remaining: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inventory {
    stock: BTreeMap<String, u32>,
    log: Vec<Event>,
    shipped_total: u64,
    low_stock: BTreeSet<String>,
    low_threshold: u32,
}

impl Inventory {
    pub fn new(low_threshold: u32) -> Self {
        Self {
            low_threshold,
            ..Self::default()
        }
    }

    pub fn receive(&mut self, sku: &str, qty: u32) {
        *self.stock.entry(sku.to_string()).or_insert(0) += qty;
        self.log.push(Event::Received { sku: sku.to_string(), qty });
        self.refresh_low_stock(sku);
    }

    pub fn ship(&mut self, sku: &str, qty: u32) -> Result<Receipt, String> {
        let available = match self.stock.get(sku) {
            Some(n) => *n,
            None => return Err(sku.to_string()),
        };
        let remaining = available - qty;
        self.stock.insert(sku.to_string(), remaining);
        self.shipped_total += u64::from(qty);
        self.log.push(Event::Shipped { sku: sku.to_string(), qty });
        self.refresh_low_stock(sku);
        Ok(Receipt {
            sku: sku.to_string(),
            qty,
            remaining,
        })
    }

    fn refresh_low_stock(&mut self, sku: &str) {
        if self.on_hand(sku) < self.low_threshold {
            self.low_stock.insert(sku.to_string());
        } else {
            self.low_stock.remove(sku);
        }
    }

    pub fn on_hand(&self, sku: &str) -> u32 {
        self.stock.get(sku).copied().unwrap_or(0)
    }

    pub fn history(&self) -> &[Event] {
        &self.log
    }

    pub fn is_low(&self, sku: &str) -> bool {
        self.low_stock.contains(sku)
    }

    pub fn low_skus(&self) -> Vec<String> {
        self.low_stock.iter().cloned().collect()
    }
}
"#;

const LEDGER_TESTS: &str = r#"use demo::*;

#[test]
fn ledger() {
    let mut inv = Inventory::new(5);
    inv.receive("BOLT-M8", 10);
    let receipt = inv.ship("BOLT-M8", 3).unwrap();
    let copy = inv.clone();
    let low = inv.low_skus();
    let fresh = Inventory::new(5);
    let built = setup();
    let staged = fixtures::stocked();
    let blank: Inventory = Default::default();
    let mut gathered = Vec::new();
    gathered.extend(inv.low_skus());
    let count: usize = 3;
    assert_eq!(inv.history(), &[Event::Received { sku: "BOLT-M8".to_string(), qty: 10 }]);
    assert_eq!(receipt, Receipt { sku: "BOLT-M8".to_string(), qty: 3, remaining: 7 });
    assert_eq!(inv.history().to_vec(), Vec::new());
    assert_eq!(inv.low_skus(), vec!["BOLT-M8".to_string()]);
    assert_eq!(vec![inv.is_low("BOLT-M8")], vec![true]);
    assert_eq!(inv, fresh);
    assert_eq!(copy, fresh);
    assert_eq!(low, Vec::<String>::new());
    assert_eq!(unbound, Receipt { sku: "X".to_string(), qty: 1, remaining: 0 });
    assert_eq!(format!("{:?}", inv), String::new());
}
"#;

fn index(files: &[(&str, &str)]) -> RustIndex {
    let mut index = RustIndex::default();
    index.package_names.insert("demo".to_string());
    for (path, text) in files {
        let facts = summarize_file(PathBuf::from(path), (*text).to_string());
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from(path), facts);
    }
    index
}

fn owner<'a>(index: &'a RustIndex, name: &str) -> &'a FunctionSummary {
    let found = index
        .functions()
        .iter()
        .find(|function| function.name == name && function.file == Path::new(LIB));
    assert!(found.is_some(), "owner `{name}` must be indexed from {LIB}");
    found.unwrap_or(index.functions().at(0))
}

fn effect_probe(owner: &FunctionSummary, expression: &str) -> Probe {
    Probe {
        id: ProbeId("probe".to_string()),
        location: SourceLocation::new(owner.file.clone(), owner.start_line + 3, 1),
        owner: Some(SymbolId(owner.id.0.clone())),
        family: ProbeFamily::CallDeletion,
        delta: DeltaKind::Effect,
        before: None,
        after: None,
        expression: expression.to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    }
}

fn establish(index: &RustIndex, expression: &str) -> Option<EffectStateCarrier> {
    let owner = owner(index, "receive");
    EffectStateCarrier::establish(&effect_probe(owner, expression), owner, index)
}

fn whole_object(text: &str) -> OracleFact {
    OracleFact {
        line: 1,
        text: text.to_string(),
        kind: OracleKind::WholeObjectEquality,
        strength: OracleStrength::Strong,
        observed_tokens: Vec::new(),
        ok_value_observed: None,
    }
}

fn the_test(index: &RustIndex) -> &TestSummary {
    assert_eq!(index.tests().len(), 1, "fixture must hold exactly one test");
    index.tests().at(0)
}

#[test]
fn ledger_carrier_bounds_written_fields_and_readers() {
    let index = index(&[(LIB, LEDGER)]);
    let carrier = establish(&index, "self.refresh_low_stock(sku)");
    assert!(
        carrier.is_some(),
        "the ledger shape must establish a carrier"
    );
    let Some(carrier) = carrier else { return };
    assert_eq!(
        carrier.written_fields,
        BTreeSet::from(["low_stock".to_string()])
    );
    assert_eq!(
        carrier.reader_methods,
        BTreeSet::from(["is_low".to_string(), "low_skus".to_string()])
    );
    for non_reader in ["history", "on_hand", "ship", "receive", "refresh_low_stock"] {
        assert!(
            carrier.non_reader_methods.contains(non_reader),
            "`{non_reader}` reads no written field"
        );
    }
}

#[test]
fn whole_object_equality_confirms_only_when_it_can_hold_the_written_field() {
    let index = index(&[(LIB, LEDGER), ("tests/ledger.rs", LEDGER_TESTS)]);
    let test = the_test(&index);
    let carrier = establish(&index, "self.refresh_low_stock(sku);");
    assert!(
        carrier.is_some(),
        "the ledger shape must establish a carrier"
    );
    let Some(carrier) = carrier else { return };
    let cases = [
        // Cannot hold `low_stock`: a non-reading getter, a value returned by
        // a non-reading method, std methods on those values.
        (
            r#"assert_eq!(inv.history(), &[Event::Received { sku: "BOLT-M8".to_string(), qty: 10 }]);"#,
            false,
        ),
        (
            r#"assert_eq!(receipt, Receipt { sku: "BOLT-M8".to_string(), qty: 3, remaining: 7 });"#,
            false,
        ),
        ("assert_eq!(inv.history().to_vec(), Vec::new());", false),
        // Reads `low_stock` through a reader method or holds the receiver.
        (
            r#"assert_eq!(inv.low_skus(), vec!["BOLT-M8".to_string()]);"#,
            true,
        ),
        (
            r#"assert_eq!(vec![inv.is_low("BOLT-M8")], vec![true]);"#,
            true,
        ),
        ("assert_eq!(inv, fresh);", true),
        ("assert_eq!(copy, fresh);", true),
        ("assert_eq!(low, Vec::<String>::new());", true),
        (r#"assert_eq!(format!("{:?}", inv), String::new());"#, true),
        (r#"assert_eq!(format!("{inv:?}"), String::new());"#, true),
        (r#"assert_eq!(format!("{{inv}}"), String::new());"#, false),
        // A fixture helper's result, a receiver-typed annotation, and a
        // `mut` binding written after its `let` may all carry the state.
        ("assert_eq!(built, fresh);", true),
        ("assert_eq!(staged, fresh);", true),
        ("assert_eq!(blank, Vec::<String>::new());", true),
        ("assert_eq!(gathered, Vec::<String>::new());", true),
        // A primitive annotation on a literal still cannot.
        ("assert_eq!(count, Vec::new());", false),
        ("assert_eq!(inv.low_stock, BTreeSet::new());", true),
        // A binding with no `let` in the test is unknown: it may carry.
        (
            r#"assert_eq!(unbound, Receipt { sku: "X".to_string(), qty: 1, remaining: 0 });"#,
            true,
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(
            carrier.admits(test, &whole_object(text)),
            expected,
            "admission of `{text}`"
        );
    }
    // Mock expectations and snapshots keep the Part C reading.
    for kind in [OracleKind::MockExpectation, OracleKind::Snapshot] {
        let mut fact = whole_object("assert_eq!(inv.history(), Vec::new());");
        fact.kind = kind;
        assert!(carrier.admits(test, &fact));
    }
}

#[test]
fn unbounded_effects_keep_the_part_c_reading() {
    let base = index(&[(LIB, LEDGER)]);
    // Not a `self.callee(..)` call, or a `?` / `&mut` argument.
    for expression in [
        "self.log.push(Event::Received { sku: sku.to_string(), qty })",
        "self.refresh_low_stock(sku)?",
        "self.refresh_low_stock(&mut buffer)",
        "refresh_low_stock(sku)",
        "self.missing(sku)",
    ] {
        assert!(
            establish(&base, expression).is_none(),
            "`{expression}` must not establish a carrier"
        );
    }

    let variant = |from: &str, to: &str| {
        assert!(LEDGER.contains(from), "fixture must contain `{from}`");
        LEDGER.replace(from, to)
    };
    let unbounded = [
        // A free function call may reach global state.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            notify(sku);",
        ),
        // A non-pure macro performs I/O.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            println!(\"low\");",
        ),
        // The whole receiver escapes.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            audit(self);",
        ),
        // Interior mutability is invisible to a field scan.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            self.hits.borrow_mut().push(1);",
        ),
        // `&self` callee: any write is interior mutability.
        variant(
            "fn refresh_low_stock(&mut self, sku: &str)",
            "fn refresh_low_stock(&self, sku: &str)",
        ),
        // A returned value can flow through the owner.
        variant(
            "fn refresh_low_stock(&mut self, sku: &str) {",
            "fn refresh_low_stock(&mut self, sku: &str) -> bool {",
        ),
        // A transitive self call that does not resolve.
        variant("if self.on_hand(sku)", "if self.level(sku)"),
        // The owner reads the written field after the call and moves it
        // into the log, so `inv.history()` can observe the deletion (#7046
        // review).
        variant(
            "        self.refresh_low_stock(sku);\n    }",
            "        self.refresh_low_stock(sku);\n        if self.low_stock.contains(sku) {\n            self.log.push(Event::Shipped { sku: sku.to_string(), qty: 0 });\n        }\n    }",
        ),
        // The owner calls a reader of the written field after the call.
        variant(
            "        self.refresh_low_stock(sku);\n    }",
            "        self.refresh_low_stock(sku);\n        if self.is_low(sku) {\n            self.log.push(Event::Shipped { sku: sku.to_string(), qty: 0 });\n        }\n    }",
        ),
        // A by-value `mut self` receiver is not the borrowed receiver the
        // gate names.
        variant(
            "fn refresh_low_stock(&mut self, sku: &str)",
            "fn refresh_low_stock(mut self, sku: &str)",
        ),
        // A trait also names the callee: the call may not reach the impl.
        variant(
            "impl Inventory {",
            "pub trait Refresh { fn refresh_low_stock(&mut self, sku: &str); }\n\nimpl Inventory {",
        ),
    ];
    for source in &unbounded {
        let index = index(&[(LIB, source)]);
        assert!(
            establish(&index, "self.refresh_low_stock(sku)").is_none(),
            "an unbounded callee must keep the Part C reading:\n{source}"
        );
    }
}

#[test]
fn a_mutating_reader_called_by_the_test_carries_the_written_state() {
    // `reorder` reads `low_stock` and writes `log`: after it runs, the log
    // depends on the deleted call, so `inv.history()` discriminates it.
    let lib = LEDGER.replace(
        "    pub fn on_hand(&self, sku: &str) -> u32 {",
        "    pub fn reorder(&mut self) {\n        for sku in self.low_stock.clone() {\n            self.log.push(Event::Shipped { sku, qty: 0 });\n        }\n    }\n\n    pub fn on_hand(&self, sku: &str) -> u32 {",
    );
    assert!(
        lib.contains("pub fn reorder(&mut self)"),
        "fixture must gain `reorder`"
    );
    let assertion = r#"assert_eq!(inv.history(), &[Event::Received { sku: "A".to_string(), qty: 2 }, Event::Shipped { sku: "A".to_string(), qty: 0 }]);"#;
    let test_with = |call: &str| {
        format!(
            "use demo::*;\n\n#[test]\nfn reorder_after_receive() {{\n    let mut inv = Inventory::new(5);\n    inv.receive(\"A\", 2);\n    {call}\n    {assertion}\n}}\n"
        )
    };
    for (call, expected) in [
        ("inv.reorder();", true),
        ("Inventory::reorder(&mut inv);", true),
        // `ship` writes but reads no written field: it cannot carry it.
        ("inv.ship(\"A\", 1).unwrap();", false),
        // `is_low` reads `low_stock` but cannot move it into the log.
        ("let _ = inv.is_low(\"A\");", false),
    ] {
        let tests = test_with(call);
        let idx = index(&[(LIB, &lib), ("tests/ledger.rs", &tests)]);
        let carrier = establish(&idx, "self.refresh_low_stock(sku);");
        assert!(
            carrier.is_some(),
            "the ledger shape must establish a carrier"
        );
        let Some(carrier) = carrier else { return };
        assert_eq!(
            carrier.mutating_readers,
            BTreeSet::from(["reorder".to_string()])
        );
        assert_eq!(
            carrier.admits(the_test(&idx), &whole_object(assertion)),
            expected,
            "admission after `{call}`"
        );
    }
}

#[test]
fn path_calls_and_ref_mut_patterns_keep_the_part_c_reading() {
    let variant = |from: &str, to: &str| {
        assert!(LEDGER.contains(from), "fixture must contain `{from}`");
        LEDGER.replace(from, to)
    };
    let remove = "self.low_stock.remove(sku);";
    for added in [
        // Path-qualified calls can reach global state like a free call.
        "Audit::record(sku);",
        "crate::audit::record(sku);",
        "audit::record(sku);",
        // A `ref mut` binding writes a place the field scan does not see.
        "match self.low_threshold { ref mut t => *t += 1 }",
    ] {
        let source = variant(remove, &format!("{remove}\n            {added}"));
        let idx = index(&[(LIB, &source)]);
        assert!(
            establish(&idx, "self.refresh_low_stock(sku)").is_none(),
            "`{added}` must keep the Part C reading"
        );
    }
    // An associated function of the self type is not traversed and may
    // publish state outside the receiver; a field method outside the known
    // in-object operations may too (#7046 review).
    for added in [
        "let _ = Self::threshold_floor();",
        "let _ = Inventory::threshold_floor();",
        "self.journal.write_all(sku.as_bytes()).ok();",
        "self.journal.as_ref().write_all(sku.as_bytes()).ok();",
        "self.tx.clone().send(sku.to_string()).ok();",
        "self.sink.publish(sku);",
        "self.events.record::<u32>(1);",
        "self.flush::<u32>();",
    ] {
        let source = variant(remove, &format!("{remove}\n            {added}"));
        let idx = index(&[(LIB, &source)]);
        assert!(
            establish(&idx, "self.refresh_low_stock(sku)").is_none(),
            "`{added}` must keep the Part C reading"
        );
    }
    // A user `Drop` impl anywhere makes collection stores unbounded.
    let with_drop = format!(
        "{LEDGER}\n\npub struct Token;\n\nimpl Drop for Token {{\n    fn drop(&mut self) {{}}\n}}\n"
    );
    let idx = index(&[(LIB, &with_drop)]);
    assert!(establish(&idx, "self.refresh_low_stock(sku)").is_none());
    // std roots stay bounded.
    for added in [
        "let _ = std::cmp::max(1, 2);",
        "let _ = u32::from(1u8);",
        "let _ = String::from(sku);",
    ] {
        let source = variant(remove, &format!("{remove}\n            {added}"));
        let idx = index(&[(LIB, &source)]);
        assert!(
            establish(&idx, "self.refresh_low_stock(sku)").is_some(),
            "`{added}` must keep the carrier bounded"
        );
    }
}

#[test]
fn primitive_and_std_path_calls_do_not_count_as_fixture_helpers() {
    let idx = index(&[(LIB, LEDGER), ("tests/ledger.rs", LEDGER_TESTS)]);
    let test = the_test(&idx);
    let carrier = establish(&idx, "self.refresh_low_stock(sku);");
    assert!(
        carrier.is_some(),
        "the ledger shape must establish a carrier"
    );
    let Some(carrier) = carrier else { return };
    for text in [
        "assert_eq!(receipt.remaining, u32::from(7u8));",
        "assert_eq!(receipt.qty, usize::try_from(3u64).unwrap_or(0));",
        "assert_eq!(receipt.sku, str::to_owned(\"A\"));",
        "assert_eq!(receipt.sku, String::from(\"A\"));",
        "assert_eq!(receipt.remaining, std::cmp::max(7, 0));",
    ] {
        assert!(
            !carrier.admits(test, &whole_object(text)),
            "`{text}` cannot carry"
        );
    }
    // A lowercase module helper is still a possible fixture.
    assert!(carrier.admits(
        test,
        &whole_object("assert_eq!(receipt, fixtures::receipt());")
    ));
}

#[test]
fn escapes_found_in_review_keep_the_part_c_reading() {
    // #7046 review: effects that leave the object, and readers or test
    // actions the first scan missed.
    let variant = |from: &str, to: &str| {
        assert!(LEDGER.contains(from), "fixture must contain `{from}`");
        LEDGER.replace(from, to)
    };
    let remove = "self.low_stock.remove(sku);";
    for added in [
        // A std path into I/O or the environment leaves the object.
        "std::fs::write(\"inv.log\", sku).ok();",
        "std::env::set_var(\"LOW\", sku);",
        // A mutating call on a parameter may write through a shared handle.
        "sku.make_ascii_uppercase();",
    ] {
        let source = variant(remove, &format!("{remove}\n            {added}"));
        let idx = index(&[(LIB, &source)]);
        assert!(
            establish(&idx, "self.refresh_low_stock(sku);").is_none(),
            "`{added}` must keep the Part C reading"
        );
    }

    let reader_lib = |method: &str| {
        variant(
            "    pub fn on_hand(&self, sku: &str) -> u32 {",
            &format!("{method}\n\n    pub fn on_hand(&self, sku: &str) -> u32 {{"),
        )
    };
    let run = |lib: &str, body: &str, assertion: &str| {
        let tests = format!(
            "use demo::*;\n\n#[test]\nfn after_receive() {{\n    let mut inv = Inventory::new(5);\n    inv.receive(\"A\", 2);\n    {body}\n    {assertion}\n}}\n"
        );
        let idx = index(&[(LIB, lib), ("tests/ledger.rs", &tests)]);
        let carrier = establish(&idx, "self.refresh_low_stock(sku);");
        assert!(
            carrier.is_some(),
            "the ledger shape must establish a carrier"
        );
        carrier.is_some_and(|carrier| carrier.admits(the_test(&idx), &whole_object(assertion)))
    };
    let history = r#"assert_eq!(inv.history(), &[]);"#;

    // A reader that formats the whole receiver inline reads every field.
    let describe =
        reader_lib("    pub fn describe(&self) -> String {\n        format!(\"{self:?}\")\n    }");
    assert!(run(&describe, "", r#"assert_eq!(inv.describe(), "x");"#));

    // A discarded call that is not a plain store reads the field.
    let drain = reader_lib(
        "    pub fn drain_low(&mut self) {\n        self.low_stock.clone_into(&mut self.seen);\n    }",
    );
    assert!(run(&drain, "inv.drain_low();", history));

    // A test helper given `&mut inv` may call a mutating reader.
    assert!(run(LEDGER, "restock(&mut inv);", history));
    // A resolved non-reader of the self type does not.
    assert!(!run(
        LEDGER,
        "Inventory::ship(&mut inv, \"A\", 1).ok();",
        history
    ));

    // A binding's field outside the self type may hold the receiver.
    assert!(run(
        LEDGER,
        "let app = App { inventory: inv.clone() };",
        "assert_eq!(app.inventory, Inventory::new(5));",
    ));
    // A workspace type's associated function may return the receiver.
    assert!(run(
        LEDGER,
        "let bed = TestBed::with_inventory();",
        "assert_eq!(bed, TestBed::expected());",
    ));
    // A macro initializer may capture the receiver inside its string.
    assert!(run(
        LEDGER,
        "let dump = format!(\"{inv:?}\");",
        "assert_eq!(dump, String::new());",
    ));
    // A `&self` reader that reaches outside the object can publish the
    // written state before the assertion.
    let publish = reader_lib(
        "    pub fn publish_low_state(&self) {\n        std::fs::write(\"low\", format!(\"{:?}\", self.low_stock)).ok();\n    }",
    );
    assert!(run(
        &publish,
        "inv.publish_low_state();",
        "assert_eq!(std::fs::read_to_string(\"low\").ok(), None);",
    ));
    // A turbofish constructor is a std path, not a fixture helper.
    assert!(!run(
        LEDGER,
        "",
        "assert_eq!(inv.history(), Vec::<Event>::new());",
    ));

    // A shared handle may alias the object's state.
    assert!(run(
        LEDGER,
        "let sink = Rc::new(Sink::default());",
        "assert_eq!(*sink, Sink::default());",
    ));
    // Lending the receiver without an argument position, or rebinding it.
    assert!(run(LEDGER, "let r = &mut inv;\n    restock(r);", history));
    assert!(run(LEDGER, "inv = restocked(inv);", history));
    // A pure by-value std method on a local does not make the callee opaque.
    let pure = variant(
        "self.low_stock.remove(sku);",
        "self.low_stock.remove(sku);\n            let _len = sku.trim().len();",
    );
    let idx = index(&[(LIB, &pure)]);
    assert!(establish(&idx, "self.refresh_low_stock(sku);").is_some());
}
